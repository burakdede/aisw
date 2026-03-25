# Docs Site Architecture

## Decision

Use **Astro with the Starlight docs theme** for the `aisw` documentation site.

This is the selected direction for the docs-site backlog track:

- `AI-53` chooses the generator and deployment model
- `AI-54` will implement the site from repository markdown
- `AI-55` will automate GitHub Pages deployment
- `AI-56` will add SEO and discoverability assets
- `AI-57` will add release-aware metadata and validate the desktop/mobile UX

## Why Astro + Starlight

This choice fits the current repo and backlog requirements better than a custom site or a heavier app framework:

- Markdown-first: Starlight is built for documentation content and works well with repo-authored markdown.
- Low maintenance: it provides navigation, search integration points, metadata hooks, and accessible defaults without building a docs shell from scratch.
- Fast output: Astro generates static pages well-suited for GitHub Pages.
- Good SEO baseline: static HTML, metadata support, sitemap support, canonical URLs, and robots handling are straightforward.
- Good mobile/desktop defaults: the base docs UI is already responsive, which reduces future UI debt.
- Release-aware extension points: Astro config, build-time data loading, and content collections make it practical to inject version metadata later without coupling docs updates to binary releases.

## Repo Layout

Add a dedicated docs app under `website/` rather than mixing site framework files into the Rust CLI root.

Planned structure:

```text
website/
  astro.config.mjs
  package.json
  tsconfig.json
  public/
  src/
    content/
    components/
    layouts/
    pages/
    styles/
  scripts/
```

Repository markdown remains the source of truth.

Content sources:

- `docs/*.md` remains the primary source for user documentation.
- selected top-level repo markdown such as `README.md`, and later possibly `CONTRIBUTING.md`, can be included where useful.
- site-specific glue files inside `website/` should be thin wrappers or generated artifacts, not duplicated copies of the same docs.

## Content Model

The site should be generated from repository markdown, not manually rewritten page-by-page.

Planned approach:

- keep human-authored docs in `docs/`
- add a small build-time ingestion layer in `website/` that reads repository markdown files
- transform those markdown files into Astro/Starlight content entries
- define navigation centrally in the site config instead of encoding navigation in each markdown file

This keeps the docs editable from the existing repo structure while allowing the site to present:

- shared navigation
- stable URLs
- page metadata
- search-friendly headings

## URL Model

Use short, stable, documentation-style paths.

Planned mapping:

- `docs/index.md` -> `/`
- `docs/quickstart.md` -> `/quickstart/`
- `docs/commands.md` -> `/commands/`
- `docs/shell-integration.md` -> `/shell-integration/`
- `docs/adding-profiles.md` -> `/adding-profiles/`
- `docs/supported-tools.md` -> `/supported-tools/`
- `docs/config.md` -> `/configuration/`
- `docs/releases.md` -> `/releases/`

If a top-level repo markdown page is exposed on the site, it should receive an equally clean URL rather than a raw filename URL.

## GitHub Pages Deployment Model

Deploy from this repository with GitHub Actions and GitHub Pages.

Planned model:

- build the static site in CI on pushes to `main`
- publish the generated output with the GitHub Pages workflow
- no manual hosting, no separate server, no separate CMS
- domain can remain the default GitHub Pages domain unless a custom domain is explicitly added later

This keeps docs hosting aligned with the existing repo and release workflow while avoiding a second hosting system to manage.

## Release-Aware Metadata Direction

Release-aware behavior should be **build-time metadata**, not handwritten docs churn.

Planned later implementation:

- read the current CLI version from repository metadata such as `Cargo.toml` and/or release context
- expose it to the docs layout in a small number of useful places
- avoid hard-coding version numbers into normal documentation prose unless necessary

This keeps documentation updates decoupled from every release while still allowing the site to show current version context.

## Non-Goals for AI-53

This decision does **not** implement the docs site yet.

Not part of this step:

- bootstrapping Astro files
- generating pages from markdown
- deploying to GitHub Pages
- adding sitemap, robots, canonical tags, or release metadata in the UI

Those belong to the follow-up issues already split out in the backlog.

## AI-56 Implementation Direction

The SEO and discoverability step should stay **manifest-driven** rather than adding metadata manually page by page.

Implementation principles:

- keep page titles and descriptions in the docs manifest that already maps repo markdown to site routes
- rely on Starlight defaults for canonical URLs, page titles, and Open Graph URL generation
- generate `robots.txt`, `sitemap.xml`, `llms.txt`, `llms-full.txt`, and `site.webmanifest` from the same manifest data
- add JSON-LD structured data at the site and page level so search engines and machine readers can identify the docs cleanly
- add a build-time validation script to catch canonical URL regressions or missing discoverability artifacts before deployment

## Next Order

The proper backlog order from here is:

1. `AI-53` document and lock the docs-site architecture
2. `AI-54` implement the site from `docs/` markdown
3. `AI-55` add GitHub Pages deployment
4. `AI-56` add SEO and discoverability metadata
5. `AI-57` add release-aware version metadata and validate desktop/mobile UX
