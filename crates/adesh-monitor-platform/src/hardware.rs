//! `hardware` — one-shot, cached hardware discovery (CPU + GPU) with live
//! system memory sampling for the ARO desktop app.
//!
//! CPU/GPU inventory rarely changes, so it is detected once and cached in a
//! `OnceLock`. Total/available memory is sampled live on every call.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Auto-detected GPU adapter.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct GpuInfo {
    pub name: String,
    pub vendor: Option<String>,
    pub vram_bytes: Option<u64>,
    pub driver_version: Option<String>,
    pub status: Option<String>,
}

/// Auto-detected processor information.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CpuInfo {
    pub name: String,
    pub physical_cores: Option<u32>,
    pub logical_cores: u32,
    pub max_clock_mhz: Option<u32>,
}

/// Full hardware report returned to the observatory frontend.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HardwareReport {
    pub os_name: String,
    pub arch: String,
    pub cpu: CpuInfo,
    pub gpus: Vec<GpuInfo>,
    pub total_memory_bytes: u64,
    pub available_memory_bytes: Option<u64>,
    pub gpu_detection_method: String,
}

static CACHED_CPU_GPU: OnceLock<(CpuInfo, Vec<GpuInfo>, String)> = OnceLock::new();

/// Cached CPU + GPU inventory (detected once per process lifetime).
pub fn detect_hardware() -> HardwareReport {
    let (cpu, gpus, method) = CACHED_CPU_GPU.get_or_init(detect_cpu_gpus);
    let (total_memory_bytes, available_memory_bytes) = system_memory();
    HardwareReport {
        os_name: os_pretty_name().to_string(),
        arch: std::env::consts::ARCH.to_string(),
        cpu: cpu.clone(),
        gpus: gpus.clone(),
        total_memory_bytes,
        available_memory_bytes,
        gpu_detection_method: method.clone(),
    }
}

fn os_pretty_name() -> &'static str {
    #[cfg(windows)]
    {
        "Windows"
    }
    #[cfg(target_os = "linux")]
    {
        "Linux"
    }
    #[cfg(target_os = "macos")]
    {
        "macOS"
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        std::env::consts::OS
    }
}

fn logical_cores() -> u32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1)
}

fn non_empty<S: AsRef<str>>(s: S) -> Option<String> {
    let t = s.as_ref().trim().to_string();
    if t.is_empty() { None } else { Some(t) }
}

/// Best-effort vendor inference from a GPU product name.
fn guess_vendor(name: &str) -> Option<String> {
    let l = name.to_lowercase();
    if l.contains("nvidia")
        || l.contains("geforce")
        || l.contains("rtx")
        || l.contains("gtx")
        || l.contains("quadro")
        || l.contains("tesla")
        || l.contains("riva")
    {
        return Some("NVIDIA".into());
    }
    if l.contains("amd") || l.contains("radeon") || l.contains("ati ") || l.contains("firepro") {
        return Some("AMD".into());
    }
    if l.contains("intel")
        || l.contains("uhd graphics")
        || l.contains("iris")
        || l.contains("arc a")
    {
        return Some("Intel".into());
    }
    if l.contains("apple") {
        return Some("Apple".into());
    }
    if l.contains("qualcomm") || l.contains("adreno") {
        return Some("Qualcomm".into());
    }
    if l.contains("arm") || l.contains("mali") {
        return Some("Arm".into());
    }
    if l.contains("microsoft") || l.contains("warp") {
        return Some("Microsoft".into());
    }
    if l.contains("vmware") {
        return Some("VMware".into());
    }
    if l.contains("virtualbox") || l.contains("vbox") {
        return Some("Oracle".into());
    }
    if l.contains("red hat") || l.contains("virtio") {
        return Some("Red Hat".into());
    }
    None
}

/// Spawn a short-lived process, capture stdout, enforce a hard timeout.
/// Never shows a console window on Windows (`CREATE_NO_WINDOW`).
fn run_capture(program: &str, args: &[&str], timeout: Duration) -> Option<String> {
    use std::io::Read;
    use std::process::{Command, Stdio};

    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW — keep the GUI free of flashing console windows
        cmd.creation_flags(0x0800_0000);
    }

    let mut child = cmd.spawn().ok()?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }

    let mut out = String::new();
    child.stdout.as_mut()?.read_to_string(&mut out).ok()?;
    Some(out)
}

