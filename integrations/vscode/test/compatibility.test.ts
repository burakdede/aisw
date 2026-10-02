import assert from 'node:assert/strict';
import test from 'node:test';

import { CliAdapter } from '../src/cliAdapter';

const binaryPath = process.env.AISW_CLI_PATH;

test('real CLI satisfies the editor handshake and read contract', { skip: !binaryPath }, async () => {
  const adapter = new CliAdapter({ binaryPath, cwd: process.cwd() });
  const diagnosis = await adapter.diagnose();
  assert.equal(diagnosis.version.cli_api_version, 1);
  assert.equal(diagnosis.version.json_schema_version, 1);
  assert.equal(diagnosis.capabilities.features.mutation_json, true);
  assert.equal(diagnosis.capabilities.features.verify, true);
  assert.equal(diagnosis.capabilities.features.contexts, true);
  const profiles = await adapter.profiles();
  assert.equal(typeof profiles, 'object');
  const contexts = await adapter.contexts();
  assert.ok(Array.isArray(contexts.contexts));
  const status = await adapter.status();
  assert.ok(Array.isArray(status.tools));
  assert.equal(typeof status.context.status, 'string');
});
