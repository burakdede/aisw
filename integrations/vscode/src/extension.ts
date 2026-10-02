import * as vscode from 'vscode';

import { CliAdapter, CliError } from './cliAdapter';
import { ContextEntry, JsonEnvelope, Profile, ProfileList, StatusPayload } from './contract';

export function activate(context: vscode.ExtensionContext): void {
  const output = vscode.window.createOutputChannel('AISW');
  const statusBar = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
  statusBar.command = 'aisw.switchContext';
  statusBar.text = '$(account) AISW: loading';
  statusBar.show();

  const extension = new AiswExtension(output, statusBar);
  context.subscriptions.push(output, statusBar, ...extension.register());
  void extension.start();
}

export function deactivate(): void {}

class AiswExtension {
  private adapter?: CliAdapter;
  private status?: StatusPayload;
  private profiles?: ProfileList;
  private contexts?: ContextEntry[];
  private workspaceStatus?: { status?: string; expected_context?: string | null };
  private refreshTimer?: NodeJS.Timeout;
  private refreshSequence = 0;
  private handshake?: { binary: string; promise: ReturnType<CliAdapter['diagnose']> };

  constructor(
    private readonly output: vscode.OutputChannel,
    private readonly statusBar: vscode.StatusBarItem,
  ) {}

  register(): vscode.Disposable[] {
    return [
      vscode.commands.registerCommand('aisw.refresh', () => this.refresh(true)),
      vscode.commands.registerCommand('aisw.switchContext', () => this.switchContext()),
      vscode.commands.registerCommand('aisw.switchProfile', () => this.switchProfile()),
      vscode.commands.registerCommand('aisw.verify', () => this.verify()),
      vscode.commands.registerCommand('aisw.diagnose', () => this.diagnose()),
      vscode.commands.registerCommand('aisw.addProfile', () => this.addProfile()),
      vscode.commands.registerCommand('aisw.removeProfile', () => this.removeProfile()),
      vscode.workspace.onDidChangeWorkspaceFolders(() => void this.refresh(false)),
      vscode.window.onDidChangeWindowState((state) => {
        if (state.focused) this.scheduleRefresh();
      }),
      vscode.workspace.onDidChangeConfiguration((event) => {
        if (event.affectsConfiguration('aisw')) void this.refresh(false);
      }),
    ];
  }

  async start(): Promise<void> {
    await this.refresh(false);
  }

  private async refresh(showErrors: boolean, full = true): Promise<void> {
    const sequence = ++this.refreshSequence;
    try {
      const adapter = this.createAdapter();
      await this.ensureHandshake(adapter);
      const readFullState = full || !this.profiles || !this.contexts;
      const [status, workspaceStatus, profiles, contexts] = await Promise.all([
        adapter.status(),
        adapter.workspaceStatus(),
        readFullState ? adapter.profiles() : Promise.resolve(this.profiles),
        readFullState ? adapter.contexts() : Promise.resolve(this.contexts ? { contexts: this.contexts } : undefined),
      ]);
      if (sequence !== this.refreshSequence) return;
      this.adapter = adapter;
      this.status = status;
      this.profiles = profiles as ProfileList;
      this.contexts = (contexts as { contexts: ContextEntry[] }).contexts;
      this.workspaceStatus = workspaceStatus as { status?: string; expected_context?: string | null };
      this.renderStatus();
    } catch (error) {
      if (sequence !== this.refreshSequence) return;
      this.setFailure(messageFor(error));
      if (showErrors) void vscode.window.showErrorMessage(messageFor(error));
    }
  }

  private async switchContext(): Promise<void> {
    if (!this.adapter || !this.contexts) return this.requireRefresh();
    const selection = await vscode.window.showQuickPick(
      this.contexts.map((entry) => ({ label: entry.name, description: describeMappings(entry.profiles), entry })),
      { placeHolder: 'Select an AISW context' },
    );
    if (!selection) return;
    try {
      const result = await this.adapter.useContext(selection.entry.name);
      await this.refresh(false, false);
      await this.confirmVerified(result, 'Context switched and verified.');
    } catch (error) {
      await vscode.window.showErrorMessage(messageFor(error));
    }
  }

