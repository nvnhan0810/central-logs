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
# fnm is checked by absolute path too: its binary usually lives in ~/.local/share/fnm,
# which is not on the runner's PATH even though the install itself is fine.
ensure_npm() {
  if command -v npm >/dev/null 2>&1; then
    return
  fi
  local fnm_dir="${FNM_DIR:-$HOME/.local/share/fnm}"
  local fnm_bin=""
  if command -v fnm >/dev/null 2>&1; then
    fnm_bin="fnm"
  elif [[ -x "$fnm_dir/fnm" ]]; then
    fnm_bin="$fnm_dir/fnm"
  fi
  if [[ -n "$fnm_bin" ]]; then
    eval "$("$fnm_bin" env --shell bash)" || true
  fi
  if ! command -v npm >/dev/null 2>&1 && [[ -x "$fnm_dir/aliases/default/bin/npm" ]]; then
    PATH="$fnm_dir/aliases/default/bin:$PATH"
    export PATH
  fi
  if ! command -v npm >/dev/null 2>&1 && [[ -s "${NVM_DIR:-$HOME/.nvm}/nvm.sh" ]]; then
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
