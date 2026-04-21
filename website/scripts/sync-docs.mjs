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
  'Claude Code',
  'Codex CLI',
  'Gemini CLI',
  'account switching',
  'cli tooling',
];

const DOCS = [
  {
    source: 'index.md',
    output: 'index.md',
    title: 'aisw documentation',
    description: 'aisw manages named profiles for Claude Code, Codex CLI, and Gemini CLI. Switch between work, personal, and client accounts with one command on macOS, Linux, and Windows.',
    section: 'overview',
    queries: [
      'aisw docs',
      'aisw install',
      'claude code account manager',
      'codex cli account switcher',
      'gemini cli account switcher',
    ],
  },
  {
    source: 'quickstart.md',
    output: 'quickstart.md',
    title: 'Quickstart',
    description: 'Install aisw, store your first profiles, and switch between Claude Code, Codex CLI, and Gemini CLI accounts in under five minutes.',
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
    description: 'Complete syntax and flag reference for all aisw commands — add, use, list, status, remove, rename, backup, init, uninstall, shell-hook, and doctor.',
    section: 'reference',
    queries: [
      'aisw command reference',
      'aisw add use list status',
      'aisw flags',
    ],
  },
  {
    source: 'automation.md',
    output: 'automation.md',
    title: 'Automation and Scripting',
    description: 'Using aisw in CI pipelines, shell scripts, and non-interactive environments — flags, JSON output, exit codes, and common patterns.',
    section: 'reference',
    queries: [
      'aisw automation',
      'aisw json output',
      'aisw scripting',
      'aisw CI non-interactive',
      'aisw GitHub Actions',
    ],
  },
  {
    source: 'shell-integration.md',
    output: 'shell-integration.md',
    title: 'Shell Integration',
    description: 'Install and configure the aisw shell hook for bash, zsh, and fish. Understand what the hook does and how shell completions work.',
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
    description: 'How to add and capture named profiles in aisw using API keys, OAuth, environment variables, and live credential import.',
    section: 'reference',
    queries: [
      'add second Claude Code account',
      'add second Codex CLI account',
      'add second Gemini CLI account',
      'AI CLI OAuth profile manager',
      'aisw add --from-live',
    ],
  },
  {
    source: 'supported-tools.md',
    output: 'supported-tools.md',
    title: 'Supported Tools',
    description: 'Claude Code, Codex CLI, and Gemini CLI support matrix — auth methods, credential locations, OS keyring support, and state mode behavior per platform.',
    section: 'reference',
    queries: [
      'does aisw support Claude Code',
      'does aisw support Codex CLI',
      'does aisw support Gemini CLI',
      'aisw macOS Linux Windows support',
    ],
  },
  {
    source: 'config.md',
    output: 'configuration.md',
    title: 'Configuration',
    description: 'aisw configuration file location, schema, field reference, directory layout, and AISW_HOME override.',
    section: 'reference',
    queries: [
      'aisw config.json',
      'aisw configuration file',
      'aisw AISW_HOME',
    ],
  },
  {
    source: 'how-it-works.md',
    output: 'how-it-works.md',
    title: 'How It Works',
    description: 'Profile model, atomic credential switching, OS keyring integration, and per-tool implementation details for Claude Code, Codex CLI, and Gemini CLI.',
    section: 'reference',
    queries: [
      'how does aisw switch accounts',
      'aisw credential storage',
      'aisw keyring integration',
      'aisw atomic switching',
      'aisw profile model',
    ],
  },
  {
    source: 'security.md',
    output: 'security.md',
    title: 'Security',
    description: 'How aisw stores and protects credentials — local-only storage, OS keyring integration, file permissions, transactional writes, and OAuth flow safety.',
    section: 'reference',
    queries: [
      'aisw credential security',
      'is aisw safe to use',
      'aisw file permissions',
      'aisw keychain security',
      'aisw oauth safety',
    ],
  },
  {
    source: 'why-aisw.md',
    output: 'why-aisw.md',
    title: 'Why aisw?',
    description: 'Why aisw exists — the problems with manual credential switching across Claude Code, Codex CLI, and Gemini CLI, and how named profiles solve them.',
    section: 'overview',
    queries: [
      'why use aisw',
      'AI CLI account switching problem',
      'Claude Code account manager',
      'Codex CLI account switcher',
      'Gemini CLI account switcher',
      'multiple AI coding agent accounts',
    ],
  },
  {
    source: 'troubleshooting.md',
    output: 'troubleshooting.md',
    title: 'Troubleshooting',
    description: 'Diagnosing and fixing common aisw failures — missing tools, hook problems, keyring issues, permission errors, and OAuth failures.',
    section: 'reference',
    queries: [
      'aisw troubleshooting',
      'aisw shell hook not working',
      'aisw tool not found',
      'aisw gemini oauth fail',
      'aisw keyring not available',
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
      rewriteRepoMarkdownLinks(stripLeadingTitle(stripFrontmatter(raw)).trim()),
      currentVersion
    );
    const editUrl = `https://github.com/burakdede/aisw/edit/main/docs/${doc.source}`;
    const route = docRouteBySource.get(doc.source);
    const schema = buildDocSchema(doc, route, currentVersion);
    const keywords = buildKeywords(doc);
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

function stripFrontmatter(markdown) {
  return markdown.replace(/^---\n[\s\S]*?\n---\n+/, '');
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
    '> Documentation for aisw CLI.',
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
      keywords: buildKeywords(doc),
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
          name: 'What does aisw actually change when I switch accounts?',
          acceptedAnswer: {
            '@type': 'Answer',
            text: 'aisw use applies the selected profile into the live config location that Claude Code, Codex CLI, or Gemini CLI already reads. It does not patch the tool binary, install a proxy, or change anything outside the relevant local credential and config files.',
          },
        },
        {
          '@type': 'Question',
          name: 'Does aisw send credentials or prompts over the network?',
          acceptedAnswer: {
            '@type': 'Answer',
            text: 'No. aisw itself does not proxy requests, inspect prompts, or send your credentials to a remote service. It is a local credential and profile switcher.',
          },
        },
        {
          '@type': 'Question',
          name: 'Where are profiles stored, and how are they protected?',
          acceptedAnswer: {
            '@type': 'Answer',
            text: 'Stored profiles live under ~/.aisw/profiles/<tool>/<name>/. Credential files are written with 0600 permissions so only your user can read or write them, and aisw status reports files that are broader than that.',
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

function buildKeywords(doc) {
  const items = [
    ...docsKeywords,
    doc.title,
    doc.section,
  ].map((item) => item.trim().toLowerCase());
  return [...new Set(items)].join(', ');
}

function buildHeroFrontmatter(doc, currentVersion) {
  if (doc.source !== 'index.md') {
    return '';
  }
  return `template: splash\nhero:\n  title: "aisw"\n  tagline: "Account manager and switcher for Claude Code, Codex CLI, and Gemini CLI. Current release: v${currentVersion}."\n  actions:\n    - text: Quickstart\n      link: ${withBasePath('/quickstart/')}\n      variant: primary\n    - text: Commands\n      link: ${withBasePath('/commands/')}\n      variant: secondary\n    - text: Releases\n      link: https://github.com/burakdede/aisw/releases\n      variant: minimal\n`;
}

function injectVersionContext(doc, body, currentVersion) {
  if (doc.source === 'index.md') {
    return body;
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
