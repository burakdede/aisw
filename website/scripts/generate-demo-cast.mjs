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
  'aisw-important-workflows.cast'
);

const prompt = 'demo@aisw:~$ ';
const ansi = {
  reset: '\u001b[0m',
  dim: '\u001b[2m',
  prompt: '\u001b[1;38;5;114m',
  command: '\u001b[1;38;5;81m',
  body: '\u001b[38;5;252m',
  muted: '\u001b[38;5;245m',
  heading: '\u001b[1;38;5;223m',
  note: '\u001b[38;5;186m',
};

const header = {
  version: 2,
  width: 108,
  height: 32,
  timestamp: 1774526400,
  title: 'aisw important workflows',
  env: {
    SHELL: '/bin/bash',
    TERM: 'xterm-256color',
  },
};

const steps = [
  {
    marker: 'Init',
    title: '1/10 First-run setup',
    detail:
      'Initialize aisw, install shell integration, detect supported tools, and import an existing Codex login.',
    command: 'aisw init --yes',
  },
  {
    marker: 'Add work',
    title: '2/10 Add a work profile',
    detail:
      'Save a Claude Code API key for your work account without switching away from the imported default state yet.',
    command:
      'aisw add claude work --api-key sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA --label "Work account"',
  },
  {
    marker: 'Add personal',
    title: '3/10 Add a personal fallback',
    detail:
      'Keep a second Claude profile ready for quota rotation or personal usage on the same machine.',
    command:
      'aisw add claude personal --api-key sk-ant-api03-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB --label "Personal account"',
  },
  {
    marker: 'Switch',
    title: '4/10 Switch the live account',
    detail:
      'Activate the personal Claude profile. aisw updates the live credentials and snapshots the previous state.',
    command: 'aisw use claude personal',
  },
  {
    marker: 'Status',
    title: '5/10 Confirm the active state',
    detail:
      'Check which profile is active for each tool and whether the current live credentials are present locally.',
    command: 'aisw status',
  },
  {
    marker: 'Rename',
    title: '6/10 Rename a stored profile',
    detail:
      'Rename a generic profile to something more specific after the account purpose becomes clear.',
    command: 'aisw rename claude work client-acme',
  },
  {
    marker: 'List',
    title: '7/10 Review every stored profile',
    detail:
      'List all saved profiles grouped by tool so it is obvious which accounts exist and which one is active.',
    command: 'aisw list',
  },
  {
    marker: 'Remove',
    title: '8/10 Clean up an unused profile',
    detail:
      'Remove a stored profile you no longer need. aisw keeps a backup before deleting the profile directory.',
    command: 'aisw remove claude client-acme --yes',
  },
  {
    marker: 'Backups',
    title: '9/10 Inspect restore points',
    detail:
      'Review backup history after switching and deletion so you can recover a removed profile if needed.',
    command: 'aisw backup list',
    after({ output, state }) {
      state.clientBackupId = parseBackupId(output, 'claude', 'client-acme');
      if (!state.clientBackupId) {
        throw new Error('Could not find backup id for removed client-acme profile');
      }
    },
  },
  {
    marker: 'Restore',
    title: '10/10 Restore a removed profile',
    detail:
      'Recover the removed client profile from backup instead of re-entering credentials or repeating setup.',
    command({ state }) {
      if (!state.clientBackupId) {
        throw new Error('Backup id missing before restore step');
      }
      return `aisw backup restore ${state.clientBackupId} --yes`;
    },
  },
];

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

function printIntro() {
  const lines = [
    `${ansi.heading}aisw demo${ansi.reset}`,
    `${ansi.body}Manage and switch Claude Code, Codex CLI, and Gemini CLI accounts from one local profile store.${ansi.reset}`,
    `${ansi.muted}This walkthrough uses real command output captured from an isolated demo environment.${ansi.reset}`,
    '',
  ];
  pushOutput(`${lines.join('\r\n')}\r\n`);
  pause(2.4);
}

function printStepBanner(title, detail) {
  const lines = [
    `${ansi.note}${title}${ansi.reset}`,
    `${ansi.dim}${detail}${ansi.reset}`,
    '',
  ];
  pushOutput(`${lines.join('\r\n')}\r\n`);
  pause(2.1);
}

