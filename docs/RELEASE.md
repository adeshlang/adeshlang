# AdeshLang Release Engineering Guide

Step-by-step instructions for building, packaging, and launching official
AdeshLang releases on Windows, Linux, and macOS.

Everything in this guide runs from the repository root. All build outputs land
under `dist/` (gitignored). The two machine-readable configuration anchors are:

| File | Role |
|------|------|
| `Cargo.toml` | Single source of truth for the product version |
| `installer/manifests/toolchain-manifest.json` | Pins the upstream LLVM/MLIR toolchain (URLs + SHA-256) downloaded **at install time** |
| `ai/models/manifest.json` | Pins the AI model artifacts (URLs + SHA-256). The default Q4_0 model is **bundled** into every distribution |

---

## 1. What every distribution contains

| Payload | Source | Notes |
|---------|--------|-------|
| `bin/adesh`, `bin/adl`, `bin/als`, `bin/adesh-editor` | `cargo build --release` | Core binaries |
| `std/*.adesh` | `src/stdlib/` | Standard library sources |
| `config/toolchain-manifest.json` | `installer/manifests/` | LLVM/MLIR download contract |
| `licenses/` | `LICENSE` + `THIRD_PARTY_LICENSES/` | Legal texts |
| `ai/models/adesh-coder-0.5b-q4_0.gguf` (~275 MB) | local `ai/models/` or HuggingFace (per manifest) | Bundled AI model |
| `ai/models/manifest.json`, `ai/deploy/ollama/Modelfile`, `ai/deploy/openrouter/config.json` | `ai/models`, `ai/deploy` | AI model manifest + Ollama/OpenRouter server configs |

The LLVM/MLIR toolchain is **never bundled**. Installers download it at install
time from the official llvm-project releases (SHA-256 verified against the
toolchain manifest) and place it **inside the AdeshLang installation** under
`<install-dir>\toolchain\llvm` — never directly into `C:\Program Files\LLVM` —
and register it system-wide. If the machine already has an LLVM toolchain
(any version), nothing is downloaded: the existing one is reused and exposed.
Uninstalling AdeshLang removes the whole toolchain with it. The AI model Q4_0
quantization **is bundled**, so `adesh ai` works offline immediately after
install; larger quantizations (Q8_0, F16) are opt-in via `adesh ai setup`.

---

## 2. Prerequisites per OS

### Windows (10/11 x64; ARM64 cross-compile optional)

```powershell
# Rust (1.70+, Edition 2024) — https://rustup.rs/
rustc --version

# Inno Setup 6 (GUI installer) — for the setup EXE
choco install innosetup --version=6.4.3 --yes
# or install from https://jrsoftware.org/isdl.php

# WiX v5 CLI (MSI + Burn bundle) — v7 requires a separate OSMF EULA; use v5
dotnet tool install --global wix --version 5.0.2
wix extension add -g WixToolset.Bal.wixext/5.0.2
wix extension add -g WixToolset.Util.wixext/5.0.2
```

### Linux (glibc 2.28+; x86_64 and aarch64)

```bash
rustc --version
# Debian packaging:
sudo apt-get install dpkg-deb        # usually preinstalled on Debian/Ubuntu
# RPM packaging:
sudo apt-get install rpm             # provides rpmbuild (Debian/Ubuntu)
# Fedora/RHEL: rpm is preinstalled; install dpkg via 'dnf install dpkg-dev' if
# you also want .deb artifacts on a Fedora build host.
```

### macOS (12+; arm64 and x86_64)

```bash
rustc --version
xcode-select --install               # pkgbuild, productbuild, hdiutil
# Optional: pretty DMG backgrounds
brew install create-dmg
```

### Cross-platform build-time options

| Variable / flag | Effect |
|---|---|
| `-SkipAiModel` (Windows script) / `ADESH_SKIP_AI_MODEL=1` (Unix) | Build a model-less distribution (installers then rely on `adesh ai setup`) |
| `ADESH_REPO` env | Overrides the release repository URL (`adeshlang/adeshlang` by default) |
| `ADESH_SIGNING_IDENTITY` | macOS: codesign / productbuild --sign identity |
| `ADESH_NOTARY_PROFILE` | macOS: notarytool keychain profile (sign + staple automatically) |

