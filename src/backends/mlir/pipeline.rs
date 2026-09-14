//! MLIR Pipeline Executor
//!
//! Orchestrates the MLIR compilation pipeline: .mlir → LLVM IR → assembly → binary
//! Handles subprocess invocation of mlir-opt, mlir-translate, llc, clang

use crate::backends::common::vir_adapter::{BackendError, BackendResult};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// MLIR toolchain configuration
#[derive(Clone)]
pub struct MlirToolchain {
    pub mlir_opt: PathBuf,
    pub mlir_translate: PathBuf,
    pub llc: PathBuf,
    pub clang: PathBuf,
}

impl Default for MlirToolchain {
    fn default() -> Self {
        Self {
            mlir_opt: std::env::var("ADESH_MLIR_OPT")
                .unwrap_or_else(|_| "mlir-opt".to_string())
                .into(),
            mlir_translate: std::env::var("ADESH_MLIR_TRANSLATE")
                .unwrap_or_else(|_| "mlir-translate".to_string())
                .into(),
            llc: std::env::var("ADESH_LLC")
                .unwrap_or_else(|_| "llc".to_string())
                .into(),
            clang: std::env::var_os("ADESH_CLANG")
                .map(PathBuf::from)
                .or_else(|| {
                    crate::toolchain::resolver::bundled_root()
                        .map(|root| root.join("bin").join(if cfg!(windows) { "clang.exe" } else { "clang" }))
                })
                .unwrap_or_else(|| PathBuf::from("clang")),
        }
    }
}

/// MLIR pipeline stage
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineStage {
    GpuKernelOutlining,
    LlvmDialectLowering,
    MlirToLlvmIR,
    LlvmIRToAssembly,
    AssemblyToObject,
    LinkExecutable,
}

impl PipelineStage {
    pub fn name(&self) -> &'static str {
        match self {
            Self::GpuKernelOutlining => "GPU kernel outlining",
            Self::LlvmDialectLowering => "LLVM dialect lowering",
            Self::MlirToLlvmIR => "MLIR→LLVM IR translation",
            Self::LlvmIRToAssembly => "LLVM IR→Assembly",
            Self::AssemblyToObject => "Assembly→Object file",
            Self::LinkExecutable => "Linking executable",
        }
    }
}

/// Pipeline execution result
#[derive(Debug)]
pub struct PipelineResult {
    pub success: bool,
    pub stage: PipelineStage,
    pub stdout: String,
    pub stderr: String,
    pub output_file: Option<PathBuf>,
}

/// MLIR compilation pipeline
#[allow(dead_code)]
pub struct MlirPipeline {
    toolchain: MlirToolchain,
    keep_intermediates: bool,
}

impl MlirPipeline {
    pub fn new() -> Self {
        Self {
            toolchain: MlirToolchain::default(),
            keep_intermediates: true, // Always keep for debugging
        }
    }

    pub fn with_toolchain(toolchain: MlirToolchain) -> Self {
        Self {
            toolchain,
            keep_intermediates: true,
        }
    }

    /// Run GPU kernel outlining pass
    /// Input: .mlir file
    /// Output: .lowered.mlir file
    pub fn run_gpu_outlining(&self, input: &Path, output: &Path) -> BackendResult<PipelineResult> {
        let mut cmd = Command::new(&self.toolchain.mlir_opt);
        cmd.arg(input)
            .arg("--gpu-kernel-outlining")
            .arg("--canonicalize")
            .arg("-o")
            .arg(output)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = cmd
            .spawn()
            .map_err(|e| BackendError::RuntimeError(format!("Failed to spawn mlir-opt: {}", e)))?;

        let result = child.wait_with_output().map_err(|e| {
            BackendError::RuntimeError(format!("Failed to wait for mlir-opt: {}", e))
        })?;

        Ok(PipelineResult {
            success: result.status.success(),
            stage: PipelineStage::GpuKernelOutlining,
            stdout: String::from_utf8_lossy(&result.stdout).to_string(),
            stderr: String::from_utf8_lossy(&result.stderr).to_string(),
            output_file: if result.status.success() {
                Some(output.to_path_buf())
            } else {
                None
            },
        })
    }

