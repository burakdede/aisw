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
			name: 'aisw  -  AI coding agent account manager',
			url: siteUrl,
			description: 'aisw is a named profile manager for Claude Code, Codex CLI, and Gemini CLI. Switch between work, personal, and client accounts in one command on macOS, Linux, and Windows.',
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
			description: 'Named profile manager for Claude Code, Codex CLI, and Gemini CLI. Store and switch multiple accounts  -  work, personal, client  -  in one command. Supports OAuth, API keys, OS keychain, and CI automation.',
			url: 'https://github.com/burakdede/aisw',
			image: `${siteOrigin}${logoPath}`,
			featureList: [
				'Switch Claude Code accounts with one command',
				'Switch Codex CLI accounts with one command',
				'Switch Gemini CLI accounts with one command',
				'macOS Keychain integration',
				'Linux Secret Service integration',
				'Windows Credential Manager integration',
				'OAuth and API key support',
				'Atomic profile switching with rollback',
				'Automatic backups before destructive operations',
				'CI-safe non-interactive mode',
				'JSON output for scripting',
			],
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
			description: 'CLI docs for install, setup, commands, automation, and troubleshooting.',
			logo: {
				light: './public/aisw-logo.png',
				dark: './public/aisw-logo.png',
				alt: 'aisw logo',
			},
			favicon: '/favicon-32.png',
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
							type: 'image/png',
							href: `${siteBasePath}/favicon-32.png`,
							sizes: '32x32',
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
						{ label: 'How It Works', slug: 'how-it-works' },
						{ label: 'Security', slug: 'security' },
					],
				},
				{
					label: 'Guides',
					items: [
						{ label: 'Automation and Scripting', slug: 'automation' },
						{ label: 'Troubleshooting', slug: 'troubleshooting' },
						{ label: 'Why aisw', slug: 'why-aisw' },
					],
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
