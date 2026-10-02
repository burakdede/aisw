import * as vscode from 'vscode';

import { CliAdapter, CliError } from './cliAdapter';
import { BinaryPathError, resolveBinary } from './binaryResolver';
import { CapabilitiesPayload, ContextEntry, JsonEnvelope, Profile, ProfileList, StatusPayload } from './contract';
import { deriveViewState, toolLabel, WorkspaceStatusPayload } from './state';

const INSTALL_URL = 'https://github.com/burakdede/aisw#installation';

export function activate(context: vscode.ExtensionContext): void {
  const output = vscode.window.createOutputChannel('AISW', { log: true });
  const statusBar = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
  statusBar.show();
  const extension = new AiswExtension(context, output, statusBar);
  context.subscriptions.push(output, statusBar, ...extension.register());
  void extension.start();
}

export function deactivate(): void {}

class AiswExtension {
  private adapter?: CliAdapter;
  private status?: StatusPayload;
  private profiles?: ProfileList;
  private contexts?: ContextEntry[];
  private capabilities?: CapabilitiesPayload;
  private workspaceStatus?: WorkspaceStatusPayload;
  private refreshTimer?: NodeJS.Timeout;
  private refreshSequence = 0;
  private mutationQueue: Promise<void> = Promise.resolve();
  private mutationActive = false;
  private missingCliNotified = false;
  private handshake?: { binary: string; promise: ReturnType<CliAdapter['diagnose']> };

  constructor(
    private readonly extensionContext: vscode.ExtensionContext,
    private readonly output: vscode.OutputChannel,
    private readonly statusBar: vscode.StatusBarItem,
  ) {}

  register(): vscode.Disposable[] {
    return [
      vscode.commands.registerCommand('aisw.refresh', () => this.refresh(true)),
      vscode.commands.registerCommand('aisw.switchContext', () => this.switchContext()),
      vscode.commands.registerCommand('aisw.switchToWorkspaceContext', () => this.switchToWorkspaceContext()),
      vscode.commands.registerCommand('aisw.switchProfile', () => this.switchProfile()),
      vscode.commands.registerCommand('aisw.verify', () => this.verify()),
      vscode.commands.registerCommand('aisw.diagnose', () => this.diagnose()),
      vscode.commands.registerCommand('aisw.showOutput', () => this.output.show(true)),
      vscode.commands.registerCommand('aisw.getStarted', () => this.getStarted()),
      vscode.commands.registerCommand('aisw.installCli', () => this.installCli()),
      vscode.commands.registerCommand('aisw.locateCli', () => this.locateCli()),
      vscode.commands.registerCommand('aisw.addProfile', () => this.addProfile()),
      vscode.commands.registerCommand('aisw.removeProfile', () => this.removeProfile()),
      vscode.workspace.onDidChangeWorkspaceFolders(() => void this.refresh(false)),
      vscode.window.onDidChangeActiveTextEditor(() => this.scheduleRefresh()),
      vscode.window.onDidChangeWindowState((state) => { if (state.focused) this.scheduleRefresh(); }),
      vscode.workspace.onDidChangeConfiguration((event) => {
        if (event.affectsConfiguration('aisw')) { this.handshake = undefined; void this.refresh(false); }
      }),
      vscode.window.onDidEndTerminalShellExecution((event) => {
        if (event.terminal.name.startsWith('AISW:')) { this.output.appendLine('AISW terminal flow completed.'); void this.refresh(false); }
      }),
    ];
  }

  async start(): Promise<void> { await this.refresh(false); }

