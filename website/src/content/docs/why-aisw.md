---
title: Why aisw?
description: Problem statement and scope of aisw.
editUrl: https://github.com/burakdede/aisw/edit/main/docs/why-aisw.md
head:
  - tag: meta
    attrs:
      name: robots
      content: index,follow,max-image-preview:large,max-snippet:-1,max-video-preview:-1
  - tag: meta
    attrs:
      name: keywords
      content: aisw, claude code, codex cli, gemini cli, account switching, cli tooling, why aisw?, overview
  - tag: meta
    attrs:
      property: article:section
      content: overview
  - tag: script
    attrs:
      type: application/ld+json
    content: >-
      {"@context":"https://schema.org","@graph":[{"@type":"TechArticle","name":"Why aisw?","headline":"Why aisw?","description":"Problem statement and scope of aisw.","url":"https://burakdede.github.io/aisw/why-aisw/","inLanguage":"en","keywords":"aisw, claude code, codex cli, gemini cli, account switching, cli tooling, why aisw?, overview","image":"https://burakdede.github.io/aisw/aisw-512.png","isPartOf":{"@type":"WebSite","name":"aisw Documentation","url":"https://burakdede.github.io/aisw/"},"about":{"@type":"SoftwareApplication","name":"aisw","applicationCategory":"DeveloperApplication","operatingSystem":"macOS, Linux, Windows","softwareVersion":"0.3.2","url":"https://github.com/burakdede/aisw","image":"https://burakdede.github.io/aisw/aisw-512.png"}},{"@type":"BreadcrumbList","itemListElement":[{"@type":"ListItem","position":1,"name":"Documentation","item":"https://burakdede.github.io/aisw/"},{"@type":"ListItem","position":2,"name":"Why aisw?","item":"https://burakdede.github.io/aisw/why-aisw/"}]}]}
---

`aisw` exists to make multi-account usage across AI coding CLIs predictable.

## Problem

Manual switching usually means editing or copying hidden auth files in tool-specific directories. That causes:

- unclear active account state
- fragile hand-edited configs
- missing rollback when a switch fails

## What aisw does

- stores named profiles per tool under `~/.aisw/profiles/`
- applies a profile with one command: `aisw use <tool> <profile>`
- creates switch backups for restore
- enforces secure local file permissions (`0600` on sensitive files)
- exposes status and inventory via human output and JSON

## What aisw does not do

- does not proxy model traffic
- does not inspect prompts
- does not send credentials to a remote service

## Typical users

- developers with separate work/personal accounts
- consultants switching between client accounts
- teams sharing one account for some tasks and personal accounts for others

## Start

- [Quickstart](/aisw/quickstart/)
- [Commands](/aisw/commands/)
