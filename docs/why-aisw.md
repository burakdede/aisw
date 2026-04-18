# Why aisw

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

- [Quickstart](quickstart.md)
- [Commands](commands.md)
