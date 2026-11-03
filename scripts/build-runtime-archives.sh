#!/usr/bin/env bash
#
# Build and stage the per-target native runtime archives.
#
# `adesh build --target <triple>` links against `adesh-runtime`'s Cargo
# staticlib for that exact triple: src/backends/aot/cranelift_impl/linking.rs
# searches `target/<triple>/<profile>/`, an install's `lib/<triple>/` and
# `$ADESH_RUNTIME_DIR/<triple>/`, and refuses to fall back to the host archive,
# because a host-arch runtime linked into a foreign image produces an
# executable that cannot run. This script is the thing that produces those
# archives.
#
# Usage:
#   scripts/build-runtime-archives.sh [options]
#
#   --profile <debug|release>   Cargo profile to build and stage. Default: debug.
#   --targets "<triple> ..."    Triples to build. Default: the supported list.
#   --all                       The supported list, explicitly.
#   --install-targets           `rustup target add` each missing target first.
#   --list                      Print the plan (triple, archive path) and exit
#                               without building anything.
#   -h, --help                  This text.
#
# Each triple's build is independent: one failure does not stop the others, and
# the summary lists everything that failed, so a CI job reports every missing
# target rather than only the first.

set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

PROFILE="debug"
TARGETS=""
INSTALL_TARGETS=0
LIST_ONLY=0

# Every target the native codegen backends encode for at Tier 1 or 2 in
# linker/src/target.rs. Building an archive for one needs a C toolchain for
# that target: crates/adesh-runtime/src/native_output.c includes <windows.h>
# on Windows and <unistd.h> on every other OS, so a *-linux-* archive needs a
# matching cross gcc plus its libc headers, and an *-apple-darwin archive needs
# the macOS SDK — which is why the CI matrix builds the Apple archives on a
# macOS runner instead of cross-building them from Linux.
SUPPORTED_TARGETS="
x86_64-unknown-linux-gnu
x86_64-unknown-linux-musl
aarch64-unknown-linux-gnu
riscv64gc-unknown-linux-gnu
aarch64-apple-darwin
x86_64-apple-darwin
x86_64-pc-windows-msvc
aarch64-pc-windows-msvc
"

usage() {
    sed -n '3,26p' "$0" | sed 's/^# \{0,1\}//'
}

while [ $# -gt 0 ]; do
    case "$1" in
        --profile)
            PROFILE="${2:-}"
            shift 2
            ;;
        --targets)
            TARGETS="${2:-}"
            shift 2
            ;;
        --all)
            TARGETS="$SUPPORTED_TARGETS"
            shift
            ;;
        --install-targets)
            INSTALL_TARGETS=1
            shift
            ;;
        --list)
            LIST_ONLY=1
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            echo "unknown option: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

# Spelled out as a literal so an unquoted expansion adds no argument at all
# when the profile is debug; bash 3.2 on the macOS runner cannot expand an
# empty array under `set -u`.
CARGO_PROFILE_FLAG=""
if [ "$PROFILE" = "release" ]; then
    CARGO_PROFILE_FLAG="--release"
elif [ "$PROFILE" != "debug" ]; then
    echo "--profile must be debug or release, got '$PROFILE'" >&2
    exit 2
fi

[ -n "$TARGETS" ] || TARGETS="$SUPPORTED_TARGETS"

TARGET_DIR="${CARGO_TARGET_DIR:-target}"

# Must match `runtime_archive_names` in
# src/backends/aot/cranelift_impl/linking.rs, which is what looks these up.
archive_name_for() {
    case "$1" in
        *-windows-*) echo "adesh_runtime.lib" ;;
        *) echo "libadesh_runtime.a" ;;
    esac
}

# Both the GNU and the COFF archive format start with this magic, so the check
# stops the script from staging an empty or half-written file that the linker
# would reject somewhere much less obvious.
is_archive() {
    if [ ! -f "$1" ]; then
        return 1
    fi
    [ "$(head -c 8 "$1")" = "!<arch>" ]
}

status=0
built=()
skipped=()
failed=()

for triple in $TARGETS; do
    name="$(archive_name_for "$triple")"
    staged="$TARGET_DIR/$triple/$PROFILE/$name"

    if [ "$LIST_ONLY" -eq 1 ]; then
        printf '%-30s %s\n' "$triple" "$staged"
        continue
    fi

    if ! rustup target list --installed 2>/dev/null | grep -qx "$triple"; then
        if [ "$INSTALL_TARGETS" -eq 1 ]; then
            if ! rustup target add "$triple" >/dev/null 2>&1; then
                echo "✗ $triple: rustup target add failed" >&2
                failed+=("$triple (rustup target add failed)")
                status=1
                continue
            fi
        else
            echo "✗ $triple: target not installed — run \`rustup target add $triple\`" >&2
            skipped+=("$triple (rustup target not installed)")
            status=1
            continue
        fi
    fi

    echo "▶ cargo build -p adesh-runtime --target $triple $CARGO_PROFILE_FLAG"
    if ! cargo build -p adesh-runtime --target "$triple" $CARGO_PROFILE_FLAG 2>&1 |
        tail -3; then
        echo "✗ $triple: cargo build failed" >&2
        failed+=("$triple (cargo build failed)")
        status=1
        continue
    fi

    # Cargo writes a directly-built staticlib in the profile directory; one
    # reached through a dependency carries a hash in deps/. Normalise either to
    # the canonical name so resolution has one predictable path to find.
    if [ ! -f "$staged" ]; then
        deps_dir="$TARGET_DIR/$triple/$PROFILE/deps"
        stem="${name%%.*}"
        ext="${name##*.}"
        candidate=""
        for probe in "$deps_dir/$name" "$deps_dir/$stem"-*."$ext"; do
            if [ -f "$probe" ]; then
                candidate="$probe"
                break
            fi
        done
        if [ -n "$candidate" ]; then
            if ! cp "$candidate" "$staged"; then
                echo "✗ $triple: could not stage $candidate to $staged" >&2
                failed+=("$triple (staging failed)")
                status=1
                continue
            fi
        fi
    fi

    if [ ! -s "$staged" ]; then
        echo "✗ $triple: no archive at $staged after a successful build" >&2
        failed+=("$triple (archive missing after build)")
        status=1
        continue
    fi
    if ! is_archive "$staged"; then
        echo "✗ $triple: $staged is not an archive (missing !<arch> magic)" >&2
        failed+=("$triple (staged file is not an archive)")
        status=1
        continue
    fi

    size="$(wc -c <"$staged" | tr -d ' ')"
    echo "✓ $triple → $staged ($size bytes)"
    built+=("$triple")
done

if [ "$LIST_ONLY" -eq 1 ]; then
    exit 0
fi

echo
echo "profile:    $PROFILE"
echo "target dir: $TARGET_DIR"
echo "built: ${#built[@]}    skipped: ${#skipped[@]}    failed: ${#failed[@]}"
for note in "${skipped[@]:-}" "${failed[@]:-}"; do
    if [ -n "$note" ]; then
        echo "  - $note"
    fi
done
exit "$status"