If the local `ai/models/` checkout has no Q4_0 file, the build scripts
download it from the URL pinned in `ai/models/manifest.json` (HuggingFace:
`https://huggingface.co/adeshlang/adesh-coder-0.5b`) and verify the SHA-256
before staging.

---

## 3. Windows release builds

### 3.1 Portable ZIP + Inno Setup EXE (one script)

```powershell
powershell -ExecutionPolicy Bypass -File scripts\release\build_windows_dist.ps1 -Arch x86_64
```

What it does, in order:

1. `cargo build --release --bin adesh --bin adl` (plus `als` and
   `adesh-editor` if their crates are present)
2. Stages `dist\windows-x86_64\{bin,std,config,licenses,ai}\`
3. Stages + SHA-256 verifies the AI model (downloads it if missing)
4. Writes `dist\AdeshLang-0.3.0-x86_64-windows-portable.zip`
5. If `ISCC.exe` is available, compiles
   `installer\windows\installer.iss` →
   `installer\windows\Output\AdeshLang-0.3.0-x86_64-windows-setup.exe`

ARM64 Windows: add `-Arch aarch64` (produces the portable ZIP only; the GUI
installer step is x86_64-only today).

### 3.2 Enterprise MSI (WiX v5)

Requires the staged `dist\windows-x86_64\` from step 3.1:

```powershell
$env:Path = "$env:USERPROFILE\.dotnet\tools;$env:Path"
powershell -ExecutionPolicy Bypass -File installer\windows\msi\build-msi.ps1
# -> dist\windows-x86_64\AdeshLang-0.3.0-x86_64-windows.msi
```

The MSI registers machine-wide `ADESH_HOME`, `ADESH_TOOLCHAIN`,
`ADESH_CLANG`, `ADESH_LLC`, `ADESH_MLIR_OPT`, `ADESH_MLIR_TRANSLATE`, and
`PATH`, and runs `adesh toolchain install --system` after InstallFinalize
(suppressed automatically when the bundle drives the install).

### 3.3 Burn bootstrapper bundle (MSI + toolchain chain)

Requires the MSI from step 3.2:

```powershell
$env:Path = "$env:USERPROFILE\.dotnet\tools;$env:Path"
powershell -ExecutionPolicy Bypass -File installer\windows\bundle\build-bundle.ps1
# -> dist\windows-x86_64\AdeshLang-0.3.0-x86_64-windows-bootstrapper.exe
```

The script downloads the small `vs_buildtools.exe` stub (~2 MB, official
Microsoft endpoint) at build time and embeds it. At install time the chain is:

1. `AdeshLangMsi` — core files + environment (toolchain action suppressed)
2. `AdeshToolchain` — runs `adesh toolchain install --system` (skipped when
   `ADESH_TOOLCHAIN` and the in-directory `toolchain\llvm\bin\clang.exe` are
   already present; reuses any pre-existing LLVM without downloading)
3. `VSBuildTools` — optional, off by default; enable with
   `bootstrapper.exe /v VSBuildToolsInstall=1`

---

## 4. Linux release builds

### 4.1 One script: tarball + (optional) .deb + .rpm

```bash
chmod +x scripts/release/package_unix.sh
./scripts/release/package_unix.sh
# auto-detects arch (x86_64/aarch64) and distro tooling:
#   -> dist/AdeshLang-<version>-<arch>-linux-gnu.tar.xz
#   -> dist/adeshlang_<version>_<arch>.deb      (needs dpkg-deb)
#   -> dist/rpm/*.rpm                           (needs rpmbuild)
```

Explicit arguments (useful for cross-builds):

```bash
./scripts/release/package_unix.sh <version> <arch> <os> <target-triple>
# example:
./scripts/release/package_unix.sh 0.3.0 aarch64 linux aarch64-unknown-linux-gnu
```

Cross-compiling requires the target's Rust std library:

```bash
rustup target add aarch64-unknown-linux-gnu
```

### 4.2 Individual packages

```bash
# Debian/Ubuntu (amd64 | arm64)
bash installer/linux/deb/build-deb.sh amd64
# -> dist/adeshlang_0.3.0_amd64.deb

# RPM (x86_64 | aarch64)
bash installer/linux/rpm/build-rpm.sh x86_64
# -> dist/rpm/*.rpm

