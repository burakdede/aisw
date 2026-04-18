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
const shellPath = '~/workspace/aisw';
const ansi = {
  reset: '\u001b[0m',
  dim: '\u001b[2m',
  prompt: '\u001b[1;38;5;213m',
  command: '\u001b[1;38;5;226m',
  body: '\u001b[38;5;255m',
  muted: '\u001b[38;5;244m',
  heading: '\u001b[1;38;5;51m',
  note: '\u001b[1;38;5;208m',
  accent: '\u001b[1;38;5;121m',
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
const commandViewportLines = header.height - 10;

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

function printStepBanner(title, detail) {
  const border = `${ansi.accent}┌────────────────────────────────────────────────────────────────────────────────────────────────────────┐${ansi.reset}`;
  const divider = `${ansi.accent}├────────────────────────────────────────────────────────────────────────────────────────────────────────┤${ansi.reset}`;
  const footer = `${ansi.accent}└────────────────────────────────────────────────────────────────────────────────────────────────────────┘${ansi.reset}`;
  const lines = [
    border,
    `${ansi.accent}│${ansi.reset} ${ansi.note}${title}${ansi.reset}`,
    divider,
    `${ansi.accent}│${ansi.reset} ${ansi.dim}Use case:${ansi.reset} ${ansi.body}${detail}${ansi.reset}`,
    `${ansi.accent}│${ansi.reset} ${ansi.dim}Workspace:${ansi.reset} ${ansi.muted}${shellPath}${ansi.reset}`,
    footer,
    '',
  ];
  pushOutput(`${lines.join('\r\n')}\r\n`);
  pause(2.4);
}

function typeCommand(command) {
  pushOutput(`${ansi.prompt}${prompt}${ansi.reset}`);
  for (const ch of command) {
    pause(0.07);
    pushOutput(`${ansi.command}${ch}${ansi.reset}`);
  }
  pause(0.5);
  pushOutput('\r\n');
}

function printCapturedOutput(output) {
  pause(0.55);
  pushOutput(`${fitOutputToViewport(output)}\r\n`);
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

function fitOutputToViewport(output) {
  const lines = output.split('\r\n');
  if (lines.length <= commandViewportLines) {
    return output;
  }
  return lines.slice(0, commandViewportLines).join('\r\n');
}

function clearScreen() {
  // Clear screen + move cursor to top-left so each workflow starts from a clean terminal.
  pushOutput('\u001b[2J\u001b[H');
}

function transitionToNextFeature() {
  // Keep transition minimal: pause for readability, then clear.
  pause(1.6);
  clearScreen();
  pause(0.4);
}

function captureCommandOutput(command, env) {
  // Keep capture portable across macOS/Linux and CI environments.
  // CLICOLOR_FORCE in env preserves colored aisw output for the cast.
  return execFileSync('bash', ['-lc', command], {
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

  clearScreen();
  pause(0.4);

  for (let index = 0; index < steps.length; index += 1) {
    const step = steps[index];
    const nextStep = steps[index + 1];
    const command = typeof step.command === 'function' ? step.command({ state }) : step.command;
    markers.push([Number(t.toFixed(1)), step.marker]);
    printStepBanner(step.title, step.detail);
    typeCommand(command);
    const rawOutput = captureCommandOutput(command, demoEnv.env);
    const sanitizedOutput = sanitizeOutput(rawOutput, demoEnv.tempRoot);
    printCapturedOutput(sanitizedOutput);
    if (step.after) {
      step.after({ output: sanitizedOutput, state });
    }
    pause(3.2);
    if (nextStep) {
      transitionToNextFeature();
    }
  }

  clearScreen();
  pause(0.3);

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