  private async switchProfile(): Promise<void> {
    if (!this.adapter || !this.profiles) return this.requireRefresh();
    const items = profileItems(this.profiles);
    const selection = await vscode.window.showQuickPick(items, { placeHolder: 'Select an AISW profile' });
    if (!selection) return;
    try {
      const result = await this.adapter.useProfile(selection.tool, selection.profile.name);
      await this.refresh(false, false);
      await this.confirmVerified(result, 'Profile switched and verified.');
    } catch (error) {
      await vscode.window.showErrorMessage(messageFor(error));
    }
  }

  private async verify(): Promise<void> {
    try {
      const adapter = this.adapter ?? this.createAdapter();
      const report = await adapter.verify();
      await this.refresh(false, false);
      await vscode.window.showInformationMessage(formatVerification(report));
    } catch (error) {
      await vscode.window.showErrorMessage(messageFor(error));
    }
  }

  private async diagnose(): Promise<void> {
    try {
      const adapter = this.adapter ?? this.createAdapter();
      const result = await this.ensureHandshake(adapter, true);
      this.output.clear();
      this.output.appendLine(JSON.stringify(result, null, 2));
      this.output.show(true);
    } catch (error) {
      this.setFailure(messageFor(error));
      this.output.appendLine(messageFor(error));
      this.output.show(true);
      await vscode.window.showErrorMessage(messageFor(error));
    }
  }

  private async addProfile(): Promise<void> {
    const tool = await vscode.window.showQuickPick(['claude', 'codex', 'gemini', 'antigravity'], {
      placeHolder: 'Select the tool for the new profile',
    });
    if (!tool) return;
    const profile = await vscode.window.showInputBox({
      prompt: 'Profile name',
      validateInput: (value) => /^[A-Za-z0-9_-]{1,32}$/.test(value)
        ? undefined
        : 'Use 1–32 letters, numbers, hyphens, or underscores.',
    });
    if (!profile) return;
    this.openTerminal(['add', tool, profile]);
  }

  private async removeProfile(): Promise<void> {
    if (!this.profiles) return this.requireRefresh();
    const selection = await vscode.window.showQuickPick(profileItems(this.profiles), {
      placeHolder: 'Select the profile to remove',
    });
    if (!selection) return;
    const tool = selection.tool === 'agy' ? 'antigravity' : selection.tool;
    this.openTerminal(['remove', tool, selection.profile.name]);
  }

  private openTerminal(args: string[]): void {
    const command = args.join(' ');
    const terminal = vscode.window.createTerminal({
      name: `AISW: ${command}`,
      shellPath: this.configuredBinary(),
      shellArgs: args,
    });
    terminal.show();
    void vscode.window.showInformationMessage(
      'Complete the native aisw flow in the integrated terminal, then run AISW: Refresh.',
    );
  }

  private createAdapter(): CliAdapter {
    if (vscode.env.remoteName) {
      throw new CliError('Remote extension hosts are not supported by the AISW MVP.', 'remote_host_unsupported');
    }
    const activeUri = vscode.window.activeTextEditor?.document.uri;
    const activeFolder = activeUri ? vscode.workspace.getWorkspaceFolder(activeUri) : undefined;
    const cwd = activeFolder?.uri.fsPath ?? vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ?? process.cwd();
    const binaryPath = this.configuredBinary();
    return new CliAdapter({ binaryPath, cwd });
  }

  private configuredBinary(): string {
    return vscode.workspace.getConfiguration('aisw').get<string>('binaryPath', '').trim() || 'aisw';
  }

  private async confirmVerified(result: JsonEnvelope, message: string): Promise<void> {
    if (!this.adapter) return;
    const report = await this.adapter.verify();
    const warnings = mutationWarnings(result);
    const detail = [message, formatVerification(report, mutationTools(result)), ...warnings.map((warning) => `Warning: ${warning}`)].join(' ');
    await vscode.window.showInformationMessage(detail);
  }

