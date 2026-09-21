#!/usr/bin/env bash
# AdeshLang macOS installer — the curl | bash path (also the reference the
# DMG's Install-AdeshLang.command mirrors, minus the download step).
#
# Usage:
#   curl -fsSL https://github.com/adeshlang/adeshlang/releases/download/v0.3.0/install-macos.sh | bash
#   bash install.sh [--user] [--yes] [--skip-toolchain] [--with-gpu-source-build]
#                   [--use-system-packages] [--install-dir DIR]
#
# CI must publish THIS file as `install-macos.sh` on the v0.3.0 release, next
# to the core tarball AdeshLang-0.3.0-macos-<arch>.tar.xz, for the one-liner
# above to work.
#
# Flags:
#   --user                  install under ~/.adeshlang, link into ~/.local/bin
#                           (no sudo)
#   --yes / -y              answer yes to every prompt
#   --skip-toolchain        do not offer/run the LLVM toolchain install
#   --with-gpu-source-build also build the MLIR GPU tools (mlir-opt /
#                           mlir-translate with NVVM/ROCDL) from upstream
#                           source — 30–90 minutes; requires cmake, ninja and
#                           the Xcode Command Line Tools. Opt-in.
#   --use-system-packages   install LLVM 18 via Homebrew (llvm@18) instead of
#                           downloading the pinned tarball
#   --install-dir DIR       override the destination directory
#
# The default (system) installs to /Library/Application Support/AdeshLang,
# symlinks the CLI tools into /usr/local/bin and writes
# /etc/profile.d/adeshlang.sh. Idempotent: safe to re-run.
set -euo pipefail

# The single source of truth for the GitHub repository hosting releases.
# Mirrors OFFICIAL_REPO in src/toolchain/manifest.rs; ADESH_REPO overrides it
# at runtime for pre-migration testing. Update both together.
ADESH_REPO="${ADESH_REPO:-adeshlang/adeshlang}"
VERSION="0.3.0"
RELEASE_TAG="v${VERSION}"

MODE="system"
YES=0
SKIP_TOOLCHAIN=0
WITH_GPU=0
USE_SYSTEM_PACKAGES=0
INSTALL_DIR_FLAG=""

usage() {
    cat <<'EOF'
Usage: install.sh [--user] [--yes] [--skip-toolchain]
                  [--with-gpu-source-build] [--use-system-packages]
                  [--install-dir DIR]

  --user                  install under ~/.adeshlang (no sudo)
  --yes / -y              answer yes to every prompt
  --skip-toolchain        do not offer/run the LLVM toolchain install
  --with-gpu-source-build also build MLIR GPU tools from source (30-90 min,
                          needs cmake, ninja, Xcode CLT) — opt-in
  --use-system-packages   install LLVM 18 via Homebrew (llvm@18)
  --install-dir DIR       override the destination directory

Ends with `adesh doctor` to verify the installation.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --user) MODE="user"; shift ;;
        --system) MODE="system"; shift ;;
        --yes|-y) YES=1; shift ;;
        --skip-toolchain) SKIP_TOOLCHAIN=1; shift ;;
        --with-gpu-source-build) WITH_GPU=1; shift ;;
        --use-system-packages) USE_SYSTEM_PACKAGES=1; shift ;;
        --install-dir)
            if [[ $# -lt 2 ]]; then
                echo "error: --install-dir requires a path" >&2
                exit 2
            fi
            INSTALL_DIR_FLAG="$2"
            shift 2
            ;;
        --install-dir=*) INSTALL_DIR_FLAG="${1#*=}"; shift ;;
        -h|--help) usage; exit 0 ;;
        *) echo "error: unknown option: $1" >&2; usage; exit 2 ;;
    esac
done

# --- Platform checks ---------------------------------------------------------
case "$(uname -s)" in
    Darwin|darwin) ;;
    *) echo "error: this installer is for macOS only" >&2; exit 1 ;;
esac

case "$(uname -m)" in
    arm64|aarch64) ARCH="arm64" ;;
    x86_64|amd64)  ARCH="x86_64" ;;
    *) echo "error: unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

MACOS_VERSION="$(sw_vers -productVersion)"
MACOS_MAJOR="${MACOS_VERSION%%.*}"
if [[ -z "$MACOS_MAJOR" || "$MACOS_MAJOR" -lt 12 ]]; then
    echo "error: AdeshLang requires macOS 12 or newer (found: $MACOS_VERSION)" >&2
    exit 1
fi

if ! xcode-select -p >/dev/null 2>&1; then
    cat <<'MSG'

Xcode Command Line Tools are not installed.

