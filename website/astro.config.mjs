// @ts-check
import fs from 'node:fs';
import path from 'node:path';
import { defineConfig } from 'astro/config';
import sitemap from '@astrojs/sitemap';
import starlight from '@astrojs/starlight';

const siteOrigin = 'https://burakdede.github.io';
const siteBasePath = '/aisw';
const siteUrl = `${siteOrigin}${siteBasePath}/`;
const logoPath = `${siteBasePath}/aisw-512.png`;
const cargoToml = fs.readFileSync(path.resolve(import.meta.dirname, '..', 'Cargo.toml'), 'utf8');
const currentVersion = cargoToml.match(/^version = "([^"]+)"$/m)?.[1] ?? '0.0.0';
const siteSchema = {
	'@context': 'https://schema.org',
	'@graph': [
		{
			'@type': 'WebSite',
			name: 'aisw AI Switcher for Claude Code, Codex CLI, and Gemini CLI',
			url: siteUrl,
			description:
				'aisw is an AI and coding agent account manager and account switcher for Claude Code, Codex CLI, and Gemini CLI, with docs for installation, profile management, and secure local switching.',
			image: `${siteOrigin}${logoPath}`,
			publisher: {
				'@type': 'Organization',
				name: 'aisw',
				logo: {
					'@type': 'ImageObject',
					url: `${siteOrigin}${logoPath}`,
				},
			},
		},
		{
			'@type': 'SoftwareApplication',
			name: 'aisw',
			alternateName: 'AI Switcher',
			applicationCategory: 'DeveloperApplication',
			operatingSystem: 'macOS, Linux, Windows',
			softwareVersion: currentVersion,
			description:
				'AI and coding agent account manager and switcher CLI for Claude Code, Codex CLI, and Gemini CLI accounts.',
			url: 'https://github.com/burakdede/aisw',
			image: `${siteOrigin}${logoPath}`,
		},
	],
};

// https://astro.build/config
export default defineConfig({
	site: siteOrigin,
	base: siteBasePath,
	integrations: [
		sitemap(),
		starlight({
			title: 'aisw',
			description:
				'AI and coding agent account manager and switcher for Claude Code, Codex CLI, and Gemini CLI. Install aisw, manage multiple profiles, and switch accounts locally.',
			logo: {
				light: './public/aisw-mark-light.svg',
				dark: './public/aisw-mark-dark.svg',
				alt: 'aisw logo',
			},
			favicon: '/favicon.ico',
			head: [
				{
					tag: 'meta',
					attrs: {
						name: 'application-name',
						content: 'aisw',
					},
				},
				{
					tag: 'meta',
					attrs: {
						name: 'robots',
						content: 'index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1',
					},
				},
				{
					tag: 'meta',
					attrs: {
						name: 'theme-color',
						content: '#0b1020',
					},
				},
					{
						tag: 'link',
						attrs: {
							rel: 'icon',
							href: `${siteBasePath}/favicon.ico`,
							sizes: 'any',
						},
					},
					{
						tag: 'link',
						attrs: {
							rel: 'apple-touch-icon',
							href: `${siteBasePath}/apple-touch-icon.png`,
							sizes: '180x180',
						},
					},
					{
						tag: 'meta',
						attrs: {
							property: 'og:image',
						content: `${siteOrigin}${logoPath}`,
					},
				},
				{
					tag: 'meta',
					attrs: {
						name: 'twitter:card',
						content: 'summary_large_image',
					},
				},
				{
					tag: 'meta',
					attrs: {
						name: 'twitter:image',
						content: `${siteOrigin}${logoPath}`,
					},
				},
				{
					tag: 'link',
					attrs: {
						rel: 'manifest',
						href: `${siteBasePath}/site.webmanifest`,
					},
				},
				{
					tag: 'script',
					attrs: {
						type: 'application/ld+json',
					},
					content: JSON.stringify(siteSchema),
				},
			],
			social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/burakdede/aisw' }],
			sidebar: [
				{
					label: 'Getting Started',
					items: [
						{ label: 'Overview', slug: '' },
						{ label: 'Quickstart', slug: 'quickstart' },
						{ label: 'Shell Integration', slug: 'shell-integration' },
					],
				},
				{
					label: 'Reference',
					items: [
						{ label: 'Commands', slug: 'commands' },
						{ label: 'Adding Profiles', slug: 'adding-profiles' },
						{ label: 'Supported Tools', slug: 'supported-tools' },
						{ label: 'Configuration', slug: 'configuration' },
					],
				},
				{
					label: 'Automation',
					items: [{ label: 'Automation and Scripting', slug: 'automation' }],
				},
			],
			editLink: {
				baseUrl: 'https://github.com/burakdede/aisw/edit/main/',
			},
			customCss: [
				'./src/styles/custom.css',
			],
		}),
	],
});
