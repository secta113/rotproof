#!/usr/bin/env bash
# Runs the Rotproof on PATH as a user runs it: ci.yml and release.yml call it after installing a wheel on each
# platform, and release.yml in the oldest Linux the Linux wheel's tag claims too. A fresh project passes, the same
# project fails with exit 1 (a crash exits otherwise) on a forbidden import and on a day in the log that has not come,
# and passes again once both are taken out
set -euo pipefail

rotproof --version
project=$(mktemp -d)
rotproof --root "$project" init --stack python --ui atomic
rotproof --root "$project" create --yes

# check exits with $1 on the project, or the smoke fails saying what it was doing
expect() {
  local want=$1 doing=$2 status=0
  rotproof --root "$project" check || status=$?
  if [ "$status" -ne "$want" ]; then
    echo "rotproof check exited $status $doing, not $want"
    exit 1
  fi
}

# create leaves nothing for a person to write
expect 0 "on the fresh project"

echo "import infrastructure" > "$project/domain/rules.py"
expect 1 "on a forbidden import"
rm "$project/domain/rules.py"

log=$(cat "$project/docs/log.md")
printf '%s\n\n## 2999-01-01\n\n* **A day guessed rather than read from the clock**\n' "$log" > "$project/docs/log.md"
expect 1 "on a day in the log that has not come"
printf '%s\n' "$log" > "$project/docs/log.md"

expect 0 "once both were taken out"
echo "the fresh project passed, a forbidden import and a day to come failed, and it passed again"