AdeshLang needs them to drive clang/lld. Install them with:

    xcode-select --install

then re-run this installer.
MSG
    exit 1
fi

# --- Destinations ----------------------------------------------------------
if [[ "$MODE" == "user" ]]; then
    INSTALL_DIR="${INSTALL_DIR_FLAG:-$HOME/.adeshlang}"
    BIN_LINK_DIR="$HOME/.local/bin"
else
    INSTALL_DIR="${INSTALL_DIR_FLAG:-/Library/Application Support/AdeshLang}"
    BIN_LINK_DIR="/usr/local/bin"
fi

# Root wrapper: system installs elevate per command, user installs don't.
run_root() {
    if [[ "$MODE" == "system" ]]; then
        sudo "$@"
    else
        "$@"
    fi
}

cat <<'MSG'
------------------------------------------------------------------------------
 Gatekeeper note: binaries downloaded via curl carry the
 com.apple.quarantine attribute, which makes macOS refuse to launch them
 ("damaged"/unidentified). This installer strips quarantine attributes from
 the installed files after extraction (idempotent: xattr -dr
 com.apple.quarantine). The .pkg distribution is notarized and needs no
 such step.
------------------------------------------------------------------------------
MSG

if [[ "$MODE" == "system" ]]; then
    sudo -v   # prompt for the admin password up front
fi

# --- Download the core tarball ----------------------------------------------
TARBALL="AdeshLang-${VERSION}-macos-${ARCH}.tar.xz"
DL_BASE="https://github.com/${ADESH_REPO}/releases/download/${RELEASE_TAG}/"
ARCHIVE_URL="${DL_BASE}${TARBALL}"

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/adesh-install.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT
ARCHIVE="$TMP_DIR/$TARBALL"

echo "==> Downloading $ARCHIVE_URL"
if ! curl -fL -sS --retry 3 -o "$ARCHIVE" "$ARCHIVE_URL"; then
    if [[ "$ARCH" == "arm64" ]]; then
        # Legacy naming: older release builds used aarch64 for macOS arm64.
        ALT_TARBALL="AdeshLang-${VERSION}-macos-aarch64.tar.xz"
        echo "    (not found; trying legacy name $ALT_TARBALL)"
        if ! curl -fL -sS --retry 3 -o "$ARCHIVE" "${DL_BASE}${ALT_TARBALL}"; then
            echo "error: download failed for $ARCHIVE_URL" >&2
            echo "Check that the release exists and that ADESH_REPO resolves;" >&2
            echo "or install LLVM via Homebrew: --use-system-packages" >&2
            exit 1
        fi
    else
        echo "error: download failed for $ARCHIVE_URL" >&2
        exit 1
    fi
fi
echo "    saved: $(du -h "$ARCHIVE" | cut -f1)"

# --- Extract ----------------------------------------------------------------
echo "==> Extracting"
tar -xJf "$ARCHIVE" -C "$TMP_DIR"

# Releases wrap everything in a single versioned top-level directory; be
# tolerant of both naming schemes and of a flat archive.
PAYLOAD_DIR="$TMP_DIR"
for cand in "AdeshLang-$VERSION-macos-$ARCH" "AdeshLang-$VERSION-macos-aarch64"; do
    if [[ -d "$TMP_DIR/$cand" ]]; then
        PAYLOAD_DIR="$TMP_DIR/$cand"
        break
    fi
done
if [[ ! -d "$PAYLOAD_DIR/bin" ]]; then
    echo "error: the release archive has an unexpected layout (no bin/ inside)" >&2
    ls -la "$TMP_DIR" >&2 || true
    exit 1
fi

# --- Install the core payload ------------------------------------------------
echo "==> Installing core to $INSTALL_DIR"
run_root /bin/mkdir -p "$INSTALL_DIR/bin" "$INSTALL_DIR/std" "$INSTALL_DIR/config" "$INSTALL_DIR/licenses"
run_root /usr/bin/ditto "$PAYLOAD_DIR/bin" "$INSTALL_DIR/bin"

# Standard library ships at <home>/std/ (repo source: src/stdlib/*.adesh).
if [[ -d "$PAYLOAD_DIR/std" ]]; then
    run_root /usr/bin/ditto "$PAYLOAD_DIR/std" "$INSTALL_DIR/std"
else
    echo "warning: the release archive contains no std/ (standard library);" >&2
    echo "         \`adesh doctor\` will flag it. Please use a complete release." >&2
fi

