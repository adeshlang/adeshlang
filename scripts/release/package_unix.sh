#!/usr/bin/env bash
set -euo pipefail

# Usage: ./package_unix.sh [version] [arch: x86_64|aarch64|arm64] [os: linux|macos|darwin] [target_triple]

version="${1:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)}"
raw_arch="${2:-$(uname -m)}"
raw_os="${3:-$(uname -s | tr '[:upper:]' '[:lower:]')}"

case "$raw_arch" in
  arm64|aarch64) arch="aarch64" ;;
  x86_64|amd64) arch="x86_64" ;;
  *) arch="$raw_arch" ;;
esac

case "$raw_os" in
  darwin|macos) os="macos" ;;
  linux) os="linux" ;;
  *) os="$raw_os" ;;
esac

target="${4:-}"
if [[ -z "$target" ]]; then
  if [[ "$os" == "macos" ]]; then
    if [[ "$arch" == "aarch64" ]]; then target="aarch64-apple-darwin"; else target="x86_64-apple-darwin"; fi
  elif [[ "$os" == "linux" ]]; then
    if [[ "$arch" == "aarch64" ]]; then target="aarch64-unknown-linux-gnu"; else target="x86_64-unknown-linux-gnu"; fi
  fi
fi

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
if [[ "$os" == "linux" ]]; then
  # Keep a package-friendly staging path; the archive gets a release-specific
  # top-level directory below.
  stage="$root/dist/linux-$arch"
  archive_stage="$root/dist/AdeshLang-$version-$arch-$os-gnu"
else
  # macOS packaging (build-pkg.sh, build-dmg.sh) expects dist/macos-<arch>;
  # the release archive gets the versioned top-level directory below.
  stage="$root/dist/macos-$arch"
  archive_stage="$root/dist/AdeshLang-$version-$os-$arch"
fi
rm -rf "$stage"
if [[ "$archive_stage" != "$stage" ]]; then
  rm -rf "$archive_stage"
fi
mkdir -p "$stage/bin" "$stage/std" "$stage/config" "$stage/toolchain/llvm/bin" \
  "$stage/lib" "$stage/include" "$stage/share" "$stage/licenses" "$stage/THIRD_PARTY_LICENSES"

target_flags=()
# CARGO_TARGET_DIR lets containerized/cross builds keep their artifacts away
# from the host's target/ tree (e.g. a Linux Docker build of a Windows host
# checkout, which would otherwise mix ELF and PE files in target/release).
cargo_target_root="${CARGO_TARGET_DIR:-$root/target}"
target_dir="$cargo_target_root/release"
if [[ -n "$target" ]]; then
  target_flags=("--target" "$target")
  target_dir="$cargo_target_root/$target/release"
fi

echo "Building AdeshLang $version for $os-$arch (target: ${target:-native})..."
cargo build --release "${target_flags[@]}" --bin adesh --bin adl
if [[ -f "$root/als/Cargo.toml" ]]; then
  (cd "$root/als" && cargo build --release "${target_flags[@]}" --bin als)
fi
if [[ -f "$root/editor/Cargo.toml" ]]; then
  (cd "$root/editor" && cargo build --release "${target_flags[@]}" --bin adesh-editor)
fi

cp "$target_dir/adesh" "$stage/bin/"
[[ -f "$target_dir/adl" ]] && cp "$target_dir/adl" "$stage/bin/"
als_bin="$root/als/target/release/als"
[[ -n "$target" && -f "$root/als/target/$target/release/als" ]] && als_bin="$root/als/target/$target/release/als"
[[ ! -f "$als_bin" && -f "$target_dir/als" ]] && als_bin="$target_dir/als"
[[ -f "$als_bin" ]] && cp "$als_bin" "$stage/bin/"

editor_bin="$root/editor/target/release/adesh-editor"
[[ -n "$target" && -f "$root/editor/target/$target/release/adesh-editor" ]] && editor_bin="$root/editor/target/$target/release/adesh-editor"
[[ ! -f "$editor_bin" && -f "$target_dir/adesh-editor" ]] && editor_bin="$target_dir/adesh-editor"
[[ -f "$editor_bin" ]] && cp "$editor_bin" "$stage/bin/"

cp "$root/LICENSE" "$stage/licenses/AdeshLang-LICENSE"
cp "$root/LICENSE" "$stage/THIRD_PARTY_LICENSES/AdeshLang-LICENSE"
if [[ -d "$root/THIRD_PARTY_LICENSES" ]]; then
  cp -a "$root/THIRD_PARTY_LICENSES/." "$stage/licenses/"
  cp -a "$root/THIRD_PARTY_LICENSES/." "$stage/THIRD_PARTY_LICENSES/"
