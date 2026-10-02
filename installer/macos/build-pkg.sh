#!/usr/bin/env bash
# Builds the signed/notarizable macOS .pkg installer for AdeshLang.
#
# Usage:
#   ./installer/macos/build-pkg.sh [arm64|x86_64]
#
#   ARCH  defaults to the host architecture (uname -m).
#
# The pkg bundles the complete Adesh native toolchain (native codegen, ADOB
# object format, adeshlink linker, adeshlang runtime library, stdlib): no
# external LLVM/Clang/MSVC is downloaded or required during install. The
# legacy --with-toolchain flag is accepted and ignored (kept for CI compat).
# External LLVM remains an opt-in bridge the user registers after install
# with `adesh toolchain --external install`.
#
# Environment:
#   ADESH_SIGNING_IDENTITY   Developer ID Installer identity. When set, the
#                            product is signed ("productbuild --sign").
#   ADESH_NOTARY_PROFILE     notarytool keychain profile. When set (with or
#                            without ADESH_SIGNING_IDENTITY), the finished pkg
#                            is submitted to Apple's notary service (--wait)
#                            and stapled. Intended for CI; needs Developer ID
#                            credentials + network access.
#
# Prerequisites (build machine): macOS 12+, Xcode Command Line Tools
# (pkgbuild, productbuild, hdiutil), and a staged dist directory
# dist/macos-<arch>/ containing at least bin/ (see also
# scripts/release/package_unix.sh and build_windows_dist.ps1 for the dist
# convention; std/, config/, licenses/ are filled from repo sources when the
# dist dir lacks them).
#
# Output: dist/AdeshLang-0.3.0-<arch>-macos.pkg  |  arch in {arm64, x86_64}
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: build-pkg.sh [ARCH]
  ARCH  arm64 (default: host) or x86_64

The native toolchain is bundled inside the pkg; nothing external is
downloaded during installation. (--with-toolchain is a deprecated no-op.)

Environment:
  ADESH_SIGNING_IDENTITY  Developer ID Installer identity -> productbuild --sign
  ADESH_NOTARY_PROFILE    notarytool keychain profile     -> notarize + staple

Output: dist/AdeshLang-0.3.0-<arch>-macos.pkg
EOF
}

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPTS_DIR="$ROOT/installer/macos/scripts"

ARCH=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --with-toolchain)
            # Deprecated no-op: the native toolchain is bundled with the pkg.
            echo "note: --with-toolchain is deprecated and ignored; the native toolchain is bundled." >&2
            shift ;;
        -h|--help) usage; exit 0 ;;
        -*) echo "unknown option: $1" >&2; usage; exit 2 ;;
        *)
            if [[ -n "$ARCH" ]]; then
                echo "error: architecture given twice: $ARCH and $1" >&2
                exit 2
            fi
            ARCH="$1"
            shift
            ;;
    esac
done

if [[ -z "$ARCH" ]]; then
    case "$(uname -m)" in
        arm64|aarch64) ARCH="arm64" ;;
        x86_64|amd64)  ARCH="x86_64" ;;
        *) echo "error: unsupported architecture: $(uname -m) (pass arm64 or x86_64)" >&2; exit 2 ;;
    esac
fi

# Version: from the workspace Cargo.toml (like scripts/release/package_unix.sh).
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -n1)"
[[ -n "$VERSION" ]] || VERSION="0.3.0"

DIST_DIR="$ROOT/dist/macos-$ARCH"
if [[ ! -d "$DIST_DIR/bin" ]]; then
    echo "error: expected a staged dist directory at $DIST_DIR (bin/ missing)." >&2
    echo "The macOS release pipeline must produce dist/macos-arm64 and" >&2
    echo "dist/macos-x86_64 with bin/, std/, config/ and licenses/." >&2
    echo "(See installer/macos/build-dmg.sh for the same staging layout.)" >&2
    exit 1
fi

OUT_DIR="$ROOT/dist"
OUT_PKG="$OUT_DIR/AdeshLang-$VERSION-$ARCH-macos.pkg"
mkdir -p "$OUT_DIR"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/adesh-pkg.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

# ---------------------------------------------------------------------------
# 1. Stage the payload. The pkgbuild --root is the future content of
#    /Library/Application Support/AdeshLang, so bin/ etc. sit at the root and
#    --install-location carries the (space-containing) destination path.
# ---------------------------------------------------------------------------
PAYLOAD="$WORK/payload"
mkdir -p "$PAYLOAD/bin" "$PAYLOAD/std" "$PAYLOAD/config" "$PAYLOAD/licenses" "$PAYLOAD/lib"

echo "==> Staging payload from $DIST_DIR"

# bin/ — the CLI tools including the native linker (ditto preserves the
# executable bit).
/usr/bin/ditto "$DIST_DIR/bin" "$PAYLOAD/bin"
if [[ ! -x "$PAYLOAD/bin/adesh" ]]; then
    echo "error: $PAYLOAD/bin/adesh is missing or not executable." >&2
    exit 1
fi
if [[ ! -x "$PAYLOAD/bin/adeshlink" ]]; then
    echo "warning: adeshlink (native linker CLI) is not in $DIST_DIR/bin;" >&2
    echo "         builds still link through the in-process linker engine." >&2
fi
chmod +x "$PAYLOAD/bin/"* 2>/dev/null || true

# lib/ — the adeshlang runtime library (libadeshlang.a / .dylib), staged by
# scripts/release/package_unix.sh.
if [[ -d "$DIST_DIR/lib" ]]; then
    /usr/bin/ditto "$DIST_DIR/lib" "$PAYLOAD/lib"
