#!/usr/bin/env bash
set -euo pipefail

# Script to build AdeshLang Docker images
TAG="${1:-latest}"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

cd "$REPO_ROOT"

echo "==> Building AdeshLang Docker image (tag: adeshlang:$TAG)..."
docker build -t "adeshlang:$TAG" -f Dockerfile .

if [[ "$TAG" == "latest" ]]; then
    echo "==> Building AdeshLang Slim Docker image (tag: adeshlang:slim)..."
    docker build -t "adeshlang:slim" -f Dockerfile.slim .
fi

echo "==> Docker build completed successfully!"
docker images | grep adeshlang || true