/// Parse a human VRAM string such as `8 GB`, `1536 MB`, or `2147483648`.
#[allow(dead_code)]
fn parse_vram_str(raw: &str) -> Option<u64> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(rest) = s.strip_suffix("GB").or_else(|| s.strip_suffix("gb")) {
        let n: f64 = rest.trim().parse().ok()?;
        return Some((n * 1024.0 * 1024.0 * 1024.0) as u64);
    }
    if let Some(rest) = s
        .strip_suffix("MB")
        .or_else(|| s.strip_suffix("mb"))
        .or_else(|| s.strip_suffix("M"))
    {
        let n: f64 = rest.trim().parse().ok()?;
        return Some((n * 1024.0 * 1024.0) as u64);
    }
    if let Ok(n) = s.parse::<u64>() {
        // Heuristic: values below 16 MiB are almost certainly not byte counts
        return if n >= 16 * 1024 * 1024 { Some(n) } else { None };
    }
    None
}

fn detect_cpu_gpus() -> (CpuInfo, Vec<GpuInfo>, String) {
    #[cfg(windows)]
    {
        windows_impl::detect()
    }
    #[cfg(target_os = "linux")]
    {
        linux_impl::detect()
    }
    #[cfg(target_os = "macos")]
    {
        macos_impl::detect()
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        (
            CpuInfo {
                name: format!("Unknown {}", std::env::consts::ARCH),
                physical_cores: None,
                logical_cores: logical_cores(),
                max_clock_mhz: None,
            },
            Vec::new(),
            "Generic std discovery".to_string(),
        )
    }
}

// ============================================================================
// WINDOWS: Win32 CIM (WMI) + driver-store registry, via Windows PowerShell
// ============================================================================
#[cfg(windows)]
mod windows_impl {
    use super::*;

    const METHOD: &str = "Win32 CIM (WMI) + driver-store registry keys — auto-detected & cached";

    const PS_SCRIPT: &str = r#"
$ErrorActionPreference='SilentlyContinue'
$cpu = Get-CimInstance Win32_Processor | Select-Object -First 1
$regs = @(Get-ChildItem 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}' -ErrorAction SilentlyContinue |
  Where-Object { $_.PSChildName -match '^\d{4}$' } |
  ForEach-Object { Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue })
