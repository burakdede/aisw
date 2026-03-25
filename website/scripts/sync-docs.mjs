import fs from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';

const websiteRoot = process.cwd();
const repoRoot = path.resolve(websiteRoot, '..');
const sourceRoot = path.join(repoRoot, 'docs');
const outputRoot = path.join(websiteRoot, 'src', 'content', 'docs');
const publicRoot = path.join(websiteRoot, 'public');
const siteOrigin = 'https://burakdede.github.io';
const siteBasePath = '/aisw';
const siteUrl = `${siteOrigin}${siteBasePath}`;
const logoUrl = `${siteOrigin}${siteBasePath}/aisw-512.png`;
const cargoTomlPath = path.join(repoRoot, 'Cargo.toml');
const docsKeywords = [
  'aisw',
  'AI CLI account switcher',
  'Claude Code',
  'Codex CLI',
  'Gemini CLI',
  'multi-account CLI',
  'developer tooling',
];

const DOCS = [
  {
    source: 'index.md',
    output: 'index.md',
    title: 'aisw Documentation',
    description: 'Install, configure, and use aisw to switch between Claude, Codex, and Gemini CLI accounts.',
    section: 'overview',
    queries: [
      'AI CLI account switcher',
      'Claude Code account switcher',
      'Codex CLI account switcher',
      'Gemini CLI account switcher',
      'manage multiple AI CLI accounts',
    ],
  },
  {
    source: 'quickstart.md',
    output: 'quickstart.md',
    title: 'Quickstart',
    description: 'Install aisw, run first-time setup, add profiles, and switch accounts quickly.',
    section: 'getting-started',
    queries: [
      'install aisw',
      'quickstart for Claude Code account switching',
      'quickstart for Codex CLI account switching',
      'quickstart for Gemini CLI account switching',
    ],
  },
  {
    source: 'commands.md',
    output: 'commands.md',
    title: 'Commands',
    description: 'Full command reference for aisw commands, flags, and usage patterns.',
    section: 'reference',
    queries: [
      'aisw command reference',
      'aisw add use list status',
    ],
  },
  {
    source: 'shell-integration.md',
    output: 'shell-integration.md',
    title: 'Shell Integration',
    description: 'Set up shell hooks, completions, and shell-specific integration for aisw.',
    section: 'reference',
    queries: [
      'aisw shell hook',
      'aisw zsh completion',
      'aisw bash completion',
      'aisw fish completion',
    ],
  },
  {
    source: 'adding-profiles.md',
    output: 'adding-profiles.md',
    title: 'Adding Profiles',
    description: 'Understand OAuth and API key profile flows for each supported tool.',
    section: 'reference',
    queries: [
      'add second Claude Code account',
      'add second Codex CLI account',
      'add second Gemini CLI account',
      'AI CLI OAuth profile manager',
    ],
  },
  {
    source: 'supported-tools.md',
    output: 'supported-tools.md',
    title: 'Supported Tools',
    description: 'See which tools aisw supports and how authentication works for each one.',
    section: 'reference',
    queries: [
      'does aisw support Claude Code',
      'does aisw support Codex CLI',
      'does aisw support Gemini CLI',
    ],
  },
  {
    source: 'config.md',
    output: 'configuration.md',
    title: 'Configuration',
    description: 'Reference the aisw config file, active profile state, and stored settings.',
    section: 'reference',
    queries: [
      'aisw config.json',
      'aisw configuration file',
    ],
  },
];

const docRouteBySource = new Map(
  DOCS.map((doc) => [
    doc.source,
    doc.output === 'index.md' ? '/' : `/${doc.output.replace(/\.md$/, '')}/`,
  ])
);
const knownDocRoutes = new Set([...docRouteBySource.values(), '/404/']);

