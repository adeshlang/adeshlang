#!/usr/bin/env bash
set -euo pipefail

# Convenience runner for executing Adesh commands in Docker
IMAGE="${DOCKER_ADESH_IMAGE:-adeshlang:latest}"
WORKSPACE="${PWD}"

if [ -t 0 ]; then
    TTY_FLAG="-it"
else
    TTY_FLAG=""
fi

docker run --rm $TTY_FLAG \
    -v "${WORKSPACE}:/workspace" \
    -w /workspace \
    "$IMAGE" "$@"
