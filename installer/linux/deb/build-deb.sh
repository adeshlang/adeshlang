#!/usr/bin/env bash
set -euo pipefail

usage() {
  printf 'Usage: %s <amd64|arm64> [version]\n' "$0"
}

arch="${1:-}"
version="${2:-0.3.0}"
case "$arch" in
  amd64) stage_arch="x86_64" ;;
  arm64) stage_arch="aarch64" ;;
  *)
    usage >&2
    exit 2
    ;;
esac

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
stage="$root/dist/linux-$stage_arch"
package_root="$root/dist/adeshlang_${version}_${arch}"
output="$root/dist/adeshlang_${version}_${arch}.deb"

[[ -d "$stage/bin" ]] || {
  printf 'Missing staged binaries: %s/bin\nRun scripts/release/package_unix.sh first.\n' "$stage" >&2
  exit 1
}
[[ -d "$stage/std" ]] || {
  printf 'Missing staged standard library: %s/std\n' "$stage" >&2
  exit 1
}
[[ -f "$stage/config/toolchain-manifest.json" ]] || {
  printf 'Missing staged toolchain manifest: %s/config/toolchain-manifest.json\n' "$stage" >&2
  exit 1
}
command -v dpkg-deb >/dev/null 2>&1 || {
  printf 'dpkg-deb is required to build a Debian package.\n' >&2
  exit 1
}

rm -rf "$package_root"
mkdir -p "$package_root/DEBIAN" "$package_root/usr/lib/adeshlang"
cp -a "$stage/." "$package_root/usr/lib/adeshlang/"
cp "$(dirname "${BASH_SOURCE[0]}")/control" "$package_root/DEBIAN/control"
cp "$(dirname "${BASH_SOURCE[0]}")/postinst" "$package_root/DEBIAN/postinst"
cp "$(dirname "${BASH_SOURCE[0]}")/prerm" "$package_root/DEBIAN/prerm"
cp "$(dirname "${BASH_SOURCE[0]}")/postrm" "$package_root/DEBIAN/postrm"
chmod 0755 "$package_root/DEBIAN/postinst" "$package_root/DEBIAN/prerm" "$package_root/DEBIAN/postrm"

# Enforce standard package permissions
find "$package_root" -type d -exec chmod 0755 {} +
find "$package_root/usr/lib/adeshlang" -type f -exec chmod 0644 {} +
find "$package_root/usr/lib/adeshlang/bin" -type f -exec chmod 0755 {} +

sed -i "s/^Version: .*/Version: $version/" "$package_root/DEBIAN/control"
sed -i "s/^Architecture: .*/Architecture: $arch/" "$package_root/DEBIAN/control"
mkdir -p "$root/dist"
dpkg-deb --build "$package_root" "$output"
printf 'Successfully created %s\n' "$output"
