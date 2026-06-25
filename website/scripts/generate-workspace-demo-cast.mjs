import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import process from 'node:process';
import { execFileSync } from 'node:child_process';

const outputPath = path.join(
  process.cwd(),
  'website',
  'public',
  'demos',
  'aisw-workspace-workflow.cast'
);

const prompt = 'demo@aisw:~$ ';
const ansi = {
  reset: '[0m',
  dim: '[2m',
  prompt: '[1;38;5;213m',
  command: '[1;38;5;226m',
  body: '[38;5;255m',
  muted: '[38;5;244m',
  note: '[1;38;5;208m',
  accent: '[1;38;5;121m',
};

const header = {
  version: 2,
  width: 92,
  height: 26,
  timestamp: 1774526400,
  title: 'aisw workspace guardrails',
  env: {
    SHELL: '/bin/bash',
    TERM: 'xterm-256color',
  },
};

const commandViewportLines = header.height - 8;

const events = [];
const markers = [];
let t = 0;

function pushOutput(text) {
  if (!text) return;
  events.push([Number(t.toFixed(3)), 'o', text]);
}

function pause(seconds) {
  t += seconds;
}

function printStepBanner(stepLabel, title, detail, workdir) {
  const width = 90;
  const bar = '─'.repeat(width);
  const border = `${ansi.accent}┌${bar}┐${ansi.reset}`;
  const divider = `${ansi.accent}├${bar}┤${ansi.reset}`;
  const footer = `${ansi.accent}└${bar}┘${ansi.reset}`;
  const dir = workdir || '~/workspace/aisw';
  const lines = [
    border,
    `${ansi.accent}│${ansi.reset} ${ansi.note}${stepLabel}: ${title}${ansi.reset}`,
    divider,
    `${ansi.accent}│${ansi.reset} ${ansi.dim}Why:${ansi.reset} ${ansi.body}${detail}${ansi.reset}`,
    `${ansi.accent}│${ansi.reset} ${ansi.dim}Dir:${ansi.reset} ${ansi.muted}${dir}${ansi.reset}`,
    footer,
    '',
  ];
  pushOutput(`${lines.join('\r\n')}\r\n`);
  pause(2.1);
}

function typeCommand(command) {
  pushOutput(`${ansi.prompt}${prompt}${ansi.reset}`);
  for (const ch of command) {
    pause(0.06);
    pushOutput(`${ansi.command}${ch}${ansi.reset}`);
  }
  pause(0.45);
  pushOutput('\r\n');
}

function printCapturedOutput(output) {
  pause(0.45);
  pushOutput(`${fitOutputToViewport(output)}\r\n`);
}

