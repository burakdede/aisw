#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

test -f completions/aisw.bash
test -f completions/_aisw
test -f completions/aisw.fish

grep -q "claude" completions/aisw.bash
grep -q "codex" completions/aisw.bash
grep -q "gemini" completions/aisw.bash
grep -q "shell-hook" completions/aisw.bash

bash -c '
  set -euo pipefail
  source completions/aisw.bash
  complete -p aisw | grep -q -- "-F _aisw"
'
