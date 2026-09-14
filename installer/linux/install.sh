#!/usr/bin/env bash
set -euo pipefail

VERSION="0.3.0"
# Project migration note: update this one line when the release organization changes.
REPO_URL_BASE="https://github.com/adeshlang/adeshlang/releases/download/v0.3.0/"
if [[ -n "${ADESH_REPO:-}" ]]; then
  REPO_URL_BASE="https://github.com/${ADESH_REPO}/releases/download/v${VERSION}/"
fi

usage() {
  cat <<'USAGE'
Usage: install.sh [core-tarball] [options]

Options:
  --install-dir <dir>       Installation directory (default: /opt/adeshlang as root,
                            $HOME/.adeshlang otherwise)
  --skip-toolchain          Do not install LLVM/MLIR during this run
  --with-gpu-source-build   Build MLIR GPU tools from pinned LLVM source
  --use-system-packages     Use the host package manager for the toolchain
  --yes                     Accept prompts (does not enable the GPU source build)
  -h, --help               Show this help
USAGE
}

die() {
  printf 'AdeshLang installer: %s\n' "$*" >&2
  exit 1
}

require_value() {
  if [[ $# -lt 2 || -z "$2" || "$2" == -* ]]; then
    die "$1 requires a value"
  fi
}

core_tarball=""
install_dir=""
skip_toolchain=0
gpu_source_build=0
use_system_packages=0
assume_yes=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --install-dir)
      require_value "$1" "${2:-}"
      install_dir="$2"
      shift 2
      ;;
    --skip-toolchain)
      skip_toolchain=1
      shift
      ;;
    --with-gpu-source-build)
      gpu_source_build=1
      shift
      ;;
    --use-system-packages)
      use_system_packages=1
      shift
      ;;
    --yes)
      assume_yes=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    -*)
      die "unknown option: $1 (use --help for usage)"
      ;;
    *)
      if [[ -n "$core_tarball" ]]; then
        die "only one core tarball may be specified"
      fi
      core_tarball="$1"
      shift
      ;;
  esac
done

if [[ "$(id -u)" -eq 0 ]]; then
  system_mode=1
  install_dir="${install_dir:-/opt/adeshlang}"
  link_dir="/usr/local/bin"
else
  system_mode=0
  install_dir="${install_dir:-${HOME:?HOME is not set}/.adeshlang}"
  link_dir="${HOME:?HOME is not set}/.local/bin"
  case "$install_dir" in
    /opt|/opt/*|/usr|/usr/*|/etc|/etc/*)
      die "a non-root install must use a directory under \$HOME (got $install_dir)"
      ;;
  esac
fi

case "$install_dir" in
  /*) ;;
  *) install_dir="$PWD/$install_dir" ;;
esac

case "$install_dir" in
  ""|/)
    die "refusing to install into an empty path or /"
    ;;
esac

case "$(uname -m)" in
  x86_64) arch="x86_64" ;;
  aarch64) arch="aarch64" ;;
  *)
    die "unsupported architecture '$(uname -m)'; supported Linux architectures are x86_64 and aarch64"
    ;;
esac

if ! command -v ldd >/dev/null 2>&1; then
  die "ldd is required to check the Linux C library"
fi
ldd_version="$(ldd --version 2>&1 || true)"
if printf '%s\n' "$ldd_version" | grep -qi 'musl'; then
  die "musl-based Linux is not supported yet; use a glibc-based distribution or build AdeshLang from source"
fi
glibc_version="$(printf '%s\n' "$ldd_version" | sed -n '1s/.* \([0-9][0-9]*\)\.\([0-9][0-9]*\).*/\1.\2/p')"
if [[ -z "$glibc_version" ]]; then
  die "could not determine the glibc version from ldd --version"
fi
glibc_major="${glibc_version%%.*}"
glibc_minor="${glibc_version#*.}"
if (( glibc_major < 2 || (glibc_major == 2 && glibc_minor < 28) )); then
  die "glibc ${glibc_version} is too old; AdeshLang requires glibc 2.28 or newer"
fi

temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/adeshlang-install.XXXXXX")"
cleanup() {
  rm -rf "$temp_dir"
}
trap cleanup EXIT

if [[ -z "$core_tarball" ]]; then
  command -v curl >/dev/null 2>&1 || die "curl is required when no core tarball is supplied"
  archive_name="AdeshLang-${VERSION}-${arch}-linux-gnu.tar.xz"
  core_tarball="$temp_dir/$archive_name"
  printf 'Downloading %s...\n' "${REPO_URL_BASE%/}/$archive_name"
  if ! curl -fL --retry 2 --connect-timeout 10 --max-time 600 \
      "${REPO_URL_BASE%/}/$archive_name" -o "$core_tarball"; then
    die "failed to download the core tarball; provide it as the first argument for an offline install"
  fi
elif [[ ! -f "$core_tarball" ]]; then
  die "core tarball not found: $core_tarball"
fi

extract_dir="$temp_dir/extracted"
mkdir -p "$extract_dir"
tar -xJf "$core_tarball" -C "$extract_dir"

source_dir="$extract_dir"
if [[ ! -d "$source_dir/bin" ]]; then
  source_dir="$(find "$extract_dir" -mindepth 1 -maxdepth 1 -type d -print -quit)"