  private async refresh(showErrors: boolean, full = true): Promise<void> {
    if (this.mutationActive && !full) return;
    const sequence = ++this.refreshSequence;
    this.render({ loading: true });
    try {
      const adapter = this.createAdapter();
      const handshake = await this.ensureHandshake(adapter);
      const readFullState = full || !this.profiles || !this.contexts;
      const [status, workspaceStatus, profiles, contexts] = await Promise.all([
        adapter.status(), adapter.workspaceStatus(),
        readFullState ? adapter.profiles() : Promise.resolve(this.profiles),
        readFullState ? adapter.contexts() : Promise.resolve(this.contexts ? { contexts: this.contexts } : undefined),
      ]);
      if (sequence !== this.refreshSequence) return;
      this.adapter = adapter;
      this.capabilities = handshake.capabilities;
      this.status = status;
      this.profiles = profiles as ProfileList;
      this.contexts = (contexts as { contexts: ContextEntry[] }).contexts;
      this.workspaceStatus = workspaceStatus as WorkspaceStatusPayload;
      this.render();
      this.notifyWorkspaceMismatch();
    } catch (error) {
      if (sequence !== this.refreshSequence) return;
      const kind = error instanceof CliError ? error.kind : error instanceof BinaryPathError ? error.kind : isCompatibilityError(error) ? 'incompatible_cli' : error instanceof Error && error.message.startsWith('Could not find aisw') ? 'cli_not_found' : undefined;
      this.render({ error: { kind, message: messageFor(error) } });
      this.logError(error);
      if (showErrors) await this.showError(error);
      if (kind === 'cli_not_found' && !this.missingCliNotified) {
        this.missingCliNotified = true;
        const action = await vscode.window.showInformationMessage('AISW needs the aisw command-line tool to switch accounts.', 'Install aisw', 'Locate aisw…');
        if (action === 'Install aisw') await this.installCli();
        if (action === 'Locate aisw…') await this.locateCli();
      }
    }
  }

  private async switchContext(): Promise<void> {
    if (!this.adapter || !this.contexts) return this.requireRefresh();
    const selection = await vscode.window.showQuickPick(this.contexts.map((entry) => ({ label: entry.name, description: describeMappings(entry.profiles), entry })), { placeHolder: 'Select an AISW context' });
    if (selection) await this.runMutation(() => this.adapter!.useContext(selection.entry.name), 'Context switched.');
  }

  private async switchToWorkspaceContext(): Promise<void> {
    const expected = this.workspaceStatus?.expected_context;
    if (!expected) return this.switchContext();
    if (!this.contexts?.some((context) => context.name === expected)) { await vscode.window.showErrorMessage(`Workspace context '${expected}' is not available.`); return; }
    await this.runMutation(() => this.adapter!.useContext(expected), 'Workspace context switched.');
  }

  private async switchProfile(): Promise<void> {
    if (!this.adapter || !this.profiles) return this.requireRefresh();
    const items = profileItems(this.profiles);
    if (!items.length) return this.getStarted();
    const selection = await vscode.window.showQuickPick(items, { placeHolder: 'Select an AISW profile' });
    if (selection) await this.runMutation(() => this.adapter!.useProfile(selection.tool, selection.profile.name), 'Profile switched.');
  }

  private async runMutation(mutation: () => Promise<JsonEnvelope>, success: string): Promise<void> {
    const task = this.mutationQueue.then(() => this.performMutation(mutation, success));
    this.mutationQueue = task.catch(() => undefined);
    return task;
  }

  private async performMutation(mutation: () => Promise<JsonEnvelope>, success: string): Promise<void> {
    this.mutationActive = true;
    try {
      const result = await mutation();
      await this.refresh(false, true);
      const report = await this.scopedVerify(mutationTools(result));
      const warnings = mutationWarnings(result);
      if (warnings.length) await vscode.window.showWarningMessage(warnings.join(' '));
      if (report.failed.length) await vscode.window.showWarningMessage(`${success} ${report.failed.join(' ')}`);
      else await vscode.window.showInformationMessage(success);
    } catch (error) { await this.showError(error); }
    finally { this.mutationActive = false; }
  }

  private async verify(): Promise<void> {
    try {
      const report = await (this.adapter ?? this.createAdapter()).verify();
      await this.refresh(false, false);
      const formatted = formatVerification(report, undefined, this.profiles);
      if (formatted.failed.length) await vscode.window.showWarningMessage(formatted.message);
      else await vscode.window.showInformationMessage(formatted.message);
    } catch (error) { await this.showError(error); }
  }

