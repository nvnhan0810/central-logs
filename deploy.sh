#!/usr/bin/env bash
# Deploy Central Logs from the main branch.
#
# Steps:
#   1. Pull the main branch from origin.
#   2. backend:  make build, make migrate, restart the supervisor program central-logs.
#   3. frontend: npm install, npm run build.
#
# The script stops at the first failing step.
# Run it from anywhere: it changes to the repository root itself.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BRANCH="main"

log() {
  printf '\n==> %s\n' "$*"
}

cd "$REPO_ROOT"

log "Pulling $BRANCH from origin"
git fetch origin "$BRANCH"
git checkout "$BRANCH"
git pull --ff-only origin "$BRANCH"

log "Backend: build and migrate"
cd "$REPO_ROOT/backend"
make build
make migrate

log "Backend: restarting central-logs"
sudo -n -u nvnhan0810 supervisorctl restart central-logs

log "Frontend: install and build"
cd "$REPO_ROOT/frontend"
npm install
npm run build

log "Deploy finished"