fi
cp "$root"/src/stdlib/*.adesh "$stage/std/"
cp "$root/installer/manifests/toolchain-manifest.json" "$stage/config/toolchain-manifest.json"
printf '%s\n' "$version" > "$stage/VERSION"

# ---------------------------------------------------------------------------
# AI model staging: bundle the default quantized model (adesh-coder-0.5b q4_0,
# ~275 MB) plus the Ollama/OpenRouter deployment configs. The local checkout
# wins; otherwise the pinned URL from ai/models/manifest.json is downloaded
# and verified against that manifest's SHA-256. ADESH_SKIP_AI_MODEL=1 builds
# a model-less distribution.
# ---------------------------------------------------------------------------
if [[ "${ADESH_SKIP_AI_MODEL:-0}" == "1" ]]; then
  echo "Warning: ADESH_SKIP_AI_MODEL=1; the AI model will not be bundled." >&2
else
  ai_manifest="$root/ai/models/manifest.json"
  if [[ ! -f "$ai_manifest" ]]; then
    echo "error: ai/models/manifest.json is missing; it pins the AI model URL and SHA-256." >&2
    echo "       (Re-run with ADESH_SKIP_AI_MODEL=1 for a model-less distribution.)" >&2
    exit 1
  fi
  # Parse the Q4_0 artifact entry without python. Field order inside each
  # artifact object is filename, quantization, size, sha256, url — so the
  # filename must be remembered from before the quantization marker, while
  # sha256/url are only trusted from the marker onward.
  q4_filename_line=""
  q4_sha256_line=""
  q4_url_line=""
  q4_seen=0
  while IFS= read -r line; do
    case "$line" in
      *'"filename"'*)
        if (( q4_seen )); then break; fi
        q4_filename_line="$line"
        ;;
      *'"quantization": "Q4_0"'*) q4_seen=1 ;;
      *'"sha256"'*)
        if (( q4_seen )) && [[ -z "$q4_sha256_line" ]]; then q4_sha256_line="$line"; fi
        ;;
      *'"url"'*)
        if (( q4_seen )) && [[ -z "$q4_url_line" ]]; then q4_url_line="$line"; fi
        ;;
    esac
    if (( q4_seen )) && [[ -n "$q4_sha256_line" && -n "$q4_url_line" ]]; then
      break
    fi
  done < "$ai_manifest"
  q4_filename="$(printf '%s' "$q4_filename_line" | sed 's/.*: *"\([^"]*\)".*/\1/')"
  q4_sha256="$(printf '%s' "$q4_sha256_line" | sed 's/.*: *"\([^"]*\)".*/\1/')"
  q4_url="$(printf '%s' "$q4_url_line" | sed 's/.*: *"\([^"]*\)".*/\1/')"
  if [[ -z "${q4_filename:-}" || -z "${q4_sha256:-}" || -z "${q4_url:-}" ]]; then
    echo "error: ai/models/manifest.json does not describe a Q4_0 artifact." >&2
    exit 1
  fi

  local_model="$root/ai/models/$q4_filename"
  if [[ ! -f "$local_model" ]]; then
    echo "Local $q4_filename not found; downloading from the manifest URL..."
    command -v curl >/dev/null 2>&1 || {
      echo "error: curl is required to fetch the AI model." >&2
      exit 1
    }
    if ! curl -fL --retry 3 --connect-timeout 15 "$q4_url" -o "$local_model"; then
      echo "error: could not download the AI model from $q4_url" >&2
      exit 1
    fi
  fi

  if command -v sha256sum >/dev/null 2>&1; then
    actual_sha256="$(sha256sum "$local_model" | cut -d' ' -f1)"
  else
    actual_sha256="$(shasum -a 256 "$local_model" | cut -d' ' -f1)"
  fi
  # Lowercase both hashes portably (macOS still ships bash 3.2, which has no
  # ${var,,} expansion).
  actual_lc="$(printf '%s' "$actual_sha256" | tr '[:upper:]' '[:lower:]')"
  expect_lc="$(printf '%s' "$q4_sha256" | tr '[:upper:]' '[:lower:]')"
  if [[ "$actual_lc" != "$expect_lc" ]]; then
    echo "error: SHA-256 mismatch for $q4_filename (expected $q4_sha256, got $actual_sha256)." >&2
    exit 1
  fi
  echo "AI model verified against manifest (SHA-256 ok)."

  mkdir -p "$stage/ai/models" "$stage/ai/deploy/ollama" "$stage/ai/deploy/openrouter"
  cp "$local_model" "$stage/ai/models/$q4_filename"
  cp "$ai_manifest" "$stage/ai/models/manifest.json"
  [[ -f "$root/ai/deploy/ollama/Modelfile" ]] && \
    cp "$root/ai/deploy/ollama/Modelfile" "$stage/ai/deploy/ollama/Modelfile"
  [[ -f "$root/ai/deploy/openrouter/config.json" ]] && \
    cp "$root/ai/deploy/openrouter/config.json" "$stage/ai/deploy/openrouter/config.json"
fi

mkdir -p "$root/dist"
if [[ "$archive_stage" != "$stage" ]]; then
  mkdir -p "$archive_stage"
  cp -a "$stage/." "$archive_stage/"
fi

if [[ "$os" == "linux" ]]; then
  archive="$root/dist/AdeshLang-$version-$arch-$os-gnu.tar.xz"
else
  archive="$root/dist/AdeshLang-$version-$os-$arch.tar.xz"
fi
tar -C "$root/dist" -cJf "$archive" "$(basename "$archive_stage")"
echo "Successfully created $archive"

if [[ "$os" == "linux" ]]; then
  if command -v dpkg-deb >/dev/null 2>&1; then
    deb_arch="amd64"
    [[ "$arch" == "aarch64" ]] && deb_arch="arm64"
    bash "$root/installer/linux/deb/build-deb.sh" "$deb_arch" "$version"
  else
    echo "Warning: dpkg-deb not found; skipping Debian package build." >&2
  fi
  if command -v rpmbuild >/dev/null 2>&1; then
    bash "$root/installer/linux/rpm/build-rpm.sh" "$arch" "$version"
  else
    echo "Warning: rpmbuild not found; skipping RPM package build." >&2
  fi
fi
