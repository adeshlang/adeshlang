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

# --- Dependency checks & helper resolution ---
check_and_install_dependencies() {
  local missing=()
  command -v curl >/dev/null 2>&1 || command -v wget >/dev/null 2>&1 || missing+=("curl")
  command -v tar >/dev/null 2>&1 || missing+=("tar")
  command -v xz >/dev/null 2>&1 || missing+=("xz")
  
  if [[ ${#missing[@]} -gt 0 ]]; then
    printf 'Missing utility packages: %s\n' "${missing[*]}"
    if [[ "$(id -u)" -eq 0 ]]; then
      if command -v dnf >/dev/null 2>&1; then
        dnf install -y "${missing[@]}" ca-certificates
      elif command -v yum >/dev/null 2>&1; then
        yum install -y "${missing[@]}" ca-certificates
      elif command -v apt-get >/dev/null 2>&1; then
        apt-get update -qq && apt-get install -y -qq "${missing[@]}" ca-certificates
      elif command -v zypper >/dev/null 2>&1; then
        zypper install -y "${missing[@]}" ca-certificates
      elif command -v pacman >/dev/null 2>&1; then
        pacman -Sy --noconfirm "${missing[@]}" ca-certificates
      elif command -v apk >/dev/null 2>&1; then
        apk add --no-cache "${missing[@]}" ca-certificates
      fi
    elif command -v sudo >/dev/null 2>&1 && (( assume_yes )); then
      if command -v dnf >/dev/null 2>&1; then
        sudo dnf install -y "${missing[@]}" ca-certificates
      elif command -v yum >/dev/null 2>&1; then
        sudo yum install -y "${missing[@]}" ca-certificates
      elif command -v apt-get >/dev/null 2>&1; then
        sudo apt-get update -qq && sudo apt-get install -y -qq "${missing[@]}" ca-certificates
      fi
    else
      printf 'Warning: Missing tools: %s. Attempting to proceed...\n' "${missing[*]}"
    fi
  fi
}

check_and_install_dependencies

verify_checksum() {
  local file="$1"
  local expected_sha=""
  case "$arch" in
    x86_64)  expected_sha="8d17ebdf87e3d9e51e53ac5a8bfb27677c311634e28d7866d86e05a4154900d5" ;;
    aarch64) expected_sha="a3a6b256f4cc03e28ae9e8105ce31e170695f39c7dcd427b2ed0a8c952cfcd1b" ;;
  esac

  if [[ -n "$expected_sha" ]]; then
    local actual_sha=""
    if command -v sha256sum >/dev/null 2>&1; then
      actual_sha="$(sha256sum "$file" | awk '{print $1}')"
    elif command -v shasum >/dev/null 2>&1; then
      actual_sha="$(shasum -a 256 "$file" | awk '{print $1}')"
    fi

    if [[ -n "$actual_sha" ]]; then
      local actual_lc expect_lc
      actual_lc="$(printf '%s' "$actual_sha" | tr '[:upper:]' '[:lower:]')"
      expect_lc="$(printf '%s' "$expected_sha" | tr '[:upper:]' '[:lower:]')"
      if [[ "$actual_lc" != "$expect_lc" ]]; then
        # Check if checksums file exists online as override
        local sha_file="$temp_dir/SHA256SUMS"
        if curl -fsL "${REPO_URL_BASE%/}/SHA256SUMS" -o "$sha_file" 2>/dev/null; then
          local remote_expected
          remote_expected="$(grep "$(basename "$file")" "$sha_file" | awk '{print $1}' || true)"
          if [[ -n "$remote_expected" && "$actual_lc" == "$(printf '%s' "$remote_expected" | tr '[:upper:]' '[:lower:]')" ]]; then
            printf 'Checksum verified against release SHA256SUMS: OK\n'
            return 0
          fi
        fi
        die "SHA256 checksum verification failed for $(basename "$file")!\nExpected: $expected_sha\nGot:      $actual_sha"
      else
        printf 'Checksum verified: OK (SHA-256 matches release manifest)\n'
      fi
    fi
  fi
}

get_temp_dir() {
  local base="${TMPDIR:-}"
  if [[ -n "$base" && -d "$base" && -w "$base" ]]; then
    mktemp -d "$base/adeshlang-install.XXXXXX"
    return
  fi
  local tmp_avail=0
  if command -v df >/dev/null 2>&1; then
    tmp_avail="$(df -m /tmp 2>/dev/null | awk 'NR==2 {print $4}')"
  fi
  if [[ -n "$tmp_avail" && "$tmp_avail" =~ ^[0-9]+$ && "$tmp_avail" -ge 300 ]]; then
    mktemp -d "/tmp/adeshlang-install.XXXXXX"
  elif [[ -n "${HOME:-}" && -d "$HOME" && -w "$HOME" ]]; then
    mkdir -p "$HOME/.cache"
    mktemp -d "$HOME/.cache/adeshlang-install.XXXXXX"
  else
    mktemp -d "/tmp/adeshlang-install.XXXXXX"
  fi
}

temp_dir="$(get_temp_dir)"
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
  verify_checksum "$core_tarball"
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

# Configure file permissions
find "$install_dir" -type d -exec chmod 0755 {} +
find "$install_dir" -type f -exec chmod 0644 {} +
if [[ -d "$install_dir/bin" ]]; then
  find "$install_dir/bin" -type f -exec chmod 0755 {} +
fi

link_binary() {
  local name="$1"
  local source="$install_dir/bin/$name"
  [[ -x "$source" ]] || die "required binary is missing or not executable: $source"
  ln -sfn "$source" "$link_dir/$name"
}

for binary in adesh adl als adesh-editor; do
  link_binary "$binary"
done

# Verify binary compatibility with the local C library
if ! "$install_dir/bin/adesh" --version >/dev/null 2>&1; then
  bin_err="$("$install_dir/bin/adesh" --version 2>&1 || true)"
  if printf '%s\n' "$bin_err" | grep -q "GLIBC_"; then
    die "The precompiled AdeshLang release binary requires a newer GLIBC than is available on this system ($glibc_version).\nDetails: $bin_err\n\nTo build AdeshLang natively from source on this system, run:\n  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh\n  git clone https://github.com/adeshlang/adeshlang.git\n  cd adeshlang && cargo build --release\n\nOr run with Docker:\n  docker run --rm -it adeshlang/adeshlang"
  fi
fi

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
export PATH='$quoted_home/bin:$quoted_home/toolchain/llvm/bin:\$PATH'
PROFILE
  chmod 0644 "$2" 2>/dev/null || true
}

