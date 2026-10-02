import assert from 'node:assert/strict';
import test from 'node:test';

import { assertCompatible, assertFeatures } from '../src/contract';

const payload = { version: '0.0.0', cli_api_version: 1, json_schema_version: 1, progress_schema_version: 1 };

test('accepts the supported contract versions and features', () => {
  assert.doesNotThrow(() => assertCompatible(payload));
  assert.doesNotThrow(() => assertFeatures({ ...payload, features: { mutation_json: true, verify: true, contexts: true } }));
});

test('rejects incompatible versions', () => {
  assert.throws(() => assertCompatible({ ...payload, cli_api_version: 2 }), /Unsupported aisw CLI API version/);
  assert.throws(() => assertCompatible({ ...payload, json_schema_version: 2 }), /Unsupported aisw JSON schema version/);
});

test('rejects missing required features', () => {
  assert.throws(() => assertFeatures({ ...payload, features: {} }), /missing required features/);
});
