//! Bounded Linux-user emulation, native on Linux or through WSL on Windows.
use std::{
    ffi::OsStr,
    path::Path,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

pub fn run(command: &mut Command) -> Output {
    // Files avoid blocking on a full stdout/stderr pipe while waiting.
    let stdout = tempfile::tempfile().unwrap();
    let stderr = tempfile::tempfile().unwrap();
    let mut child = command
        .stdin(Stdio::null())
        .stdout(stdout.try_clone().unwrap())
        .stderr(stderr.try_clone().unwrap())
        .spawn()
        .unwrap_or_else(|error| panic!("cannot start {command:?}: {error}"));
    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("timed out: {command:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    use std::io::{Read, Seek};
    let read = |mut file: std::fs::File| {
        file.rewind().unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        bytes
    };
    Output {
        status,
        stdout: read(stdout),
        stderr: read(stderr),
    }
}

pub struct LinuxTools;

impl LinuxTools {
    fn command(program: &str) -> Command {
        if cfg!(windows) {
            let mut command = Command::new("wsl.exe");
            if let Some(distro) = std::env::var_os("ADESH_TEST_WSL_DISTRO") {
                command.arg("--distribution").arg(distro);
            }
            command.args(["--exec", program]);
            command
        } else {
            Command::new(program)
        }
    }

    pub fn discover(assembler: &str, emulator: &str) -> Option<Self> {
        let required = std::env::var_os("ADESH_REQUIRE_FOREIGN_EXEC").is_some();
        let missing = |reason: &str| {
            assert!(
                !required,
                "required foreign execution unavailable: {reason}"
            );
            eprintln!("SKIP: foreign execution: {reason}");
            None
        };
        if !cfg!(any(target_os = "linux", windows)) {
            return missing("needs Linux or Windows/WSL");
        }
        // Probe through a Linux timeout, which also bounds the QEMU process
        // inside WSL if the outer Windows client is terminated.
        for program in [assembler, emulator] {
            let mut command = Self::command("timeout");
            command.args(["10s", program, "--version"]);
            // A missing executable/WSL should be a loud local skip, not panic.
            match command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(mut child) => {
                    let deadline = Instant::now() + Duration::from_secs(20);
                    loop {
                        match child.try_wait().unwrap() {
                            Some(status) if status.success() => break,
                            Some(_) => {
                                return missing(&format!(
                                    "install {program} and coreutils timeout"
                                ));
                            }
                            None if Instant::now() >= deadline => {
                                child.kill().unwrap();
                                child.wait().unwrap();
                                return missing("Linux tool probe timed out");
                            }
                            None => std::thread::sleep(Duration::from_millis(10)),
                        }
                    }
                }
                Err(error) => return missing(&format!("{program}: {error}")),
            }
        }
        Some(Self)
    }

    pub fn path(&self, path: &Path) -> String {
        let absolute = path.canonicalize().unwrap();
        if cfg!(windows) {
            let absolute = absolute.to_str().unwrap();
            let absolute = absolute.strip_prefix(r"\\?\").unwrap_or(absolute);
            let mut command = Self::command("wslpath");
            command.args(["-a", "-u", absolute]);
            let output = run(&mut command);
            assert!(output.status.success(), "wslpath: {output:?}");
            String::from_utf8(output.stdout).unwrap().trim().to_owned()
        } else {
            absolute.to_str().unwrap().to_owned()
        }
    }

    pub fn tool(&self, program: &str, args: &[&OsStr]) -> Output {
        let mut command = Self::command("timeout");
        command.args(["10s", program]).args(args);
        run(&mut command)
    }
}