$list = @()
foreach ($g in @(Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue)) {
  $name = [string]$g.Name
  if ([string]::IsNullOrWhiteSpace($name)) { continue }
  $vram = [uint64]0
  $r = $regs | Where-Object { [string]$_.DriverDesc -eq $name } | Select-Object -First 1
  if ($r) {
    $x = $r.'HardwareInformation.qwMemorySize'
    if (-not $x) { $x = $r.'HardwareInformation.MemorySize' }
    if ($x) { try { $vram = [uint64]$x } catch { $vram = [uint64]0 } }
  }
  if ((-not $vram) -and $g.AdapterRAM) { try { $vram = [uint64]$g.AdapterRAM } catch { $vram = [uint64]0 } }
  $list += [PSCustomObject]@{
    name = $name
    vram = $vram
    driver = [string]$g.DriverVersion
    status = [string]$g.Status
  }
}
$obj = [PSCustomObject]@{
  cpu_name = if ($cpu) { [string]$cpu.Name } else { '' }
  cores = if ($cpu) { [int]$cpu.NumberOfCores } else { 0 }
  logical = if ($cpu) { [int]$cpu.NumberOfLogicalProcessors } else { 0 }
  mhz = if ($cpu) { [int]$cpu.MaxClockSpeed } else { 0 }
  gpus = @($list)
}
ConvertTo-Json -InputObject $obj -Compress -Depth 6
"#;

    fn json_u64(v: Option<&serde_json::Value>) -> Option<u64> {
        match v? {
            serde_json::Value::Number(n) => {
                let n = n.as_u64()?;
                if n == 0 { None } else { Some(n) }
            }
            serde_json::Value::String(s) => {
                if s.trim().is_empty() {
                    None
                } else {
                    Some(s.trim().parse().ok()?)
                }
            }
            _ => None,
        }
    }

    pub(super) fn detect() -> (CpuInfo, Vec<GpuInfo>, String) {
        let fallback_cpu = CpuInfo {
            name: format!("Unknown {} processor", std::env::consts::ARCH),
            physical_cores: None,
            logical_cores: logical_cores(),
            max_clock_mhz: None,
        };

        let out = run_capture(
            "powershell",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                PS_SCRIPT,
            ],
            // First WMI/CIM query can be slow while the repository warms up;
            // the result is cached for the process lifetime afterwards.
            Duration::from_secs(15),
        );

        let Some(raw) = out else {
            return (fallback_cpu, Vec::new(), METHOD.to_string());
        };
        // Strip any stray banner lines before the JSON object
        let Some(start) = raw.find('{') else {
            return (fallback_cpu, Vec::new(), METHOD.to_string());
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw[start..]) else {
            return (fallback_cpu, Vec::new(), METHOD.to_string());
        };

        let cpu = CpuInfo {
            name: non_empty(
                v.get("cpu_name")
                    .and_then(|x| x.as_str())
                    .unwrap_or_default(),
            )
            .unwrap_or_else(|| fallback_cpu.name.clone()),
            physical_cores: json_u64(v.get("cores")).map(|n| n as u32),
            logical_cores: json_u64(v.get("logical"))
                .map(|n| n as u32)
                .unwrap_or_else(logical_cores),
            max_clock_mhz: json_u64(v.get("mhz")).map(|n| n as u32),
        };

        let mut gpus = Vec::new();
        if let Some(arr) = v.get("gpus").and_then(|g| g.as_array()) {
            for g in arr {
                let Some(name) =
                    non_empty(g.get("name").and_then(|x| x.as_str()).unwrap_or_default())
                else {
                    continue;
                };
                let vendor = g
                    .get("vendor")
                    .and_then(|x| x.as_str())
                    .and_then(non_empty)
                    .filter(|s| s.len() < 64)
                    .or_else(|| guess_vendor(&name));
                gpus.push(GpuInfo {
                    vram_bytes: json_u64(g.get("vram")),
                    name,
                    vendor,
                    driver_version: g.get("driver").and_then(|x| x.as_str()).and_then(non_empty),
                    status: g.get("status").and_then(|x| x.as_str()).and_then(non_empty),
                });
            }
        }

        (cpu, gpus, METHOD.to_string())
    }
}

// ============================================================================
// LINUX: /proc/cpuinfo + lspci
// ============================================================================
#[cfg(target_os = "linux")]
mod linux_impl {
    use super::*;

    const METHOD: &str = "/proc/cpuinfo + PCI lspci — auto-detected & cached";

    fn kernel_driver(slot: &str) -> Option<String> {
        let out = run_capture("lspci", &["-k", "-s", slot], Duration::from_secs(2))?;
        for line in out.lines() {
            if let Some(v) = line.strip_prefix("Kernel driver in use:") {
                return non_empty(v);
            }
        }
        None
    }

    fn detect_gpus() -> Vec<GpuInfo> {
        let Some(out) = run_capture("lspci", &[], Duration::from_secs(3)) else {
            return Vec::new();
        };
        let mut gpus = Vec::new();
        for line in out.lines() {
            let low = line.to_lowercase();
            if !(low.contains("vga compatible controller")
                || low.contains("3d controller")
                || low.contains("display controller"))
            {
                continue;
            }
            let device = line
                .split_once(": ")
                .map(|(_, d)| d.trim())
                .unwrap_or(line.trim());
            // Strip trailing " (rev NN)"
            let device = match device.rfind(" (rev ") {
                Some(i) => device[..i].trim(),
                None => device,
            };
            if device.is_empty() {
                continue;
            }
            let slot = line.split_whitespace().next().unwrap_or("").to_string();
            let driver = if slot.is_empty() {
                None
            } else {
                kernel_driver(&slot)
            };
            gpus.push(GpuInfo {
                name: device.to_string(),
                vendor: guess_vendor(device),
                vram_bytes: None,
                status: Some("OK".into()),
                driver_version: driver,
            });
        }
        gpus
    }

