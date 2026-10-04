//! Phase 9 Native Plugin & Shared Library ABI Framework.
//!
//! Provides:
//! - Stable native plugin ABI header (`AdeshPluginHeader`)
//! - Cross-platform dynamic plugin loader (`AdeshPluginManager`)
//! - Function invocation bridge across shared library boundaries

use crate::platform::DynamicLibrary;
use std::path::Path;

pub const ADESH_PLUGIN_MAGIC: [u8; 8] = *b"ADLPLUG\x01";
pub const ADESH_PLUGIN_ABI_VERSION: u32 = 1;

/// Standard plugin descriptor returned by `adesh_plugin_init`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdeshPluginHeader {
    pub magic: [u8; 8],
    pub abi_version: u32,
    pub plugin_version: u32,
    pub name: *const std::os::raw::c_char,
    pub author: *const std::os::raw::c_char,
}

impl Default for AdeshPluginHeader {
    fn default() -> Self {
        Self {
            magic: ADESH_PLUGIN_MAGIC,
            abi_version: ADESH_PLUGIN_ABI_VERSION,
            plugin_version: 1,
            name: std::ptr::null(),
            author: std::ptr::null(),
        }
    }
}

pub type PluginInitFn = unsafe extern "C" fn() -> AdeshPluginHeader;
pub type PluginInvokeFn = unsafe extern "C" fn(*const u8, usize, *mut u8, usize) -> i32;

/// Loaded Native Plugin Instance.
pub struct PluginInstance {
    pub name: String,
    pub header: AdeshPluginHeader,
    pub dylib: DynamicLibrary,
}

impl PluginInstance {
    /// Load plugin from shared library file (`.dll`, `.so`, `.dylib`).
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path_str = path.as_ref().to_string_lossy();
        let dylib = DynamicLibrary::load(&path_str)
            .ok_or_else(|| format!("Failed to load dynamic library '{}'", path_str))?;

        // Resolve adesh_plugin_init
        let init_sym = dylib
            .symbol("adesh_plugin_init")
            .ok_or_else(|| "Plugin missing 'adesh_plugin_init' symbol".to_string())?;

        let init_fn: PluginInitFn = unsafe { std::mem::transmute(init_sym) };
        let header = unsafe { init_fn() };

        if header.magic != ADESH_PLUGIN_MAGIC {
            return Err("Plugin header magic mismatch".to_string());
        }

        if header.abi_version != ADESH_PLUGIN_ABI_VERSION {
            return Err(format!(
                "Plugin ABI version mismatch: expected {}, got {}",
                ADESH_PLUGIN_ABI_VERSION, header.abi_version
            ));
        }

        let name = if !header.name.is_null() {
            unsafe { std::ffi::CStr::from_ptr(header.name) }
                .to_string_lossy()
                .to_string()
        } else {
            "unnamed_plugin".to_string()
        };

        Ok(Self {
            name,
            header,
            dylib,
        })
    }

    /// Invoke a plugin export function.
    pub fn invoke(&self, symbol_name: &str, input: &[u8], output: &mut [u8]) -> Result<i32, String> {
        let sym = self
            .dylib
            .symbol(symbol_name)
            .ok_or_else(|| format!("Symbol '{}' not found in plugin", symbol_name))?;

        let invoke_fn: PluginInvokeFn = unsafe { std::mem::transmute(sym) };
        let ret = unsafe {
            invoke_fn(
                input.as_ptr(),
                input.len(),
                output.as_mut_ptr(),
                output.len(),
            )
        };
        Ok(ret)
    }
}