  private async scopedVerify(affectedTools?: string[]): Promise<{ failed: string[] }> {
    if (!this.adapter) return { failed: [] };
    return formatVerification(await this.adapter.verify(), affectedTools, this.profiles);
  }

  private async diagnose(): Promise<void> {
    try {
      const result = await this.ensureHandshake(this.adapter ?? this.createAdapter(), true);
      this.output.clear(); this.output.appendLine(JSON.stringify(result, null, 2)); this.output.show(true);
    } catch (error) { this.logError(error); await this.showError(error); }
  }

  private async addProfile(): Promise<void> {
    if (!this.adapter || !this.capabilities) return this.requireRefresh();
    const tools = Object.entries(this.capabilities.tools ?? {});
    const tool = await vscode.window.showQuickPick(tools.map(([name, metadata]) => ({ label: toolLabel(name), description: metadata.auth_methods?.length ? `Auth: ${metadata.auth_methods.join(', ')}` : undefined, name })), { placeHolder: 'Select the tool for the new profile' });
    if (!tool) return;
    const profile = await vscode.window.showInputBox({ prompt: 'Profile name', validateInput: (value) => /^[A-Za-z0-9_-]{1,32}$/.test(value) ? undefined : 'Use 1–32 letters, numbers, hyphens, or underscores.' });
    if (profile) this.openTerminal(this.terminalExecutable(), ['add', tool.name, profile], `Add ${toolLabel(tool.name)} profile`);
  }

  private async installCli(): Promise<void> {
    if (vscode.env.remoteName) return;
    const methods = availableInstallMethods();
    const selection = await vscode.window.showQuickPick(methods, { placeHolder: 'Choose how to install aisw' });
    if (!selection) return;
    if (selection.methodKind === 'docs') { await vscode.env.openExternal(vscode.Uri.parse(INSTALL_URL)); return; }
    if (selection.methodKind === 'locate') { await this.locateCli(); return; }
    this.openTerminal(selection.executable, selection.args, selection.label);
  }

  private async locateCli(): Promise<void> {
    const selected = await vscode.window.showOpenDialog({ canSelectFiles: true, canSelectFolders: false, canSelectMany: false, openLabel: 'Use This aisw' });
    const file = selected?.[0]?.fsPath;
    if (!file) return;
    try {
      const adapter = new CliAdapter({ binaryPath: file, cwd: this.workspaceCwd() });
      await adapter.diagnose();
      await vscode.workspace.getConfiguration('aisw').update('binaryPath', file, vscode.ConfigurationTarget.Global);
      this.handshake = undefined;
      await this.refresh(false);
    } catch (error) { await vscode.window.showErrorMessage(`That file isn't a compatible aisw: ${messageFor(error)}`); }
  }

  private async removeProfile(): Promise<void> {
    if (!this.adapter || !this.profiles) return this.requireRefresh();
    const selection = await vscode.window.showQuickPick(profileItems(this.profiles), { placeHolder: 'Select the profile to remove' });
    if (!selection) return;
    if (selection.active) { await vscode.window.showInformationMessage('Switch away from the active profile before removing it.'); return; }
    const confirmed = await vscode.window.showWarningMessage(`Remove ${toolLabel(selection.tool)} profile '${selection.profile.name}'?`, { modal: true }, 'Remove');
    if (confirmed !== 'Remove') return;
    try {
      const result = await this.adapter.removeProfile(selection.tool, selection.profile.name);
      await this.refresh(false);
      const backupId = backupIds(result)[0];
      const action = await vscode.window.showInformationMessage('Profile removed.', ...(backupId ? ['Undo'] as const : []));
      if (action === 'Undo' && backupId) { await this.adapter.restoreBackup(backupId); await this.refresh(false); await vscode.window.showInformationMessage('Profile restored.'); }
    } catch (error) { await this.showError(error); }
  }