    pub(super) fn detect() -> (CpuInfo, Vec<GpuInfo>, String) {
        let mut name = String::new();
        let mut physical: Option<u32> = None;
        let mut logical: u32 = 0;

        if let Ok(text) = std::fs::read_to_string("/proc/cpuinfo") {
            for line in text.lines() {
                let Some((k, val)) = line.split_once(':') else {
                    continue;
                };
                let k = k.trim();
                let val = val.trim();
                match k {
                    "processor" => logical += 1,
                    "model name" if name.is_empty() => name = val.to_string(),
                    "model" if name.is_empty() => name = val.to_string(),
                    "Hardware" if name.is_empty() => name = val.to_string(),
                    "cpu cores" if physical.is_none() => physical = val.parse().ok(),
                    _ => {}
                }
            }
        }

        let cpu = CpuInfo {
            name: non_empty(name)
                .unwrap_or_else(|| format!("Unknown {} processor", std::env::consts::ARCH)),
            physical_cores: physical,
            logical_cores: if logical > 0 {
                logical
            } else {
                logical_cores()
            },
            max_clock_mhz: None,
        };
        let gpus = detect_gpus();
        (cpu, gpus, METHOD.to_string())
    }
}

// ============================================================================
// macOS: sysctl + system_profiler
// ============================================================================
#[cfg(target_os = "macos")]
mod macos_impl {
    use super::*;

    const METHOD: &str = "sysctl + system_profiler SPDisplaysDataType — auto-detected & cached";

    fn sysctl(key: &str) -> Option<String> {
        let out = run_capture("sysctl", &["-n", key], Duration::from_secs(2))?;
        non_empty(out)
    }

    fn detect_gpus() -> Vec<GpuInfo> {
        let Some(out) = run_capture(
            "system_profiler",
            &["SPDisplaysDataType", "-json"],
            Duration::from_secs(5),
        ) else {
            return Vec::new();
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&out) else {
            return Vec::new();
        };
        let Some(arr) = v.get("SPDisplaysDataType").and_then(|x| x.as_array()) else {
            return Vec::new();
        };

        let mut gpus = Vec::new();
        for entry in arr {
            // Top-level model (Apple Silicon SoC GPU) or per-display card model
            let mut models: Vec<String> = Vec::new();
            if let Some(m) = entry.get("sppci_model").and_then(|x| x.as_str()) {
                models.push(m.to_string());
            }
            if let Some(displays) = entry.get("spdisplays_ndrvs").and_then(|x| x.as_array()) {
                for d in displays {
                    if let Some(m) = d.get("sppci_model").and_then(|x| x.as_str()) {
                        models.push(m.to_string());
                    }
                }
            }
            let model = models
                .iter()
                .find_map(|m| non_empty(m.clone()))
                .unwrap_or_else(|| {
                    entry
                        .get("_name")
                        .and_then(|x| x.as_str())
                        .unwrap_or("GPU Adapter")
                        .to_string()
                });

            let vram = entry
                .get("spdisplays_vram")
                .and_then(|x| x.as_str())
                .and_then(parse_vram_str)
                .or_else(|| {
                    entry
                        .get("spdisplays_vram_shared")
                        .and_then(|x| x.as_str())
                        .and_then(parse_vram_str)
                });

            gpus.push(GpuInfo {
                vendor: entry
                    .get("spdisplays_vendor")
                    .and_then(|x| x.as_str())
                    .and_then(non_empty)
                    .or_else(|| guess_vendor(&model)),
                vram_bytes: vram,
                status: Some("OK".into()),
                name: model,
                driver_version: None,
            });
        }
        gpus
    }

