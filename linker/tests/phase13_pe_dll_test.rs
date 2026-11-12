//! Shared PE images go through the real linker and, on Win64, the OS loader.
use adesh_linker::{
    config::LinkConfig,
    linker::Linker,
    object::{ObjectFile, writer::ObjectWriter},
    relocation::{Relocation, RelocationKind},
    section::Section,
    symbol::{Symbol, SymbolBinding, SymbolType},
    target::Target,
};
use std::path::Path;

fn fixture(dir: &Path, entry: Option<&str>, accepts: bool) -> LinkConfig {
    let target = Target::from_triple("x86_64-pc-windows-msvc").unwrap();
    let input = dir.join("input.adob");
    let mut obj = ObjectFile::new(input.clone(), target.clone(), 0);
    // An unrelated function comes first and returns FALSE. It must never be
    // chosen as a DLL initializer merely because it starts the text section.
    let mut code = vec![0x31, 0xc0, 0xc3];
    code.resize(16, 0x90);
    // probe(): mov rax, &state; mov eax, [rax]; ret
    code.extend_from_slice(&[0x48, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0x8b, 0x00, 0xc3]);
    code.resize(32, 0x90);
    // DllMain(HINSTANCE, DWORD reason, LPVOID): store the reason for
    // observation by probe(), then return the requested BOOL.
    code.extend_from_slice(&[
        0x89,
        0x15,
        0,
        0,
        0,
        0,
        0xb8,
        u8::from(accepts),
        0,
        0,
        0,
        0xc3,
    ]);
    let mut text = Section::new_code(".text", code, 16);
    text.relocations
        .push(Relocation::new(18, "state", RelocationKind::Absolute64, 0));
    text.relocations.push(Relocation::new(
        34,
        "state",
        RelocationKind::PcRelative32,
        -4,
    ));
    obj.add_section(text);
    obj.add_section(Section::new_data(
        ".data",
        42u32.to_le_bytes().to_vec(),
        true,
        8,
    ));
    for (name, section, offset, size, exported, ty) in [
        ("not_an_initializer", 0, 0, 3, false, SymbolType::Function),
        ("probe", 0, 16, 13, true, SymbolType::Function),
        ("state", 1, 0, 4, false, SymbolType::Object),
    ] {
        let mut symbol =
            Symbol::new_defined(name, SymbolBinding::Global, ty, section, offset, size, 0);
        symbol.is_exported = exported;
        obj.add_symbol(symbol);
    }
    if let Some(name) = entry {
        obj.add_symbol(Symbol::new_defined(
            name,
            SymbolBinding::Global,
            SymbolType::Function,
            0,
            32,
            12,
            0,
        ));
    }
    ObjectWriter::write_to_file(&obj, &input).unwrap();
    let mut config = LinkConfig::new(dir.join("probe.dll"), target);
    config.shared = true;
    config
}

fn link_and_check(config: LinkConfig, has_entry: bool) {
    let input = config.output_path.parent().unwrap().join("input.adob");
    Linker::link(&[input], config.clone()).unwrap();
    let bytes = std::fs::read(&config.output_path).unwrap();
    let pe = u32::from_le_bytes(bytes[0x3c..0x40].try_into().unwrap()) as usize;
    let entry = u32::from_le_bytes(bytes[pe + 40..pe + 44].try_into().unwrap());
    assert_eq!(
        entry != 0,
        has_entry,
        "DLL entry RVA must reflect an initializer, not a text fallback"
    );
    if has_entry {
        assert_eq!(
            entry, 0x1020,
            "initializer is at text+32, not the first export"
        );
    }
}

#[test]
fn no_initializer_has_zero_entry_rva() {
    let dir = tempfile::tempdir().unwrap();
    link_and_check(fixture(dir.path(), None, true), false);
}

#[test]
fn dll_initializer_selection() {
    for name in [
        "DllMain",
        "_DllMainCRTStartup",
        "DllMainCRTStartup",
        "custom_init",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut config = fixture(dir.path(), Some(name), true);
        if name == "custom_init" {
            config.entry_point = Some(name.to_string());
        }
        link_and_check(config, true);
    }
}

#[test]
fn explicit_missing_initializer_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = fixture(dir.path(), None, true);
    config.entry_point = Some("missing_init".into());
    let error = Linker::link(&[dir.path().join("input.adob")], config.clone()).unwrap_err();
    assert_eq!(
        error.code,
        adesh_linker::error::ErrorCode::EntryPointNotFound
    );
    assert!(error.to_string().contains("missing_init"));
    assert!(!config.output_path.exists());
}

