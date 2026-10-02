import { existsSync, symlinkSync } from 'node:fs';
import path from 'node:path';

import { downloadAndUnzipVSCode, runTests } from '@vscode/test-electron';

async function main(): Promise<void> {
  const options = {
    version: process.env.VSCODE_VERSION ?? 'stable',
    extensionDevelopmentPath: path.resolve(__dirname, '../..'),
    extensionTestsPath: path.resolve(__dirname, 'vscode-host.js'),
    launchArgs: ['--disable-workspace-trust', '--disable-gpu', '--no-sandbox'],
  };
  const vscodeExecutablePath = await downloadAndUnzipVSCode(options);
  if (process.platform === 'darwin') {
    const electronPath = path.resolve(path.dirname(vscodeExecutablePath), 'Electron');
    if (!existsSync(electronPath)) symlinkSync('Code', electronPath);
  }
  await runTests({ ...options, vscodeExecutablePath });
}

void main();