  private requireRefresh(): Thenable<void> {
    return vscode.window.showInformationMessage('AISW is still loading; run AISW: Refresh if this persists.').then(() => undefined);
  }

  private scheduleRefresh(): void {
    if (this.refreshTimer) clearTimeout(this.refreshTimer);
    const delay = vscode.workspace.getConfiguration('aisw').get<number>('refreshDebounceMs', 500);
    this.refreshTimer = setTimeout(() => void this.refresh(false, false), delay);
  }

  private renderStatus(): void {
    const context = this.status?.context;
    const active = context?.active ?? 'No context';
    const suffix = context?.status && !['exact', 'none'].includes(context.status) ? ` (${context.status})` : '';
    const workspaceSuffix = this.workspaceStatus?.status === 'mismatch' ? ' · workspace mismatch' : '';
    this.statusBar.text = `$(account) AISW: ${active}${suffix}${workspaceSuffix}`;
    this.statusBar.tooltip = context?.profiles ? describeMappings(context.profiles) : 'AISW';
  }

  private setFailure(message: string): void {
    this.statusBar.text = '$(warning) AISW: unavailable';
    this.statusBar.tooltip = message;
  }

  private ensureHandshake(adapter: CliAdapter, force = false): ReturnType<CliAdapter['diagnose']> {
    const binary = this.configuredBinary();
    if (!force && this.handshake?.binary === binary) return this.handshake.promise;
    const promise = adapter.diagnose().catch((error) => {
      if (this.handshake?.promise === promise) this.handshake = undefined;
      throw error;
    });
    this.handshake = { binary, promise };
    return promise;
  }
}

function profileItems(profiles: ProfileList): Array<{ label: string; description: string; tool: string; profile: Profile }> {
  return Object.entries(profiles).flatMap(([tool, entry]) =>
    entry.profiles.map((profile) => ({
      label: profile.name,
      description: `${tool}${entry.active === profile.name ? ' · active' : ''}${profile.auth ? ` · ${profile.auth}` : ''}`,
      tool,
      profile,
    })),
  );
}

function describeMappings(profiles: Record<string, string | null>): string {
  return Object.entries(profiles)
    .filter(([, profile]) => profile)
    .map(([tool, profile]) => `${tool}: ${profile}`)
    .join(' · ');
}

function messageFor(error: unknown): string {
  if (error instanceof CliError) return error.message;
  return error instanceof Error ? error.message : 'AISW command failed.';
}

function mutationTools(result: JsonEnvelope): string[] | undefined {
  const value = result.result;
  if (!value || typeof value !== 'object' || !Array.isArray((value as { affected_tools?: unknown }).affected_tools)) return undefined;
  return (value as { affected_tools: unknown[] }).affected_tools.filter((tool): tool is string => typeof tool === 'string');
}

function mutationWarnings(result: JsonEnvelope): string[] {
  const value = result.result;
  if (!value || typeof value !== 'object' || !Array.isArray((value as { warnings?: unknown }).warnings)) return [];
  return (value as { warnings: unknown[] }).warnings.filter((warning): warning is string => typeof warning === 'string');
}

function formatVerification(report: unknown, affectedTools?: string[]): string {
  if (!report || typeof report !== 'object') return 'AISW verification returned no report.';
  const value = report as {
    summary?: { status?: string; failed?: number; warnings?: number };
    tools?: Array<{ tool?: string; issues?: string[]; remediation?: string[] }>;
  };
  const summary = value.summary;
  const details = (value.tools ?? [])
    .filter((tool) => !affectedTools || affectedTools.includes(tool.tool ?? ''))
    .flatMap((tool) => [
      ...(tool.issues ?? []).map((issue) => `${tool.tool ?? 'tool'}: ${issue}`),
      ...(tool.remediation ?? []).map((step) => `${tool.tool ?? 'tool'}: ${step}`),
    ]);
  const status = summary?.status ?? 'unknown';
  const counts = summary ? ` (${summary.failed ?? 0} failed, ${summary.warnings ?? 0} warnings)` : '';
  return `AISW verification: ${status}${counts}${details.length ? ` — ${details.join('; ')}` : ''}`;
}
