import { ContextEntry, ProfileList, StatusPayload } from './contract';

export interface WorkspaceStatusPayload {
  status?: 'match' | 'mismatch' | 'unmanaged' | 'invalid_context' | 'ambiguous_active' | 'no_expected_context' | string;
  expected_context?: string | null;
  matched_rule?: string | null;
  active_context?: string | null;
}

export interface ViewInput {
  status?: StatusPayload;
  profiles?: ProfileList;
  workspace?: WorkspaceStatusPayload;
  loading?: boolean;
  switchingTo?: string;
  installing?: boolean;
  remote?: boolean;
  error?: { kind?: string; message: string };
}

export interface ViewState {
  text: string;
  icon: string;
  background?: 'warning';
  command?: string;
  tooltip: string;
  accessibilityLabel: string;
  workspaceMismatch: boolean;
  expectedContext?: string;
}

const TOOL_LABELS: Record<string, string> = {
  claude: 'Claude Code',
  codex: 'Codex CLI',
  gemini: 'Gemini CLI',
  agy: 'Antigravity',
};

export function deriveViewState(input: ViewInput): ViewState {
  if (input.remote) {
    return state('$(circle-slash) AISW', 'Remote workspace; AISW must run on the workspace host.', 'aisw.diagnose');
  }
  if (input.loading) {
    return state('$(sync~spin) AISW', 'Loading AISW state.');
  }
  if (input.switchingTo) {
    return state(`$(sync~spin) Switching to ${input.switchingTo}…`, `Switching to ${input.switchingTo}.`);
  }
  if (input.installing) {
    return state('$(sync~spin) Installing aisw…', 'Installing aisw in the AISW terminal.', 'workbench.action.terminal.focus');
  }
  if (input.error) {
    const missing = input.error.kind === 'cli_not_found';
    const invalidPath = input.error.kind === 'binary_path_invalid';
    const incompatible = input.error.kind === 'incompatible_cli';
    return state(
      missing ? '$(cloud-download) Install aisw' : invalidPath ? '$(error) AISW: Check binaryPath' : incompatible ? '$(error) AISW: Update aisw' : '$(error) AISW: Error',
      input.error.message,
      missing ? 'aisw.installCli' : invalidPath ? 'workbench.action.openSettings' : 'aisw.diagnose',
    );
  }

  const status = input.status;
  const context = status?.context;
  const allProfiles = Object.values(input.profiles ?? {});
  const hasProfiles = allProfiles.some((entry) => entry.profiles.length > 0);
  const workspace = input.workspace;
  const mismatch = workspaceMismatch(workspace);
  const expectedContext = workspace?.expected_context ?? undefined;

  if (!hasProfiles) {
    return state('$(account) AISW: Set Up', 'No AISW profiles are configured.', 'aisw.getStarted', mismatch, expectedContext);
  }

  const active = activeProfileSummary(status);
  const contextLabel = context?.status === 'ambiguous' && context.matches?.[0]
    ? context.matches[0]
    : context?.active ?? (active.length > 0 ? active[0] : 'No active profile');
  const suffix = context?.status === 'ambiguous' && context.matches?.length
    ? ` · ${context.matches.length} contexts`
    : context?.status === 'drift'
      ? ' · drift'
      : context?.status && !['exact', 'none'].includes(context.status)
        ? ` (${context.status})`
        : '';
  const noContextLabel = context?.status === 'none' && active.length > 0
    ? `${active[0]}${active.length > 1 ? ` +${active.length - 1}` : ''}`
    : contextLabel;
  const text = mismatch
    ? `$(warning) ${contextLabel}${suffix} ≠ ${expectedContext ?? 'workspace context'}`
    : `$(account) ${noContextLabel}${suffix}`;
  const command = mismatch ? 'aisw.switchToWorkspaceContext' : ['exact', 'drift', 'ambiguous', 'invalid_context'].includes(context?.status ?? '') ? 'aisw.switchContext' : 'aisw.switchProfile';
  return state(text, tooltipFor(input), command, mismatch, expectedContext, mismatch ? 'warning' : undefined);
}

function state(
  text: string,
  tooltip: string,
  command?: string,
  workspaceMismatch = false,
  expectedContext?: string,
  background?: 'warning',
): ViewState {
  return {
    text,
    icon: text.slice(0, text.indexOf(' ') > 0 ? text.indexOf(' ') : text.length),
    background,
    command,
    tooltip,
    accessibilityLabel: text.replace(/\$\([^)]*\)\s*/g, '').replace(/[·≠]/g, ' ').trim(),
    workspaceMismatch,
    expectedContext,
  };
}

function activeProfileSummary(status?: StatusPayload): string[] {
  return (status?.tools ?? [])
    .filter((tool) => tool.active_profile)
    .map((tool) => `${toolLabel(tool.tool)}:${tool.active_profile}`);
}

function workspaceMismatch(workspace?: WorkspaceStatusPayload): boolean {
  return !!workspace && ['mismatch', 'unmanaged', 'ambiguous_active'].includes(workspace.status ?? '') && !!workspace.expected_context;
}

function tooltipFor(input: ViewInput): string {
  const lines = ['**AISW Account**', ''];
  for (const tool of input.status?.tools ?? []) {
    lines.push(`- ${toolLabel(tool.tool)}: ${tool.active_profile ?? '—'}`);
  }
  if (input.workspace?.status) {
    lines.push('', `Workspace: ${workspaceDescription(input.workspace)}`);
  }
  lines.push('', '[Switch Context](command:aisw.switchContext) · [Switch Profile](command:aisw.switchProfile) · [Verify](command:aisw.verify)');
  return lines.join('\n');
}

function workspaceDescription(workspace: WorkspaceStatusPayload): string {
  if (workspace.status === 'match') return workspace.matched_rule ? `matches ${workspace.matched_rule}` : 'matches expected context';
  if (workspace.expected_context) return `expects ${workspace.expected_context}`;
  return workspace.status ?? 'unknown';
}

export function toolLabel(tool: string): string {
  return TOOL_LABELS[tool] ?? tool;
}

export function contextDescription(context: ContextEntry): string {
  return Object.entries(context.profiles)
    .filter(([, profile]) => profile)
    .map(([tool, profile]) => `${toolLabel(tool)}: ${profile}`)
    .join(' · ');
}