function sanitizeOutput(text, tempRoot, repoRoot) {
  return text
    .replaceAll(repoRoot, '~/clients/acme-api')
    .replaceAll(`${tempRoot}/home`, '~')
    .replaceAll(tempRoot, '/tmp/aisw-demo')
    .replaceAll(/\r\n/g, '\n')
    .replaceAll(/\[6n/g, '')
    .trimEnd()
    .split('\n')
    .join('\r\n');
}

function fitOutputToViewport(output) {
  const lines = output.split('\r\n');
  if (lines.length <= commandViewportLines) {
    return output;
  }
  return lines.slice(0, commandViewportLines).join('\r\n');
}

function clearScreen() {
  pushOutput('[2J[H');
}

function transitionToNextFeature() {
  pause(1.4);
  clearScreen();
  pause(0.35);
}

function captureCommandOutput(command, env, cwd) {
  return execFileSync('bash', ['-lc', command], {
    cwd: cwd || process.cwd(),
    env,
    encoding: 'utf8',
  });
}

async function setupDemoEnv() {
  const tempRoot = await fs.mkdtemp(path.join(os.tmpdir(), 'aisw-workspace-demo-'));
  const fakeHome = path.join(tempRoot, 'home');
  const aiswHome = path.join(fakeHome, '.aisw');
  const binDir = path.join(tempRoot, 'bin');
  const aiswBinDir = path.join(process.cwd(), 'target', 'debug');

  // Fake client repo with a git remote
  const repoRoot = path.join(fakeHome, 'clients', 'acme-api');
  const gitDir = path.join(repoRoot, '.git');
  const gitInfoDir = path.join(gitDir, 'info');

  await fs.mkdir(binDir, { recursive: true });
  await fs.mkdir(gitInfoDir, { recursive: true });

  // Write a minimal git config with an origin remote
  await fs.writeFile(
    path.join(gitDir, 'config'),
    '[core]\n\trepositoryformatversion = 0\n[remote "origin"]\n\turl = git@github.com:acme-corp/api.git\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n'
  );

  // Fake agent binaries
  for (const name of ['claude', 'codex', 'gemini']) {
    await fs.writeFile(
      path.join(binDir, name),
      `#!/usr/bin/env sh\nif [ "$1" = "--version" ]; then\n  echo "${name} 1.0.0"\nelse\n  echo "${name} mock launched"\nfi\n`,
      { mode: 0o755 }
    );
  }

  const env = {
    ...process.env,
    PATH: `${aiswBinDir}:${binDir}:/usr/bin:/bin`,
    HOME: fakeHome,
    AISW_HOME: aiswHome,
    SHELL: '/bin/bash',
    TERM: 'xterm-256color',
    CLICOLOR_FORCE: '1',
  };
  delete env.NO_COLOR;

  // Seed profiles and contexts
  const seed = [
    'aisw add claude acme-claude --api-key sk-ant-api03-ACMEACMEACMEACMEACMEACMEACME001 --label "Acme Claude account"',
    'aisw add codex acme-codex --api-key sk-proj-acme-OPENAI-000000000000000000001 --label "Acme Codex account"',
    'aisw add gemini acme-gemini --api-key gemini-acme-000000000000000001 --label "Acme Gemini account"',
    'aisw add claude personal --api-key sk-ant-api03-PERSONALPERSONALPERSONALPERSONAL01 --label "Personal Claude"',
    'aisw add codex personal --api-key sk-proj-personal-OPENAI-000000000000000001 --label "Personal Codex"',
    'aisw add gemini personal --api-key gemini-personal-000000000000001 --label "Personal Gemini"',
    'aisw context create acme --claude acme-claude --codex acme-codex --gemini acme-gemini',
    'aisw context create personal --claude personal --codex personal --gemini personal',
  ];

  for (const command of seed) {
    captureCommandOutput(command, env);
  }

  return { tempRoot, repoRoot, env };
}

async function main() {
  await fs.mkdir(path.dirname(outputPath), { recursive: true });
  const { tempRoot, repoRoot, env } = await setupDemoEnv();

  const sanitize = (text) => sanitizeOutput(text, tempRoot, repoRoot);

  clearScreen();
  pause(0.3);

  // Step 1: personal context active, cd into client repo
  markers.push([Number(t.toFixed(1)), 'Wrong context']);
  printStepBanner(
    '1/5',
    'Personal context is active',
    'You are set up for personal work. A client repo is waiting.',
    '~'
  );
  typeCommand('aisw context use personal');
  printCapturedOutput(sanitize(captureCommandOutput('aisw context use personal', env)));
  pause(3);
  transitionToNextFeature();

  // Step 2: bind the client repo to acme context
  markers.push([Number(t.toFixed(1)), 'Bind repo']);
  printStepBanner(
    '2/5',
    'Bind the client repo to its context',
    'Store the binding in .git/info/aisw.json  -  never committed, stays local to the repo.',
    '~/clients/acme-api'
  );
  typeCommand('aisw workspace bind . --context acme');
  printCapturedOutput(
    sanitize(captureCommandOutput('aisw workspace bind . --context acme', env, repoRoot))
  );
  pause(3);
  transitionToNextFeature();

  // Step 3: check status in the repo
  markers.push([Number(t.toFixed(1)), 'Status check']);
  printStepBanner(
    '3/5',
    'Inspect the resolved binding',
    'status shows the expected context, the active profiles, and the recommended action.',
    '~/clients/acme-api'
  );
  typeCommand('aisw workspace status');
  printCapturedOutput(
    sanitize(captureCommandOutput('aisw workspace status', env, repoRoot))
  );
  pause(3);
  transitionToNextFeature();

  // Step 4: set strict guard mode
  markers.push([Number(t.toFixed(1)), 'Strict mode']);
  printStepBanner(
    '4/5',
    'Enable strict guard mode',
    'With strict mode, launching claude, codex, or gemini is blocked when the wrong context is active.',
    '~/clients/acme-api'
  );
  typeCommand('aisw workspace guard --mode strict');
  printCapturedOutput(
    sanitize(captureCommandOutput('aisw workspace guard --mode strict', env, repoRoot))
  );
  pause(3);
  transitionToNextFeature();

  // Step 5: switch to the right context, confirm
  markers.push([Number(t.toFixed(1)), 'Fix and confirm']);
  printStepBanner(
    '5/5',
    'Switch to the right context, confirm match',
    'One command activates the client accounts. workspace status confirms everything lines up.',
    '~/clients/acme-api'
  );
  typeCommand('aisw context use acme');
  printCapturedOutput(
    sanitize(captureCommandOutput('aisw context use acme', env, repoRoot))
  );
  pause(2);
  // Show status after switch
  typeCommand('aisw workspace status');
  printCapturedOutput(
    sanitize(captureCommandOutput('aisw workspace status', env, repoRoot))
  );
  pause(3);

  clearScreen();
  pause(0.2);

  const contents = [
    JSON.stringify(header),
    ...events.map((event) => JSON.stringify(event)),
  ].join('\n');
  await fs.writeFile(outputPath, contents);
  console.log(`Wrote ${outputPath}`);
  console.log(`Markers: ${JSON.stringify(markers)}`);
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
