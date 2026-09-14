#!/usr/bin/env bash
set -euo pipefail

usage() {
  printf 'Usage: %s <x86_64|aarch64> [version] [--with-toolchain]\n' "$0"
}

arch="${1:-}"
version="${2:-0.3.0}"
with_toolchain=0
if [[ "${3:-}" == "--with-toolchain" ]]; then
  with_toolchain=1
elif [[ -n "${3:-}" ]]; then
  usage >&2
  exit 2
fi

case "$arch" in
  x86_64|aarch64) ;;
  *)
    usage >&2
    exit 2
    ;;
esac

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
stage="$root/dist/linux-$arch"
output_dir="$root/dist/rpm"
rpmbuild_root="$root/dist/rpmbuild-$arch"
spec="$root/installer/linux/rpm/adeshlang.spec"

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
command -v rpmbuild >/dev/null 2>&1 || {
  printf 'rpmbuild is required to build an RPM.\n' >&2
  exit 1
}

rm -rf "$rpmbuild_root"
mkdir -p "$output_dir" "$rpmbuild_root"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}
cp "$spec" "$rpmbuild_root/SPECS/adeshlang.spec"
sed -i "s/^Version: .*/Version: $version/" "$rpmbuild_root/SPECS/adeshlang.spec"

rpm_args=(
  -bb
  --target "$arch"
  --define "_topdir $rpmbuild_root"
  --define "_adeshlang_stage $stage"
)
if (( with_toolchain )); then
  rpm_args+=(--with toolchain)
fi
rpmbuild "${rpm_args[@]}" "$rpmbuild_root/SPECS/adeshlang.spec"

find "$rpmbuild_root/RPMS" -type f -name '*.rpm' -exec cp {} "$output_dir/" \;
printf 'RPM artifacts are in %s\n' "$output_dir"