  private openTerminal(executable: string, args: string[], label = args.join(' ')): void {
    const terminal = vscode.window.createTerminal({ name: `AISW: ${label}` });
    terminal.show();
    let executed = false;
    const execute = () => { if (!executed && terminal.shellIntegration) { executed = true; terminal.shellIntegration.executeCommand(executable, args); } };
    if (terminal.shellIntegration) execute();
    else {
      this.output.appendLine('Waiting for terminal shell integration; using a quoted command if it remains unavailable.');
      const disposable = vscode.window.onDidChangeTerminalShellIntegration((event) => { if (event.terminal === terminal) { execute(); disposable.dispose(); } });
      this.extensionContext.subscriptions.push(disposable);
      setTimeout(() => { if (!executed && !terminal.shellIntegration) { executed = true; terminal.sendText([executable, ...args].map(shellQuote).join(' ')); } }, 3000);
    }
  }

  private async getStarted(): Promise<void> {
    if (!this.adapter) { await this.installCli(); return; }
    const action = await vscode.window.showInformationMessage('Add a profile in the AISW terminal, then refresh this window.', 'Add Profile', 'Set binary path');
    if (action === 'Add Profile') await vscode.commands.executeCommand('aisw.addProfile');
    if (action === 'Set binary path') await vscode.commands.executeCommand('workbench.action.openSettings', '@ext:aisw.aisw-vscode aisw.binaryPath');
  }

  private createAdapter(): CliAdapter {
    if (vscode.env.remoteName) throw new CliError('Remote extension hosts are not supported by AISW.', 'remote_host_unsupported');
    const activeUri = vscode.window.activeTextEditor?.document.uri;
    const activeFolder = activeUri ? vscode.workspace.getWorkspaceFolder(activeUri) : undefined;
    return new CliAdapter({
      binaryPath: resolveBinary(this.configuredBinary()),
      cwd: this.workspaceCwd(),
      onCommand: (command, durationMs, exitCode) => this.output.appendLine(`${command.join(' ')} exit=${exitCode ?? 'error'} duration=${durationMs}ms`),
    });
  }

  private configuredBinary(): string { return vscode.workspace.getConfiguration('aisw').get<string>('binaryPath', '').trim() || 'aisw'; }

  private workspaceCwd(): string {
    const activeUri = vscode.window.activeTextEditor?.document.uri;
    const activeFolder = activeUri ? vscode.workspace.getWorkspaceFolder(activeUri) : undefined;
    return activeFolder?.uri.fsPath ?? vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ?? process.cwd();
  }

  private terminalExecutable(): string { return this.configuredBinary() === 'aisw' ? 'aisw' : resolveBinary(this.configuredBinary()); }

  private requireRefresh(): Thenable<void> { return vscode.window.showInformationMessage('AISW is still loading; run AISW: Refresh if this persists.').then(() => undefined); }

  private scheduleRefresh(): void {
    if (this.refreshTimer) clearTimeout(this.refreshTimer);
    const delay = vscode.workspace.getConfiguration('aisw').get<number>('refreshDebounceMs', 500);
    this.refreshTimer = setTimeout(() => void this.refresh(false, false), delay);
  }

  private render(input: { loading?: boolean; error?: { kind?: string; message: string } } = {}): void {
    const view = deriveViewState({ ...input, status: this.status, profiles: this.profiles, workspace: this.workspaceStatus, remote: !!vscode.env.remoteName });
    this.statusBar.text = view.text; this.statusBar.command = view.command; this.statusBar.tooltip = markdownTooltip(view.tooltip);
    this.statusBar.accessibilityInformation = { label: view.accessibilityLabel, role: 'button' };
    this.statusBar.backgroundColor = view.background === 'warning' ? new vscode.ThemeColor('statusBarItem.warningBackground') : undefined;
    const stateKey = input.error?.kind === 'cli_not_found' ? 'cliMissing'
      : input.error?.kind === 'binary_path_invalid' ? 'binaryPathInvalid'
        : input.error?.kind === 'incompatible_cli' ? 'incompatible'
          : input.error?.kind ?? (input.loading ? 'loading' : vscode.env.remoteName ? 'remote' : this.adapter ? 'ready' : 'unknown');
    void vscode.commands.executeCommand('setContext', 'aisw.state', stateKey);
    void vscode.commands.executeCommand('setContext', 'aisw.isRemote', !!vscode.env.remoteName);
    void vscode.commands.executeCommand('setContext', 'aisw.cliInstalled', !input.error && !input.loading && !!this.adapter);
    void vscode.commands.executeCommand('setContext', 'aisw.workspaceMismatch', view.workspaceMismatch);
    void vscode.commands.executeCommand('setContext', 'aisw.hasProfiles', Object.values(this.profiles ?? {}).some((entry) => entry.profiles.length));
    void vscode.commands.executeCommand('setContext', 'aisw.hasContexts', (this.contexts?.length ?? 0) > 0);
  }