fi
[[ -n "$source_dir" && -d "$source_dir/bin" ]] ||
  die "the core tarball does not contain a bin/ directory"
[[ -d "$source_dir/std" ]] ||
  die "the core tarball does not contain the standard library in std/"
[[ -f "$source_dir/config/toolchain-manifest.json" ]] ||
  die "the core tarball does not contain config/toolchain-manifest.json"

mkdir -p "$install_dir"
cp -a "$source_dir/." "$install_dir/"
mkdir -p "$link_dir"

link_binary() {
  local name="$1"
  local source="$install_dir/bin/$name"
  [[ -x "$source" ]] || die "required binary is missing or not executable: $source"
  ln -sfn "$source" "$link_dir/$name"
}

for binary in adesh adl als adesh-editor; do
  link_binary "$binary"
done

export ADESH_HOME="$install_dir"
export PATH="$install_dir/bin:$PATH"

write_profile() {
  local home="$1"
  local quoted_home
  quoted_home="$(printf '%s' "$home" | sed "s/'/'\\\\''/g")"
  cat > "$2" <<PROFILE
# AdeshLang environment (managed by the AdeshLang installer)
export ADESH_HOME='$quoted_home'
export ADESH_TOOLCHAIN='$quoted_home/toolchain/llvm'
export ADESH_CLANG='$quoted_home/toolchain/llvm/bin/clang'
export ADESH_LLC='$quoted_home/toolchain/llvm/bin/llc'
export ADESH_MLIR_OPT='$quoted_home/toolchain/llvm/bin/mlir-opt'
export ADESH_MLIR_TRANSLATE='$quoted_home/toolchain/llvm/bin/mlir-translate'
export PATH='$quoted_home/bin:\$PATH'
PROFILE
}

if (( system_mode )); then
  mkdir -p /etc/profile.d
  write_profile "$install_dir" /etc/profile.d/adeshlang.sh
  printf 'Installed AdeshLang %s in %s\n' "$VERSION" "$install_dir"
else
  printf 'Installed AdeshLang %s in %s\n' "$VERSION" "$install_dir"
  printf 'User binaries are linked in %s.\n' "$link_dir"
  printf 'User mode avoids /usr/local/bin and does not require sudo.\n'
  printf 'Add %s to PATH (for example: export PATH="%s:$PATH").\n' "$link_dir" "$link_dir"
fi

toolchain_command() {
  local scope
  if (( system_mode )); then
    scope="--system"
  else
    scope="--user"
  fi
  local args=(toolchain install "$scope")
  if (( gpu_source_build )); then
    args+=(--build-mlir-source)
  fi
  if (( use_system_packages )); then
    args+=(--use-system-packages)
  fi
  ADESH_HOME="$install_dir" "$install_dir/bin/adesh" "${args[@]}"
}

later_toolchain_command() {
  local scope
  if (( system_mode )); then
    scope="--system"
    printf 'Run later (as root): sudo ADESH_HOME="%s" "%s/bin/adesh" toolchain install %s' \
      "$install_dir" "$install_dir" "$scope"
  else
    scope="--user"
    printf 'Run later: ADESH_HOME="%s" "%s/bin/adesh" toolchain install %s' \
      "$install_dir" "$install_dir" "$scope"
  fi
  if (( use_system_packages )); then
    printf ' --use-system-packages'
  fi
  if (( gpu_source_build )); then
    printf ' --build-mlir-source'
  fi
  printf '\n'
}

ask_yes_no() {
  local prompt="$1"
  local default_yes="$2"
  local answer=""
  if (( assume_yes )); then
    [[ "$default_yes" == "yes" ]]
    return
  fi
  [[ -r /dev/tty && -t 1 ]] || return 1
  printf '%s ' "$prompt"
  IFS= read -r answer </dev/tty || return 1
  if [[ "$default_yes" == "yes" ]]; then
    [[ -z "$answer" || "$answer" =~ ^[Yy]([Ee][Ss])?$ ]]
  else
    [[ "$answer" =~ ^[Yy]([Ee][Ss])?$ ]]
  fi
}

if (( skip_toolchain )); then
  printf 'Toolchain installation skipped.\n'
  later_toolchain_command
else
  online=0
  if command -v curl >/dev/null 2>&1 &&
    curl -fsI --max-time 5 https://github.com >/dev/null 2>&1; then
    online=1
  fi
  install_toolchain=0
  if (( online )); then
    if ask_yes_no 'Download and install the pinned LLVM 18.1.8 toolchain now? [Y/n]' yes; then
      install_toolchain=1
    fi
  else
    printf 'GitHub is not reachable; the toolchain will not be downloaded now.\n'
  fi

  if (( install_toolchain )); then
    if (( gpu_source_build == 0 )) && ask_yes_no \
      'Build optional MLIR GPU tools from pinned LLVM source? [y/N]' no; then
      gpu_source_build=1
    fi
    toolchain_command
  else
    printf 'Toolchain installation was declined or deferred.\n'
    later_toolchain_command
  fi
fi

if (( system_mode )); then
  printf 'Running adesh doctor...\n'
  if ! ADESH_HOME="$install_dir" "$install_dir/bin/adesh" doctor; then
    printf 'AdeshLang doctor reported issues; the installation is present but needs attention.\n' >&2
  fi
else
  printf 'Open a new shell or export PATH=%s:$PATH, then run adesh doctor.\n' "$link_dir"
fi
