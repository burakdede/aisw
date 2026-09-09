import fs from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';

const websiteRoot = process.cwd();
const distRoot = path.join(websiteRoot, 'dist');
const siteUrl = 'https://burakdede.github.io/aisw/';
const requiredFiles = [
  'index.html',
  'robots.txt',
  'llms.txt',
  'llms-full.txt',
  'site.webmanifest',
  'version.json',
];

const redirects = {
  '/aisw/': 'https://aiswitcher.dev/',
  '/aisw/common-situations/': 'https://aiswitcher.dev/docs/common-situations/',
  '/aisw/faq/': 'https://aiswitcher.dev/guides/',
  '/aisw/quickstart/': 'https://aiswitcher.dev/docs/quickstart/',
  '/aisw/commands/': 'https://aiswitcher.dev/docs/commands/',
  '/aisw/automation/': 'https://aiswitcher.dev/docs/automation/',
  '/aisw/shell-integration/': 'https://aiswitcher.dev/docs/shell-integration/',
  '/aisw/workspace/': 'https://aiswitcher.dev/docs/workspace/',
  '/aisw/adding-profiles/': 'https://aiswitcher.dev/docs/adding-profiles/',
  '/aisw/supported-tools/': 'https://aiswitcher.dev/supported-tools/',
  '/aisw/acceptance-matrix/': 'https://aiswitcher.dev/supported-tools/',
  '/aisw/configuration/': 'https://aiswitcher.dev/docs/configuration/',
  '/aisw/how-it-works/': 'https://aiswitcher.dev/docs/how-it-works/',
  '/aisw/security/': 'https://aiswitcher.dev/security/',
  '/aisw/why-aisw/': 'https://aiswitcher.dev/docs/why-aisw/',
  '/aisw/troubleshooting/': 'https://aiswitcher.dev/docs/troubleshooting/',
  '/aisw/releases/': 'https://aiswitcher.dev/releases/',
};

async function main() {
  for (const file of requiredFiles) {
    await assertFile(path.join(distRoot, file));
  }

  const versionJson = await fs.readFile(path.join(distRoot, 'version.json'), 'utf8');
  const { version } = JSON.parse(versionJson);
  if (!version) {
    throw new Error('Missing version metadata in version.json');
  }

  const robotsTxt = await fs.readFile(path.join(distRoot, 'robots.txt'), 'utf8');
  assertContains(robotsTxt, 'User-agent: *\nDisallow: /', 'legacy robots policy');

  for (const [route, target] of Object.entries(redirects)) {
    const file = path.join(distRoot, route.replace(/^\/aisw\//, ''), 'index.html');
    const html = await fs.readFile(file, 'utf8');
    assertContains(html, `<link rel="canonical" href="${target}">`, `${route} canonical redirect`);
    assertContains(html, `location.replace(${JSON.stringify(target)})`, `${route} redirect script`);
    assertContains(html, 'noindex,nofollow', `${route} noindex policy`);
  }

  const llms = await fs.readFile(path.join(distRoot, 'llms.txt'), 'utf8');
  assertContains(llms, 'https://aiswitcher.dev/', 'canonical LLM destination');
  assertNotContains(llms, 'burakdede.github.io/aisw', 'legacy LLM URL');
  assertContains(versionJson, `"version": "${version}"`, 'version metadata artifact');
}

async function assertFile(target) {
  try {
    await fs.access(target);
  } catch {
    throw new Error(`Missing expected build artifact: ${path.relative(distRoot, target)}`);
  }
}

function assertContains(text, expected, label) {
  if (!text.includes(expected)) {
    throw new Error(`Missing ${label}: ${expected}`);
  }
}

function assertNotContains(text, unexpected, label) {
  if (text.includes(unexpected)) {
    throw new Error(`Found ${label}: ${unexpected}`);
  }
}

main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
