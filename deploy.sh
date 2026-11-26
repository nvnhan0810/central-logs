#!/usr/bin/env bash
# Deploy Central Logs from the main branch.
#
# Steps:
#   1. Pull the main branch from origin.
#   2. backend:  make build, make migrate, restart the supervisor program central-logs.
#   3. frontend: npm i --include=dev --no-save, npm run build.
#
# The script stops at the first failing step.
# Run it from anywhere: it changes to the repository root itself.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BRANCH="main"

log() {
  printf '\n==> %s\n' "$*"
}

die() {
  printf 'ERROR: %s\n' "$*" >&2
  exit 1
}

# Non-interactive shells (webhook/CI runners) do not load the login profile, so a
# Node version manager's npm is often missing from PATH. Try the common managers.
ensure_npm() {
  if command -v npm >/dev/null 2>&1; then
    return
  fi
  if command -v fnm >/dev/null 2>&1; then
    eval "$(fnm env --shell bash)"
  elif [[ -s "${NVM_DIR:-$HOME/.nvm}/nvm.sh" ]]; then
    set +u
    # shellcheck disable=SC1091
    . "${NVM_DIR:-$HOME/.nvm}/nvm.sh"
    set -u
  fi
  command -v npm >/dev/null 2>&1 || die "npm not found on PATH ($PATH). Install Node.js or add its bin directory to the deploy environment."
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
sudo -n /usr/bin/supervisorctl -c /etc/supervisor/supervisord.conf restart central-logs

log "Frontend: install and build"
ensure_npm
cd "$REPO_ROOT/frontend"
# --include=dev: build tools (vite, vue-tsc) are devDependencies and must install even if NODE_ENV=production.
# --no-save: keep package.json and package-lock.json unchanged so the next git pull stays fast-forward.
npm i --include=dev --no-save
npm run build

log "Deploy finished"