  private notifyWorkspaceMismatch(): void {
    const workspace = this.workspaceStatus; const expected = workspace?.expected_context; const active = this.status?.context.active ?? 'none';
    if (!expected || active === expected || !['mismatch', 'unmanaged', 'ambiguous_active'].includes(workspace?.status ?? '')) return;
    if (!vscode.workspace.getConfiguration('aisw').get<boolean>('workspaceMismatch.notify', true)) return;
    const key = `${active}->${expected}`; const seen = this.extensionContext.workspaceState.get<string[]>('workspaceMismatchNotifications', []);
    if (seen.includes(key)) return;
    void this.extensionContext.workspaceState.update('workspaceMismatchNotifications', [...seen, key]).then(async () => {
      const action = await vscode.window.showWarningMessage(`This workspace expects AISW context '${expected}', but '${active}' is active.`, 'Switch', 'Not now', "Don't ask again");
      if (action === 'Switch') await this.switchToWorkspaceContext();
      if (action === "Don't ask again") await vscode.workspace.getConfiguration('aisw').update('workspaceMismatch.notify', false, vscode.ConfigurationTarget.Workspace);
    });
  }

  private async showError(error: unknown): Promise<void> {
    this.logError(error); const message = messageFor(error);
    const action = error instanceof CliError && error.kind === 'cli_not_found' ? await vscode.window.showErrorMessage(message, 'Install aisw', 'Set binary path') : await vscode.window.showErrorMessage(message);
    if (action === 'Install aisw') await vscode.env.openExternal(vscode.Uri.parse(INSTALL_URL));
    if (action === 'Set binary path') await vscode.commands.executeCommand('workbench.action.openSettings', '@ext:aisw.aisw-vscode aisw.binaryPath');
  }

  private logError(error: unknown): void { this.output.appendLine(`[${new Date().toISOString()}] ${messageFor(error)}`); }

  private ensureHandshake(adapter: CliAdapter, force = false): ReturnType<CliAdapter['diagnose']> {
    const binary = this.configuredBinary();
    if (!force && this.handshake?.binary === binary) return this.handshake.promise;
    const promise = adapter.diagnose().catch((error) => { if (this.handshake?.promise === promise) this.handshake = undefined; throw error; });
    this.handshake = { binary, promise }; return promise;
  }
}

function profileItems(profiles: ProfileList): Array<{ label: string; description: string; tool: string; profile: Profile; active: boolean }> {
  return Object.entries(profiles).flatMap(([tool, entry]) => entry.profiles.map((profile) => ({ label: `${toolLabel(tool)} · ${profile.name}`, description: `${entry.active === profile.name ? 'active · switch away first' : 'available'}${profile.auth ? ` · ${profile.auth}` : ''}`, tool, profile, active: entry.active === profile.name })));
}

function describeMappings(profiles: Record<string, string | null>): string { return Object.entries(profiles).filter(([, profile]) => profile).map(([tool, profile]) => `${toolLabel(tool)}: ${profile}`).join(' · '); }

