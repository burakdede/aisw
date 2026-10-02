import * as assert from 'node:assert/strict';

import * as vscode from 'vscode';

export async function run(): Promise<void> {
  const extension = vscode.extensions.getExtension('aisw-vscode');
  assert.ok(extension, 'AISW extension should be discoverable in the Extension Host');
  await extension.activate();

  const commands = await vscode.commands.getCommands(true);
  for (const command of [
    'aisw.refresh',
    'aisw.switchContext',
    'aisw.switchProfile',
    'aisw.verify',
    'aisw.diagnose',
    'aisw.addProfile',
    'aisw.removeProfile',
  ]) {
    assert.ok(commands.includes(command), `missing registered command: ${command}`);
  }
}