else
    echo "warning: no lib/ staged in $DIST_DIR; the runtime library is not bundled." >&2
fi

# std/ — prefer the dist's copy; fall back to the repo's standard library
# sources (they ship at <home>/std/ for the compiler).
if [[ -d "$DIST_DIR/std" ]]; then
    /usr/bin/ditto "$DIST_DIR/std" "$PAYLOAD/std"
else
    shopt -s nullglob
    for f in "$ROOT"/src/stdlib/*.adesh; do
        cp "$f" "$PAYLOAD/std/"
    done
    shopt -u nullglob
    if [[ -z "$(ls -A "$PAYLOAD/std")" ]]; then
        echo "warning: no standard library staged (expected src/stdlib/*.adesh)" >&2
    fi
fi

# config/ — toolchain manifest v2 consumed by `adesh toolchain install`
# (it pins LLVM 18.1.8 URLs + SHA-256; resolver looks at
# <home>/config/toolchain-manifest.json — see src/toolchain/manifest.rs).
if [[ -f "$DIST_DIR/config/toolchain-manifest.json" ]]; then
    cp "$DIST_DIR/config/toolchain-manifest.json" "$PAYLOAD/config/toolchain-manifest.json"
else
    cp "$ROOT/installer/manifests/toolchain-manifest.json" "$PAYLOAD/config/toolchain-manifest.json"
fi

# licenses/
if [[ -d "$DIST_DIR/licenses" ]]; then
    /usr/bin/ditto "$DIST_DIR/licenses" "$PAYLOAD/licenses"
else
    cp "$ROOT/LICENSE" "$PAYLOAD/licenses/AdeshLang-LICENSE"
    if [[ -d "$ROOT/THIRD_PARTY_LICENSES" ]]; then
        /usr/bin/ditto "$ROOT/THIRD_PARTY_LICENSES" "$PAYLOAD/licenses/THIRD_PARTY_LICENSES"
    fi
fi

# ai/ — the bundled default AI model (ai/models) and the Ollama/OpenRouter
# deployment configs (ai/deploy), staged by package_unix.sh.
if [[ -d "$DIST_DIR/ai" ]]; then
    /usr/bin/ditto "$DIST_DIR/ai" "$PAYLOAD/ai"
elif [[ -f "$ROOT/ai/models/manifest.json" ]]; then
    echo "warning: $DIST_DIR has no ai/ payload; the pkg will not bundle the AI model." >&2
fi

# ---------------------------------------------------------------------------
# 2. Stage package scripts (single core component; the native toolchain is
#    bundled in the payload, so there is no separate toolchain package).
# ---------------------------------------------------------------------------
mkdir -p "$WORK/scripts/core" "$WORK/pkgs"

cp "$SCRIPTS_DIR/postinstall" "$WORK/scripts/core/postinstall"
chmod +x "$WORK/scripts/core/postinstall"

# ---------------------------------------------------------------------------
# 3. Component packages (pkgbuild).
# ---------------------------------------------------------------------------
echo "==> pkgbuild: org.adeshlang.pkg.core"
pkgbuild \
    --root "$PAYLOAD" \
    --scripts "$WORK/scripts/core" \
    --identifier "org.adeshlang.pkg.core" \
    --version "$VERSION" \
    --min-os-version 12.0 \
    --install-location "/Library/Application Support/AdeshLang" \
    --ownership recommended \
    "$WORK/pkgs/core.pkg"

# ---------------------------------------------------------------------------
# 4. Distribution (choices.xml), then productbuild.
# ---------------------------------------------------------------------------
DIST_XML="$WORK/Distribution"
cp "$SCRIPTS_DIR/choices.xml" "$DIST_XML"
# Keep the committed XML's version attribute in sync with the build. The
# pattern only matches version="0.3.0" (safe: nothing else in the file).
sed -i '' -e "s/version=\"0\.3\.0\"/version=\"$VERSION\"/g" "$DIST_XML"
echo "==> using Distribution: $SCRIPTS_DIR/choices.xml"

PB=(productbuild --distribution "$DIST_XML" --package-path "$WORK/pkgs")
if [[ -n "${ADESH_SIGNING_IDENTITY:-}" ]]; then
    echo "==> signing with identity: $ADESH_SIGNING_IDENTITY"
    PB+=(--sign "$ADESH_SIGNING_IDENTITY")
fi
"${PB[@]}" "$OUT_PKG"
echo "created: $OUT_PKG"

# ---------------------------------------------------------------------------
# 5. Notarization stub — CI-owned. Only runs when ADESH_NOTARY_PROFILE is set.
#    `--wait` blocks until Apple reports a status; stapler then glues the
#    ticket onto the pkg so Gatekeeper trusts it on first download.
#    The PKG must be signed for notarization to succeed.
# ---------------------------------------------------------------------------
if [[ -n "${ADESH_NOTARY_PROFILE:-}" ]]; then
    echo "==> notarizing with notarytool (profile: $ADESH_NOTARY_PROFILE)"
    xcrun notarytool submit "$OUT_PKG" --wait --keychain-profile "$ADESH_NOTARY_PROFILE"
    echo "==> stapling notarization ticket"
    xcrun stapler staple "$OUT_PKG"
    echo "notarized and stapled: $OUT_PKG"
else
    cat <<MSG
note: package built${ADESH_SIGNING_IDENTITY:+ and signed}. Notarization is left to CI:
  xcrun notarytool submit "$OUT_PKG" --wait --keychain-profile <profile>
  xcrun stapler staple "$OUT_PKG"
MSG
fi

echo "done. Install with:  open $OUT_PKG"
