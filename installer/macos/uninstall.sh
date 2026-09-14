#!/usr/bin/env bash
# AdeshLang macOS uninstaller — the inverse of install.sh / the pkg.
#
# Removal philosophy: only touch things this installer created.
#   * Symlinks in /usr/local/bin (or ~/.local/bin for --user) whose target
#     lives inside the AdeshLang install directory — this covers both the
#     four CLI tools and any LLVM tools linked by `adesh toolchain expose`.
#   * /etc/profile.d/adeshlang.sh (system) or the AdeshLang exports appended
#     to ~/.zprofile (--user).
#   * The install directory itself (after confirmation).
#
# Usage:
#   sudo bash uninstall.sh [--yes]
#   bash uninstall.sh --user [--yes]
#
# --yes skips the confirmation prompt. System uninstalls need sudo/root.
set -euo pipefail

MODE="system"
YES=0

usage() {
    cat <<'EOF'
Usage: uninstall.sh [--user] [--yes]
  --user  remove the ~/.adeshlang user installation instead of the system one
  --yes   skip the confirmation prompt
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --user) MODE="user"; shift ;;
        --yes|-y) YES=1; shift ;;
        -h|--help) usage; exit 0 ;;
        *) echo "error: unknown option: $1" >&2; usage; exit 2 ;;
    esac
done

if [[ "$MODE" == "user" ]]; then
    INSTALL_DIR="$HOME/.adeshlang"
    BIN_LINK_DIR="$HOME/.local/bin"
    PROFILE_FILE="$HOME/.zprofile"
else
    INSTALL_DIR="/Library/Application Support/AdeshLang"
    BIN_LINK_DIR="/usr/local/bin"
    PROFILE_FILE="/etc/profile.d/adeshlang.sh"
fi

# Root wrapper: system uninstalls elevate per command.
run_root() {
    if [[ "$MODE" == "system" ]]; then
        sudo "$@"
    else
        "$@"
    fi
}

if [[ "$MODE" == "system" ]]; then
    sudo -v   # prompt for the admin password up front
fi

echo "AdeshLang uninstaller"
echo "  Mode     : $MODE"
echo "  Install  : $INSTALL_DIR"
echo "  Links in : $BIN_LINK_DIR"

confirm() {
    if [[ "$YES" == "1" ]]; then
        return 0
    fi
    local ans=""
    read -r -p "$1 [y/N] " ans || ans=""
    case "$ans" in
        y|Y|yes|YES) return 0 ;;
        *) return 1 ;;
    esac
}

# 1. Symlinks pointing into the AdeshLang install dir --------------------------
echo
if [[ -d "$BIN_LINK_DIR" ]]; then
    for link in "$BIN_LINK_DIR"/*; do
        [[ -L "$link" ]] || continue
        target="$(readlink "$link" 2>/dev/null || true)"
        case "$target" in
            "$INSTALL_DIR"/*)
                run_root /bin/rm -f "$link"
                echo "removed symlink: $link"
                ;;
        esac
    done
fi

# 2. Shell profile -------------------------------------------------------------
if [[ "$MODE" == "system" ]]; then
    if [[ -f "$PROFILE_FILE" ]]; then
        run_root /bin/rm -f "$PROFILE_FILE"
        echo "removed: $PROFILE_FILE"
    else
        echo "note: $PROFILE_FILE not present"
    fi
else
    if [[ -f "$PROFILE_FILE" ]] && grep -qsF "# AdeshLang environment" "$PROFILE_FILE"; then
        # Delete exactly the block install.sh appended (marker line + the 3
        # export lines that follow it). GNU-style /regex/,+N is a BSD sed
        # extension on macOS but awk gives the same result portably.
        awk 'BEGIN{d=0}
             /^# AdeshLang environment/ && d==0 {d=4}
             d>0 {d--; next}
             {print}' "$PROFILE_FILE" > "$PROFILE_FILE.tmp"
        mv "$PROFILE_FILE.tmp" "$PROFILE_FILE"
        echo "removed AdeshLang exports from $PROFILE_FILE"
    else
        echo "note: no AdeshLang configuration found in $PROFILE_FILE"
    fi
fi

# 3. The install directory itself (after confirmation) --------------------------
echo
if confirm "Remove $INSTALL_DIR (all AdeshLang files and the LLVM toolchain)? "; then
    if [[ -d "$INSTALL_DIR" ]]; then
        run_root /bin/rm -rf "$INSTALL_DIR"
        echo "removed: $INSTALL_DIR"
    else
        echo "note: $INSTALL_DIR not present"
    fi
else
    echo "keeping: $INSTALL_DIR"
fi

# Leftover choice marker from the .pkg toolchain flag trick, if any.
run_root /bin/rm -f /tmp/.adesh-install-toolchain

echo
echo "AdeshLang uninstall complete."
