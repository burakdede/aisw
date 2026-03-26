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
  section: '\u001b[1;38;5;153m',
  success: '\u001b[1;38;5;151m',
  warning: '\u001b[1;38;5;215m',
  note: '\u001b[38;5;186m',
  accent: '\u001b[1;38;5;111m',
  table: '\u001b[38;5;153m',
};

const header = {
  version: 2,
  width: 100,
  height: 30,
  timestamp: 1774526400,
  title: 'aisw important workflows',
  env: {
    SHELL: '/bin/bash',
    TERM: 'xterm-256color',
  },
};

const steps = [
  {
    title: '1/7 First-run setup',
    detail: 'Initialize aisw and import an existing Codex login already present on the machine.',
    command: 'aisw init --yes',
  },
  {
    title: '2/7 Add a work profile',
    detail: 'Store a dedicated Claude Code account for work without activating it yet.',
    command:
      'aisw add claude work --api-key sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA --label "Work quota"',
  },
  {
    title: '3/7 Add a fallback profile',
    detail: 'Keep a second Claude profile ready for quota or account rotation.',
    command:
      'aisw add claude personal --api-key sk-ant-api03-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB --label "Personal quota"',
  },
  {
    title: '4/7 Switch accounts',
    detail: 'Change the live Claude Code credentials in one command and keep a backup automatically.',
    command: 'aisw use claude personal',
  },
  {
    title: '5/7 Verify current state',
    detail: 'Check the active profile and local credential state for every supported tool.',
    command: 'aisw status',
  },
  {
    title: '6/7 Review stored profiles',
    detail: 'See all saved accounts grouped by tool, with active profiles clearly marked.',
    command: 'aisw list',
  },
  {
    title: '7/7 Inspect backups',
    detail: 'Every switch can leave behind a restore point you can list and recover later.',
    command: 'aisw backup list',
  },
];

const events = [];
let t = 0;

function pushOutput(text) {
  if (!text) return;
  events.push([Number(t.toFixed(3)), 'o', text]);
}

function pause(seconds) {
  t += seconds;
}

function lineColor(line) {
  if (!line) {
    return ansi.body;
  }
  if (
    line === 'Initialize aisw' ||
    line === 'Setup complete' ||
    line === 'Added profile' ||
    line === 'Switched profile' ||
    line === 'Status' ||
    line === 'Profiles' ||
    line === 'Backups'
  ) {
    return ansi.success;
  }
  if (line === 'Shell integration' || line === 'Import existing credentials' || line === 'Effects' || line === 'Next') {
    return ansi.section;
  }
  if (line.startsWith('Could not verify')) {
    return ansi.warning;
  }
  if (line === 'Claude Code' || line === 'Codex CLI' || line === 'Gemini CLI') {
    return ansi.heading;
  }
  if (line.startsWith('BACKUP ID')) {
    return ansi.table;
  }
  if (line.includes('[active]')) {
    return ansi.accent;
  }
  if (line.startsWith('  Run ')) {
    return ansi.note;
  }
  if ([...line].every((ch) => ch === '─')) {
    return ansi.muted;
  }
  if (line.startsWith('  Active') || line.startsWith('  Auth') || line.startsWith('  State') || line.startsWith('  Label') || line.startsWith('  Tool') || line.startsWith('  Profile') || line.startsWith('  Activation') || line.startsWith('  Home')) {
    return ansi.body;
  }
  return ansi.body;
}

function colorizeLine(line) {
  if (line === '') {
    return '';
  }
  return `${lineColor(line)}${line}${ansi.reset}`;
}

function printIntro() {
  const lines = [
    `${ansi.heading}aisw demo${ansi.reset}`,
    `${ansi.body}Switch between Claude Code, Codex CLI, and Gemini CLI accounts${ansi.reset}`,
    `${ansi.muted}This walkthrough shows setup, profile storage, switching, status, and backup inspection.${ansi.reset}`,
    '',
  ];
  pushOutput(`${lines.join('\r\n')}\r\n`);
  pause(2.1);
}

function printStepBanner(title, detail) {
  const lines = [
    `${ansi.note}${title}${ansi.reset}`,
    `${ansi.dim}${detail}${ansi.reset}`,
    '',
  ];
  pushOutput(`${lines.join('\r\n')}\r\n`);
  pause(1.9);
}

function typeCommand(command) {
  pushOutput(`${ansi.prompt}${prompt}${ansi.reset}`);
  for (const ch of command) {
    pause(0.078);
    pushOutput(`${ansi.command}${ch}${ansi.reset}`);
  }
  pause(0.28);
  pushOutput('\r\n');
}

function printBlock(lines) {
  pause(0.62);
  pushOutput(`${lines.map(colorizeLine).join('\r\n')}\r\n`);
}

function printOutro() {
  pause(1.8);
  const lines = [
    '',
    `${ansi.heading}Done${ansi.reset}`,
    `${ansi.body}aisw stores named profiles, updates live tool credentials on ${ansi.command}use${ansi.reset}${ansi.body}, and keeps backups available for restore.${ansi.reset}`,
    '',
  ];
  pushOutput(`${lines.join('\r\n')}\r\n`);
}

function sanitizeOutput(text, tempRoot) {
  return text
    .replaceAll(tempRoot, '/tmp/aisw-demo')
    .replaceAll(`${tempRoot}/home`, '/tmp/aisw-demo/home')
    .replaceAll(/\r\n/g, '\n')
    .trimEnd();
}

function captureCommandOutput(command, env) {
  const output = execFileSync(
    'script',
    ['-qec', command, '/dev/null'],
    {
      cwd: process.cwd(),
      env,
      encoding: 'utf8',
    }
  );

  return output;
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
    '#!/usr/bin/env sh\necho "claude 1.0.0"\n',
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

  return {
    tempRoot,
    env: {
      ...process.env,
      PATH: `${aiswBinDir}:${binDir}:${process.env.PATH ?? ''}`,
      HOME: fakeHome,
      AISW_HOME: aiswHome,
      SHELL: '/bin/bash',
      TERM: 'xterm-256color',
    },
  };
}

async function main() {
  await fs.mkdir(path.dirname(outputPath), { recursive: true });
  const demoEnv = await setupDemoEnv();
  printIntro();

  for (const step of steps) {
    const rawOutput = captureCommandOutput(step.command, demoEnv.env);
    const lines = sanitizeOutput(rawOutput, demoEnv.tempRoot).split('\n');
    printStepBanner(step.title, step.detail);
    typeCommand(step.command);
    printBlock(lines);
    pause(2.35);
  }

  printOutro();
  pushOutput(`${ansi.prompt}${prompt}${ansi.reset}`);

  const contents = [JSON.stringify(header), ...events.map((event) => JSON.stringify(event))].join('\n');
  await fs.writeFile(outputPath, contents);
  console.log(`Wrote ${outputPath}`);
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
