#!/usr/bin/env bash
# sign-mac.sh — sign, notarize, and package the macOS agent-eval CLI binary.
#
# Produces:
#   <dist>/agent-eval-<TAG>-macos-universal.tar.gz   (notarized universal binary)
#   <dist>/agent-eval-<TAG>-macos-SHA256SUMS.txt
#
# Prerequisites (env vars):
#   APPLE_SIGNING_IDENTITY   — "Developer ID Application: …" certificate CN
#   APPLE_ID                 — Apple ID used for notarytool
#   APPLE_PASSWORD           — App-specific password for notarytool
#   APPLE_TEAM_ID            — 10-character Apple Team ID
#
# Usage:
#   ./scripts/sign-mac.sh [--tag TAG] [--dist DIR] [--binary PATH]
#
#   --tag     Release tag (e.g. v0.1.0). Auto-detected from git/Cargo.toml if omitted.
#   --dist    Output directory for packaged artefacts. Default: dist/release.
#   --binary  Path to the pre-built universal binary. Default: dist/stage/agent-eval.
#
# To build and sign in one shot:
#   cargo build --release --target aarch64-apple-darwin
#   cargo build --release --target x86_64-apple-darwin
#   mkdir -p dist/stage
#   lipo -create \
#     target/aarch64-apple-darwin/release/agent-eval \
#     target/x86_64-apple-darwin/release/agent-eval \
#     -output dist/stage/agent-eval
#   ./scripts/sign-mac.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/packaging/common.sh
source "${SCRIPT_DIR}/packaging/common.sh"

# ── Argument parsing ──────────────────────────────────────────────────────────

TAG=""
DIST_DIR="dist/release"
BINARY_PATH="dist/stage/agent-eval"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --tag)    TAG="$2";         shift 2 ;;
    --dist)   DIST_DIR="$2";   shift 2 ;;
    --binary) BINARY_PATH="$2"; shift 2 ;;
    *) die "Unknown argument: $1" ;;
  esac
done

resolve_tag   # sets TAG and VERSION_NUM if TAG is empty

# ── Validate prerequisites ────────────────────────────────────────────────────

require_cmd codesign
require_cmd xcrun
require_cmd lipo
require_cmd tar
require_cmd shasum

: "${APPLE_SIGNING_IDENTITY:?APPLE_SIGNING_IDENTITY must be set}"
: "${APPLE_ID:?APPLE_ID must be set}"
: "${APPLE_PASSWORD:?APPLE_PASSWORD must be set}"
: "${APPLE_TEAM_ID:?APPLE_TEAM_ID must be set}"

[ -f "$BINARY_PATH" ] || die "Binary not found: ${BINARY_PATH} — build first or pass --binary"

log_info "Tag:    ${TAG}"
log_info "Binary: ${BINARY_PATH}"
log_info "Output: ${DIST_DIR}"

mkdir -p "$DIST_DIR"

# ── Sign ──────────────────────────────────────────────────────────────────────

log_info "Signing ${BINARY_PATH} …"
codesign \
  --sign        "$APPLE_SIGNING_IDENTITY" \
  --timestamp \
  --options     runtime \
  --verbose=2 \
  "$BINARY_PATH"

# ── Notarize ──────────────────────────────────────────────────────────────────
# notarytool requires a zip — it does not accept raw binaries or tarballs.
# After submission the notarization ticket is recorded by Apple; Gatekeeper
# will fetch it online when users first run the binary.  Raw binaries cannot
# be stapled (only .app / .dmg / .pkg bundles support stapling).

NOTARIZE_ZIP="$(mktemp -t notarize-XXXXXX).zip"
log_info "Creating notarization archive: ${NOTARIZE_ZIP}"
ditto -c -k --keepParent "$BINARY_PATH" "$NOTARIZE_ZIP"

log_info "Submitting to Apple Notary Service (this may take several minutes) …"
xcrun notarytool submit "$NOTARIZE_ZIP" \
  --apple-id  "$APPLE_ID" \
  --password  "$APPLE_PASSWORD" \
  --team-id   "$APPLE_TEAM_ID" \
  --wait \
  --timeout   30m

rm -f "$NOTARIZE_ZIP"

# ── Verify ────────────────────────────────────────────────────────────────────

log_info "Verifying code signature …"
codesign --verify --deep --strict --verbose=2 "$BINARY_PATH"
# spctl --assess is for .app bundles; plain CLI binaries always return exit 3
# ("does not seem to be an app") even when correctly signed and notarized.
# codesign --verify above is the authoritative check for CLI tools.
log_info "Signature OK"

# ── Package ───────────────────────────────────────────────────────────────────

ARCHIVE="${DIST_DIR}/agent-eval-${TAG}-macos-universal.tar.gz"
log_info "Creating archive: ${ARCHIVE}"
tar -czf "$ARCHIVE" -C "$(dirname "$BINARY_PATH")" "$(basename "$BINARY_PATH")"

# ── Checksums ─────────────────────────────────────────────────────────────────

write_sha256sums "$DIST_DIR" "agent-eval-${TAG}-macos-SHA256SUMS.txt"

log_info "Done. Artefacts written to ${DIST_DIR}/"
ls -lh "${DIST_DIR}/"