# With the toolchain installed during package configuration (advanced):
bash installer/linux/rpm/build-rpm.sh x86_64 --with-toolchain
```

### 4.3 The offline install script (what end users run)

```bash
# Interactive: downloads the tarball from GitHub Releases, installs to
# /opt/adeshlang (root) or ~/.adeshlang (user), links binaries, writes
# /etc/profile.d/adeshlang.sh or a user profile snippet, and offers the
# pinned LLVM toolchain download.
curl -fsSL https://raw.githubusercontent.com/adeshlang/adeshlang/main/installer/linux/install.sh | bash
# or offline/air-gapped with a pre-downloaded tarball:
bash install.sh AdeshLang-0.3.0-x86_64-linux-gnu.tar.xz
```

---

## 5. macOS release builds

### 5.1 Stage + build the tarball (also feeds pkg/dmg)

```bash
./scripts/release/package_unix.sh 0.3.0 arm64 macos aarch64-apple-darwin
# -> dist/AdeshLang-0.3.0-macos-arm64.tar.xz  (staging: dist/macos-arm64/)
```

### 5.2 .pkg installer

```bash
chmod +x installer/macos/build-pkg.sh
./installer/macos/build-pkg.sh arm64
# -> dist/AdeshLang-0.3.0-arm64-macos.pkg
# always-on toolchain variant:
./installer/macos/build-pkg.sh arm64 --with-toolchain
```

Installs to `/Library/Application Support/AdeshLang`, links the four binaries
into `/usr/local/bin`, and writes `/etc/profile.d/adeshlang.sh` (ADESH_HOME +
PATH). The Distribution presents the toolchain choice; users can also run
`adesh toolchain install --system` afterwards.

### 5.3 .dmg drag-install image

```bash
chmod +x installer/macos/build-dmg.sh
./installer/macos/build-dmg.sh arm64
# -> dist/AdeshLang-0.3.0-arm64-macos.dmg
```

The image contains an `AdeshLang/` folder (bin, std, config, licenses, ai)
plus the double-clickable `Install-AdeshLang.command`.

### 5.4 Signed + notarized builds (CI or local)

```bash
export ADESH_SIGNING_IDENTITY="Developer ID Installer: Your Name (TEAMID)"
export ADESH_NOTARY_PROFILE="notary-profile"   # xcrun notarytool store-credentials ...
./installer/macos/build-pkg.sh arm64           # signs, notarizes, staples
./installer/macos/build-dmg.sh arm64            # signs, notarizes, staples
```

---

## 6. Deploying the AI model to servers (optional)

The bundled `ai/deploy/` configs wire the model into standard servers:

```bash
# Ollama (uses the bundled Modelfile; run next to ai/models/)
ollama create adeshlang -f ai/deploy/ollama/Modelfile

# llama.cpp server (bundled Q4_0 weights)
llama-server -m ai/models/adesh-coder-0.5b-q4_0.gguf --port 8080 -c 8192 -ngl 99

# vLLM (uses the merged safetensors; not bundled — download via ai/models/manifest.json)
python3 -m vllm.entrypoints.openai.api_server \
  --model ai/models/merged-latest \
  --served-model-name adeshlang/adesh-coder-0.5b --port 8000 --max-model-len 32768
```

See `ai/deploy/openrouter/config.json` for the OpenRouter-hosted deployment
descriptor (model naming, pricing, architecture).

---

## 7. Launching a release (GitHub Actions)

The automated path builds all five host platforms from a tag.

### 7.1 Pre-flight checklist

1. **Version bump** — the version string lives in several places; update all
   of them together (see §7.2).
2. **Toolchain manifest** — every `sha256` field in
   `installer/manifests/toolchain-manifest.json` must be filled. The
   `.github/workflows/update-toolchain-manifest.yml` workflow (manual
   dispatch) recomputes them from the pinned URLs.
3. **AI model** — the HuggingFace repo
   `https://huggingface.co/adeshlang/adesh-coder-0.5b` must contain the three
   GGUFs at the URLs pinned by `ai/models/manifest.json` (CI runners do not
   have the weights in the repository; they download and verify them).
   Alternatively set `ADESH_SKIP_AI_MODEL=1` in the workflow environment for
   a model-less release.
4. **macOS signing/notarization** (optional) — add `ADESH_SIGNING_IDENTITY`
   and `ADESH_NOTARY_PROFILE` secrets to the repository settings.
