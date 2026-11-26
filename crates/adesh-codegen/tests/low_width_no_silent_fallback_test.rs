//! Low-width plan TODO-7.3: the 32/24/16/8-bit backends must never guess a width
//! or hide an unsupported form behind a default. This scans the backend sources
//! for the patterns that caused such bugs elsewhere and fails on any new use.

use std::fs;
use std::path::PathBuf;

const BACKENDS: &[&str] = &["i686", "i8086", "avr", "dsp56000"];

/// `unwrap_or(...)` call sites that are reviewed and intentional.
/// Each entry is the exact trimmed source line.
const ALLOWED_UNWRAP_OR: &[(&str, &str)] = &[
    // The destination is a spilled value: the load goes through the X scratch
    // pair and is stored back to the frame slot immediately afterwards.
    ("avr", "let d_pair = pair_reg(dst).unwrap_or(REG_XL);"),
];

fn source(backend: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src/targets")
        .join(backend)
        .join("mod.rs");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn code_lines(src: &str) -> impl Iterator<Item = (usize, &str)> {
    src.lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.trim()))
        .filter(|(_, l)| !l.starts_with("//"))
}

#[test]
fn backends_never_default_the_pointer_or_register_width() {
    for backend in BACKENDS {
        let src = source(backend);
        for (line_no, line) in code_lines(&src) {
            for banned in ["is_64bit()", "PointerWidth::U64", "default_pointer_width"] {
                assert!(
                    !line.contains(banned),
                    "{backend}/mod.rs:{line_no}: `{banned}` guesses a width; read it from the target descriptor instead: {line}"
                );
            }
        }
    }
}

#[test]
fn catch_all_match_arms_never_emit_a_nop() {
    for backend in BACKENDS {
        let src = source(backend);
        for (line_no, line) in code_lines(&src) {
            if let Some(rest) = line.strip_prefix("_ =>") {
                let lowered = rest.to_ascii_lowercase();
                assert!(
                    !lowered.contains("nop") && !lowered.contains("0x90"),
                    "{backend}/mod.rs:{line_no}: a catch-all arm must return a CodegenError, not emit a NOP: {line}"
                );
            }
        }
    }
}

#[test]
fn defaults_are_never_substituted_for_missing_operands() {
    for backend in BACKENDS {
        let src = source(backend);
        for (line_no, line) in code_lines(&src) {
            assert!(
                !line.contains("unwrap_or_default("),
                "{backend}/mod.rs:{line_no}: unwrap_or_default hides a missing operand: {line}"
            );
            if line.contains(".unwrap_or(") {
                assert!(
                    ALLOWED_UNWRAP_OR
                        .iter()
                        .any(|(b, allowed)| b == backend && *allowed == line),
                    "{backend}/mod.rs:{line_no}: unreviewed unwrap_or default (add a CodegenError, or allowlist it with a reason): {line}"
                );
            }
        }
    }
}

#[test]
fn allowlist_entries_still_exist() {
    for (backend, allowed) in ALLOWED_UNWRAP_OR {
        let src = source(backend);
        assert!(
            code_lines(&src).any(|(_, l)| l == *allowed),
            "stale allowlist entry for {backend}: {allowed}"
        );
    }
}
