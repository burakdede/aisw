// Renders a recorded session into an animated GIF of a terminal window in the AI Switcher palette.
// Frames are screenshots of an HTML terminal in Chromium; ffmpeg assembles them with exact
// per-frame durations and a shared palette so text stays crisp.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { chromium } from 'playwright';

const fontFile = path.join(import.meta.dirname, 'fonts', 'GeistMono-Variable.woff2');

const palette = {
  text: '#e8eaed',
  dim: '#8a909a',
  prompt: '#7c8cff',
  30: '#5c6370',
  31: '#ff6b6b',
  32: '#4ade80',
  33: '#f5b544',
  34: '#7c8cff',
  35: '#c4a7ff',
  36: '#a5b1ff',
  37: '#e8eaed',
};

const escapeHtml = (text) => text.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');

// Converts SGR-coloured text into HTML lines. Handles the codes aisw emits: reset, bold, dim,
// and the 8 standard and bright foreground colours.
function ansiToLines(text) {
  const lines = [];
  let current = '';
  let style = { bold: false, dim: false, color: null };
  const open = () => {
    const css = [];
    if (style.color) css.push(`color:${style.color}`);
    if (style.dim && !style.color) css.push(`color:${palette.dim}`);
    if (style.bold) css.push('font-weight:650');
    return css.length ? `<span style="${css.join(';')}">` : '<span>';
  };
  for (const part of text.split(/(\u001b\[[0-9;]*m)/)) {
    const sgr = part.match(/^\u001b\[([0-9;]*)m$/);
    if (sgr) {
      for (const code of (sgr[1] || '0').split(';').map(Number)) {
        if (code === 0) style = { bold: false, dim: false, color: null };
        else if (code === 1) style.bold = true;
        else if (code === 2) style.dim = true;
        else if (code === 22) style.bold = style.dim = false;
        else if (code === 39) style.color = null;
        else if (code >= 30 && code <= 37) style.color = palette[code];
        else if (code >= 90 && code <= 97) style.color = palette[code - 60];
      }
      continue;
    }
    const pieces = part.split('\n');
    pieces.forEach((piece, index) => {
      if (index > 0) {
        lines.push(current);
        current = '';
      }
      if (piece) current += `${open()}${escapeHtml(piece)}</span>`;
    });
  }
  lines.push(current);
  return lines;
}

const promptHtml = (cwd) =>
  `<span style="color:${palette.dim}">${escapeHtml(cwd)}</span> <span style="color:${palette.prompt};font-weight:650">$</span> `;
const commandHtml = (text) => `<span style="color:#ffffff;font-weight:600">${escapeHtml(text)}</span>`;
const commentHtml = (text) => `<span style="color:${palette.dim}"># ${escapeHtml(text)}</span>`;
const cursor = '<span class="cursor"></span>';

// Builds the sequence of screen states and how long each stays on screen, in milliseconds.
export function timeline(steps) {
  const frames = [];
  const history = [];
  const push = (lines, duration) => frames.push({ lines: [...lines], duration });
  let cwd = '~';
  push([promptHtml(cwd) + cursor], 700);
  for (const step of steps) {
    if (step.comment) history.push(commentHtml(step.comment));
    for (let typed = 0; typed <= step.command.length; typed += 3) {
      push([...history, promptHtml(cwd) + commandHtml(step.command.slice(0, typed)) + cursor], 45);
    }
    history.push(promptHtml(cwd) + commandHtml(step.command));
    push([...history, ''], 380);
    if (step.cd) cwd = step.cd;
    const output = step.output ? ansiToLines(step.output) : [];
    history.push(...output, '');
    const hold = Math.min(3400, 1500 + output.length * 70);
    push([...history, promptHtml(cwd) + cursor], hold);
  }
  frames.at(-1).duration = 4200;
  return frames;
}

function pageHtml(title, rows) {
  const font = fs.readFileSync(fontFile).toString('base64');
  return `<!doctype html><html><head><style>
    @font-face { font-family: "Geist Mono"; src: url(data:font/woff2;base64,${font}) format("woff2"); font-weight: 100 900; }
    html, body { margin: 0; background: #0b0d10; }
    body { width: 1200px; padding: 28px; box-sizing: border-box; font-family: "Geist Mono", monospace; }
    .window { border: 1px solid rgba(255,255,255,0.09); border-radius: 14px; background: #111419; overflow: hidden;
      box-shadow: 0 24px 60px rgba(0,0,0,0.45), inset 0 1px 0 rgba(255,255,255,0.05); }
    .bar { position: relative; height: 46px; display: flex; align-items: center; padding: 0 18px;
      background: #15181e; border-bottom: 1px solid rgba(255,255,255,0.07); }
    .dots { display: flex; gap: 8px; }
    .dots i { width: 12px; height: 12px; border-radius: 50%; }
    .title { position: absolute; left: 0; right: 0; text-align: center; color: #8a909a; font-size: 15px; letter-spacing: 0.01em; }
    .screen { height: calc(${rows} * 32px); padding: 22px 30px 26px; color: ${palette.text};
      font-size: 20px; line-height: 32px; white-space: pre; overflow: hidden; display: flex; flex-direction: column; justify-content: flex-end; }
    .line { height: 32px; flex: none; }
    .cursor { display: inline-block; width: 11px; height: 22px; margin-left: 1px; vertical-align: -4px; background: ${palette.prompt}; }
  </style></head><body><div class="window"><div class="bar">
    <div class="dots"><i style="background:#ff5f57"></i><i style="background:#febc2e"></i><i style="background:#28c840"></i></div>
    <div class="title">${escapeHtml(title)}</div></div><div class="screen" id="screen"></div></div></body></html>`;
}

export async function renderGif({ title, steps, rows, output }) {
  const frames = timeline(steps);
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'aisw-demo-frames-'));
  const browser = await chromium.launch();
  try {
    const height = 56 + 46 + rows * 32 + 48 + 2;
    const page = await browser.newPage({ viewport: { width: 1200, height }, deviceScaleFactor: 1 });
    await page.setContent(pageHtml(title, rows));
    await page.evaluate(() => document.fonts.ready);
    const list = [];
    for (const [index, frame] of frames.entries()) {
      await page.evaluate((lines) => {
        document.getElementById('screen').innerHTML = lines.map((line) => `<div class="line">${line}</div>`).join('');
      }, frame.lines);
      const file = path.join(dir, `${String(index).padStart(4, '0')}.png`);
      await page.screenshot({ path: file, clip: { x: 0, y: 0, width: 1200, height } });
      list.push(`file '${file}'\nduration ${(frame.duration / 1000).toFixed(3)}`);
    }
    // ffmpeg's concat demuxer ignores the last duration unless the final file is repeated.
    list.push(`file '${path.join(dir, `${String(frames.length - 1).padStart(4, '0')}.png`)}'`);
    fs.writeFileSync(path.join(dir, 'frames.txt'), list.join('\n'));
    execFileSync('ffmpeg', [
      '-y', '-loglevel', 'error', '-f', 'concat', '-safe', '0', '-i', path.join(dir, 'frames.txt'),
      '-vf', 'split[a][b];[a]palettegen=max_colors=96:stats_mode=full[p];[b][p]paletteuse=dither=none',
      '-loop', '0', output,
    ]);
    const last = path.join(dir, `${String(frames.length - 1).padStart(4, '0')}.png`);
    return { frames: frames.length, poster: last, cleanup: () => fs.rmSync(dir, { recursive: true, force: true }) };
  } finally {
    await browser.close();
  }
}
