#!/usr/bin/env bash
# Builds the drag-install macOS .dmg for AdeshLang.
#
# Usage:
#   ./installer/macos/build-dmg.sh [arm64|x86_64]
#
# Stages a folder "AdeshLang" containing bin/, std/, config/, licenses/ plus
# Install-AdeshLang.command (a Finder-double-clickable installer that copies
# the bundled payload into /Library with a sudo prompt — the offline twin of
# installer/macos/install.sh), then creates the image:
#   - with `create-dmg` when it is installed (Homebrew) for a pretty
#     background / drag layout; the .command is hidden and an /Applications
#     drop link added;
#   - otherwise plain `hdiutil create -format UDZO` (no extra dependencies).
#
# Environment:
#   ADESH_SIGNING_IDENTITY   codesign identity. When set, the .command file is
#                            codesigned before the image is built (see note
#                            below).
#   ADESH_NOTARY_PROFILE     notarytool keychain profile. When set, the built
#                            dmg is submitted to Apple's notary service
#                            (--wait) and stapled. Intended for CI.
#
# Output: dist/AdeshLang-0.3.0-<arch>-macos.dmg  |  arch in {arm64, x86_64}
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: build-dmg.sh [arm64|x86_64]
  ARCH  arm64 (default: host) or x86_64

Environment:
  ADESH_SIGNING_IDENTITY  codesign identity -> sign Install-AdeshLang.command
  ADESH_NOTARY_PROFILE    notarytool profile -> notarize + staple the dmg

Output: dist/AdeshLang-0.3.0-<arch>-macos.dmg
EOF
}

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

