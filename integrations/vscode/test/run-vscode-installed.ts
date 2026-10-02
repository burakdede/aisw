import { mkdir, mkdtemp, readFile, rename, writeFile } from 'node:fs/promises';
import { existsSync, symlinkSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';

import { downloadAndUnzipVSCode, resolveCliPathFromVSCodeExecutablePath, runTests } from '@vscode/test-electron';

async function main(): Promise<void> {
  const root = path.resolve(__dirname, '../..');
  const vsix = path.join(root, 'aisw-vscode-0.1.0.vsix');
  if (!(await readFile(vsix)).length) throw new Error('Packaged VSIX is missing. Run npm run package first.');

  const isolated = await mkdtemp(path.join(os.tmpdir(), 'aisw-vscode-installed-'));
  const userDataDir = path.join(isolated, 'user-data');
  const extensionsDir = path.join(isolated, 'extensions');
  const home = path.join(isolated, 'home');
  const env = { ...process.env, HOME: home, AISW_HOME: path.join(home, 'aisw') };
  const binary = path.resolve(root, '../../target/release/aisw');
  await mkdir(path.join(userDataDir, 'User'), { recursive: true });
  await writeFile(path.join(userDataDir, 'User', 'settings.json'), JSON.stringify({ 'aisw.binaryPath': binary }));
  const vscodeVersion = process.env.VSCODE_VERSION ?? 'stable';
  let vscodeExecutablePath = process.env.CURSOR_EXECUTABLE_PATH ?? process.env.VSCODE_EXECUTABLE_PATH;
  if (!vscodeExecutablePath) {
    vscodeExecutablePath = await downloadAndUnzipVSCode({ version: vscodeVersion });
    linkMacOSExecutable(vscodeExecutablePath);
    if (!existsSync(vscodeExecutablePath)) {
      const cachedVersionDirectory = path.resolve(vscodeExecutablePath, '../../../../');
      await rename(cachedVersionDirectory, `${cachedVersionDirectory}.stale-${Date.now()}`);
      vscodeExecutablePath = await downloadAndUnzipVSCode({ version: vscodeVersion });
      linkMacOSExecutable(vscodeExecutablePath);
    }
  }

  const vscodeCli = process.env.CURSOR_CLI_PATH ?? process.env.VSCODE_CLI_PATH ?? resolveCliPathFromVSCodeExecutablePath(vscodeExecutablePath);
  await runCommand(vscodeCli, [
    `--user-data-dir=${userDataDir}`,
    `--extensions-dir=${extensionsDir}`,
    `--install-extension=${vsix}`,
    '--force',
  ], env);

  await runTests({
    vscodeExecutablePath,
    extensionDevelopmentPath: path.join(root, 'test', 'installed-host'),
    extensionTestsPath: path.resolve(__dirname, 'vscode-installed-host.js'),
    extensionTestsEnv: env,
    launchArgs: [
      '--user-data-dir', userDataDir,
      '--extensions-dir', extensionsDir,
      '--disable-workspace-trust',
      '--disable-gpu',
      '--password-store=basic',
      '--disable-extension=vscode.github',
      '--disable-extension=vscode.github-authentication',
    ],
  });
}

async function runCommand(command: string, args: string[], env: NodeJS.ProcessEnv): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    const child = spawn(command, args, { env, stdio: 'inherit' });
    child.once('error', reject);
    child.once('exit', (code, signal) => code === 0 ? resolve() : reject(new Error(`VS Code command failed (${code ?? signal}).`)));
  });
}

function linkMacOSExecutable(executablePath: string): void {
  if (process.platform !== 'darwin' || existsSync(executablePath)) return;
  const codePath = path.join(path.dirname(executablePath), 'Code');
  if (existsSync(codePath)) symlinkSync('Code', executablePath);
}

void main();
