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
  'sitemap-index.xml',
  'sitemap-0.xml',
];

async function main() {
  for (const file of requiredFiles) {
    await assertFile(path.join(distRoot, file));
  }

  const indexHtml = await fs.readFile(path.join(distRoot, 'index.html'), 'utf8');
  assertContains(indexHtml, `<link rel="canonical" href="${siteUrl}"/>`, 'root canonical URL');
  assertContains(indexHtml, 'application/ld+json', 'structured data');
  assertContains(indexHtml, 'Current release: v0.2.0.', 'release-aware hero tagline');
  assertContains(indexHtml, 'href="/aisw/quickstart/"', 'base-aware internal docs link');
  assertNotContains(indexHtml, '/aisw/aisw/', 'duplicate base path in root HTML');
  assertNotContains(indexHtml, 'href="/quickstart/"', 'root-relative docs link without base');

  const sitemap = await fs.readFile(path.join(distRoot, 'sitemap-0.xml'), 'utf8');
  assertContains(sitemap, '<loc>https://burakdede.github.io/aisw/</loc>', 'root sitemap entry');
  assertNotContains(sitemap, '/aisw/aisw/', 'duplicate base path in sitemap');

  const robotsTxt = await fs.readFile(path.join(distRoot, 'robots.txt'), 'utf8');
  assertContains(robotsTxt, 'Sitemap: https://burakdede.github.io/aisw/sitemap-index.xml', 'robots sitemap');

  const llmsFull = await fs.readFile(path.join(distRoot, 'llms-full.txt'), 'utf8');
  assertContains(llmsFull, 'Current version: 0.2.0', 'llms-full current version');
  assertContains(llmsFull, '## Quickstart', 'llms-full quickstart entry');
  assertContains(llmsFull, 'Headings:', 'llms-full heading inventory');

  const versionJson = await fs.readFile(path.join(distRoot, 'version.json'), 'utf8');
  assertContains(versionJson, '"version": "0.2.0"', 'version metadata artifact');
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
