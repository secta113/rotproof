#!/usr/bin/env bash
# Runs the Rotproof on PATH as a user runs it: ci.yml and release.yml call it after installing a wheel on each
# platform, and release.yml in the oldest Linux the Linux wheel's tag claims too. A fresh project fails until a person
# writes its first milestone, then passes, and the same project with a forbidden import fails with exit 1 (a crash
# exits otherwise)
set -euo pipefail

rotproof --version
project=$(mktemp -d)
rotproof --root "$project" init --stack python
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

# create writes a milestone whose condition only a person can write, so the fresh project fails until one does
expect 1 "on the milestone create left for a person"
sed -i 's/^areas = \[\]/areas = ["app"]/' "$project/.config/rotproof.toml"
cat > "$project/docs/work/next-milestone.md" <<'EOF'
---
type: Milestone
title: The first release
description: The first version of the project is out.
tags: [app]
status: stable
---

# Condition

The release workflow of tag v0.1.0 passes every job.
EOF
rotproof --root "$project" index
expect 0 "once a person wrote the milestone"

echo "import infrastructure" > "$project/domain/rules.py"
expect 1 "on a forbidden import"
echo "the fresh project failed until its milestone was written, then passed, and the forbidden import failed"
