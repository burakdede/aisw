import { chmod, mkdir, mkdtemp, readdir, readFile, writeFile } from 'node:fs/promises';
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
  const cliLog = path.join(isolated, 'fake-cli.jsonl');
  const smokeMarker = path.join(isolated, 'smoke-marker.json');
  const env = { ...process.env, HOME: home, AISW_HOME: path.join(home, 'aisw') };
  const fakeCli = await createFakeCli(isolated, cliLog);
  await mkdir(path.join(userDataDir, 'User'), { recursive: true });
  await mkdir(workspace, { recursive: true });
  await writeFile(path.join(userDataDir, 'User', 'settings.json'), JSON.stringify({ 'aisw.binaryPath': fakeCli }));
  await writeFile(path.join(workspace, 'README.md'), '# AISW Cursor smoke test\n');
  await createSmokeDriver(extensionsDir, smokeMarker, env);

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
    '--disable-gpu',
    '--password-store=basic',
    '--use-inmemory-secretstorage',
    '--disable-workspace-trust',
    '--new-window',
    `--user-data-dir=${userDataDir}`,
    `--extensions-dir=${extensionsDir}`,
    workspace,
  ], { detached: process.platform !== 'win32', env, stdio: 'ignore' });
  try {
    const log = await waitForActivation(userDataDir, cursor, 90_000);
    console.log(`Cursor activation confirmed for aisw.aisw-vscode: ${log}`);
    await waitForSmokeDriver(smokeMarker, cursor, 30_000);
    const calls = (await readFile(cliLog, 'utf8')).trim().split('\n').filter(Boolean).map((line) => JSON.parse(line) as string[]);
    if (!calls.some((args) => args.includes('status') && args.includes('--context'))) throw new Error('Cursor smoke driver did not reach aisw status --context.');
    if (!calls.some((args) => args.includes('verify'))) throw new Error('Cursor smoke driver did not reach aisw verify.');
    if (!calls.some((args) => args.includes('init') && args.includes('--no-shell-hook'))) throw new Error('Cursor smoke driver did not execute the native aisw init terminal flow.');
    console.log('Cursor command smoke confirmed: aisw.refresh, aisw.verify, and the native aisw init terminal flow executed in the real extension host.');
  } finally {
    await terminate(cursor);
  }
}

async function createFakeCli(directory: string, logPath: string): Promise<string> {
  const source = path.join(directory, 'fake-aisw.cjs');
  const binary = path.join(directory, process.platform === 'win32' ? 'fake-aisw.cmd' : 'fake-aisw');
  await writeFile(source, `
const fs = require('node:fs');
const args = process.argv.slice(2).filter((arg) => !['--non-interactive', '--json', '--no-color'].includes(arg));
fs.appendFileSync(${JSON.stringify(logPath)}, JSON.stringify(args) + '\\n');
const key = args.join(' ');
const responses = {
  version: { version: '0.3.10', cli_api_version: 1, json_schema_version: 1, progress_schema_version: 1 },
  capabilities: { version: '0.3.10', cli_api_version: 1, json_schema_version: 1, progress_schema_version: 1, features: { mutation_json: true, verify: true, contexts: true }, tools: { claude: { auth_methods: ['oauth'] } } },
  'status --context': { tools: [{ tool: 'claude', active_profile: null }], context: { status: 'none', active: null, profiles: {} } },
  list: { claude: { active: null, profiles: [] } },
  'context list': { contexts: [] },
  'workspace status': { status: 'unmanaged', expected_context: null },
  verify: { summary: { status: 'pass', passed: 0, warnings: 0, failed: 0 }, tools: [] },
};
process.stdout.write(JSON.stringify(responses[key] || { ok: true, command: key, result: { affected_tools: [], warnings: [] } }) + '\\n');
`);
  if (process.platform === 'win32') {
    await writeFile(binary, `@echo off\n"${process.execPath}" "%~dp0fake-aisw.cjs" %*\n`);
  } else {
    await writeFile(binary, `#!/bin/sh\nexec ${JSON.stringify(process.execPath)} ${JSON.stringify(source)} "$@"\n`);
    await chmod(binary, 0o755);
  }
  return binary;
}

async function createSmokeDriver(extensionsDir: string, markerPath: string, env: NodeJS.ProcessEnv): Promise<void> {
  const directory = path.join(extensionsDir, 'aisw-smoke-driver-0.0.1');
  await mkdir(directory, { recursive: true });
  await writeFile(path.join(directory, 'package.json'), JSON.stringify({
    name: 'aisw-smoke-driver',
    publisher: 'aisw',
    version: '0.0.1',
    engines: { vscode: '^1.93.0' },
    activationEvents: ['onStartupFinished'],
    main: './extension.js',
  }));
  await writeFile(path.join(directory, 'extension.js'), `
const fs = require('node:fs');
const vscode = require('vscode');
exports.activate = async () => {
  await new Promise((resolve) => setTimeout(resolve, 2500));
  const extension = vscode.extensions.getExtension('aisw.aisw-vscode');
  if (!extension) throw new Error('AISW extension is not installed in the Cursor smoke host.');
  await extension.activate();
  await vscode.commands.executeCommand('aisw.refresh');
  await vscode.commands.executeCommand('aisw.verify');
  await vscode.commands.executeCommand('aisw.importLogins');
  await new Promise((resolve) => setTimeout(resolve, 2500));
  fs.writeFileSync(${JSON.stringify(markerPath)}, JSON.stringify({ refresh: true, verify: true, importLogins: true }));
};
`);
  env.AISW_CURSOR_SMOKE_MARKER = markerPath;
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

async function waitForActivation(userDataDir: string, cursor: ChildProcess, timeoutMs: number): Promise<string> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (cursor.exitCode !== null) throw new Error(`Cursor exited before AISW activation (code ${cursor.exitCode}).`);
    const logs = await activationLogs(userDataDir);
    const activationLog = logs.find((log) => log.content.includes(ACTIVATION_LINE));
    if (activationLog) return activationLog.path;
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`Timed out waiting for Cursor to activate AISW. Logs: ${path.join(userDataDir, 'logs')}`);
}

async function waitForSmokeDriver(markerPath: string, cursor: ChildProcess, timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (cursor.exitCode !== null) throw new Error(`Cursor exited before command smoke completed (code ${cursor.exitCode}).`);
    if (existsSync(markerPath)) return;
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`Timed out waiting for the Cursor command smoke driver. Marker: ${markerPath}`);
}

async function activationLogs(userDataDir: string): Promise<Array<{ path: string; content: string }>> {
  const logsRoot = path.join(userDataDir, 'logs');
  if (!existsSync(logsRoot)) return [];
  const entries = await readdir(logsRoot, { withFileTypes: true });
  const values: Array<{ path: string; content: string }> = [];
  for (const entry of entries) {
    if (!entry.isDirectory()) continue;
    const file = path.join(logsRoot, entry.name, 'window1', 'exthost', 'exthost.log');
    if (existsSync(file)) values.push({ path: file, content: await readFile(file, 'utf8') });
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
