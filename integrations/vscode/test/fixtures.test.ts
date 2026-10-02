import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';

const fixtureDirectory = path.resolve(__dirname, '../../fixtures/v1');

async function load(name: string): Promise<unknown> {
  return JSON.parse(await readFile(path.join(fixtureDirectory, name), 'utf8')) as unknown;
}

test('version 1 fixtures cover reads, verification, and mutations', async () => {
  const status = await load('status-context.json') as { context: { status: string } };
  const contexts = await load('contexts.json') as { contexts: unknown[] };
  const workspace = await load('workspace-status.json') as { status: string };
  const pass = await load('verify-pass.json') as { summary: { status: string } };
  const fail = await load('verify-fail.json') as { summary: { status: string }; tools: unknown[] };
  const use = await load('use-success.json') as { ok: boolean; result: { affected_tools: string[] } };
  const contextUse = await load('context-use-success.json') as { ok: boolean; result: { warnings: string[] } };

  assert.equal(status.context.status, 'exact');
  assert.ok(contexts.contexts.length > 0);
  assert.equal(workspace.status, 'match');
  assert.equal(pass.summary.status, 'pass');
  assert.equal(fail.summary.status, 'fail');
  assert.ok(fail.tools.length > 0);
  assert.deepEqual(use.result.affected_tools, ['claude']);
  assert.ok(contextUse.ok);
  assert.ok(contextUse.result.warnings.length > 0);
});
