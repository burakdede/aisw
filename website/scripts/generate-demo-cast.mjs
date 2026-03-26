import fs from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';

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
  prompt: '\u001b[1;38;5;114m',
  command: '\u001b[1;38;5;81m',
  output: '\u001b[38;5;252m',
  success: '\u001b[1;38;5;151m',
  warning: '\u001b[1;38;5;215m',
  note: '\u001b[38;5;180m',
  heading: '\u001b[1;38;5;223m',
};
const header = {
  version: 2,
  width: 100,
  height: 28,
  timestamp: 1774526400,
  title: 'aisw important workflows',
  env: {
    SHELL: '/bin/bash',
    TERM: 'xterm-256color',
  },
};

const steps = [
  {
    command: 'aisw init --yes',
    output: [
      'Created /tmp/aisw-demo/home/.aisw.',
      '  Appended to /tmp/aisw-demo/home/.zshrc. Restart your shell or run: source /tmp/aisw-demo/home/.zshrc',
      '',
      'Import existing credentials as profiles?',
      '  Claude Code: no existing credentials found.',
      '  Codex CLI: found /tmp/aisw-demo/home/.codex/auth.json',
      "Warning: could not verify whether codex OAuth profile 'default' belongs to a distinct account identity.",
      "  Imported Codex CLI credentials as profile 'default' and marked it active.",
      '  Gemini CLI: no existing credentials found.',
      '',
      'Setup complete.',
      "Next: run 'aisw list' to review profiles, then 'aisw use <tool> <name>' to switch.",
    ],
  },
  {
    command: 'aisw add claude work --api-key sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA --label "Work quota"',
    output: [
      "Added claude profile 'work'.",
      "Next: run 'aisw use claude work' to activate it.",
    ],
  },
  {
    command: 'aisw add claude personal --api-key sk-ant-api03-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB --label "Personal quota"',
    output: [
      "Added claude profile 'personal'.",
      "Next: run 'aisw use claude personal' to activate it.",
    ],
  },
  {
    command: 'aisw use claude personal',
    output: [
      "Switched claude to profile 'personal'.",
      "Next: run 'aisw status' to confirm the current state.",
    ],
  },
  {
    command: 'aisw status',
    output: [
      'Claude Code       personal (api_key)        credentials present (validity not checked)',
      'Codex CLI         default (oauth)           credentials present (validity not checked)',
      'Gemini CLI        —                         no active profile',
    ],
  },
  {
    command: 'aisw list',
    output: [
      'TOOL    PROFILE   ACTIVE  AUTH METHOD  LABEL',
      'claude  personal  *       api_key      Personal quota',
      'claude  work              api_key      Work quota',
      'codex   default   *       oauth        imported',
    ],
  },
  {
    command: 'aisw backup list',
    output: [
      'BACKUP ID                       TOOL     PROFILE',
      '2026-03-26T11-13-06.529Z-0000   claude   personal',
    ],
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

function colorizeLine(line) {
  if (line === '') {
    return '';
  }
  if (line.startsWith('Warning:')) {
    return `${ansi.warning}${line}${ansi.reset}`;
  }
  if (line === 'Setup complete.' || line.startsWith('Added ') || line.startsWith('Switched ')) {
    return `${ansi.success}${line}${ansi.reset}`;
  }
  if (line.startsWith('Next:')) {
    return `${ansi.note}${line}${ansi.reset}`;
  }
  if (line.endsWith('?') || line.startsWith('TOOL') || line.startsWith('BACKUP ID')) {
    return `${ansi.heading}${line}${ansi.reset}`;
  }
  return `${ansi.output}${line}${ansi.reset}`;
}

function typeCommand(command) {
  pushOutput(`${ansi.prompt}${prompt}${ansi.reset}`);
  for (const ch of command) {
    pause(0.065);
    pushOutput(`${ansi.command}${ch}${ansi.reset}`);
  }
  pause(0.14);
  pushOutput('\r\n');
}

function printBlock(lines) {
  pause(0.3);
  pushOutput(`${lines.map(colorizeLine).join('\r\n')}\r\n`);
}

async function main() {
  await fs.mkdir(path.dirname(outputPath), { recursive: true });
  for (const step of steps) {
    typeCommand(step.command);
    printBlock(step.output);
    pause(1.15);
  }
  pushOutput(`${ansi.prompt}${prompt}${ansi.reset}`);

  const contents = [JSON.stringify(header), ...events.map((event) => JSON.stringify(event))].join('\n');
  await fs.writeFile(outputPath, contents);
  console.log(`Wrote ${outputPath}`);
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
