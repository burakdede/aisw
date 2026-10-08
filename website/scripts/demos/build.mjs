// Records and renders the README demo GIFs. Run from website/: `npm run demos`.
// Needs a debug build (`cargo build`), ffmpeg on PATH, and Playwright's Chromium.
import fs from 'node:fs';
import path from 'node:path';
import { createSandbox, recordSession } from './record.mjs';
import { renderGif } from './render.mjs';

const outDir = path.resolve(import.meta.dirname, '..', '..', 'public', 'demos');

const keys = {
  ANTHROPIC_API_KEY: `sk-ant-api03-${'w'.repeat(90)}`,
  OPENAI_API_KEY: `sk-proj-${'c'.repeat(60)}`,
  GEMINI_WORK: `AIza${'g'.repeat(35)}`,
  GEMINI_PERSONAL: `AIza${'p'.repeat(35)}`,
};

// Output is shown verbatim except for the "Next" hints, the macOS-only Keychain notice and
// blank-line padding, which only cost height in a short demo.
function tidy(output, { untilBlank = false } = {}) {
  const kept = [];
  let skipping = false;
  for (const line of output.split('\n')) {
    const plain = line.replace(/\u001b\[[0-9;]*m/g, '');
    if (plain.includes('Keychain prompt')) continue;
    if (plain.trim() === 'Next') {
      skipping = true;
      continue;
    }
    if (skipping) {
      if (plain.trim() === '') skipping = false;
      continue;
    }
    if (plain.trim() === '' && (kept.length === 0 || kept.at(-1).replace(/\u001b\[[0-9;]*m/g, '').trim() === '')) continue;
    if (untilBlank && plain.trim() === '' && kept.length > 3) break;
    kept.push(line);
  }
  while (kept.length && kept.at(-1).replace(/\u001b\[[0-9;]*m/g, '').trim() === '') kept.pop();
  return kept.join('\n');
}

const allProfiles = [
  { hidden: true, command: 'aisw add claude personal --from-live' },
  { hidden: true, command: 'aisw add claude work --from-env' },
  { hidden: true, command: 'aisw add codex personal --from-live' },
  { hidden: true, command: 'aisw add codex work-api-key --from-env' },
  { hidden: true, command: `GEMINI_API_KEY=${keys.GEMINI_PERSONAL} aisw add gemini personal --from-env` },
  { hidden: true, command: `GEMINI_API_KEY=${keys.GEMINI_WORK} aisw add gemini work --from-env` },
  { hidden: true, command: `GEMINI_API_KEY=${keys.GEMINI_WORK} aisw add antigravity work --from-env` },
];

const demos = [
  {
    file: 'aisw-switch-accounts',
    title: 'aisw · save and switch accounts',
    rows: 19,
    setup: (box) => box.liveClaudeLogin('you@personal.dev'),
    steps: [
      { comment: "save the Claude Code login you're already using", command: 'aisw add claude personal --from-live' },
      { comment: 'add a work API key from the environment', command: 'aisw add claude work --from-env' },
      { command: 'aisw list claude' },
      { comment: 'switch; aisw backs up the current login first', command: 'aisw use claude work' },
    ],
  },
  {
    file: 'aisw-switch-contexts',
    title: 'aisw · switch every agent at once',
    rows: 19,
    setup: (box) => {
      box.liveClaudeLogin('you@personal.dev');
      box.liveCodexLogin('you@personal.dev');
    },
    steps: [
      ...allProfiles,
      { hidden: true, command: 'aisw context create personal --claude personal --codex personal --gemini personal' },
      { hidden: true, command: 'aisw context use personal' },
      {
        comment: 'one name for the profile each agent should use',
        command: 'aisw context create work --claude work --codex work-api-key --gemini work --antigravity work',
      },
      { command: 'aisw context use work' },
      { command: 'aisw status --context', tidy: { untilBlank: true } },
    ],
  },
  {
    file: 'aisw-guard-repository',
    title: 'aisw · guard a repository',
    rows: 19,
    setup: (box) => {
      box.liveClaudeLogin('you@personal.dev');
      box.liveCodexLogin('you@personal.dev');
      box.git('code/company-api', 'git@github.com:your-org/company-api.git');
    },
    steps: [
      ...allProfiles,
      { hidden: true, command: 'aisw context create personal --claude personal --codex personal --gemini personal' },
      { hidden: true, command: 'aisw context create work --claude work --codex work-api-key --gemini work' },
      { hidden: true, command: 'aisw context use personal' },
      { command: 'cd ~/code/company-api', cd: '~/code/company-api' },
      { comment: 'this repository must use the work accounts', command: 'aisw workspace bind . --context work' },
      { command: 'aisw workspace guard --mode strict' },
      { comment: 'the shell hook checks before the agent starts', command: 'claude' },
      { command: 'aisw context use work' },
      { command: 'aisw workspace status' },
    ],
  },
];

fs.mkdirSync(outDir, { recursive: true });
for (const demo of demos) {
  const box = createSandbox();
  try {
    box.env.ANTHROPIC_API_KEY = keys.ANTHROPIC_API_KEY;
    box.env.OPENAI_API_KEY = keys.OPENAI_API_KEY;
    demo.setup(box);
    const recorded = recordSession(box, demo.steps)
      .filter((step) => !step.hidden)
      .map((step) => ({ ...step, output: tidy(step.output, step.tidy) }));
    const output = path.join(outDir, `${demo.file}.gif`);
    const result = await renderGif({ title: demo.title, steps: recorded, rows: demo.rows, output });
    fs.copyFileSync(result.poster, path.join(outDir, `${demo.file}.png`));
    result.cleanup();
    const size = (fs.statSync(output).size / 1024).toFixed(0);
    console.log(`wrote public/demos/${demo.file}.gif (${result.frames} frames, ${size} KB) and .png`);
  } finally {
    box.cleanup();
  }
}
