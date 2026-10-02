import { mkdir, mkdtemp, readdir, readFile, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn, type ChildProcess } from 'node:child_process';

const ACTIVATION_LINE = 'Extension activated success: aisw.aisw-vscode';

async function main(): Promise<void> {
  const executable = process.env.CURSOR_EXECUTABLE_PATH;
  const cli = process.env.CURSOR_CLI_PATH;
  if (!executable || !cli) throw new Error('Set CURSOR_EXECUTABLE_PATH and CURSOR_CLI_PATH to run the isolated Cursor smoke test.');
  if (!existsSync(executable) || !existsSync(cli)) throw new Error('The configured Cursor executable or CLI does not exist.');

  const root = path.resolve(__dirname, '../..');
  const vsix = path.join(root, 'aisw-vscode-0.1.0.vsix');
  if (!existsSync(vsix)) throw new Error('Packaged VSIX is missing. Run npm run package first.');

  const isolated = await mkdtemp(path.join(os.tmpdir(), 'aisw-cursor-installed-'));
  const userDataDir = path.join(isolated, 'user-data');
  const extensionsDir = path.join(isolated, 'extensions');
  const home = path.join(isolated, 'home');
  const workspace = path.join(isolated, 'workspace');
  const env = { ...process.env, HOME: home, AISW_HOME: path.join(home, 'aisw') };
  await mkdir(path.join(userDataDir, 'User'), { recursive: true });
  await mkdir(workspace, { recursive: true });
  await writeFile(path.join(userDataDir, 'User', 'settings.json'), JSON.stringify({}));
  await writeFile(path.join(workspace, 'README.md'), '# AISW Cursor smoke test\n');

  await runCommand(cli, [
    `--user-data-dir=${userDataDir}`,
    `--extensions-dir=${extensionsDir}`,
    `--install-extension=${vsix}`,
    '--force',
  ], env);

  const listed = await runCommand(cli, [
    `--user-data-dir=${userDataDir}`,
    `--extensions-dir=${extensionsDir}`,
    '--list-extensions',
    '--show-versions',
  ], env);
  if (!listed.includes('aisw.aisw-vscode@0.1.0')) throw new Error('Cursor did not list the installed AISW VSIX.');

  const cursor = spawn(executable, [
    '--classic',
    '--skip-onboarding',
    '--disable-updates',
    '--disable-telemetry',
    `--user-data-dir=${userDataDir}`,
    `--extensions-dir=${extensionsDir}`,
    '--new-window',
    workspace,
  ], { detached: process.platform !== 'win32', env, stdio: 'ignore' });
  try {
    await waitForActivation(userDataDir, cursor, 90_000);
  } finally {
    await terminate(cursor);
  }
}

async function runCommand(command: string, args: string[], env: NodeJS.ProcessEnv): Promise<string> {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, { env, stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk: Buffer) => { stdout += chunk.toString(); });
    child.stderr.on('data', (chunk: Buffer) => { stderr += chunk.toString(); });
    child.once('error', reject);
    child.once('exit', (code, signal) => code === 0 ? resolve(stdout) : reject(new Error(`Cursor command failed (${code ?? signal}): ${stderr}`)));
  });
}

async function waitForActivation(userDataDir: string, cursor: ChildProcess, timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (cursor.exitCode !== null) throw new Error(`Cursor exited before AISW activation (code ${cursor.exitCode}).`);
    const logs = await activationLogs(userDataDir);
    if (logs.some((log) => log.includes(ACTIVATION_LINE))) return;
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`Timed out waiting for Cursor to activate AISW. Logs: ${path.join(userDataDir, 'logs')}`);
}

async function activationLogs(userDataDir: string): Promise<string[]> {
  const logsRoot = path.join(userDataDir, 'logs');
  if (!existsSync(logsRoot)) return [];
  const entries = await readdir(logsRoot, { withFileTypes: true });
  const values: string[] = [];
  for (const entry of entries) {
    if (!entry.isDirectory()) continue;
    const file = path.join(logsRoot, entry.name, 'window1', 'exthost', 'exthost.log');
    if (existsSync(file)) values.push(await readFile(file, 'utf8'));
  }
  return values;
}

async function terminate(child: ChildProcess): Promise<void> {
  if (child.exitCode !== null) return;
  if (child.pid && process.platform !== 'win32') {
    try { process.kill(-child.pid, 'SIGTERM'); } catch {}
  } else {
    child.kill();
  }
  await new Promise((resolve) => setTimeout(resolve, 1000));
  if (child.exitCode === null) {
    if (child.pid && process.platform !== 'win32') {
      try { process.kill(-child.pid, 'SIGKILL'); } catch {}
    } else child.kill('SIGKILL');
  }
}

void main().catch((error: unknown) => { console.error(error); process.exitCode = 1; });
