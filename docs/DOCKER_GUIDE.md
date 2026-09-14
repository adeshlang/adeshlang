# AdeshLang Docker & Containerization Guide

AdeshLang provides first-class support for containerized environments. You can run AdeshLang programs, REPL sessions, package manager commands, and language servers inside lightweight, reproducible OCI/Docker containers across Linux, macOS, and Windows.

---

## 🚀 Quick Start

### 1. Build the Docker Image

From the root of the AdeshLang repository:

```bash
docker build -t adeshlang:latest .
```

Or using the helper script:

```bash
# Linux / macOS
./scripts/docker/build.sh

# Windows PowerShell
.\scripts\docker\build.ps1
```

### 2. Interactive REPL

Start an interactive AdeshLang REPL session in a container:

```bash
docker run --rm -it adeshlang:latest
```

```text
Starting AdeshLang Interactive REPL (type 'exit()' or Ctrl+D to quit)...
adesh> println("Hello from Adesh inside Docker!")
Hello from Adesh inside Docker!
adesh>
```

### 3. Run an Adesh Script

Mount your local workspace directory into `/workspace` and execute your `.adesh` script:

```bash
docker run --rm -v "$(pwd):/workspace" adeshlang:latest my_script.adesh
```

Or evaluate inline code:

```bash
docker run --rm adeshlang:latest adesh run -e 'println(40 + 2)'
```

---

## 📦 Container Variants

| Image Tag | Dockerfile | Size | Use Case |
| :--- | :--- | :--- | :--- |
| `adeshlang:latest` | `Dockerfile` | Standard | Development, full standard library, C toolchain for AOT & FFI, Language Server (`als`), TUI Editor (`adesh-editor`), ADL package manager |
| `adeshlang:slim` | `Dockerfile.slim` | Minimal (~150MB) | Production microservices, serverless jobs, CI runners |

### Building the Slim Image

```bash
docker build -t adeshlang:slim -f Dockerfile.slim .
```

---

## 🛠️ Included Tools in the Container

The standard container image bundles the complete AdeshLang toolchain:

- `adesh` — The core compiler CLI, interpreter, JIT runner, and AOT compiler
- `adl` — The Adesh package manager for dependency resolution and package publication
- `als` — Adesh Language Server (LSP) for editor integration
- `adesh-editor` — Terminal UI code editor for in-container development
- `gcc` / `make` / `libc` / `libffi` — C build tools for native linking and C FFI

Verify the environment with `adesh doctor`:

```bash
docker run --rm adeshlang:latest adesh doctor
```

---

## 🐳 Docker Compose

You can use `docker-compose.yml` for simplified workflows:

```bash
# Start an interactive CLI & REPL session
docker compose run --rm adesh

# Run adesh doctor or a healthcheck
docker compose run --rm runner

# Start ALS (Language Server) for remote editors
docker compose up als
```

---

## 💻 Dev Containers / Remote IDE Setup

To use AdeshLang in VS Code Dev Containers or remote LSP setups, configure `.devcontainer/devcontainer.json`:

```json
{
  "name": "AdeshLang Dev Environment",
  "image": "adeshlang:latest",
  "workspaceFolder": "/workspace",
  "customizations": {
    "vscode": {
      "settings": {
        "adesh.languageServer.path": "/opt/adeshlang/bin/als"
      }
    }
  }
}
```

---

## 🌐 Deploying Containerized Adesh Applications

### Example Multi-Stage App Dockerfile

To package an Adesh application as a minimal production container:

```dockerfile
# syntax=docker/dockerfile:1
FROM adeshlang:latest AS builder
WORKDIR /app
COPY . .
# Run checks or ahead-of-time builds
RUN adesh check main.adesh

FROM adeshlang:slim AS runner
WORKDIR /app
COPY --from=builder /app /app
EXPOSE 8080
CMD ["adesh", "run", "main.adesh"]
```

---

## 🔒 Security & Best Practices

- **Non-root user**: The container runs under non-privileged user `adesh` (UID `1000`, GID `1000`) by default.
- **SSRF Protection**: Adesh's built-in networking restricts unsafe local address probing unless configured.
- **Clean builds**: `.dockerignore` excludes temporary files and local target directories for fast, small build contexts.