configure_user_shell() {
  local profile_snippet="
# AdeshLang environment
export ADESH_HOME=\"$install_dir\"
case \":\$PATH:\" in
  *:\$ADESH_HOME/bin:*) ;;
  *) export PATH=\"\$ADESH_HOME/bin:$link_dir:\$PATH\" ;;
esac
"
  for rc_file in "$HOME/.bashrc" "$HOME/.zshrc" "$HOME/.profile"; do
    if [[ -f "$rc_file" || -w "$HOME" ]]; then
      if ! grep -qs "ADESH_HOME" "$rc_file" 2>/dev/null; then
        printf '%s\n' "$profile_snippet" >> "$rc_file" 2>/dev/null || true
      fi
    fi
  done
}

if (( system_mode )); then
  mkdir -p /etc/profile.d
  write_profile "$install_dir" /etc/profile.d/adeshlang.sh
  printf 'Installed AdeshLang %s in %s\n' "$VERSION" "$install_dir"
else
  configure_user_shell
  printf 'Installed AdeshLang %s in %s\n' "$VERSION" "$install_dir"
  printf 'User binaries are linked in %s.\n' "$link_dir"
  printf 'User mode avoids /usr/local/bin and does not require sudo.\n'
  printf 'Shell configuration (~/.bashrc, ~/.profile) was automatically updated.\n'
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
    if ask_yes_no 'Download and install pinned LLVM 18.1.8 toolchain now (~190 MB download, ~900 MB disk space)? [Y/n]' yes; then
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

printf '\n═══════════════════════════════════════════════════════\n'
printf ' AdeshLang v%s Installation Complete!\n' "$VERSION"
printf '═══════════════════════════════════════════════════════\n'
if [[ ! -f "$install_dir/ai/models/adesh-coder-0.5b-q4_0.gguf" ]]; then
  printf ' • Note: AI neural models are not bundled with this lightweight installer.\n'
  printf ' • To download and set up the default offline AI coder model (~275 MB):\n'
  printf '     adesh ai setup\n'
  printf ' • For custom AI model training & MLIR source builds, install Python 3.12:\n'
  printf '     (e.g., sudo apt install python3 python3-pip / sudo dnf install python3)\n'
fi
printf ' • Verify health & toolchains:  adesh doctor\n'
printf ' • Documentation & Guides:      https://adeshlang.org\n\n'
