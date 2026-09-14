#!/usr/bin/env bash
# Linux distribution build that runs INSIDE a rust container, so a Windows or
# macOS host can produce the Linux artifacts:
#
#   docker run --rm -v <repo>:/work -w /work rust:1-bookworm \
#       bash scripts/release/docker/linux-build.sh <x86_64|aarch64>
#
# Produces (under /work/dist/): the linux tar.xz for the given architecture,
# plus the .deb and .rpm when the packaging tools are available. Uses its own
# CARGO_TARGET_DIR so container artifacts never mix with host build outputs.
set -euo pipefail

arch="${1:-}"
case "$arch" in
  x86_64)  target="x86_64-unknown-linux-gnu" ;;
  aarch64) target="aarch64-unknown-linux-gnu" ;;
  *) echo "usage: linux-build.sh <x86_64|aarch64>" >&2; exit 2 ;;
esac

root="/work"
cd "$root"

# Normalize any Windows CRLF line endings the bind mount may expose; a no-op
# when the scripts are already LF.
if grep -q $'\r' scripts/release/package_unix.sh 2>/dev/null; then
  find scripts installer -type f -name '*.sh' -exec sed -i 's/\r$//' {} +
  echo "note: normalized CRLF line endings in shell scripts."
fi

export DEBIAN_FRONTEND=noninteractive
echo "==> installing packaging tooling (rpm for rpmbuild)"
apt-get update -qq
apt-get install -y -qq --no-install-recommends rpm ca-certificates >/dev/null

if [[ "$arch" == "aarch64" ]]; then
  echo "==> installing aarch64 cross toolchain"
  apt-get install -y -qq --no-install-recommends gcc-aarch64-linux-gnu \
    g++-aarch64-linux-gnu binutils-aarch64-linux-gnu >/dev/null
  rustup target add aarch64-unknown-linux-gnu
  export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc
  export CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
  export CXX_aarch64_unknown_linux_gnu=aarch64-linux-gnu-g++
  export AR_aarch64_unknown_linux_gnu=aarch64-linux-gnu-ar
fi

# Keep container artifacts away from the host's target/ tree (and away from a
# parallel container building the other architecture).
export CARGO_TARGET_DIR="$root/target-docker-$arch"

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)"
echo "==> building AdeshLang $version for linux-$arch"
bash scripts/release/package_unix.sh "$version" "$arch" linux "$target"

echo "==> linux-$arch artifacts:"
find "$root/dist" -maxdepth 2 -type f \
  \( -name "*linux-gnu.tar.xz" -o -name "*.deb" -o -name "*.rpm" \) -printf "  %p\n" | sort
