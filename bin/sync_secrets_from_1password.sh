#!/usr/bin/env bash
# Copies release signing secrets from a 1Password item into the GitHub
# "release" environment, which only the release workflow can use.
set -euo pipefail

readonly ENVIRONMENT=release
readonly SECRETS=(
  APPLE_CERTIFICATE_P12_BASE64
  APPLE_CERTIFICATE_PASSWORD
  APPLE_API_ISSUER_ID
  APPLE_API_KEY_ID
  APPLE_API_KEY_P8_BASE64
)

usage() {
  cat <<'USAGE'
Usage: bin/sync_secrets_from_1password.sh [--dry-run] [--repo owner/repo] op://VAULT/ITEM

Fields in the 1Password item must be named after the secrets:
  APPLE_CERTIFICATE_P12_BASE64   Developer ID Application .p12, base64
  APPLE_CERTIFICATE_PASSWORD     its password
  APPLE_API_ISSUER_ID            App Store Connect API issuer ID
  APPLE_API_KEY_ID               App Store Connect API key ID
  APPLE_API_KEY_P8_BASE64        the key's .p8, base64
USAGE
}

die() {
  echo "$1" >&2
  exit 1
}

parse_args() {
  dry_run=0
  repo=
  item=
  while [ "$#" -gt 0 ]; do
    case "$1" in
      -h | --help) usage; exit 0 ;;
      --dry-run) dry_run=1; shift ;;
      --repo) [ "$#" -ge 2 ] || die "--repo needs a value"; repo=$2; shift 2 ;;
      --repo=*) repo=${1#--repo=}; shift ;;
      -*) usage >&2; exit 1 ;;
      *) [ -z "$item" ] || die "one 1Password item only"; item=${1%/}; shift ;;
    esac
  done
  [ -n "$item" ] || { usage >&2; exit 1; }
}

check_dependencies() {
  command -v op >/dev/null || die "1Password CLI is required: op"
  command -v gh >/dev/null || die "GitHub CLI is required: gh"
}

resolve_repo() {
  [ -n "$repo" ] || repo=$(gh repo view --json nameWithOwner -q .nameWithOwner)
}

# Only tags matching v* may deploy to the environment.
ensure_environment() {
  gh api -X PUT "repos/$repo/environments/$ENVIRONMENT" \
    --input - >/dev/null <<'JSON'
{"deployment_branch_policy": {"protected_branches": false, "custom_branch_policies": true}}
JSON
  if ! gh api "repos/$repo/environments/$ENVIRONMENT/deployment-branch-policies" \
    -q '.branch_policies[] | select(.type == "tag") | .name' | grep -qx 'v\*'; then
    gh api -X POST "repos/$repo/environments/$ENVIRONMENT/deployment-branch-policies" \
      -f name='v*' -f type=tag >/dev/null
  fi
}

read_secret() {
  op read "$item/$1"
}

dry_run_secrets() {
  local s
  for s in "${SECRETS[@]}"; do
    read_secret "$s" >/dev/null
    echo "Would set $s in $repo/$ENVIRONMENT."
  done
  echo "Dry run succeeded. Nothing changed."
}

sync_secrets() {
  local s
  ensure_environment
  for s in "${SECRETS[@]}"; do
    echo "Setting $s..."
    read_secret "$s" | gh secret set "$s" --repo "$repo" --env "$ENVIRONMENT"
  done
  echo "$repo/$ENVIRONMENT secrets are up to date."
}

main() {
  parse_args "$@"
  check_dependencies
  resolve_repo
  if [ "$dry_run" -eq 1 ]; then
    dry_run_secrets
  else
    sync_secrets
  fi
}

main "$@"