5. **Repository URL** — `manifest.rs`, `release.yml`, and the installers all
   default to `github.com/adeshlang/adeshlang`; the `ADESH_REPO` env var
   overrides it everywhere if the migration destination changes.

### 7.2 Version bump checklist (0.3.0 → 0.4.0 example)

| File | What changes |
|------|--------------|
| `Cargo.toml` | `version = "0.4.0"` (drives package_unix.sh, build-pkg/dmg, release.yml) |
| `installer/windows/installer.iss` | `#define MyAppVersion` and `OutputBaseFilename` |
| `installer/windows/msi/Product.wxs` | `<?define ProductVersion ... ?>` |
| `installer/windows/bundle/Bundle.wxs` | `<?define ProductVersion ... ?>` (4-part, e.g. `0.4.0.0`) |
| `scripts/release/build_windows_dist.ps1` | hardcoded portable-zip name |
| `installer/windows/msi/build-msi.ps1` | hardcoded `.msi` name |
| `installer/windows/bundle/build-bundle.ps1` | hardcoded bootstrapper name |
| `installer/linux/install.sh` | `VERSION=` and `REPO_URL_BASE=.../releases/download/v0.4.0/` |
| `installer/linux/deb/build-deb.sh` | default `version` argument |
| `installer/linux/rpm/build-rpm.sh` + `adeshlang.spec` | default `version` / spec `Version:` |
| `installer/macos/scripts/choices.xml` | `version="0.4.0"` |
| `README.md` and `docs/README.md` version badge/heading | marketing version |

### 7.3 Tag and push

```bash
git add -A
git commit -m "Release v0.4.0"
git tag v0.4.0
git push origin main --follow-tags   # or: git push origin v0.4.0
```

Pushing the tag triggers `.github/workflows/release.yml`:

- 5-platform matrix (windows-x86_64, linux-x86_64, linux-aarch64,
  macos-arm64, macos-x86_64) builds: portable ZIP, setup EXE, MSI, bundle,
  tar.xz, .deb, .rpm, .pkg, .dmg — each embedding the verified AI model.
- `checksums` job downloads everything and writes `SHA256SUMS.txt`.
- `release` job publishes the GitHub release with all artifacts,
  `SHA256SUMS.txt`, and `toolchain-manifest.json` as a stable asset.

Watch progress: **Actions → Release → <tag run>**. Total time is roughly
25–40 minutes (the release-optimized Rust builds dominate).

### 7.4 Manual publishing fallback

If Actions is unavailable, build the artifacts per §3–§5 on each OS, then:

```bash
gh release create v0.4.0 dist/* \
  --repo adeshlang/adeshlang \
  --title "AdeshLang v0.4.0" \
  --generate-notes
sha256sum dist/* > dist/SHA256SUMS.txt && gh release upload v0.4.0 dist/SHA256SUMS.txt --repo adeshlang/adeshlang
```

### 7.5 Post-release verification

```powershell
# Windows: fresh-VM checks
.\AdeshLang-0.4.0-x86_64-windows-setup.exe /SILENT     # then: adesh doctor; adesh ai status
msiexec /i AdeshLang-0.4.0-x86_64-windows.msi /qn      # silent enterprise path
```

```bash
# Linux
sudo apt install ./adeshlang_0.4.0_amd64.deb && adesh doctor && adesh ai status
# macOS
sudo installer -pkg AdeshLang-0.4.0-arm64-macos.pkg -target / && adesh doctor
```

Each check should show the core binaries on PATH, the bundled AI model as
`Tier 1 (Native GGUF): ACTIVE`, and the toolchain either installed or one
`adesh toolchain install --system` away.

---

## 8. Troubleshooting