# Toolchain manifest v2 ships at <home>/config/toolchain-manifest.json; it
# pins the LLVM 23.1.1 URLs + SHA-256 that `adesh toolchain install` uses.
if [[ -d "$PAYLOAD_DIR/config" ]]; then
    run_root /usr/bin/ditto "$PAYLOAD_DIR/config" "$INSTALL_DIR/config"
else
    echo "warning: the release archive contains no config/ (toolchain manifest);" >&2
    echo "         \`adesh toolchain install\` will fetch the manifest from GitHub." >&2
fi

if [[ -d "$PAYLOAD_DIR/licenses" ]]; then
    run_root /usr/bin/ditto "$PAYLOAD_DIR/licenses" "$INSTALL_DIR/licenses"
elif [[ -d "$PAYLOAD_DIR/THIRD_PARTY_LICENSES" ]]; then
    # Legacy archive naming for the license folder.
    run_root /usr/bin/ditto "$PAYLOAD_DIR/THIRD_PARTY_LICENSES" "$INSTALL_DIR/licenses"
fi

# --- Link the CLI tools -------------------------------------------------------
echo "==> Linking CLI tools into $BIN_LINK_DIR"
run_root /bin/mkdir -p "$BIN_LINK_DIR"
for tool in adesh adl als adesh-editor; do
    if [[ -x "$INSTALL_DIR/bin/$tool" ]]; then
        run_root /bin/ln -sfn "$INSTALL_DIR/bin/$tool" "$BIN_LINK_DIR/$tool"
        echo "    linked $BIN_LINK_DIR/$tool"
    else
        echo "    note: $tool not bundled; skipping"
    fi
done

# --- Shell profile -------------------------------------------------------------
echo "==> Writing shell profile"
if [[ "$MODE" == "system" ]]; then
    sudo /usr/bin/tee /etc/profile.d/adeshlang.sh >/dev/null <<EOF
# AdeshLang environment (managed by the AdeshLang installer; safe to remove)
export ADESH_HOME='$INSTALL_DIR'
export ADESH_STD='$INSTALL_DIR/std'
export ADESH_TOOLCHAIN='$INSTALL_DIR/toolchain/llvm'
export ADESH_CLANG='$INSTALL_DIR/toolchain/llvm/bin/clang'
export ADESH_LLC='$INSTALL_DIR/toolchain/llvm/bin/llc'
export ADESH_MLIR_OPT='$INSTALL_DIR/toolchain/llvm/bin/mlir-opt'
export ADESH_MLIR_TRANSLATE='$INSTALL_DIR/toolchain/llvm/bin/mlir-translate'
export PATH='$INSTALL_DIR/bin:$INSTALL_DIR/toolchain/llvm/bin:\$PATH'
EOF
    echo "    wrote /etc/profile.d/adeshlang.sh"
else
    # User scope: append to ~/.zprofile (the Rust toolchain exposure uses the
    # same file on macOS — see src/toolchain/expose.rs) only if not present.
    PROFILE="$HOME/.zprofile"
    touch "$PROFILE"
    if grep -qsF "# AdeshLang environment" "$PROFILE"; then
        echo "    $PROFILE already configured; leaving it untouched"
    else
        cat >> "$PROFILE" <<EOF

# AdeshLang environment (managed by the AdeshLang installer; safe to remove)
export ADESH_HOME='$INSTALL_DIR'
export ADESH_STD='$INSTALL_DIR/std'
export ADESH_TOOLCHAIN='$INSTALL_DIR/toolchain/llvm'
export ADESH_CLANG='$INSTALL_DIR/toolchain/llvm/bin/clang'
export ADESH_LLC='$INSTALL_DIR/toolchain/llvm/bin/llc'
export ADESH_MLIR_OPT='$INSTALL_DIR/toolchain/llvm/bin/mlir-opt'
export ADESH_MLIR_TRANSLATE='$INSTALL_DIR/toolchain/llvm/bin/mlir-translate'
export PATH='$INSTALL_DIR/bin:$INSTALL_DIR/toolchain/llvm/bin:\$PATH'
EOF
        echo "    appended exports to $PROFILE"
    fi
fi

# --- Gatekeeper: strip quarantine (idempotent) --------------------------------
echo "==> Stripping quarantine attributes from installed files (idempotent)"
run_root /usr/bin/xattr -dr com.apple.quarantine "$INSTALL_DIR/bin" 2>/dev/null || true

