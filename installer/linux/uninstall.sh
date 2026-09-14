#!/usr/bin/env bash
set -euo pipefail

install_dir="/opt/adeshlang"
link_dir="/usr/local/bin"
profile_file="/etc/profile.d/adeshlang.sh"
assume_yes=0

usage() {
  cat <<'USAGE'
Usage: uninstall.sh [--yes] [--user]

  --yes     Do not ask for confirmation
  --user    Remove a user-mode installation from $HOME/.adeshlang
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --yes)
      assume_yes=1
      shift
      ;;
    --user)
      install_dir="${HOME:?HOME is not set}/.adeshlang"
      link_dir="${HOME:?HOME is not set}/.local/bin"
      profile_file=""
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'AdeshLang uninstaller: unknown option: %s\n' "$1" >&2
      exit 2
      ;;
  esac
done

if [[ "$(id -u)" -ne 0 && "$install_dir" == /opt/adeshlang ]]; then
  printf 'AdeshLang uninstaller: system uninstall requires root (try sudo).\n' >&2
  exit 1
fi

if [[ "$install_dir" == "/" || -z "$install_dir" ]]; then
  printf 'AdeshLang uninstaller: refusing to remove an unsafe path.\n' >&2
  exit 1
fi

if (( assume_yes == 0 )); then
  printf 'Remove AdeshLang from %s, including its downloaded LLVM toolchain? [y/N] ' "$install_dir"
  read -r answer
  case "$answer" in
    y|Y|yes|YES) ;;
    *) printf 'Uninstall cancelled.\n'; exit 0 ;;
  esac
fi

for binary in adesh adl als adesh-editor; do
  link="$link_dir/$binary"
  target="$install_dir/bin/$binary"
  if [[ -L "$link" && "$(readlink "$link")" == "$target" ]]; then
    rm -f "$link"
  fi
done

if [[ -n "$profile_file" ]]; then
  rm -f "$profile_file"
fi
rm -rf "$install_dir"

printf 'AdeshLang has been removed from %s.\n' "$install_dir"
printf 'The downloaded LLVM toolchain under %s/toolchain was removed with it.\n' "$install_dir"
printf 'AdeshLang tool symlinks in %s were removed when they pointed to this installation.\n' "$link_dir"