| Symptom | Cause / fix |
|---------|-------------|
| `LNK1104: cannot open file ...deps\adeshlang-*.exe` | A stale test/installer process holds the binary. Kill `adeshlang-*` / `cargo` / `rustc` processes and rebuild. |
| WiX `WIX7015` (OSMF EULA) | WiX v7 is installed. `dotnet tool uninstall --global wix` then install 5.0.2 (§2). |
| WiX `WIX0144: extension 'WixToolset.Bal.wixext' could not be found` | Extension cache damaged (usually v7 residue). Delete `%USERPROFILE%\.wix\extensions` and re-add both v5 extensions. The build script also resolves the Bal DLL by path, since the official 5.0.2 package ships it as `WixToolset.BootstrapperApplications.wixext.dll`. |
| `error WIX0118: Additional argument '-dName=Value'` | WiX v3-style attached define. The checked-in scripts pass `-d Name=Value`; keep that form. |
| `SHA-256 mismatch for ...gguf` | The local model file differs from `ai/models/manifest.json` (partial download or re-export). Delete the file and rebuild — the script re-downloads and re-verifies. |
| `Blocking waiting for file lock on build directory` | Another cargo invocation is running in the same workspace. Serialize builds; never run two release scripts concurrently. |
| ISCC not found in CI | release.yml installs Inno via Chocolatey and falls back to Program Files paths; a missing ISCC only skips the setup EXE, the ZIP still publishes. |
| deb/rpm steps skipped locally | `dpkg-deb` / `rpmbuild` not installed (§2). The tarball is always produced. |
| musl Linux, glibc < 2.28 | Unsupported by the install script; use a glibc 2.28+ distribution or build from source. |

---

## 8. Uninstalling

Every packaging technology ships with a complete uninstaller that removes the
binaries, the bundled AI model, the standard library, and the environment
variables/PATH entries it registered.

| Platform | Uninstall method |
|----------|------------------|
| Windows (setup EXE) | Settings → Apps → **AdeshLang**, or run `C:\Program Files\AdeshLang\unins000.exe`. The uninstaller also removes the machine `ADESH_*` variables, the `PATH` entries it added (including the in-directory toolchain `toolchain\llvm\bin`), the `.adl`/`.adesh` file associations, and the toolchain it installed inside the program directory. A pre-existing system LLVM at `C:\Program Files\LLVM` is left untouched with an explanatory notice. |
| Windows (MSI) | `msiexec /x AdeshLang-<version>-x86_64-windows.msi` or Settings → Apps. Same environment cleanup as above. |
| Windows (bootstrapper) | Settings → Apps → **AdeshLang** (the bootstrapper chains the MSI uninstall). |
| Linux (.deb) | `sudo apt remove adeshlang` (add `--purge` to also drop config). Removes `/usr/lib/adeshlang`, `/usr/bin` symlinks, and the profile script. |
| Linux (.rpm) | `sudo rpm -e adeshlang` / `sudo dnf remove adeshlang`. |
| Linux (tar + install.sh) | `sudo bash installer/linux/uninstall.sh` (system) — removes `/opt/adeshlang`, `/usr/local/bin` links, and `/etc/profile.d/adeshlang.sh`. User installs: delete `~/.adeshlang` and the `~/.local/bin` links. |
| macOS (.pkg) | `sudo bash installer/macos/uninstall.sh` — removes `/Library/Application Support/AdeshLang`, `/usr/local/bin` links, and `/etc/profile.d/adeshlang.sh` (macOS packages have no built-in uninstaller; this script is the supported path). |
| macOS (.dmg) | Drag-installs are removed by deleting the `AdeshLang` folder; the `.command` installer writes the same profile files as the .pkg, so run `uninstall.sh` if it was used. |

The toolchain downloaded at install time lives inside the installation
directory (`<install-dir>\toolchain\llvm`) on every platform, so it is removed
with the package. A pre-existing system LLVM (found before the install, on any
platform) is never touched by an uninstaller.

---

## 9. Quick reference — artifact map

| Platform | Artifacts (dist/) |
|----------|-------------------|
| Windows x64 | `AdeshLang-<v>-x86_64-windows-portable.zip`, `...-x86_64-windows-setup.exe`, `...-x86_64-windows.msi`, `...-x86_64-windows-bootstrapper.exe` |
| Windows ARM64 | portable ZIP only |
| Linux x64 / ARM64 | `AdeshLang-<v>-<arch>-linux-gnu.tar.xz`, `adeshlang_<v>_<arch>.deb`, `rpm/*.rpm` |
| macOS arm64 / x64 | `AdeshLang-<v>-<arch>-macos.tar.xz`, `AdeshLang-<v>-<arch>-macos.pkg`, `AdeshLang-<v>-<arch>-macos.dmg` |
| All platforms | + `SHA256SUMS.txt` and `toolchain-manifest.json` (release assets) |
