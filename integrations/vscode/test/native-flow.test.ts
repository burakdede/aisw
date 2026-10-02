import assert from 'node:assert/strict';
import test from 'node:test';

import { addProfileCommand, importLoginsCommand, workspaceBindCommand } from '../src/nativeFlow';

test('profile flow passes only the validated profile identifier', () => {
  assert.deepEqual(addProfileCommand('/tmp/aisw', 'claude', 'client-work'), {
    executable: '/tmp/aisw',
    args: ['add', 'claude', 'client-work'],
  });
});

test('import flow disables shell-hook changes', () => {
  assert.deepEqual(importLoginsCommand('/tmp/aisw'), {
    executable: '/tmp/aisw',
    args: ['init', '--no-shell-hook'],
  });
});

test('workspace binding is scoped to the selected repository and context', () => {
  assert.deepEqual(workspaceBindCommand('/tmp/aisw', '/workspace/repo', 'client-work'), {
    executable: '/tmp/aisw',
    args: ['workspace', 'bind', '/workspace/repo', '--context', 'client-work'],
  });
});
