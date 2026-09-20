//! Build MLIR from the upstream LLVM source when prebuilt binaries lack the
//! GPU dialect plugins (NVVM for CUDA, ROCDL for ROCm).
//!
//! The GPU pipeline lowers AdeshLang IR to MLIR with the `gpu` dialect and
//! needs `mlir-opt`/`mlir-translate` that register NVVM/ROCDL translations.
//! Official LLVM release binaries usually do not include them, so
//! `adesh toolchain install --build-mlir-source` builds `mlir-opt`,
//! `mlir-translate`, `llc`, and `clang` from the pinned upstream source
//! (`llvm-project-18.1.8.src.tar.xz`) with:
//!
//! - `-DMLIR_ENABLE_CUDA=ON` and `-DMLIR_ENABLE_ROCM=ON`
//! - `-DLLVM_ENABLE_PROJECTS="mlir;clang"` (mlir pulls in llvm)
//!
//! Requirements: CMake ≥ 3.20, Ninja (recommended) or Make, a C++ compiler,
//! and Python 3. The build takes roughly 30–90 minutes depending on hardware.

use std::path::{Path, PathBuf};
use std::process::Command;

use super::manifest::LLVM_RELEASE_TAG;

/// Minimal MLIR module whose translation requires NVVM dialect support.
const NVVM_PROBE: &str = r#"module attributes {gpu.container_module} {
  gpu.module @probe {
    gpu.func @kernel(%arg0: memref<f32>) kernel {
      gpu.return
    }
  }
}
"#;

/// Upstream source tarball URL for the pinned LLVM release.
pub fn source_url() -> String {
    if let Ok(repo) = std::env::var("ADESH_LLVM_SOURCE_URL") {
        return repo;
    }
    format!(
        "https://github.com/llvm/llvm-project/releases/download/{tag}/llvm-project-18.1.8.src.tar.xz",
        tag = LLVM_RELEASE_TAG
    )
}

/// True when the toolchain's `mlir-translate` can lower `gpu.module` IR —
/// i.e. the NVVM/ROCDL dialects are registered.
pub fn mlir_gpu_dialects_available(toolchain_bin: &Path) -> bool {
    let mlir_translate = toolchain_bin.join(if cfg!(windows) {
        "mlir-translate.exe"
    } else {
        "mlir-translate"
    });
    if !mlir_translate.is_file() {
        return false;
    }
    let temp = std::env::temp_dir().join("adesh-mlir-gpu-probe.mlir");
    if std::fs::write(&temp, NVVM_PROBE).is_err() {
        return false;
    }
    let output = Command::new(&mlir_translate)
        .arg("--mlir-to-llvmir")
        .arg(&temp)
        .output();
    let _ = std::fs::remove_file(&temp);
    matches!(output, Ok(o) if o.status.success())
}