function typeCommand(command) {
  pushOutput(`${ansi.prompt}${prompt}${ansi.reset}`);
  for (const ch of command) {
    pause(0.076);
    pushOutput(`${ansi.command}${ch}${ansi.reset}`);
  }
  pause(0.32);
  pushOutput('\r\n');
}

function printCapturedOutput(output) {
  pause(0.8);
  pushOutput(output);
}

function printOutro() {
  pause(2.0);
  const lines = [
    '',
    `${ansi.heading}Done${ansi.reset}`,
    `${ansi.body}aisw keeps named profiles separate from the live tool config, switches with one command, and preserves backups for safe recovery.${ansi.reset}`,
    '',
  ];
  pushOutput(`${lines.join('\r\n')}\r\n`);
}

function sanitizeOutput(text, tempRoot) {
  return text
    .replaceAll(`${tempRoot}/home`, '/tmp/aisw-demo/home')
    .replaceAll(tempRoot, '/tmp/aisw-demo')
    .replaceAll(/\r\n/g, '\n')
    .replaceAll(/\u001b\[6n/g, '')
    .trimEnd()
    .split('\n')
    .join('\r\n');
}

function captureCommandOutput(command, env) {
  return execFileSync('script', ['-qec', command, '/dev/null'], {
    cwd: process.cwd(),
    env,
    encoding: 'utf8',
  });
}

function stripAnsi(value) {
  return value.replace(/\u001b\[[0-9;?]*[ -/]*[@-~]/g, '');
}

function parseBackupId(output, tool, profile) {
  const cleaned = stripAnsi(output);
  const lines = cleaned.split('\n').map((line) => line.trim());
  for (const line of lines) {
    const parts = line.split(/\s+/);
    if (parts.length >= 3 && parts[1] === tool && parts[2] === profile) {
      return parts[0];
    }
  }
  return null;
}

async function setupDemoEnv() {
  const tempRoot = await fs.mkdtemp(path.join(os.tmpdir(), 'aisw-demo-'));
  const fakeHome = path.join(tempRoot, 'home');
  const aiswHome = path.join(fakeHome, '.aisw');
  const binDir = path.join(tempRoot, 'bin');
  const aiswBinDir = path.join(process.cwd(), 'target', 'debug');
  const codexDir = path.join(fakeHome, '.codex');

  await fs.mkdir(binDir, { recursive: true });
  await fs.mkdir(codexDir, { recursive: true });

  await fs.writeFile(
    path.join(binDir, 'claude'),
    '#!/usr/bin/env sh\nif [ "$1" = "--version" ]; then\n  echo "claude 1.0.0"\nelse\n  echo "claude mock"\nfi\n',
    { mode: 0o755 }
  );
  await fs.writeFile(
    path.join(binDir, 'codex'),
    '#!/usr/bin/env sh\nif [ "$1" = "--version" ]; then\n  echo "codex 1.0.0"\nelse\n  echo "codex mock"\nfi\n',
    { mode: 0o755 }
  );

  await fs.writeFile(
    path.join(codexDir, 'auth.json'),
    '{"provider":"chatgpt","access_token":"test","refresh_token":"test"}\n'
  );

  const env = { ...process.env };
  delete env.NO_COLOR;

  return {
    tempRoot,
    env: {
      ...env,
      PATH: `${aiswBinDir}:${binDir}:/usr/bin:/bin`,
      HOME: fakeHome,
      AISW_HOME: aiswHome,
      SHELL: '/bin/bash',
      TERM: 'xterm-256color',
      CLICOLOR_FORCE: '1',
    },
  };
}

async function main() {
  await fs.mkdir(path.dirname(outputPath), { recursive: true });
  const demoEnv = await setupDemoEnv();
  const state = {};

  printIntro();

  for (const step of steps) {
    const command = typeof step.command === 'function' ? step.command({ state }) : step.command;
    markers.push([Number(t.toFixed(1)), step.marker]);
    printStepBanner(step.title, step.detail);
    typeCommand(command);
    const rawOutput = captureCommandOutput(command, demoEnv.env);
    const sanitizedOutput = sanitizeOutput(rawOutput, demoEnv.tempRoot);
    printCapturedOutput(`${sanitizedOutput}\r\n`);
    if (step.after) {
      step.after({ output: sanitizedOutput, state });
    }
    pause(2.8);
  }

  printOutro();
  pushOutput(`${ansi.prompt}${prompt}${ansi.reset}`);

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
