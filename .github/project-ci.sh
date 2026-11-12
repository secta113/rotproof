#!/usr/bin/env bash
# Installs Rotproof from the assets of a release in the directory $1 as the CI rotproof create writes for rust and
# none does (project/release/ci.yml.in), then runs it through smoke.sh. release.yml calls it before publishing: the
# step "Install Rotproof" of that workflow runs as it is written, with gh release download answered from $1, so an
# archive release.yml renames or lays out otherwise fails here, while the release can still be made again
set -euo pipefail

assets=$(cd "$1" && pwd)
template=project/release/ci.yml.in

# The run block of the step, without its indentation: every line from "run: |" until one indented less
script=$(awk '
  /name: Install Rotproof/ { step = 1; next }
  step && !run && /run: \|/ { run = 1; next }
  run && NF && !/^          / { exit }
  run { sub(/^          /, ""); print }
' "$template")
if [ -z "$script" ]; then
  echo "$template has no step \"Install Rotproof\" with a run block"
  exit 1
fi

# gh release download <tag> --repo <repo> --dir <dir> --pattern <glob>..., as gh does it: every pattern matches an
# asset, or it fails
gh() {
  if [ "$1 $2" != "release download" ]; then
    echo "the step runs gh $*: only gh release download is answered here"
    return 1
  fi
  shift 3
  local dir=. patterns=() pattern file matched
  while [ $# -gt 0 ]; do
    case $1 in
      --repo) shift 2 ;;
      --dir) dir=$2; shift 2 ;;
      --pattern) patterns+=("$2"); shift 2 ;;
      *) echo "gh release download $1: not answered here"; return 1 ;;
    esac
  done
  mkdir -p "$dir"
  for pattern in "${patterns[@]}"; do
    matched=0
    for file in "$assets"/$pattern; do
      if [ -f "$file" ]; then cp "$file" "$dir/"; matched=1; fi
    done
    if [ "$matched" -eq 0 ]; then
      echo "no asset of the release matches $pattern"
      return 1
    fi
  done
}

runner=$(mktemp -d)
(
  export RUNNER_TEMP=$runner GITHUB_PATH=$runner/path ROTPROOF_VERSION=the-release-in-$1
  eval "$script"
)
while read -r dir; do PATH="$dir:$PATH"; done < "$runner/path"
bash .github/smoke.sh
