#!/usr/bin/env bash
# AdeshLang drag-install helper for the DMG distribution.
#
# Finder-DOUBLE-CLICKABLE (.command): double-clicking opens Terminal and runs
# this script as the current user; it elevates with sudo when copying into
# /Library. It performs the same install steps as installer/macos/install.sh
# (copy payload, symlink tools, write /etc/profile.d/adeshlang.sh, optional
# LLVM toolchain, `adesh doctor`) but copies from the bundled payload inside
# the DMG instead of downloading from GitHub (offline install).
#
# Files inside a downloaded DMG carry the com.apple.quarantine attribute, so
# after copying we strip it with xattr (idempotent) — identical handling to
# install.sh. The .pkg distribution is notarized and needs no such step.
set -euo pipefail

VERSION="0.3.0"

# This file lives in the "AdeshLang" folder at the root of the DMG, next to
# bin/, std/, config/ and licenses/.
SRC_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEST_DIR="/Library/Application Support/AdeshLang"
LINK_DIR="/usr/local/bin"

echo "=================================================="
echo " AdeshLang $VERSION — macOS installer (from DMG)"
echo "=================================================="
echo
echo "This will install AdeshLang to:"
echo "    $DEST_DIR"
echo "and link its tools into:"
echo "    $LINK_DIR"
echo

# Ask for the admin password up front (sudo -v just validates/timestamps).
sudo -v

# 1. Copy the bundled payload into place (ditto overwrites -> idempotent).
sudo /bin/mkdir -p "$DEST_DIR/bin" "$DEST_DIR/std" "$DEST_DIR/config" "$DEST_DIR/licenses"
sudo /usr/bin/ditto "$SRC_DIR/bin" "$DEST_DIR/bin"
[[ -d "$SRC_DIR/std" ]] && sudo /usr/bin/ditto "$SRC_DIR/std" "$DEST_DIR/std"
[[ -d "$SRC_DIR/config" ]] && sudo /usr/bin/ditto "$SRC_DIR/config" "$DEST_DIR/config"
[[ -d "$SRC_DIR/licenses" ]] && sudo /usr/bin/ditto "$SRC_DIR/licenses" "$DEST_DIR/licenses"

# 2. Symlink the CLI tools into /usr/local/bin (idempotent).
sudo /bin/mkdir -p "$LINK_DIR"
for tool in adesh adl als adesh-editor; do
    if [[ -x "$DEST_DIR/bin/$tool" ]]; then
        sudo /bin/ln -sfn "$DEST_DIR/bin/$tool" "$LINK_DIR/$tool"
        echo "linked: $LINK_DIR/$tool"
    else
        echo "note: $tool not bundled; skipping"
    fi
done

# 3. Shell profile: login shells pick up ADESH_HOME (the toolchain installer
#    later appends its own variables to this same file).
sudo /usr/bin/tee /etc/profile.d/adeshlang.sh >/dev/null <<EOF
# AdeshLang environment (managed by the AdeshLang installer; safe to remove)
export ADESH_HOME='$DEST_DIR'
export ADESH_STD='$DEST_DIR/std'
export PATH='$DEST_DIR/bin:\$PATH'
EOF
echo "wrote: /etc/profile.d/adeshlang.sh"

# 4. Gatekeeper: strip quarantine from the copied files (idempotent).
sudo /usr/bin/xattr -dr com.apple.quarantine "$DEST_DIR" 2>/dev/null || true

# 5. Optional pinned LLVM 18.1.8 toolchain (~190 MB download, ~900 MB disk space).
#    Same prompt as install.sh. (For the 30–90 min GPU-capable MLIR source build, run:
#    sudo '$DEST_DIR/bin/adesh' toolchain install --system --build-mlir-source)
if [[ -x "$DEST_DIR/bin/adesh" ]]; then
    echo
    ans=""
    read -r -p "Download and install pinned LLVM 18.1.8 toolchain now (~190 MB download, ~900 MB disk space)? [Y/n] " ans || ans=""
    case "${ans:-y}" in
        n|N|no)
            echo "Skipping. Install later with: sudo '$DEST_DIR/bin/adesh' toolchain install --system"
            ;;
        *)
            echo "Installing LLVM 18.1.8 toolchain (~190 MB download, ~900 MB disk space)..."
            if sudo ADESH_HOME="$DEST_DIR" "$DEST_DIR/bin/adesh" toolchain install --system; then
                echo "LLVM 18.1.8 toolchain installed and exposed."
            else
                echo
                echo "warning: the toolchain install failed."
                echo "AdeshLang is installed; interpreter/JIT/VM backends work without the"
                echo "toolchain, AOT compilation needs it. Retry later with:"
                echo "    sudo '$DEST_DIR/bin/adesh' toolchain install --system"
                echo "    sudo '$DEST_DIR/bin/adesh' toolchain install --use-system-packages"
                echo "    adesh doctor"
            fi
            ;;
    esac
fi

# 6. Health check (doctor only reports; it never fails the install).
echo
echo "Verifying with 'adesh doctor'..."
sudo ADESH_HOME="$DEST_DIR" "$DEST_DIR/bin/adesh" doctor

echo
echo "=================================================="
echo " AdeshLang $VERSION installed at $DEST_DIR"
echo " Open a new terminal and run:  adesh"
echo "=================================================="
read -r -p "Press Return to close this window... " _ || true
exit 0
