import assert from 'node:assert/strict';
import test from 'node:test';

import { deriveViewState } from '../src/state';

const profiles = {
  claude: { active: 'work', profiles: [{ name: 'work' }] },
  codex: { active: null, profiles: [] },
};

test('derives an exact context state', () => {
  const view = deriveViewState({
    status: {
      tools: [
        { tool: 'claude', active_profile: 'work' },
        { tool: 'codex', active_profile: null },
      ],
      context: { status: 'exact', active: 'client-acme', profiles: { claude: 'work' } },
    },
    profiles,
    workspace: { status: 'match', expected_context: 'client-acme', matched_rule: 'repo' },
  });
  assert.equal(view.text, '$(account) client-acme');
  assert.equal(view.command, 'aisw.switchContext');
  assert.equal(view.workspaceMismatch, false);
});

test('derives a workspace mismatch with warning styling', () => {
  const view = deriveViewState({
    status: {
      tools: [{ tool: 'claude', active_profile: 'work' }],
      context: { status: 'exact', active: 'personal', profiles: { claude: 'work' } },
    },
    profiles,
    workspace: { status: 'mismatch', expected_context: 'client-acme' },
  });
  assert.equal(view.text, '$(warning) personal ≠ client-acme');
  assert.equal(view.background, 'warning');
  assert.equal(view.command, 'aisw.switchToWorkspaceContext');
});

test('derives setup and missing-cli states', () => {
  assert.equal(deriveViewState({ profiles: {} }).command, 'aisw.getStarted');
  assert.equal(
    deriveViewState({ error: { kind: 'cli_not_found', message: 'Install aisw.' } }).text,
    '$(cloud-download) Install aisw',
  );
});

test('summarizes active profiles when no context matches', () => {
  const view = deriveViewState({
    status: {
      tools: [
        { tool: 'claude', active_profile: 'work' },
        { tool: 'codex', active_profile: 'personal' },
      ],
      context: { status: 'none', active: null, profiles: null },
    },
    profiles,
  });
  assert.equal(view.text, '$(account) Claude Code:work +1');
  assert.equal(view.command, 'aisw.switchProfile');
});

test('shows no active profile when profiles exist but none is active', () => {
  const view = deriveViewState({
    status: {
      tools: [{ tool: 'claude', active_profile: null }],
      context: { status: 'none', active: null, profiles: null },
    },
    profiles: { claude: { active: null, profiles: [{ name: 'work' }] } },
  });
  assert.equal(view.text, '$(account) No active profile');
  assert.equal(view.command, 'aisw.switchProfile');
});