/// Locate a build tool on PATH.
fn find_tool(names: &[&str]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for name in names {
            let candidate = dir.join(if cfg!(windows) {
                format!("{name}.exe")
            } else {
                name.to_string()
            });
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

pub struct BuildRequirements {
    pub missing: Vec<&'static str>,
}

/// Check the host for the tools needed to build LLVM/MLIR from source.
pub fn check_requirements() -> BuildRequirements {
    let mut missing = Vec::new();
    if find_tool(&["cmake"]).is_none() {
        missing.push("cmake (https://cmake.org/download/)");
    }
    if find_tool(&["ninja", "make"]).is_none() {
        missing.push("ninja (recommended) or make");
    }
    let compiler = find_tool(&["clang", "clang++", "gcc", "g++", "cl"]);
    if compiler.is_none() {
        missing.push("a C++ compiler (clang, gcc, or MSVC)");
    }
    BuildRequirements { missing }
}

/// Build MLIR (with NVVM/ROCDL) and clang/llc from upstream source into
/// `llvm_root` (`<home>/toolchain/llvm`). Returns the build directory so
/// repeated builds can be incremental.
pub fn build_mlir_from_source(source_url: &str, llvm_root: &Path) -> Result<PathBuf, String> {
    let requirements = check_requirements();
    if !requirements.missing.is_empty() {
        return Err(format!(
            "Building MLIR from source requires: {}. Install them and re-run.",
            requirements.missing.join(", ")
        ));
    }

    let work_root = std::env::temp_dir().join("adesh-mlir-build");
    std::fs::create_dir_all(&work_root)
        .map_err(|e| format!("Failed to create {}: {e}", work_root.display()))?;
    let archive = work_root.join(format!("{LLVM_RELEASE_TAG}.src.tar.xz"));
    let source_dir = work_root.join("llvm-project-src");
    let build_dir = work_root.join("build");

    // 1. Download (reuse when already fetched).
    if !archive.is_file() {
        println!("  ▶ Downloading MLIR/LLVM source: {source_url}");
        let result = super::download::download_to(source_url, &archive)?;
        println!("  ✓ Source downloaded ({})", result.bytes);
    } else {
        println!("  ✓ Reusing downloaded source archive");
    }

    // 2. Extract (reuse when already extracted).
    let llvm_source = source_dir.join("llvm");
    if !llvm_source.is_dir() {
        println!("  ▶ Extracting source");
        if source_dir.exists() {
            std::fs::remove_dir_all(&source_dir)
                .map_err(|e| format!("Failed to clean {}: {e}", source_dir.display()))?;
        }
        super::archive::extract(&archive, "tar.xz", &source_dir)?;
        let root = std::fs::read_dir(&source_dir)
            .map_err(|e| format!("Failed to list {}: {e}", source_dir.display()))?
            .flatten()
            .next()
            .map(|e| e.path())
            .ok_or_else(|| "Source archive was empty".to_string())?;
        std::fs::rename(&root, &source_dir)
            .map_err(|e| format!("Failed to prepare source directory: {e}"))?;
    }

    // 3. Configure.
    std::fs::create_dir_all(&build_dir)
        .map_err(|e| format!("Failed to create {}: {e}", build_dir.display()))?;
    let generator = find_tool(&["ninja"]).map(|_| "Ninja".to_string());
    let mut configure = Command::new(find_tool(&["cmake"]).unwrap());
    configure
        .arg(format!("-S={}", source_dir.join("llvm").display()))
        .arg(format!("-B={}", build_dir.display()))
        .arg("-DCMAKE_BUILD_TYPE=Release")
        .arg(r#"-DLLVM_ENABLE_PROJECTS="mlir;clang""#)
        .arg("-DLLVM_TARGETS_TO_BUILD=X86;AArch64;NVPTX;AMDGPU")
        .arg("-DMLIR_ENABLE_CUDA=ON")
        .arg("-DMLIR_ENABLE_ROCM=ON")
        .arg("-DLLVM_INCLUDE_EXAMPLES=OFF")
        .arg("-DLLVM_INCLUDE_TESTS=OFF")
        .arg("-DLLVM_INCLUDE_BENCHMARKS=OFF")
        .arg("-DLLVM_ENABLE_ASSERTIONS=OFF")
        .arg("-DLLVM_OPTIMIZED_TABLEGEN=ON");
    if let Some(generator) = generator {
        configure.arg(format!("-G={generator}"));
    }
    println!("  ▶ Configuring (this picks the host toolchain)");
    let status = configure
        .output()
        .map_err(|e| format!("Failed to run cmake: {e}"))?;
    if !status.status.success() {
        return Err(format!(
            "CMake configure failed:\n{}",
            String::from_utf8_lossy(&status.stderr)
        ));
    }

    // 4. Build only the tools we need.
    println!("  ▶ Building mlir-opt, mlir-translate, clang, llc (30–90 minutes)");
    let mut build = Command::new(find_tool(&["cmake"]).unwrap());
    build
        .arg(format!("--build={}", build_dir.display()))
        .arg("--target")
        .arg("mlir-opt")
        .arg("mlir-translate")
        .arg("clang")
        .arg("llc")
        .arg("--");
    let parallel = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    build.arg(format!("-j{parallel}"));
    let status = build
        .output()
        .map_err(|e| format!("Failed to run cmake --build: {e}"))?;
    if !status.status.success() {
        return Err(format!(
            "Build failed:\n{}",
            String::from_utf8_lossy(&status.stderr)
        ));
    }

    // 5. Install into the shared toolchain root via a copy (no privilege
    //    escalation needed; cmake --install would want a prefix layout).
    let bin_out = build_dir.join("bin");
    let dest_bin = llvm_root.join("bin");
    std::fs::create_dir_all(&dest_bin)
        .map_err(|e| format!("Failed to create {}: {e}", dest_bin.display()))?;
    let tools = ["mlir-opt", "mlir-translate", "clang", "llc"];
    for tool in tools {
        let tool_name = if cfg!(windows) {
            format!("{tool}.exe")
        } else {
            tool.to_string()
        };
        let src = bin_out.join(&tool_name);
        if !src.is_file() {
            return Err(format!(
                "Build completed but `{}` was not produced — report this bug",
                src.display()
            ));
        }
        std::fs::copy(&src, dest_bin.join(&tool_name))
            .map_err(|e| format!("Failed to install {tool_name}: {e}"))?;
    }
    println!("  ✓ Built tools installed to {}", dest_bin.display());
    Ok(build_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_url_points_at_upstream_llvm() {
        let url = source_url();
        assert!(url.starts_with("https://github.com/llvm/llvm-project/releases/"));
        assert!(url.ends_with("llvm-project-18.1.8.src.tar.xz"));
    }

    #[test]
    fn probe_is_a_gpu_module() {
        assert!(NVVM_PROBE.contains("gpu.module"));
        assert!(NVVM_PROBE.contains("gpu.container_module"));
    }

    #[test]
    fn dialect_check_fails_without_toolchain() {
        let missing = Path::new("Z:/definitely-not-a-toolchain");
        assert!(!mlir_gpu_dialects_available(missing));
    }
}