function markdownTooltip(value: string): vscode.MarkdownString { const tooltip = new vscode.MarkdownString(value); tooltip.supportThemeIcons = true; tooltip.isTrusted = { enabledCommands: ['aisw.switchContext', 'aisw.switchProfile', 'aisw.verify'] }; return tooltip; }
function messageFor(error: unknown): string { return error instanceof Error ? error.message : 'AISW command failed.'; }
function isCompatibilityError(error: unknown): boolean {
  return error instanceof Error && (/Unsupported aisw CLI API version|Unsupported aisw JSON schema version|Unsupported aisw progress schema version|missing required features/i.test(error.message));
}
function mutationTools(result: JsonEnvelope): string[] | undefined { const value = result.result; return value && typeof value === 'object' && Array.isArray((value as { affected_tools?: unknown }).affected_tools) ? (value as { affected_tools: unknown[] }).affected_tools.filter((tool): tool is string => typeof tool === 'string') : undefined; }
function mutationWarnings(result: JsonEnvelope): string[] { const value = result.result; return value && typeof value === 'object' && Array.isArray((value as { warnings?: unknown }).warnings) ? (value as { warnings: unknown[] }).warnings.filter((warning): warning is string => typeof warning === 'string') : []; }
function backupIds(result: JsonEnvelope): string[] { const value = result.result; return value && typeof value === 'object' && Array.isArray((value as { backup_ids?: unknown }).backup_ids) ? (value as { backup_ids: unknown[] }).backup_ids.filter((id): id is string => typeof id === 'string') : []; }

function formatVerification(report: unknown, affectedTools?: string[], profiles?: ProfileList): { message: string; failed: string[] } {
  if (!report || typeof report !== 'object') return { message: 'AISW verification returned no report.', failed: ['No report'] };
  const value = report as { summary?: { status?: string; failed?: number; warnings?: number }; tools?: Array<{ tool?: string; status?: string; issues?: string[] }> };
  const configured = new Set(affectedTools ?? Object.keys(profiles ?? {}));
  const details = (value.tools ?? []).filter((tool) => configured.has(tool.tool ?? '') && (profiles?.[tool.tool ?? '']?.profiles.length ?? 1) > 0);
  const failed = details.filter((tool) => tool.status === 'fail' || (tool.issues?.length ?? 0) > 0).map((tool) => `${toolLabel(tool.tool ?? 'tool')}: ${(tool.issues ?? ['verification failed']).join(', ')}`);
  const summary = value.summary; const counts = summary ? ` (${summary.failed ?? 0} failed, ${summary.warnings ?? 0} warnings)` : '';
  return { message: `AISW verification: ${summary?.status ?? 'unknown'}${counts}${failed.length ? ` — ${failed.join('; ')}` : ''}`, failed };
}

type InstallMethod = { label: string; description: string; methodKind: 'command' | 'locate' | 'docs'; executable: string; args: string[] };

function availableInstallMethods(): InstallMethod[] {
  const methods: InstallMethod[] = [];
  if (commandAvailable('brew')) methods.push({ label: 'Homebrew', description: 'brew install burakdede/tap/aisw', methodKind: 'command', executable: 'brew', args: ['install', 'burakdede/tap/aisw'] });
  methods.push({ label: 'Install script', description: 'curl -fsSL https://raw.githubusercontent.com/burakdede/aisw/main/install.sh | sh', methodKind: 'command', executable: process.env.SHELL || 'sh', args: ['-c', 'curl -fsSL https://raw.githubusercontent.com/burakdede/aisw/main/install.sh | sh'] });
  if (commandAvailable('cargo')) methods.push({ label: 'Cargo', description: 'cargo install aisw --locked', methodKind: 'command', executable: 'cargo', args: ['install', 'aisw', '--locked'] });
  methods.push({ label: 'Locate an Existing aisw…', description: 'Choose an installed aisw executable', methodKind: 'locate', executable: '', args: [] });
  methods.push({ label: 'Open Installation Guide', description: INSTALL_URL, methodKind: 'docs', executable: '', args: [] });
  return methods;
}

function shellQuote(value: string): string {
  return process.platform === 'win32' ? `"${value.replace(/"/g, '\\"')}"` : `'${value.replace(/'/g, `'"'"'`)}'`;
}

function commandAvailable(command: string): boolean {
  try {
    require('node:child_process').execFileSync(process.platform === 'win32' ? 'where' : 'which', [command], { stdio: 'ignore' });
    return true;
  } catch { return false; }
}
