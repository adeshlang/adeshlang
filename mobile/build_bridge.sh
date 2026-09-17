#!/usr/bin/env bash
set -e

echo "==================================================="
echo "Building AdeshLang Mobile Bridge for Android (NDK)"
echo "==================================================="

if [ -z "$ANDROID_NDK_HOME" ]; then
    if [ -d "$HOME/Android/Sdk/ndk" ]; then
        export ANDROID_NDK_HOME=$(find "$HOME/Android/Sdk/ndk" -maxdepth 1 -mindepth 1 | sort -V | tail -n 1)
    elif [ -d "$ANDROID_HOME/ndk" ]; then
        export ANDROID_NDK_HOME=$(find "$ANDROID_HOME/ndk" -maxdepth 1 -mindepth 1 | sort -V | tail -n 1)
    fi
fi

if [ -z "$ANDROID_NDK_HOME" ]; then
    echo "[ERROR] ANDROID_NDK_HOME is not set and could not be detected."
    exit 1
fi

echo "Using NDK at: $ANDROID_NDK_HOME"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUTPUT_DIR="$SCRIPT_DIR/flutter/android/app/src/main/jniLibs"
MANIFEST_PATH="$SCRIPT_DIR/bridge/Cargo.toml"

echo "Target output directory: $OUTPUT_DIR"

cargo ndk -t arm64-v8a -t x86_64 -o "$OUTPUT_DIR" --manifest-path "$MANIFEST_PATH" build --release

echo "Copying AdeshLang standard library to Flutter assets..."
STD_ASSETS="$SCRIPT_DIR/flutter/assets/std"
mkdir -p "$STD_ASSETS"
cp "$SCRIPT_DIR/../src/stdlib/"*.adesh "$STD_ASSETS/"

echo "[SUCCESS] AdeshLang Mobile Bridge libraries built and standard library bound successfully!"
