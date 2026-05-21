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
  'aisw-context-workflow.cast'
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
  note: '\u001b[1;38;5;208m',
  accent: '\u001b[1;38;5;121m',
};

const header = {
  version: 2,
  width: 92,
  height: 24,
  timestamp: 1774526400,
  title: 'aisw context workflow',
  env: {
    SHELL: '/bin/bash',
    TERM: 'xterm-256color',
  },
};

const commandViewportLines = header.height - 8;

const steps = [
  {
    marker: 'Profiles',
    title: '1/5 Start with real saved profiles',
    detail:
      'The names do not line up across tools, which is exactly where contexts become useful.',
    command: 'aisw list',
  },
  {
    marker: 'Create context',
    title: '2/5 Save the mixed client setup',
    detail:
      'Create one named context that points at the right profile for each provider.',
    command:
      'aisw context create acme --claude acme-claude --codex acme-openai --gemini acme-gemini',
  },
  {
    marker: 'Same-name switch',
    title: '3/5 Same-name switching still works',
    detail:
      'Switch everything to personal first. This is easy because the profile names match.',
    command: 'aisw use --all --profile personal',
  },
  {
    marker: 'Context switch',
    title: '4/5 Move into the client context',
    detail:
      'One command activates the full mixed-name stack as a single transactional change.',
    command: 'aisw context use acme',
  },
  {
    marker: 'Confirm',
    title: '5/5 Confirm the derived active context',
    detail:
      'status --context makes it obvious that the live state now matches the saved client mode.',
    command: 'aisw status --context',
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
  const border = `${ansi.accent}┌──────────────────────────────────────────────────────────────────────────────────────────┐${ansi.reset}`;
  const divider = `${ansi.accent}├──────────────────────────────────────────────────────────────────────────────────────────┤${ansi.reset}`;
  const footer = `${ansi.accent}└──────────────────────────────────────────────────────────────────────────────────────────┘${ansi.reset}`;
  const lines = [
    border,
    `${ansi.accent}│${ansi.reset} ${ansi.note}${title}${ansi.reset}`,
    divider,
    `${ansi.accent}│${ansi.reset} ${ansi.dim}Why:${ansi.reset} ${ansi.body}${detail}${ansi.reset}`,
    `${ansi.accent}│${ansi.reset} ${ansi.dim}Workspace:${ansi.reset} ${ansi.muted}${shellPath}${ansi.reset}`,
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
  pushOutput('\u001b[2J\u001b[H');
}

function transitionToNextFeature() {
  pause(1.4);
  clearScreen();
  pause(0.35);
}

function captureCommandOutput(command, env) {
  return execFileSync('bash', ['-lc', command], {
    cwd: process.cwd(),
    env,
    encoding: 'utf8',
  });
}

async function setupDemoEnv() {
  const tempRoot = await fs.mkdtemp(path.join(os.tmpdir(), 'aisw-context-demo-'));
  const fakeHome = path.join(tempRoot, 'home');
  const aiswHome = path.join(fakeHome, '.aisw');
  const binDir = path.join(tempRoot, 'bin');
  const aiswBinDir = path.join(process.cwd(), 'target', 'debug');

  await fs.mkdir(binDir, { recursive: true });

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
    path.join(binDir, 'gemini'),
    '#!/usr/bin/env sh\nif [ "$1" = "--version" ]; then\n  echo "gemini 1.0.0"\nelse\n  echo "gemini mock"\nfi\n',
    { mode: 0o755 }
  );

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

  const seed = [
    'aisw add claude personal --api-key sk-ant-api03-PERSONALPERSONALPERSONALPERSONAL01 --label "Personal Claude"',
    'aisw add codex personal --api-key sk-proj-personal-OPENAI-000000000000000000000001 --label "Personal Codex"',
    'aisw add gemini personal --api-key gemini-personal-000000000000000000000001 --label "Personal Gemini"',
    'aisw add claude acme-claude --api-key sk-ant-api03-ACMEACMEACMEACMEACMEACME0001 --label "Acme Claude"',
    'aisw add codex acme-openai --api-key sk-proj-acme-OPENAI-000000000000000000000001 --label "Acme Codex"',
    'aisw add gemini acme-gemini --api-key gemini-acme-000000000000000000000001 --label "Acme Gemini"',
  ];

  for (const command of seed) {
    captureCommandOutput(command, env);
  }

  return { tempRoot, env };
}

async function main() {
  await fs.mkdir(path.dirname(outputPath), { recursive: true });
  const demoEnv = await setupDemoEnv();

  clearScreen();
  pause(0.3);

  for (let index = 0; index < steps.length; index += 1) {
    const step = steps[index];
    const nextStep = steps[index + 1];
    markers.push([Number(t.toFixed(1)), step.marker]);
    printStepBanner(step.title, step.detail);
    typeCommand(step.command);
    const rawOutput = captureCommandOutput(step.command, demoEnv.env);
    const sanitizedOutput = sanitizeOutput(rawOutput, demoEnv.tempRoot);
    printCapturedOutput(sanitizedOutput);
    pause(3);
    if (nextStep) {
      transitionToNextFeature();
    }
  }

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
