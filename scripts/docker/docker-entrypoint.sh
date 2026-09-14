#!/usr/bin/env bash
set -e

# If no arguments provided
if [ $# -eq 0 ]; then
    if [ -t 0 ]; then
        echo "Starting AdeshLang Interactive REPL (type 'exit()' or Ctrl+D to quit)..."
        exec adesh repl
    else
        exec adesh --help
    fi
fi

# If first argument is an option flag (e.g., -v, --version, -e, --eval)
if [ "${1#-}" != "$1" ]; then
    exec adesh "$@"
fi

# If first argument is a file ending in .adesh
if [[ "$1" == *.adesh ]]; then
    exec adesh run "$@"
fi

# Known adesh ecosystem executables or shell commands
case "$1" in
    adesh|adl|als|adesh-editor|bash|sh|ash|zsh)
        exec "$@"
        ;;
    run|repl|doctor|build|check|fmt|lint|test|bench|doc|clean|new|init|info|ai|toolchain)
        exec adesh "$@"
        ;;
    *)
        exec "$@"
        ;;
esac
