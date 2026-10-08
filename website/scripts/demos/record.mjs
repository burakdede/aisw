// Records real aisw sessions for the README demos in a hermetic temp home.
//
// It runs the debug build because only debug builds honor the test-only switches that keep
// a recording away from the macOS Keychain, the system keyring and the real ~/.aisw:
// AISW_CLAUDE_AUTH_STORAGE, AISW_KEYRING_TEST_DIR and AISW_SECURITY_BIN.
import { execFileSync, spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const repoRoot = path.resolve(import.meta.dirname, '..', '..', '..');
const aiswDebug = path.join(repoRoot, 'target', 'debug');

const toolVersions = {
  claude: '2.1.266 (Claude Code)',
  codex: 'codex-cli 0.153.4',
  gemini: '0.59.0',
  agy: '1.1.28',
};

function fakeJwt(claims) {
  const part = (value) => Buffer.from(JSON.stringify(value)).toString('base64url');
  return `${part({ alg: 'none', typ: 'JWT' })}.${part(claims)}.sig`;
}

export function createSandbox() {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'aisw-demo-')));
  const home = path.join(root, 'home');
  const bin = path.join(root, 'bin');
  fs.mkdirSync(home, { recursive: true });
  fs.mkdirSync(bin, { recursive: true });
  fs.mkdirSync(path.join(root, 'keyring'), { recursive: true });

  for (const [tool, version] of Object.entries(toolVersions)) {
    fs.writeFileSync(
      path.join(bin, tool),
      `#!/bin/sh\nif [ "$1" = "--version" ]; then echo "${version}"; fi\n`,
      { mode: 0o755 },
    );
  }
  // The real `security` CLI would touch the login Keychain; this stub reports "not found".
  fs.writeFileSync(path.join(bin, 'security'), '#!/bin/sh\nexit 44\n', { mode: 0o755 });

  const env = {
    PATH: `${aiswDebug}:${bin}:/usr/bin:/bin`,
    HOME: home,
    AISW_HOME: path.join(home, '.aisw'),
    AISW_CLAUDE_AUTH_STORAGE: 'file',
    AISW_KEYRING_TEST_DIR: path.join(root, 'keyring'),
    AISW_SECURITY_BIN: path.join(bin, 'security'),
    SHELL: '/bin/bash',
    TERM: 'xterm-256color',
    CLICOLOR_FORCE: '1',
    COLUMNS: '92',
    LANG: 'en_US.UTF-8',
  };
  if (!env.AISW_HOME.startsWith(root)) throw new Error('sandbox AISW_HOME escaped the temp root');

  const write = (relative, contents) => {
    const file = path.join(home, relative);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, contents, { mode: 0o600 });
  };

  return {
    root,
    home,
    env,
    write,
    liveClaudeLogin(email) {
      write(
        '.claude/.credentials.json',
        JSON.stringify({
          claudeAiOauth: {
            accessToken: `sk-ant-oat01-${email}`,
            refreshToken: `sk-ant-ort01-${email}`,
            expiresAt: Date.now() + 8 * 3600_000,
            scopes: ['user:inference', 'user:profile'],
            subscriptionType: 'max',
          },
        }),
      );
      write('.claude.json', JSON.stringify({ oauthAccount: { emailAddress: email, organizationUuid: `org-${email}` } }));
    },
    liveCodexLogin(email) {
      write(
        '.codex/auth.json',
        JSON.stringify({
          auth_mode: 'chatgpt',
          OPENAI_API_KEY: null,
          tokens: {
            id_token: fakeJwt({ email, exp: Math.floor(Date.now() / 1000) + 864000 }),
            access_token: fakeJwt({ email, exp: Math.floor(Date.now() / 1000) + 864000 }),
            refresh_token: `rt-${email}`,
            account_id: `acct-${email}`,
          },
          last_refresh: new Date().toISOString(),
        }),
      );
      write('.codex/config.toml', 'cli_auth_credentials_store = "file"\n');
    },
    git(dir, remote) {
      const target = path.join(home, dir);
      fs.mkdirSync(target, { recursive: true });
      execFileSync('git', ['init', '-q'], { cwd: target });
      if (remote) execFileSync('git', ['remote', 'add', 'origin', remote], { cwd: target });
    },
    cleanup() {
      fs.rmSync(root, { recursive: true, force: true });
    },
  };
}

// Runs a whole demo in one bash session with the shell hook loaded, as a user's terminal would
// be, so exports from `aisw use` and `cd` carry over between steps. Hidden steps set up state.
export function recordSession(sandbox, steps) {
  const marker = (index) => `\u001e${index}\u001e`;
  const body = steps
    .map((step, index) => {
      const quiet = step.hidden ? ' >/dev/null 2>&1' : ' 2>&1';
      return `printf '${marker(index)}\\n'\n${step.command}${quiet}`;
    })
    .join('\n');
  const script = `eval "$(aisw shell-hook bash)"\ncd "$HOME"\n${body}\nprintf '${marker(steps.length)}\\n'\n`;
  const result = spawnSync('/bin/bash', ['--noprofile', '--norc', '-c', script], {
    env: sandbox.env,
    encoding: 'utf8',
  });
  if (result.error) throw result.error;
  const raw = result.stdout
    .replaceAll(sandbox.home, '~')
    .replaceAll(sandbox.root, '/tmp/demo');
  return steps.map((step, index) => {
    const from = raw.indexOf(marker(index));
    const to = raw.indexOf(marker(index + 1));
    if (from < 0 || to < 0) throw new Error(`demo step did not finish: ${step.command}\n${raw}`);
    return { ...step, output: raw.slice(from + marker(index).length + 1, to).replace(/\n+$/, '') };
  });
}
