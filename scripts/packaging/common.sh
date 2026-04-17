# shellcheck shell=bash
#
# Common helper functions shared by packaging scripts.
#
# Source this file; do NOT execute it directly.  It intentionally does not set
# shell options (set -euo pipefail) so that callers remain in control.

log_info()  { printf "[info] %s\n" "$*"; }
log_warn()  { printf "[warn] %s\n" "$*" >&2; }
log_error() { printf "[error] %s\n" "$*" >&2; }
die()       { log_error "$*"; exit 1; }

require_cmd() {
  local cmd="$1"
  command -v "$cmd" >/dev/null 2>&1 || die "Missing required command: ${cmd}"
}

repo_root() {
  git rev-parse --show-toplevel 2>/dev/null
}

# cargo_version prints the version field from a Cargo.toml file.
cargo_version() {
  local cargo_toml="$1"
  grep '^version' "$cargo_toml" | head -1 | sed 's/.*"\(.*\)"/\1/'
}

# resolve_tag determines the release tag used for artifact naming.
#
# Precedence:
#   1) TAG environment variable (e.g. v0.2.0)
#   2) Most recent git tag (git describe --tags --abbrev=0)
#   3) Cargo.toml version prefixed with "v"
#
# Sets global variables:
#   TAG         e.g. v0.2.0
#   VERSION_NUM e.g. 0.2.0
resolve_tag() {
  if [ -z "${TAG:-}" ]; then
    TAG="$(git describe --tags --abbrev=0 2>/dev/null || true)"
  fi

  if [ -z "${TAG:-}" ]; then
    local root
    root="$(repo_root)"
    [ -n "$root" ] || die "Not in a git repository; set TAG=vX.Y.Z"

    if [ -f "$root/Cargo.toml" ]; then
      local v
      v="$(cargo_version "$root/Cargo.toml")"
      if [ -n "$v" ] && [ "$v" != "0.0.0" ]; then
        TAG="v${v}"
      fi
    fi
  fi

  [ -n "${TAG:-}" ] || die "Could not determine release tag; set TAG=vX.Y.Z"
  VERSION_NUM="${TAG#v}"
}

# ensure_fresh_dist_dir creates dist_dir, falling back to a timestamped
# alternative if the directory already exists (avoids permission issues
# from prior runs that used sudo or a different user).
ensure_fresh_dist_dir() {
  local dist_dir="$1"
  if [ -d "$dist_dir" ]; then
    local ts
    ts="$(date +"%Y%m%d-%H%M%S")"
    log_warn "Existing dist directory (${dist_dir}) detected; using ${dist_dir}-${ts}"
    dist_dir="${dist_dir}-${ts}"
  fi
  mkdir -p "$dist_dir"
  printf "%s" "$dist_dir"
}

# write_sha256sums writes a SHA256SUMS manifest for all files in out_dir.
#
# Arguments:
#   1) out_dir       Directory to scan.
#   2) manifest_name Filename to write (e.g. agent-eval-v0.1.0-SHA256SUMS.txt).
write_sha256sums() {
  local out_dir="$1"
  local manifest_name="$2"

  (cd "$out_dir" || exit 1
    shopt -s nullglob

    local files=()
    local f
    for f in *; do
      [ -f "$f" ] || continue
      [ "$f" = "$manifest_name" ] && continue
      files+=("$f")
    done

    if [ ${#files[@]} -eq 0 ]; then
      log_warn "No files found in ${out_dir}; skipping checksum manifest"
      exit 0
    fi

    if command -v sha256sum >/dev/null 2>&1; then
      sha256sum "${files[@]}" > "$manifest_name"
    elif command -v shasum >/dev/null 2>&1; then
      shasum -a 256 "${files[@]}" > "$manifest_name"
    else
      log_warn "Neither sha256sum nor shasum found; skipping checksum manifest"
      exit 0
    fi
  )
}
