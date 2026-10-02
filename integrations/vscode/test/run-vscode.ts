import { existsSync, symlinkSync } from 'node:fs';
import { rename } from 'node:fs/promises';
import path from 'node:path';

import { downloadAndUnzipVSCode, runTests } from '@vscode/test-electron';

async function main(): Promise<void> {
  const options = {
    version: process.env.VSCODE_VERSION ?? 'stable',
    extensionDevelopmentPath: path.resolve(__dirname, '../..'),
    extensionTestsPath: path.resolve(__dirname, 'vscode-host.js'),
    launchArgs: [
      '--disable-workspace-trust', '--disable-gpu', '--no-sandbox', '--password-store=basic',
      '--disable-extension=vscode.github', '--disable-extension=vscode.github-authentication',
    ],
  };
  let vscodeExecutablePath = await downloadAndUnzipVSCode(options);
  linkMacOSExecutable(vscodeExecutablePath);
  if (!existsSync(vscodeExecutablePath)) {
    const cachedVersionDirectory = path.resolve(vscodeExecutablePath, '../../../../');
    if (existsSync(cachedVersionDirectory)) await rename(cachedVersionDirectory, `${cachedVersionDirectory}.stale-${Date.now()}`);
    vscodeExecutablePath = await downloadAndUnzipVSCode(options);
    linkMacOSExecutable(vscodeExecutablePath);
  }
  if (!existsSync(vscodeExecutablePath)) throw new Error(`VS Code test executable is missing: ${vscodeExecutablePath}`);
  await runTests({ ...options, vscodeExecutablePath });
}

function linkMacOSExecutable(executablePath: string): void {
  if (process.platform !== 'darwin' || existsSync(executablePath)) return;
  const codePath = path.join(path.dirname(executablePath), 'Code');
  if (existsSync(codePath)) symlinkSync('Code', executablePath);
}

void main();
