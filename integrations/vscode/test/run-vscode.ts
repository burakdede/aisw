import { existsSync, symlinkSync } from 'node:fs';
import { chmod, mkdir, mkdtemp, rename, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';

import { downloadAndUnzipVSCode, runTests } from '@vscode/test-electron';

async function main(): Promise<void> {
  const options = {
    version: process.env.VSCODE_VERSION ?? '1.93.1',
    extensionDevelopmentPath: path.resolve(__dirname, '../..'),
    extensionTestsPath: path.resolve(__dirname, 'vscode-host.js'),
  };
  const isolated = await mkdtemp(path.join(os.tmpdir(), 'aisw-vscode-host-'));
  const userDataDir = path.join(isolated, 'user-data');
  const extensionsDir = path.join(isolated, 'extensions');
  const home = path.join(isolated, 'home');
  const binary = await createFakeCli(isolated);
  await mkdir(home, { recursive: true });
  await mkdir(path.join(userDataDir, 'User'), { recursive: true });
  await writeFile(path.join(userDataDir, 'User', 'settings.json'), JSON.stringify({}));
  const extensionTestsEnv: NodeJS.ProcessEnv = { ...process.env, HOME: home, AISW_HOME: path.join(home, 'aisw') };
  extensionTestsEnv.PATH = `${isolated}${path.delimiter}${process.env.PATH ?? ''}`;
  let vscodeExecutablePath = await downloadAndUnzipVSCode(options);
  linkMacOSExecutable(vscodeExecutablePath);
  if (!existsSync(vscodeExecutablePath)) {
    const cachedVersionDirectory = path.resolve(vscodeExecutablePath, '../../../../');
    if (existsSync(cachedVersionDirectory)) await rename(cachedVersionDirectory, `${cachedVersionDirectory}.stale-${Date.now()}`);
    vscodeExecutablePath = await downloadAndUnzipVSCode(options);
    linkMacOSExecutable(vscodeExecutablePath);
  }
  if (!existsSync(vscodeExecutablePath)) throw new Error(`VS Code test executable is missing: ${vscodeExecutablePath}`);
  await runTests({
    ...options,
    vscodeExecutablePath,
    extensionTestsEnv,
    launchArgs: [
      `--user-data-dir=${userDataDir}`,
      `--extensions-dir=${extensionsDir}`,
      '--disable-workspace-trust', '--disable-gpu', '--no-sandbox', '--password-store=basic',
      '--disable-extension=vscode.github', '--disable-extension=vscode.github-authentication',
    ],
  });
}

async function createFakeCli(directory: string): Promise<string> {
  const script = path.join(directory, 'fake-aisw.cjs');
  const binary = path.join(directory, process.platform === 'win32' ? 'aisw.cmd' : 'aisw');
  await writeFile(script, fakeCliSource());
  if (process.platform === 'win32') {
    await writeFile(binary, `@echo off\n"${process.execPath}" "%~dp0fake-aisw.cjs" %*\n`);
  } else {
    await writeFile(binary, `#!/bin/sh\nexec "${process.execPath}" "${script}" "$@"\n`);
    await chmod(binary, 0o755);
  }
  return binary;
}

function fakeCliSource(): string {
  return String.raw`
const command = process.argv.slice(2).filter((arg) => arg !== '--non-interactive' && arg !== '--json');
const key = command.join(' ');
const responses = {
  version: { version: '0.3.10', cli_api_version: 1, json_schema_version: 1, progress_schema_version: 1 },
  capabilities: { version: '0.3.10', cli_api_version: 1, json_schema_version: 1, progress_schema_version: 1, features: { mutation_json: true, verify: true, contexts: true }, tools: { claude: { auth_methods: ['oauth'] } } },
  'status --context': { tools: [{ tool: 'claude', active_profile: 'work' }], context: { status: 'exact', active: 'work', profiles: { claude: 'work' } } },
  list: { claude: { active: 'work', profiles: [{ name: 'work', auth: 'oauth' }] } },
  'context list': { contexts: [{ name: 'work', profiles: { claude: 'work' } }] },
  'workspace status': { status: 'match', expected_context: 'work' },
  verify: { summary: { status: 'pass', passed: 1, warnings: 0, failed: 0 }, tools: [] },
};
const response = responses[key] ?? { ok: true, command: key, result: { affected_tools: ['claude'], warnings: [] } };
process.stdout.write(JSON.stringify(response) + '\n');
`;
}

function linkMacOSExecutable(executablePath: string): void {
  if (process.platform !== 'darwin' || existsSync(executablePath)) return;
  const codePath = path.join(path.dirname(executablePath), 'Code');
  if (existsSync(codePath)) symlinkSync('Code', executablePath);
}

void main();