async function main() {
  await fs.mkdir(outputRoot, { recursive: true });
  await fs.mkdir(publicRoot, { recursive: true });
  await clearOutputDirectory(outputRoot);
  const currentVersion = await readCurrentVersion();
  const llmsFullEntries = [];

  for (const doc of DOCS) {
    const sourcePath = path.join(sourceRoot, doc.source);
    const outputPath = path.join(outputRoot, doc.output);
    const raw = await fs.readFile(sourcePath, 'utf8');
    const body = injectVersionContext(
      doc,
      rewriteRepoMarkdownLinks(stripLeadingTitle(raw).trim()),
      currentVersion
    );
    const editUrl = `https://github.com/burakdede/aisw/edit/main/docs/${doc.source}`;
    const route = docRouteBySource.get(doc.source);
    const schema = buildDocSchema(doc, route, currentVersion);
    const keywords = [...docsKeywords, doc.title, doc.section, ...(doc.queries ?? [])].join(', ');
    const heroBlock = buildHeroFrontmatter(doc, currentVersion);
    const contents = `---\ntitle: ${doc.title}\ndescription: ${doc.description}\neditUrl: ${editUrl}\n${heroBlock}head:\n  - tag: meta\n    attrs:\n      name: robots\n      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1\n  - tag: meta\n    attrs:\n      name: keywords\n      content: ${keywords}\n  - tag: meta\n    attrs:\n      property: article:section\n      content: ${doc.section}\n  - tag: script\n    attrs:\n      type: application/ld+json\n    content: >-\n      ${JSON.stringify(schema)}\n---\n\n${body}\n`;
    await fs.writeFile(outputPath, contents);
    llmsFullEntries.push({
      ...doc,
      route,
      headings: extractHeadings(raw),
    });
  }

  await fs.writeFile(
    path.join(outputRoot, '404.md'),
    `---\ntitle: Page Not Found\ndescription: The requested aisw documentation page could not be found.\neditUrl: false\nhead:\n  - tag: meta\n    attrs:\n      name: robots\n      content: noindex,follow\n---\n\nThe page you requested does not exist.\n\n- Return to [the documentation home](${withBasePath('/')}).\n- Start with [Quickstart](${withBasePath('/quickstart/')}) if you are looking for install or setup guidance.\n- Use the sidebar to browse the rest of the docs.\n`
  );
  await fs.writeFile(path.join(publicRoot, 'robots.txt'), buildRobotsTxt());
  await fs.writeFile(path.join(publicRoot, 'llms.txt'), buildLlmsTxt(currentVersion));
  await fs.writeFile(path.join(publicRoot, 'llms-full.txt'), buildLlmsFullTxt(llmsFullEntries, currentVersion));
  await fs.writeFile(path.join(publicRoot, 'site.webmanifest'), buildWebManifest());
  await fs.writeFile(path.join(publicRoot, 'version.json'), JSON.stringify({ version: currentVersion }, null, 2));
}

async function clearOutputDirectory(dir) {
  const entries = await fs.readdir(dir, { withFileTypes: true });
  for (const entry of entries) {
    const target = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      await fs.rm(target, { recursive: true, force: true });
      continue;
    }
    await fs.unlink(target);
  }
}

function stripLeadingTitle(markdown) {
  return markdown.replace(/^# .+\n+/, '');
}

function rewriteRepoMarkdownLinks(markdown) {
  const mdLinked = markdown.replace(/\[([^\]]+)\]\(([^)]+\.md)\)/g, (_match, label, target) => {
    const normalized = target.replace(/^\.\//, '');
    const route = docRouteBySource.get(normalized);
    if (!route) {
      return `[${label}](${target})`;
    }
    return `[${label}](${withBasePath(route)})`;
  });

  return mdLinked.replace(/\[([^\]]+)\]\((\/[^)#]*\/?)(#[^)]+)?\)/g, (_match, label, target, hash = '') => {
    const normalizedTarget = target.endsWith('/') ? target : `${target}/`;
    if (!knownDocRoutes.has(normalizedTarget)) {
      return `[${label}](${target}${hash})`;
    }
    return `[${label}](${withBasePath(normalizedTarget)}${hash})`;
  });
}

function buildRobotsTxt() {
  return `User-agent: *\nAllow: /\n\nSitemap: ${siteUrl}/sitemap-index.xml\n`;
}

function buildLlmsTxt(currentVersion) {
  const lines = [
    '# aisw',
    '',
    '> Documentation for aisw, a CLI for switching between Claude Code, Codex CLI, and Gemini CLI accounts.',
    '',
    `Current version: ${currentVersion}`,
    '',
    '## Docs',
    '',
  ];

  for (const doc of DOCS) {
    const href = doc.output === 'index.md'
      ? `${siteOrigin}${siteBasePath}/`
      : `${siteOrigin}${siteBasePath}/${doc.output.replace(/\.md$/, '')}/`;
    lines.push(`- [${doc.title}](${href}): ${doc.description}`);
  }

  lines.push('');
  lines.push('## Project');
  lines.push('');
  lines.push(`- [GitHub Repository](https://github.com/burakdede/aisw): Source code, releases, and issue tracking.`);
  lines.push(`- [README](https://github.com/burakdede/aisw/blob/main/README.md): Project overview and install guidance.`);
  lines.push('');

  return lines.join('\n');
}

function buildLlmsFullTxt(entries, currentVersion) {
  const lines = [
    '# aisw',
    '',
    '> Structured documentation index for language models and retrieval systems.',
    '',
    `Current version: ${currentVersion}`,
    '',
  ];

  for (const doc of entries) {
    lines.push(`## ${doc.title}`);
    lines.push('');
    lines.push(`URL: ${siteUrl}${doc.route}`);
    lines.push(`Section: ${doc.section}`);
    lines.push(`Summary: ${doc.description}`);
    lines.push(`Source: https://github.com/burakdede/aisw/blob/main/docs/${doc.source}`);
    lines.push(`Headings: ${doc.headings.join(' | ') || 'None'}`);
    lines.push(`Common queries: ${(doc.queries ?? []).join(' | ') || 'None'}`);
    lines.push('');
  }

  return lines.join('\n');
}

