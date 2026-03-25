# aisw Docs Site

This directory contains the Astro + Starlight documentation site for `aisw`.

Repository markdown in `../docs/` is the source of truth. The site does not maintain hand-copied docs pages. Instead, `npm run sync-docs` generates Starlight content entries from the repository markdown before local dev and production builds.

## Commands

Run these from [website](/Users/burakdede/Projects/aisw/website):

| Command | Action |
|---|---|
| `npm ci` | Install site dependencies |
| `npm run sync-docs` | Generate site pages from `../docs/*.md` |
| `npm run dev` | Start the local docs site |
| `npm run build` | Build the static site into `dist/` |
| `npm run preview` | Preview the built site locally |