    /// Run LLVM dialect lowering pass
    /// Input: .lowered.mlir file
    /// Output: .llvm_ready.mlir file
    pub fn run_llvm_lowering(&self, input: &Path, output: &Path) -> BackendResult<PipelineResult> {
        let mut cmd = Command::new(&self.toolchain.mlir_opt);
        cmd.arg(input)
            .arg("--convert-func-to-llvm")
            .arg("--reconcile-unrealized-casts")
            .arg("-o")
            .arg(output)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = cmd
            .spawn()
            .map_err(|e| BackendError::RuntimeError(format!("Failed to spawn mlir-opt: {}", e)))?;

        let result = child.wait_with_output().map_err(|e| {
            BackendError::RuntimeError(format!("Failed to wait for mlir-opt: {}", e))
        })?;

        Ok(PipelineResult {
            success: result.status.success(),
            stage: PipelineStage::LlvmDialectLowering,
            stdout: String::from_utf8_lossy(&result.stdout).to_string(),
            stderr: String::from_utf8_lossy(&result.stderr).to_string(),
            output_file: if result.status.success() {
                Some(output.to_path_buf())
            } else {
                None
            },
        })
    }

    /// Translate MLIR to LLVM IR
    /// Input: .llvm_ready.mlir file
    /// Output: .ll file
    pub fn translate_to_llvm_ir(
        &self,
        input: &Path,
        output: &Path,
    ) -> BackendResult<PipelineResult> {
        let mut cmd = Command::new(&self.toolchain.mlir_translate);
        cmd.arg("--mlir-to-llvmir")
            .arg(input)
            .arg("-o")
            .arg(output)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = cmd.spawn().map_err(|e| {
            BackendError::RuntimeError(format!("Failed to spawn mlir-translate: {}", e))
        })?;

        let result = child.wait_with_output().map_err(|e| {
            BackendError::RuntimeError(format!("Failed to wait for mlir-translate: {}", e))
        })?;

        Ok(PipelineResult {
            success: result.status.success(),
            stage: PipelineStage::MlirToLlvmIR,
            stdout: String::from_utf8_lossy(&result.stdout).to_string(),
            stderr: String::from_utf8_lossy(&result.stderr).to_string(),
            output_file: if result.status.success() {
                Some(output.to_path_buf())
            } else {
                None
            },
        })
    }

    /// Compile LLVM IR to assembly
    /// Input: .ll file
    /// Output: .s file
    pub fn compile_to_assembly(
        &self,
        input: &Path,
        output: &Path,
    ) -> BackendResult<PipelineResult> {
        let mut cmd = Command::new(&self.toolchain.llc);
        cmd.arg(input)
            .arg("-o")
            .arg(output)
            .arg("-filetype=asm")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = cmd
            .spawn()
            .map_err(|e| BackendError::RuntimeError(format!("Failed to spawn llc: {}", e)))?;

        let result = child
            .wait_with_output()
            .map_err(|e| BackendError::RuntimeError(format!("Failed to wait for llc: {}", e)))?;

        Ok(PipelineResult {
            success: result.status.success(),
            stage: PipelineStage::LlvmIRToAssembly,
            stdout: String::from_utf8_lossy(&result.stdout).to_string(),
            stderr: String::from_utf8_lossy(&result.stderr).to_string(),
            output_file: if result.status.success() {
                Some(output.to_path_buf())
            } else {
                None
            },
        })
    }

    /// Link to executable
    /// Input: .s file or .o file
    /// Output: executable
    pub fn link_executable(&self, input: &Path, output: &Path) -> BackendResult<PipelineResult> {
        let mut cmd = Command::new(&self.toolchain.clang);
        cmd.arg(input)
            .arg("-o")
            .arg(output)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = cmd
            .spawn()
            .map_err(|e| BackendError::RuntimeError(format!("Failed to spawn clang: {}", e)))?;

        let result = child
            .wait_with_output()
            .map_err(|e| BackendError::RuntimeError(format!("Failed to wait for clang: {}", e)))?;

        Ok(PipelineResult {
            success: result.status.success(),
            stage: PipelineStage::LinkExecutable,
            stdout: String::from_utf8_lossy(&result.stdout).to_string(),
            stderr: String::from_utf8_lossy(&result.stderr).to_string(),
            output_file: if result.status.success() {
                Some(output.to_path_buf())
            } else {
                None
            },
        })
    }

