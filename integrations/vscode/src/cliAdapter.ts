import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

import {
  assertCompatible,
  assertFeatures,
  CapabilitiesPayload,
  ContextList,
  JsonEnvelope,
  ProfileList,
  StatusPayload,
  VersionPayload,
} from './contract';

const execFileAsync = promisify(execFile);

export class CliError extends Error {
  constructor(
    message: string,
    readonly kind = 'cli_error',
    readonly exitCode?: number,
  ) {
    super(message);
    this.name = 'CliError';
  }
}

export interface CliAdapterOptions {
  binaryPath?: string;
  cwd: string;
  env?: NodeJS.ProcessEnv;
  timeoutMs?: number;
  onCommand?: (command: string[], durationMs: number, exitCode?: number) => void;
}

export class CliAdapter {
  private readonly binary: string;
  private readonly cwd: string;
  private readonly env: NodeJS.ProcessEnv;
  private readonly timeoutMs: number;
  private readonly onCommand?: CliAdapterOptions['onCommand'];

  constructor(options: CliAdapterOptions) {
    this.binary = options.binaryPath?.trim() || 'aisw';
    this.cwd = options.cwd;
    this.env = options.env ?? process.env;
    this.timeoutMs = options.timeoutMs ?? 30_000;
    this.onCommand = options.onCommand;
  }

  async version(): Promise<VersionPayload> {
    const result = await this.json<VersionPayload>(['version']);
    assertCompatible(result);
    return result;
  }

  async capabilities(): Promise<CapabilitiesPayload> {
    const result = await this.json<CapabilitiesPayload>(['capabilities']);
    assertCompatible(result);
    assertFeatures(result);
    return result;
  }

  status(): Promise<StatusPayload> {
    return this.json<StatusPayload>(['status', '--context']);
  }

  profiles(): Promise<ProfileList> {
    return this.json<ProfileList>(['list']);
  }

  contexts(): Promise<ContextList> {
    return this.json<ContextList>(['context', 'list']);
  }

  workspaceStatus(): Promise<unknown> {
    return this.json<unknown>(['workspace', 'status']);
  }

  projectBindings(): Promise<unknown> {
    return this.json<unknown>(['project-bindings', 'list']);
  }

  verify(): Promise<unknown> {
    return this.json<unknown>(['verify'], { allowNonZeroJson: true });
  }

  useProfile(tool: string, profile: string): Promise<JsonEnvelope> {
    return this.json<JsonEnvelope>(['use', tool === 'agy' ? 'antigravity' : tool, profile]);
  }

  useContext(context: string): Promise<JsonEnvelope> {
    return this.json<JsonEnvelope>(['context', 'use', context]);
  }

  removeProfile(tool: string, profile: string): Promise<JsonEnvelope> {
    return this.json<JsonEnvelope>(['remove', tool === 'agy' ? 'antigravity' : tool, profile, '--yes']);
  }

  restoreBackup(backupId: string): Promise<JsonEnvelope> {
    return this.json<JsonEnvelope>(['backup', 'restore', backupId, '--yes']);
  }

  async diagnose(): Promise<{ version: VersionPayload; capabilities: CapabilitiesPayload }> {
    const version = await this.version();
    const capabilities = await this.capabilities();
    return { version, capabilities };
  }

  private async json<T>(command: string[], options: { allowNonZeroJson?: boolean } = {}): Promise<T> {
    const args = ['--non-interactive', ...command, '--json'];
    const started = Date.now();
    try {
      const { stdout, stderr } = await execFileAsync(this.binary, args, {
        cwd: this.cwd,
        env: this.env,
        timeout: this.timeoutMs,
        maxBuffer: 2 * 1024 * 1024,
        windowsHide: true,
      });
      let parsed: unknown;
      try {
        parsed = JSON.parse(stdout);
      } catch {
        throw new CliError(`aisw returned invalid JSON for ${command.join(' ')}.`);
      }
      if (isEnvelopeFailure(parsed)) {
        throw new CliError(
          parsed.error?.message ?? 'aisw reported a command failure.',
          parsed.error?.kind ?? 'cli_error',
        );
      }
      if (stderr.trim()) {
        throw new CliError('aisw wrote unexpected diagnostics to stderr.');
      }
      this.onCommand?.(command, Date.now() - started, 0);
      return parsed as T;
    } catch (error) {
      const childError = error as NodeJS.ErrnoException & { code?: string | number; killed?: boolean; stdout?: string; stderr?: string };
      this.onCommand?.(command, Date.now() - started, typeof childError.code === 'number' ? childError.code : undefined);
      if (error instanceof CliError) throw error;
      if (childError.code === 'ENOENT') {
        throw new CliError(
          `Could not find aisw at '${this.binary}'. Install aisw or set aisw.binaryPath.`,
          'cli_not_found',
        );
      }
      if (childError.killed) {
        throw new CliError('aisw timed out while producing JSON output.', 'cli_timeout');
      }
      const output = parseJson(childError.stdout);
      if (options.allowNonZeroJson && output !== undefined && !isEnvelopeFailure(output)) {
        return output as T;
      }
      if (isEnvelopeFailure(output)) {
        throw new CliError(
          output.error?.message ?? 'aisw reported a command failure.',
          output.error?.kind ?? 'cli_error',
          typeof childError.code === 'number' ? childError.code : undefined,
        );
      }
      throw new CliError(sanitize(childError.stderr || childError.message || 'aisw failed.'));
    }
  }
}

function isEnvelopeFailure(value: unknown): value is JsonEnvelope {
  return typeof value === 'object' && value !== null && (value as JsonEnvelope).ok === false;
}

function parseJson(value: string | undefined): unknown {
  if (!value) return undefined;
  try {
    return JSON.parse(value);
  } catch {
    return undefined;
  }
}

function sanitize(value: string): string {
  return value
    .replace(/(sk-[A-Za-z0-9_-]{8,}|AIza[A-Za-z0-9_-]{12,})/g, '[REDACTED]')
    .replace(/(api[_-]?key|token|secret|password)(\s*[=:]\s*)\S+/gi, '$1$2[REDACTED]')
    .trim();
}
