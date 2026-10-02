import assert from 'node:assert/strict';
import { chmod, mkdtemp, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { BinaryPathError, isExecutable, resolveBinary } from '../src/binaryResolver';

test('configured binary paths are validated without falling back', async () => {
  const directory = await mkdtemp(path.join(os.tmpdir(), 'aisw-binary-'));
  const binary = path.join(directory, 'aisw');
  await writeFile(binary, '#!/bin/sh\nexit 0\n');
  await chmod(binary, 0o755);
  assert.equal(resolveBinary(binary), binary);
  assert.equal(isExecutable(binary), true);
  assert.throws(() => resolveBinary(path.join(directory, 'missing')), (error: unknown) => error instanceof BinaryPathError);
});
