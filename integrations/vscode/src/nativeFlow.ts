export interface NativeCommand {
  executable: string;
  args: string[];
}

export type TerminalFlowOutcome = 'success' | 'failure' | 'canceled';

export function terminalFlowOutcome(exitCode?: number): TerminalFlowOutcome {
  if (exitCode === 0) return 'success';
  if (exitCode === undefined) return 'canceled';
  return 'failure';
}

export function contextCreateCommand(executable: string): NativeCommand {
  return { executable, args: ['context', 'create'] };
}

export function addProfileCommand(binary: string, tool: string, profile: string): NativeCommand {
  return { executable: binary, args: ['add', tool, profile] };
}

export function importLoginsCommand(binary: string): NativeCommand {
  return { executable: binary, args: ['init', '--no-shell-hook'] };
}

export function workspaceBindCommand(binary: string, workspace: string, context: string): NativeCommand {
  return { executable: binary, args: ['workspace', 'bind', workspace, '--context', context] };
}