function buildWebManifest() {
  return JSON.stringify(
    {
      name: 'aisw Documentation',
      short_name: 'aisw docs',
      description: 'Documentation for aisw, the CLI for switching between Claude, Codex, and Gemini accounts.',
      start_url: `${siteBasePath}/`,
      scope: `${siteBasePath}/`,
      display: 'standalone',
      background_color: '#0b1020',
      theme_color: '#0b1020',
      icons: [
        {
          src: `${siteBasePath}/aisw-192.png`,
          sizes: '192x192',
          type: 'image/png',
          purpose: 'any',
        },
        {
          src: `${siteBasePath}/aisw-512.png`,
          sizes: '512x512',
          type: 'image/png',
          purpose: 'any',
        },
      ],
    },
    null,
    2
  );
}

function buildDocSchema(doc, route, currentVersion) {
  const url = `${siteUrl}${route}`;
  const breadcrumbItems = [
    {
      '@type': 'ListItem',
      position: 1,
      name: 'Documentation',
      item: `${siteUrl}/`,
    },
  ];
  if (route !== '/') {
    breadcrumbItems.push({
      '@type': 'ListItem',
      position: 2,
      name: doc.title,
      item: url,
    });
  }

  const graph = [
    {
      '@type': route === '/' ? 'WebPage' : 'TechArticle',
      name: doc.title,
      headline: doc.title,
      description: doc.description,
      url,
      inLanguage: 'en',
      keywords: [...docsKeywords, ...(doc.queries ?? [])].join(', '),
      image: logoUrl,
      isPartOf: {
        '@type': 'WebSite',
        name: 'aisw Documentation',
        url: `${siteUrl}/`,
      },
      about: {
        '@type': 'SoftwareApplication',
        name: 'aisw',
        applicationCategory: 'DeveloperApplication',
        operatingSystem: 'macOS, Linux, Windows',
        softwareVersion: currentVersion,
        url: 'https://github.com/burakdede/aisw',
        image: logoUrl,
      },
    },
    {
      '@type': 'BreadcrumbList',
      itemListElement: breadcrumbItems,
    },
  ];

  if (route === '/') {
    graph.push({
      '@type': 'FAQPage',
      mainEntity: [
        {
          '@type': 'Question',
          name: 'Can aisw switch between multiple Claude Code accounts?',
          acceptedAnswer: {
            '@type': 'Answer',
            text: 'Yes. aisw can store and switch multiple Claude Code profiles, including API key and OAuth-based profiles.',
          },
        },
        {
          '@type': 'Question',
          name: 'Can aisw manage both Codex CLI and Gemini CLI accounts too?',
          acceptedAnswer: {
            '@type': 'Answer',
            text: 'Yes. aisw supports Claude Code, Codex CLI, and Gemini CLI in one local profile manager.',
          },
        },
        {
          '@type': 'Question',
          name: 'Does aisw proxy requests or inspect prompts?',
          acceptedAnswer: {
            '@type': 'Answer',
            text: 'No. aisw is a local credential and profile switcher. It does not proxy traffic, inspect prompts, or run a gateway service.',
          },
        },
      ],
    });
  }

  return {
    '@context': 'https://schema.org',
    '@graph': graph,
  };
}

function buildHeroFrontmatter(doc, currentVersion) {
  if (doc.source !== 'index.md') {
    return '';
  }
  return `template: splash\nhero:\n  title: "aisw"\n  tagline: "Switch between Claude Code, Codex CLI, and Gemini CLI accounts with one local CLI. Current release: v${currentVersion}."\n  actions:\n    - text: Quickstart\n      link: ${withBasePath('/quickstart/')}\n      variant: primary\n    - text: Releases\n      link: ${withBasePath('/releases/')}\n      variant: secondary\n    - text: GitHub\n      link: https://github.com/burakdede/aisw\n      variant: minimal\n`;
}

function injectVersionContext(doc, body, currentVersion) {
  if (doc.source === 'index.md') {
    return `> Current documented CLI release: \`v${currentVersion}\`. Use [Quickstart](${withBasePath('/quickstart/')}) to install and start switching profiles.\n\n${body}`;
  }

  if (doc.source === 'releases.md') {
    return `> Current documented CLI release: \`v${currentVersion}\`. Release tags should follow this versioning format as the project evolves.\n\n${body}`;
  }

  return body;
}

async function readCurrentVersion() {
  const cargoToml = await fs.readFile(cargoTomlPath, 'utf8');
  return readVersionFromString(cargoToml);
}

function readVersionFromString(cargoToml) {
  const match = cargoToml.match(/^version = "([^"]+)"$/m);
  if (!match) {
    throw new Error('Could not read current version from Cargo.toml');
  }
  return match[1];
}

function withBasePath(route) {
  return `${siteBasePath}${route === '/' ? '/' : route}`;
}

function extractHeadings(markdown) {
  return markdown
    .split('\n')
    .map((line) => line.match(/^##+\s+(.+)$/)?.[1]?.trim())
    .filter(Boolean);
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