ARCH=""
while [[ $# -gt 0 ]]; do
    case "$1" in
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

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -n1)"
[[ -n "$VERSION" ]] || VERSION="0.3.0"

DIST_DIR="$ROOT/dist/macos-$ARCH"
if [[ ! -d "$DIST_DIR/bin" ]]; then
    echo "error: expected a staged dist directory at $DIST_DIR (bin/ missing)." >&2
    echo "The macOS release pipeline must produce dist/macos-arm64 and" >&2
    echo "dist/macos-x86_64 with bin/, std/, config/ and licenses/." >&2
    exit 1
fi

OUT_DIR="$ROOT/dist"
OUT_DMG="$OUT_DIR/AdeshLang-$VERSION-$ARCH-macos.dmg"
mkdir -p "$OUT_DIR"

WORK="$(mktemp -d "${TMPDIR:-/tmp}/adesh-dmg.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

# ---------------------------------------------------------------------------
# 1. Stage the drag-install layout: a folder "AdeshLang" with the payload and
#    the double-clickable installer. Same payload assembly as build-pkg.sh.
# ---------------------------------------------------------------------------
STAGE="$WORK/dmgstage/AdeshLang"
mkdir -p "$STAGE/bin" "$STAGE/std" "$STAGE/config" "$STAGE/licenses"

echo "==> Staging DMG contents from $DIST_DIR"
/usr/bin/ditto "$DIST_DIR/bin" "$STAGE/bin"
if [[ ! -x "$STAGE/bin/adesh" ]]; then
    echo "error: $STAGE/bin/adesh is missing or not executable." >&2
    exit 1
fi
chmod +x "$STAGE/bin/"* 2>/dev/null || true

if [[ -d "$DIST_DIR/std" ]]; then
    /usr/bin/ditto "$DIST_DIR/std" "$STAGE/std"
else
    shopt -s nullglob
    for f in "$ROOT"/src/stdlib/*.adesh; do
        cp "$f" "$STAGE/std/"
    done
    shopt -u nullglob
fi

if [[ -f "$DIST_DIR/config/toolchain-manifest.json" ]]; then
    cp "$DIST_DIR/config/toolchain-manifest.json" "$STAGE/config/toolchain-manifest.json"
else
    cp "$ROOT/installer/manifests/toolchain-manifest.json" "$STAGE/config/toolchain-manifest.json"
fi

if [[ -d "$DIST_DIR/licenses" ]]; then
    /usr/bin/ditto "$DIST_DIR/licenses" "$STAGE/licenses"
else
    cp "$ROOT/LICENSE" "$STAGE/licenses/AdeshLang-LICENSE"
    if [[ -d "$ROOT/THIRD_PARTY_LICENSES" ]]; then
        /usr/bin/ditto "$ROOT/THIRD_PARTY_LICENSES" "$STAGE/licenses/THIRD_PARTY_LICENSES"
    fi
fi

# ai/ — the bundled default AI model (ai/models) and the Ollama/OpenRouter
# deployment configs (ai/deploy), staged by package_unix.sh.
if [[ -d "$DIST_DIR/ai" ]]; then
    /usr/bin/ditto "$DIST_DIR/ai" "$STAGE/ai"
elif [[ -f "$ROOT/ai/models/manifest.json" ]]; then
    echo "warning: $DIST_DIR has no ai/ payload; the dmg will not bundle the AI model." >&2
fi

# The Finder-double-clickable installer (offline install from this DMG).
cp "$ROOT/installer/macos/Install-AdeshLang.command" "$STAGE/Install-AdeshLang.command"
chmod +x "$STAGE/Install-AdeshLang.command"

# ---------------------------------------------------------------------------
# 2. Optional codesigning. Note: codesign refuses plain (non-Mach-O) script
#    files on some macOS versions; when that happens we warn and continue —
#    the binaries inside a drag-install DMG are developer builds and CI signs
#    them at link time when releasing. Notarizing the dmg still works for the
#    container even if this step is a no-op.
# ---------------------------------------------------------------------------
if [[ -n "${ADESH_SIGNING_IDENTITY:-}" ]]; then
    echo "==> codesigning Install-AdeshLang.command"
    if codesign --force --sign "$ADESH_SIGNING_IDENTITY" "$STAGE/Install-AdeshLang.command"; then
        echo "    signed: $STAGE/Install-AdeshLang.command"
    else
        echo "warning: codesign failed for the .command script (plain text file); continuing unsigned." >&2
    fi
fi

# ---------------------------------------------------------------------------
# 3. Create the image. UDZO (bzip2) is the classic distributable format.
# ---------------------------------------------------------------------------
if command -v create-dmg >/dev/null 2>&1; then
    echo "==> create-dmg (pretty background + /Applications drop link)"
    create-dmg \
        --volname "AdeshLang $VERSION" \
        --window-pos 200 120 \
        --window-size 660 400 \
        --icon-size 120 \
        --icon "AdeshLang" 170 200 \
        --app-drop-link 490 200 \
        --hide-extension "Install-AdeshLang.command" \
        "$OUT_DMG" \
        "$WORK/dmgstage"
else
    echo "==> hdiutil create (create-dmg not found; plain UDZO, no extra deps)"
    hdiutil create \
        -volname "AdeshLang $VERSION" \
        -srcfolder "$WORK/dmgstage" \
        -ov \
        -format UDZO \
        "$OUT_DMG"
fi
echo "created: $OUT_DMG"

# ---------------------------------------------------------------------------
# 4. Notarization stub — CI-owned, symmetric with build-pkg.sh. Stapling a
#    dmg works even though the dmg itself is not codesigned (stapler v2
#    supports plain dmgs); the payloads inside still need to be signed for
#    Gatekeeper to pass, which is CI's job at release time.
# ---------------------------------------------------------------------------
if [[ -n "${ADESH_NOTARY_PROFILE:-}" ]]; then
    echo "==> notarizing with notarytool (profile: $ADESH_NOTARY_PROFILE)"
    xcrun notarytool submit "$OUT_DMG" --wait --keychain-profile "$ADESH_NOTARY_PROFILE"
    echo "==> stapling notarization ticket"
    xcrun stapler staple "$OUT_DMG"
    echo "notarized and stapled: $OUT_DMG"
else
    cat <<MSG
note: dmg built${ADESH_SIGNING_IDENTITY:+ with the .command signed}. Notarization is left to CI:
  xcrun notarytool submit "$OUT_DMG" --wait --keychain-profile <profile>
  xcrun stapler staple "$OUT_DMG"
MSG
fi

echo "done. Mount with:  hdiutil attach $OUT_DMG"