    pub(super) fn detect() -> (CpuInfo, Vec<GpuInfo>, String) {
        let name = sysctl("machdep.cpu.brand_string")
            .or_else(|| sysctl("hw.model"))
            .unwrap_or_else(|| "Apple Silicon".to_string());
        let physical = sysctl("hw.physicalcpu").and_then(|s| s.parse().ok());
        let logical = sysctl("hw.logicalcpu")
            .and_then(|s| s.parse().ok())
            .unwrap_or_else(logical_cores);
        let max_clock_mhz = sysctl("hw.cpufrequency")
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|hz| *hz > 0)
            .map(|hz| (hz / 1_000_000) as u32);

        let cpu = CpuInfo {
            name,
            physical_cores: physical,
            logical_cores: logical,
            max_clock_mhz,
        };
        let gpus = detect_gpus();
        (cpu, gpus, METHOD.to_string())
    }
}

// ============================================================================
// LIVE MEMORY SAMPLING
// ============================================================================

/// Total physical memory and (when available) currently available memory.
#[cfg(windows)]
pub fn system_memory() -> (u64, Option<u64>) {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    unsafe {
        let mut ms: MEMORYSTATUSEX = std::mem::zeroed();
        ms.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut ms) != 0 && ms.ullTotalPhys > 0 {
            (ms.ullTotalPhys, Some(ms.ullAvailPhys))
        } else {
            (0, None)
        }
    }
}

#[cfg(target_os = "linux")]
pub fn system_memory() -> (u64, Option<u64>) {
    let (page, total, avail) = unsafe {
        (
            libc::sysconf(libc::_SC_PAGESIZE),
            libc::sysconf(libc::_SC_PHYS_PAGES),
            libc::sysconf(libc::_SC_AVPHYS_PAGES),
        )
    };
    if page <= 0 {
        return (0, None);
    }
    let page = page as u64;
    let total_bytes = if total > 0 { total as u64 * page } else { 0 };
    let avail_bytes = if avail > 0 {
        Some(avail as u64 * page)
    } else {
        None
    };
    (total_bytes, avail_bytes)
}

#[cfg(target_os = "macos")]
pub fn system_memory() -> (u64, Option<u64>) {
    fn sysctl_u64(name: &str) -> Option<u64> {
        let cname = std::ffi::CString::new(name).ok()?;
        let mut val: u64 = 0;
        let mut len = std::mem::size_of::<u64>();
        let rc = unsafe {
            libc::sysctlbyname(
                cname.as_ptr(),
                &mut val as *mut u64 as *mut libc::c_void,
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        if rc == 0 && val > 0 { Some(val) } else { None }
    }

    (sysctl_u64("hw.memsize").unwrap_or(0), None)
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn system_memory() -> (u64, Option<u64>) {
    (0, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_detection_smoke() {
        let report = detect_hardware();
        assert!(
            report.os_name == "Windows" || report.os_name == "Linux" || report.os_name == "macOS"
        );
        assert!(report.arch.len() >= 3);
        assert!(report.cpu.logical_cores >= 1);
        assert!(!report.cpu.name.is_empty());
        assert!(!report.gpu_detection_method.is_empty());
        // Second call must be served from cache and remain consistent
        let again = detect_hardware();
        assert_eq!(report.cpu.name, again.cpu.name);
    }

    #[test]
    fn vram_parser() {
        assert_eq!(parse_vram_str("8 GB"), Some(8 * 1024 * 1024 * 1024));
        assert_eq!(parse_vram_str("1536 MB"), Some(1536 * 1024 * 1024));
        assert_eq!(parse_vram_str("2147483648"), Some(2147483648));
        assert_eq!(parse_vram_str(""), None);
        assert_eq!(parse_vram_str("8192"), None); // too small to be bytes
    }

    #[test]
    fn vendor_guessing() {
        assert_eq!(
            guess_vendor("NVIDIA GeForce RTX 3060").as_deref(),
            Some("NVIDIA")
        );
        assert_eq!(guess_vendor("AMD Radeon RX 6800").as_deref(), Some("AMD"));
        assert_eq!(
            guess_vendor("Intel(R) UHD Graphics 620").as_deref(),
            Some("Intel")
        );
        assert_eq!(guess_vendor("Apple M2 Pro").as_deref(), Some("Apple"));
    }
}
