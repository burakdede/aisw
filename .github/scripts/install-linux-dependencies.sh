#!/usr/bin/env bash
set -euo pipefail

# GitHub-hosted images may preconfigure third-party APT sources. Restrict the
# update to Ubuntu's source file so an unrelated vendor index cannot block CI.
ubuntu_sources=$(find /etc/apt/sources.list.d -maxdepth 1 -type f -name 'ubuntu.sources' -print -quit)
if [[ -z "$ubuntu_sources" ]]; then
  ubuntu_sources=/etc/apt/sources.list
fi

apt_options=(
  -o "Dir::Etc::sourcelist=$ubuntu_sources"
  -o "Dir::Etc::sourceparts=-"
)

sudo apt-get "${apt_options[@]}" update
sudo apt-get "${apt_options[@]}" install -y "$@"
