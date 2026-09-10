import fs from 'node:fs/promises';
import path from 'node:path';

const distRoot = path.join(process.cwd(), 'dist');
const canonicalRoot = 'https://aiswitcher.dev';
const redirects = {
  '/aisw/': '/',
  '/aisw/common-situations/': '/docs/common-situations/',
  '/aisw/faq/': '/guides/',
  '/aisw/quickstart/': '/docs/quickstart/',
  '/aisw/commands/': '/docs/commands/',
  '/aisw/automation/': '/docs/automation/',
  '/aisw/shell-integration/': '/docs/shell-integration/',
  '/aisw/workspace/': '/docs/workspace/',
  '/aisw/adding-profiles/': '/docs/adding-profiles/',
  '/aisw/supported-tools/': '/supported-tools/',
  '/aisw/acceptance-matrix/': '/supported-tools/',
  '/aisw/configuration/': '/docs/configuration/',
  '/aisw/how-it-works/': '/docs/how-it-works/',
  '/aisw/security/': '/security/',
  '/aisw/why-aisw/': '/docs/why-aisw/',
  '/aisw/troubleshooting/': '/docs/troubleshooting/',
  '/aisw/releases/': '/releases/',
};
const canonicalLlmText = `# AI Switcher documentation

The canonical AI Switcher documentation now lives at https://aiswitcher.dev/docs/.

This legacy GitHub Pages site redirects old documentation URLs to their closest canonical pages.
`;

for (const [route, targetPath] of Object.entries(redirects)) {
  const target = `${canonicalRoot}${targetPath}`;
  const outputPath = path.join(distRoot, route.replace(/^\/aisw\//, ''), 'index.html');
  await fs.mkdir(path.dirname(outputPath), { recursive: true });
  await fs.writeFile(outputPath, redirectHtml(target));
}

await fs.rm(path.join(distRoot, 'sitemap-index.xml'), { force: true });
await fs.rm(path.join(distRoot, 'sitemap-0.xml'), { force: true });
await fs.writeFile(path.join(distRoot, 'llms.txt'), canonicalLlmText);
await fs.writeFile(path.join(distRoot, 'llms-full.txt'), canonicalLlmText);

function redirectHtml(target) {
  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="robots" content="noindex,nofollow">
    <meta http-equiv="refresh" content="0;url=${target}">
    <link rel="canonical" href="${target}">
    <title>Moved to AI Switcher</title>
  </head>
  <body>
    <p>This page moved to <a href="${target}">${target}</a>.</p>
    <script>location.replace(${JSON.stringify(target)});</script>
  </body>
</html>
`;
}
