#!/usr/bin/env bash
# Runs the Rotproof on PATH as a user runs it: release.yml calls it after installing a wheel on each platform, and in
# the oldest Linux the Linux wheel's tag claims. A fresh project passes, and the same project with a forbidden import
# fails with exit 1 (a crash exits otherwise)
set -euo pipefail

rotproof --version
project=$(mktemp -d)
rotproof --root "$project" init --stack python
rotproof --root "$project" create --yes
rotproof --root "$project" check

echo "import infrastructure" > "$project/domain/rules.py"
status=0
rotproof --root "$project" check || status=$?
if [ "$status" -ne 1 ]; then
  echo "rotproof check exited $status on a forbidden import, not 1"
  exit 1
fi
echo "the forbidden import failed, as it should"