# --- Toolchain (LLVM 18.1.8) ----------------------------------------------------
# We offer to download and install the pinned toolchain through `adesh
# toolchain install --system`, which verifies SHA-256 against the manifest.
# Skipped when non-interactive (CI) unless --toolchain is explicitly passed.
ask_toolchain() {
    if [[ "$SKIP_TOOLCHAIN" == "1" ]]; then
        return 1
    fi
    if [[ "$YES" == "1" || "$WITH_GPU" == "1" || "$USE_SYSTEM_PACKAGES" == "1" ]]; then
        return 0
    fi
    local ans=""
    if [[ "${WANT_TOOLCHAIN:-auto}" == "auto" ]]; then
        if [[ "${INTERACTIVE:-1}" == "1" ]]; then
            read -r -p "Download and install pinned LLVM 18.1.8 toolchain now (~190 MB download, ~900 MB disk space)? [Y/n] " ans || ans=""
            ans="${ans:-y}"
        elif [[ -t 0 ]]; then
            read -r -p "Download and install pinned LLVM 18.1.8 toolchain now (~190 MB download, ~900 MB disk space)? [Y/n] " ans < /dev/tty || ans=""
        fi
        case "${ans:-y}" in
            n|N|no) return 1 ;;
            *) return 0 ;;
        esac
    fi
}

install_toolchain() {
    local scope="--system"
    [[ "$MODE" == "user" ]] && scope="--user"
    # Extra flags as a plain string, not an array: bash 3.2 (the macOS
    # default) chokes on empty-array expansions under `set -u`. The values
    # are a fixed set of internal flag constants (no spaces), so the
    # intentional word-split below is safe.
    local mode_flag=""
    if [[ "$USE_SYSTEM_PACKAGES" == "1" ]]; then
        mode_flag="--use-system-packages"
    elif [[ "$WITH_GPU" == "1" ]]; then
        mode_flag="--build-mlir-source"
    fi
    echo "==> Installing pinned LLVM 18.1.8 toolchain (~190 MB compressed download, ~900 MB disk space)"
    if [[ "$MODE" == "system" ]]; then
        sudo ADESH_HOME="$INSTALL_DIR" "$INSTALL_DIR/bin/adesh" toolchain install "$scope" $mode_flag
    else
        ADESH_HOME="$INSTALL_DIR" "$INSTALL_DIR/bin/adesh" toolchain install "$scope" $mode_flag
    fi
    # Freshly downloaded/extracted binaries need the quarantine strip too.
    echo "==> Stripping quarantine attributes from the toolchain (idempotent)"
    run_root /usr/bin/xattr -dr com.apple.quarantine "$INSTALL_DIR/toolchain" 2>/dev/null || true
    if [[ "$WITH_GPU" == "1" ]]; then
        echo "note: MLIR GPU tooling was built from source if required (needs cmake, ninja, Xcode CLT)."
    fi
}

if ask_toolchain; then
    if install_toolchain; then
        echo "    LLVM 18.1.8 toolchain installed and exposed."
    else
        cat <<MSG

warning: the LLVM toolchain install failed.
AdeshLang core is installed and the interpreter/JIT/VM backends work without
it; AOT compilation needs the toolchain. Retry later with:

    sudo '$INSTALL_DIR/bin/adesh' toolchain install --system
    sudo '$INSTALL_DIR/bin/adesh' toolchain install --use-system-packages

(note: if the release manifest lists no SHA-256 for the LLVM archive,
 \`adesh toolchain install\` refuses the download by design until CI publishes
 checksums in installer/manifests/toolchain-manifest.json)
MSG
    fi
else
    if [[ "$SKIP_TOOLCHAIN" == "1" ]]; then
        echo "note: toolchain install skipped (--skip-toolchain)."
    fi
    echo "note: toolchain skipped. Install later with: sudo '$INSTALL_DIR/bin/adesh' toolchain install --system"
fi

# --- Verify -------------------------------------------------------------------
echo
echo "==> Verifying with 'adesh doctor'"
if [[ "$MODE" == "system" ]]; then
    sudo ADESH_HOME="$INSTALL_DIR" "$INSTALL_DIR/bin/adesh" doctor || true
else
    ADESH_HOME="$INSTALL_DIR" "$INSTALL_DIR/bin/adesh" doctor || true
fi

echo
echo "=================================================="
echo " AdeshLang $VERSION installed at: $INSTALL_DIR"
if [[ ! -f "$INSTALL_DIR/ai/models/adesh-coder-0.5b-q4_0.gguf" ]]; then
    echo " • Note: Local AI coder models are not bundled with this installer."
    echo " • To download and set up the default offline AI coder model (~275 MB):"
    echo "     adesh ai setup"
    echo " • Custom AI training requires Python 3.12 (brew install python@3.12)"
fi
echo " • Quick Start: adesh doctor | adesh edit | adesh run hello.adesh"
echo " • Website:     https://adeshlang.org"
echo "=================================================="
exit 0
