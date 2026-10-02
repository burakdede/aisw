import assert from 'node:assert/strict';
import { chmod, mkdtemp, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { CliAdapter, CliError } from '../src/cliAdapter';

test('adapter executes a fake CLI and preserves structured failures', { skip: process.platform === 'win32' }, async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), 'aisw-vscode-'));
  const binary = path.join(directory, 'aisw');
  await writeFile(binary, `#!/bin/sh
case "$2 $3" in
  "version --json") printf '%s\\n' '{"version":"0.3.10","cli_api_version":1,"json_schema_version":1,"progress_schema_version":1}' ;;
  "capabilities --json") printf '%s\\n' '{"version":"0.3.10","cli_api_version":1,"json_schema_version":1,"progress_schema_version":1,"features":{"mutation_json":true,"verify":true,"contexts":true}}' ;;
  "list --json") printf '%s\\n' '{"claude":{"active":null,"profiles":[]}}' ;;
  *) printf '%s\\n' '{"ok":false,"command":"fake","error":{"kind":"profile_not_found","message":"not found"}}' ; exit 1 ;;
esac
`);
  await chmod(binary, 0o755);
  const adapter = new CliAdapter({ binaryPath: binary, cwd: directory });
  assert.equal((await adapter.version()).cli_api_version, 1);
  assert.equal((await adapter.profiles()).claude.profiles.length, 0);
  await assert.rejects(() => adapter.useProfile('claude', 'missing'), (error: unknown) => {
    assert.ok(error instanceof CliError);
    assert.equal(error.kind, 'profile_not_found');
    return true;
  });
});
