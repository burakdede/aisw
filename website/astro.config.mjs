// @ts-check
import fs from 'node:fs';
import path from 'node:path';
import { defineConfig } from 'astro/config';
import sitemap from '@astrojs/sitemap';
import starlight from '@astrojs/starlight';

const siteOrigin = 'https://burakdede.github.io';
const siteBasePath = '/aisw';
const siteUrl = `${siteOrigin}${siteBasePath}/`;
const logoPath = `${siteBasePath}/aisw.png`;
const cargoToml = fs.readFileSync(path.resolve(import.meta.dirname, '..', 'Cargo.toml'), 'utf8');
const currentVersion = cargoToml.match(/^version = "([^"]+)"$/m)?.[1] ?? '0.0.0';
const siteSchema = {
	'@context': 'https://schema.org',
	'@graph': [
		{
			'@type': 'WebSite',
			name: 'aisw Documentation',
			url: siteUrl,
			description:
				'Documentation for aisw, the AI CLI account switcher for Claude, Codex, and Gemini.',
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
			applicationCategory: 'DeveloperApplication',
			operatingSystem: 'macOS, Linux, Windows',
			softwareVersion: currentVersion,
			description:
				'CLI utility for switching between Claude Code, Codex CLI, and Gemini CLI accounts.',
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
			description: 'Documentation for aisw, the AI CLI account switcher for Claude, Codex, and Gemini.',
			logo: {
				src: './public/aisw.png',
				alt: 'aisw logo',
			},
			favicon: '/aisw.png',
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
			],
			editLink: {
				baseUrl: 'https://github.com/burakdede/aisw/edit/main/',
			},
		}),
	],
});