#[test]
fn data_symbol_cannot_be_an_initializer() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = fixture(dir.path(), None, true);
    config.entry_point = Some("state".into());
    let error = Linker::link(&[dir.path().join("input.adob")], config.clone()).unwrap_err();
    assert_eq!(
        error.code,
        adesh_linker::error::ErrorCode::EntryPointNotFound
    );
    assert!(error.to_string().contains("not in executable code"));
    assert!(!config.output_path.exists());
}

#[cfg(all(windows, target_arch = "x86_64"))]
mod windows {
    use super::*;
    use std::{
        ffi::c_void,
        os::windows::ffi::OsStrExt,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LoadLibraryW(name: *const u16) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
        fn FreeLibrary(module: *mut c_void) -> i32;
    }

    struct Library(*mut c_void);
    impl Library {
        fn load(path: &Path) -> Self {
            let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            let module = unsafe { LoadLibraryW(wide.as_ptr()) };
            assert!(
                !module.is_null(),
                "LoadLibraryW: {}",
                std::io::Error::last_os_error()
            );
            Self(module)
        }
        fn probe(&self) -> u32 {
            let address = unsafe { GetProcAddress(self.0, c"probe".as_ptr().cast()) };
            assert!(
                !address.is_null(),
                "GetProcAddress: {}",
                std::io::Error::last_os_error()
            );
            let probe: unsafe extern "system" fn() -> u32 = unsafe { std::mem::transmute(address) };
            unsafe { probe() }
        }
    }
    impl Drop for Library {
        fn drop(&mut self) {
            unsafe {
                FreeLibrary(self.0);
            }
        }
    }

    #[test]
    fn load_call_rebase_and_initialization() {
        // Isolate any malformed-image loader crash from the remaining suite.
        let mut process = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "windows::loader_child",
                "--ignored",
                "--nocapture",
            ])
            .env("ADESH_PE_DLL_CHILD", "1")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        while process.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                process.kill().unwrap();
                let output = process.wait_with_output().unwrap();
                panic!(
                    "DLL loader child timed out: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let child = process.wait_with_output().unwrap();
        assert!(
            child.status.success(),
            "DLL loader child: {:?}\n{}\n{}",
            child.status,
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
    }

    #[test]
    #[ignore = "invoked in an isolated process by load_call_rebase_and_initialization"]
    fn loader_child() {
        if std::env::var("ADESH_PE_DLL_CHILD").as_deref() != Ok("1") {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let config = fixture(dir.path(), None, true);
        let linked = Command::new(env!("CARGO_BIN_EXE_adeshlink"))
            .args(["--shared", "--target=x86_64-pc-windows-msvc"])
            .arg("--output")
            .arg(&config.output_path)
            .arg(dir.path().join("input.adob"))
            .output()
            .unwrap();
        assert!(
            linked.status.success(),
            "adeshlink --shared failed: {linked:?}"
        );
        let first = Library::load(&config.output_path);
        assert_eq!(first.probe(), 42);
        let second_path = dir.path().join("rebased.dll");
        std::fs::copy(&config.output_path, &second_path).unwrap();
        let second = Library::load(&second_path);
        assert_ne!(first.0, second.0, "two images cannot share a base");
        assert_eq!(second.probe(), 42, "absolute pointer must survive rebasing");
        drop(second);
        drop(first);
        for (name, accepts) in [
            ("DllMain", true),
            ("_DllMainCRTStartup", true),
            ("DllMainCRTStartup", true),
            ("custom_init", true),
            ("DllMain", false),
        ] {
            let case = tempfile::tempdir().unwrap();
            let mut config = fixture(case.path(), Some(name), accepts);
            if name == "custom_init" {
                config.entry_point = Some(name.into());
            }
            link_and_check(config.clone(), true);
            if accepts {
                let library = Library::load(&config.output_path);
                assert_eq!(library.probe(), 1, "DLL_PROCESS_ATTACH must run");
            } else {
                let wide: Vec<u16> = config
                    .output_path
                    .as_os_str()
                    .encode_wide()
                    .chain(Some(0))
                    .collect();
                let module = unsafe { LoadLibraryW(wide.as_ptr()) };
                if !module.is_null() {
                    unsafe {
                        FreeLibrary(module);
                    }
                }
                assert!(module.is_null(), "FALSE from DllMain must reject loading");
            }
        }
    }
}
