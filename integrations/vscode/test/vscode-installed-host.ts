import assert from 'node:assert/strict';

import * as vscode from 'vscode';

export async function run(): Promise<void> {
  const extension = vscode.extensions.getExtension('aisw.aisw-vscode');
  assert.ok(extension, 'packaged AISW extension should be installed in the isolated extensions directory');
  await extension.activate();
  assert.ok(extension.isActive, 'packaged AISW extension should activate');

  const commands = await vscode.commands.getCommands(true);
  for (const command of [
    'aisw.refresh',
    'aisw.switchContext',
    'aisw.switchProfile',
    'aisw.verify',
    'aisw.diagnose',
    'aisw.addProfile',
    'aisw.removeProfile',
    'aisw.installCli',
    'aisw.locateCli',
    'aisw.clearBinaryPath',
    'aisw.importLogins',
    'aisw.bindWorkspace',
  ]) {
    assert.ok(commands.includes(command), `installed extension missing command: ${command}`);
  }

  await vscode.commands.executeCommand('aisw.refresh');
  await vscode.commands.executeCommand('workbench.action.quit');
}
