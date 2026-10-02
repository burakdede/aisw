import * as vscode from 'vscode';

import { CliAdapter, CliError } from './cliAdapter';
import { ContextEntry, Profile, ProfileList, StatusPayload } from './contract';

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
    if (vscode.env.remoteName) {
      this.setFailure('Remote extension hosts are not supported by the AISW MVP.');
      return;
    }
    await this.refresh(false);
  }

  private async refresh(showErrors: boolean): Promise<void> {
    try {
      const adapter = this.createAdapter();
      await adapter.diagnose();
      const [status, profiles, contexts, workspaceStatus] = await Promise.all([
        adapter.status(),
        adapter.profiles(),
        adapter.contexts(),
        adapter.workspaceStatus(),
      ]);
      this.adapter = adapter;
      this.status = status;
      this.profiles = profiles;
      this.contexts = contexts.contexts;
      this.workspaceStatus = workspaceStatus as { status?: string; expected_context?: string | null };
      this.renderStatus();
    } catch (error) {
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
      await this.adapter.useContext(selection.entry.name);
      await this.refresh(true);
      await this.confirmVerified('Context switched and verified.');
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
      await this.adapter.useProfile(selection.tool, selection.profile.name);
      await this.refresh(true);
      await this.confirmVerified('Profile switched and verified.');
    } catch (error) {
      await vscode.window.showErrorMessage(messageFor(error));
    }
  }

  private async verify(): Promise<void> {
    try {
      const adapter = this.adapter ?? this.createAdapter();
      await adapter.verify();
      await this.refresh(true);
      await vscode.window.showInformationMessage('AISW active state verified.');
    } catch (error) {
      await vscode.window.showErrorMessage(messageFor(error));
    }
  }

  private async diagnose(): Promise<void> {
    try {
      const result = await (this.adapter ?? this.createAdapter()).diagnose();
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
    this.openTerminal(`add ${tool} ${profile}`);
  }

  private async removeProfile(): Promise<void> {
    if (!this.profiles) return this.requireRefresh();
    const selection = await vscode.window.showQuickPick(profileItems(this.profiles), {
      placeHolder: 'Select the profile to remove',
    });
    if (!selection) return;
    const tool = selection.tool === 'agy' ? 'antigravity' : selection.tool;
    this.openTerminal(`remove ${tool} ${selection.profile.name}`);
  }

  private openTerminal(command: string): void {
    const terminal = vscode.window.createTerminal({ name: `AISW: ${command}` });
    terminal.show();
    terminal.sendText(`${shellCommand(this.configuredBinary())} ${command}`);
    void vscode.window.showInformationMessage(
      'Complete the native aisw flow in the integrated terminal, then run AISW: Refresh.',
    );
  }

  private createAdapter(): CliAdapter {
    const cwd = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ?? process.cwd();
    const binaryPath = this.configuredBinary();
    return new CliAdapter({ binaryPath, cwd });
  }

  private configuredBinary(): string {
    return vscode.workspace.getConfiguration('aisw').get<string>('binaryPath', '').trim() || 'aisw';
  }

  private async confirmVerified(message: string): Promise<void> {
    if (!this.adapter) return;
    await this.adapter.verify();
    await vscode.window.showInformationMessage(message);
  }

  private requireRefresh(): Thenable<void> {
    return vscode.window.showInformationMessage('AISW is still loading; run AISW: Refresh if this persists.').then(() => undefined);
  }

  private scheduleRefresh(): void {
    if (this.refreshTimer) clearTimeout(this.refreshTimer);
    const delay = vscode.workspace.getConfiguration('aisw').get<number>('refreshDebounceMs', 500);
    this.refreshTimer = setTimeout(() => void this.refresh(false), delay);
  }

  private renderStatus(): void {
    const context = this.status?.context;
    const active = context?.active ?? 'No context';
    const suffix = context?.status && context.status !== 'exact' ? ` (${context.status})` : '';
    const workspaceSuffix = this.workspaceStatus?.status === 'mismatch' ? ' · workspace mismatch' : '';
    this.statusBar.text = `$(account) AISW: ${active}${suffix}${workspaceSuffix}`;
    this.statusBar.tooltip = context?.profiles ? describeMappings(context.profiles) : 'AISW';
  }

  private setFailure(message: string): void {
    this.statusBar.text = '$(warning) AISW: unavailable';
    this.statusBar.tooltip = message;
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

function shellCommand(binary: string): string {
  if (binary === 'aisw') return binary;
  return `'${binary.replaceAll("'", "'\\''")}'`;
}
