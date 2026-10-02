import { execFileSync } from 'node:child_process';
import { accessSync, constants } from 'node:fs';
import os from 'node:os';
import path from 'node:path';

export class BinaryPathError extends Error {
  readonly kind = 'binary_path_invalid';
}

export function resolveBinary(configured: string): string {
  const value = configured.trim();
  if (value) {
    if (!isExecutable(value)) throw new BinaryPathError(`aisw.binaryPath points to '${value}', but the file is missing or not executable.`);
    return value;
  }
  const onPath = findOnPath(process.platform === 'win32' ? 'aisw.exe' : 'aisw');
  if (onPath) return onPath;
  for (const candidate of knownCandidates()) if (isExecutable(candidate)) return candidate;
  throw new Error("Could not find aisw. Install aisw or set aisw.binaryPath.");
}

export function isExecutable(file: string): boolean {
  try { accessSync(file, constants.X_OK); return true; } catch { return false; }
}

function findOnPath(command: string): string | undefined {
  try {
    const lookup = process.platform === 'win32' ? 'where' : 'which';
    return execFileSync(lookup, [command], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim().split(/\r?\n/)[0] || undefined;
  } catch { return undefined; }
}

function knownCandidates(): string[] {
  const home = os.homedir();
  if (process.platform === 'win32') return [path.join(home, '.cargo', 'bin', 'aisw.exe')];
  return [
    path.join(home, '.local', 'bin', 'aisw'),
    path.join(home, '.cargo', 'bin', 'aisw'),
    '/opt/homebrew/bin/aisw',
    '/usr/local/bin/aisw',
    '/home/linuxbrew/.linuxbrew/bin/aisw',
  ];
}