    /// Run the full GPU MLIR pipeline
    /// Returns the final executable path or an error
    pub fn run_full_pipeline(
        &self,
        input_mlir: &Path,
        output_base: &Path,
    ) -> BackendResult<Vec<PipelineResult>> {
        let mut results = Vec::new();

        // Step 1: GPU kernel outlining + canonicalize
        let lowered = output_base.with_extension("lowered.mlir");
        let r1 = self.run_gpu_outlining(input_mlir, &lowered)?;
        let success1 = r1.success;
        results.push(r1);

        if !success1 {
            return Ok(results);
        }

        // Step 2: Convert to LLVM dialect
        let llvm_ready = output_base.with_extension("llvm_ready.mlir");
        let r2 = self.run_llvm_lowering(&lowered, &llvm_ready)?;
        let success2 = r2.success;
        results.push(r2);

        if !success2 {
            return Ok(results);
        }

        // Step 3: Translate to LLVM IR
        let llvm_ir = output_base.with_extension("ll");
        let r3 = self.translate_to_llvm_ir(&llvm_ready, &llvm_ir)?;
        let success3 = r3.success;
        results.push(r3);

        if !success3 {
            return Ok(results);
        }

        // Step 4: Compile to assembly
        let assembly = output_base.with_extension("s");
        let r4 = self.compile_to_assembly(&llvm_ir, &assembly)?;
        let success4 = r4.success;
        results.push(r4);

        if !success4 {
            return Ok(results);
        }

        // Step 5: Link executable
        let executable = if cfg!(windows) {
            output_base.with_extension("exe")
        } else {
            output_base.to_path_buf()
        };
        let r5 = self.link_executable(&assembly, &executable)?;
        results.push(r5);

        Ok(results)
    }

    /// Run CPU-only pipeline (no GPU outlining)
    pub fn run_cpu_pipeline(
        &self,
        input_mlir: &Path,
        output_base: &Path,
    ) -> BackendResult<Vec<PipelineResult>> {
        let mut results = Vec::new();

        // Step 1: Convert to LLVM dialect (skip GPU outlining)
        let llvm_ready = output_base.with_extension("llvm_ready.mlir");
        let r1 = self.run_llvm_lowering(input_mlir, &llvm_ready)?;
        let success1 = r1.success;
        results.push(r1);

        if !success1 {
            return Ok(results);
        }

        // Step 2: Translate to LLVM IR
        let llvm_ir = output_base.with_extension("ll");
        let r2 = self.translate_to_llvm_ir(&llvm_ready, &llvm_ir)?;
        let success2 = r2.success;
        results.push(r2);

        if !success2 {
            return Ok(results);
        }

        // Step 3: Compile to assembly
        let assembly = output_base.with_extension("s");
        let r3 = self.compile_to_assembly(&llvm_ir, &assembly)?;
        let success3 = r3.success;
        results.push(r3);

        if !success3 {
            return Ok(results);
        }

        // Step 4: Link executable
        let executable = if cfg!(windows) {
            output_base.with_extension("exe")
        } else {
            output_base.to_path_buf()
        };
        let r4 = self.link_executable(&assembly, &executable)?;
        results.push(r4);

        Ok(results)
    }

    /// Write MLIR content to a temporary file
    pub fn write_mlir_file(&self, content: &str, path: &Path) -> BackendResult<()> {
        fs::write(path, content)
            .map_err(|e| BackendError::RuntimeError(format!("Failed to write MLIR file: {}", e)))
    }
}

impl Default for MlirPipeline {
    fn default() -> Self {
        Self::new()
    }
}
