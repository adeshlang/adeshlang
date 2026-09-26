//! Test Framework Module
//!
//! Rust-grade testing harness for the language with:
//! - explicit PASS/FAIL/PANIC/TIMEOUT/IGNORED status normalization
//! - fail-fast and tag filtering
//! - backend matrix comparison (`--backend-check`)
//! - JSON reporting for CI

use crate::parsing::ast::{Function, Stmt, StmtKind};
use crate::toolchain::config::ExecutionBackend;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TestStatus {
    Pass,
    Fail,
    Panic,
    Timeout,
    Ignored,
}

impl TestStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Panic => "PANIC",
            Self::Timeout => "TIMEOUT",
            Self::Ignored => "IGNORED",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BackendExecutionResult {
    pub status: TestStatus,
    pub message: Option<String>,
    pub duration: Duration,
    pub stack_trace: Option<Vec<String>>,
    pub output: String, // Captured stdout/print output during test execution
}

#[derive(Debug, Clone)]
pub struct TestInfo {
    pub name: String,
    pub should_ignore: bool,
    pub expect_fail: bool,
    pub timeout: Option<Duration>,
    pub tags: BTreeSet<String>,
}

#[derive(Debug, Clone)]
pub struct TestRunOptions {
    pub fail_fast: bool,
    pub include_tags: BTreeSet<String>,
    pub test_name: Option<String>,
    pub default_timeout: Duration,
    pub backend_check: bool,
    pub test_backends: Vec<ExecutionBackend>,
    pub json_format: bool,
    pub no_color: bool,
    pub deterministic_seed: u64,
    pub frozen_unix_time_secs: i64,
    pub quiet: bool,                           // Suppress print output from tests
    pub nocapture: bool,   // Always show captured output (even for passing tests)
    pub exact_match: bool, // Exact test name match (--exact)
    pub skip_filter: Option<String>, // Skip tests matching pattern (--skip)
    pub list_only: bool,   // List tests without running (--list)
    pub run_ignored_only: bool, // Run only ignored tests (--ignored)
    pub include_ignored: bool, // Run both regular and ignored tests (--include-ignored)
    pub test_threads: Option<usize>, // Worker threads for parallel execution (--test-threads)
    pub serial: bool,      // Run tests sequentially one-by-one (--serial)
    pub base_file: Option<std::path::PathBuf>, // Base file path for resolving module imports
}

impl Default for TestRunOptions {
    fn default() -> Self {
        Self {
            fail_fast: false,
            include_tags: BTreeSet::new(),
            test_name: None,
            default_timeout: Duration::from_secs(5),
            backend_check: false,
            test_backends: Vec::new(),
            json_format: false,
            no_color: false,
            deterministic_seed: 0xD00DFEED,
            frozen_unix_time_secs: 1_735_689_600,
            quiet: false,
            nocapture: false,
            exact_match: false,
            skip_filter: None,
            list_only: false,
            run_ignored_only: false,
            include_ignored: false,
            test_threads: None,
            serial: false,
            base_file: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TestCaseResult {
    pub name: String,
    pub effective_status: TestStatus,
    pub expected_fail_applied: bool,
    pub backend_results: BTreeMap<ExecutionBackend, BackendExecutionResult>,
}

#[derive(Debug, Clone)]
pub struct TestSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub panics: usize,
    pub ignored: usize,
    pub timeout: usize,
    pub duration: Duration,
}

impl TestSummary {
    pub fn new() -> Self {
        Self {
            total: 0,
            passed: 0,
            failed: 0,
            panics: 0,
            ignored: 0,
            timeout: 0,
            duration: Duration::from_secs(0),
        }
    }

    pub fn record(&mut self, status: TestStatus) {
        self.total += 1;
        match status {
            TestStatus::Pass => self.passed += 1,
            TestStatus::Fail => self.failed += 1,
            TestStatus::Panic => self.panics += 1,
            TestStatus::Ignored => self.ignored += 1,
            TestStatus::Timeout => self.timeout += 1,
        }
    }

    pub fn is_success(&self) -> bool {
        self.failed == 0 && self.panics == 0 && self.timeout == 0
    }
}

impl Default for TestSummary {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct BackendMatrix {
    pub backends: Vec<ExecutionBackend>,
    pub rows: BTreeMap<String, BTreeMap<ExecutionBackend, TestStatus>>,
    pub inconsistencies: Vec<String>,
}

pub trait BackendTestExecutor: Clone + Send + Sync + 'static {
    fn execute_test(
        &mut self,
        backend: ExecutionBackend,
        test: &TestInfo,
        timeout: Duration,
        options: &TestRunOptions,
    ) -> BackendExecutionResult;
}

pub fn collect_tests(ast: &[Stmt]) -> Vec<TestInfo> {
    collect_tests_with_base(ast, None)
}

pub fn collect_tests_with_base(ast: &[Stmt], base_file: Option<&std::path::Path>) -> Vec<TestInfo> {
    let mut tests = Vec::new();
    let mut visited = std::collections::HashSet::new();
    if let Some(p) = base_file {
        if let Ok(c) = p.canonicalize() {
            visited.insert(c);
        }
    }
    collect_tests_internal(ast, base_file, "", &mut tests, &mut visited);
    tests
}

fn collect_tests_internal(
    ast: &[Stmt],
    base_file: Option<&std::path::Path>,
    prefix: &str,
    out: &mut Vec<TestInfo>,
    visited: &mut std::collections::HashSet<std::path::PathBuf>,
) {
    for stmt in ast {
        match &stmt.kind {
            StmtKind::Function(func, _) => {
                if func.is_test {
                    let test_name = if prefix.is_empty() {
                        func.name.clone()
                    } else {
                        format!("{}::{}", prefix, func.name)
                    };
                    out.push(TestInfo {
                        name: test_name,
                        should_ignore: func.test_ignore,
                        expect_fail: func.test_expect_fail,
                        timeout: func.test_timeout.map(Duration::from_secs),
                        tags: extract_tags(&func.decorators),
                    });
                }
            }
            StmtKind::Import { path, alias } | StmtKind::ImportDefault { path, alias } => {
                if let Some(bf) = base_file {
                    let parent = bf.parent().unwrap_or_else(|| std::path::Path::new("."));
                    let mut mod_path = parent.join(path);
                    if !mod_path.exists() && !path.ends_with(".adesh") {
                        mod_path = parent.join(format!("{}.adesh", path));
                    }
                    if mod_path.exists() {
                        if let Ok(can) = mod_path.canonicalize() {
                            if visited.insert(can) {
                                if let Ok(src) = std::fs::read_to_string(&mod_path) {
                                    let mut lex = crate::parsing::lexer::Lexer::new(&src);
                                    if let Ok(toks) = lex.tokenize() {
                                        let mut p = crate::parsing::parser::Parser::new(
                                            toks,
                                            Some(mod_path.to_string_lossy().to_string()),
                                        );
                                        if let Ok(sub_ast) = p.parse_program() {
                                            let sub_prefix = if prefix.is_empty() {
                                                alias.clone()
                                            } else {
                                                format!("{}::{}", prefix, alias)
                                            };
                                            collect_tests_internal(
                                                &sub_ast,
                                                Some(&mod_path),
                                                &sub_prefix,
                                                out,
                                                visited,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            StmtKind::ImportNames { path, names: _ } => {
                if let Some(bf) = base_file {
                    let parent = bf.parent().unwrap_or_else(|| std::path::Path::new("."));
                    let mut mod_path = parent.join(path);
                    if !mod_path.exists() && !path.ends_with(".adesh") {
                        mod_path = parent.join(format!("{}.adesh", path));
                    }
                    if mod_path.exists() {
                        if let Ok(can) = mod_path.canonicalize() {
                            if visited.insert(can) {
                                if let Ok(src) = std::fs::read_to_string(&mod_path) {
                                    let mut lex = crate::parsing::lexer::Lexer::new(&src);
                                    if let Ok(toks) = lex.tokenize() {
                                        let mut p = crate::parsing::parser::Parser::new(
                                            toks,
                                            Some(mod_path.to_string_lossy().to_string()),
                                        );
                                        if let Ok(sub_ast) = p.parse_program() {
                                            collect_tests_internal(
                                                &sub_ast,
                                                Some(&mod_path),
                                                prefix,
                                                out,
                                                visited,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn extract_tags(decorators: &[crate::parsing::ast::Expr]) -> BTreeSet<String> {
    let mut tags = BTreeSet::new();
    for d in decorators {
        if let crate::parsing::ast::ExprKind::Call(callee, args, _) = &d.kind {
            if let crate::parsing::ast::ExprKind::Variable(name) = &callee.kind {
                if name == "tag" {
                    for arg in args {
                        if let crate::parsing::ast::ExprKind::Literal(
                            crate::parsing::ast::Value::Str(s),
                        ) = &arg.kind
                        {
                            tags.insert(s.clone());
                        } else if let crate::parsing::ast::ExprKind::Variable(s) = &arg.kind {
                            tags.insert(s.clone());
                        }
                    }
                }
            }
        }
    }
    tags
}

fn filter_by_tags(test: &TestInfo, include_tags: &BTreeSet<String>) -> bool {
    if include_tags.is_empty() {
        return true;
    }
    !test.tags.is_disjoint(include_tags)
}

pub fn all_execution_backends() -> Vec<ExecutionBackend> {
    vec![
        ExecutionBackend::Interpreter,
        ExecutionBackend::Jit,
        ExecutionBackend::NativeJit,
        ExecutionBackend::Bytecode,
        ExecutionBackend::Mixed,
        ExecutionBackend::AdaptiveJit,
        ExecutionBackend::TieredJit,
        ExecutionBackend::Aot,
        ExecutionBackend::Wasm,
    ]
}

pub fn run_tests<E: BackendTestExecutor>(
    ast: &[Stmt],
    executor: &mut E,
    options: &TestRunOptions,
    selected_backend: ExecutionBackend,
) -> (Vec<TestCaseResult>, TestSummary, Option<BackendMatrix>) {
    let mut summary = TestSummary::new();
    let started = Instant::now();
    let mut all_results = Vec::new();

    let mut tests = collect_tests_with_base(ast, options.base_file.as_deref());

    // --list support (list matching tests without running them)
    if options.list_only {
        for t in &tests {
            println!("{}: test", t.name.replace("__", "::"));
        }
        println!("\n{} tests", tests.len());
        return (Vec::new(), TestSummary::default(), None);
    }

    // --skip <pattern> filter
    if let Some(skip) = &options.skip_filter {
        tests.retain(|t| !t.name.contains(skip) && !t.name.replace("__", "::").contains(skip));
    }

    // test name filter (cargo test style: substring match by default, exact match with --exact)
    if let Some(name) = options.test_name.as_deref() {
        let normalized = name.replace("::", "__");
        let prefix = format!("{}__", normalized);
        let colon_prefix = format!("{}::", name);
        if options.exact_match {
            tests.retain(|t| {
                t.name == name || t.name == normalized || t.name.replace("__", "::") == name
            });
        } else {
            tests.retain(|t| {
                t.name == normalized
                    || t.name.starts_with(&prefix)
                    || t.name == name
                    || t.name.starts_with(&colon_prefix)
                    || t.name.contains(name)
                    || t.name.contains(&normalized)
                    || t.name.replace("__", "::").contains(name)
            });
        }
    }

    // --ignored support
    if options.run_ignored_only {
        tests.retain(|t| t.should_ignore);
    }

    let backends = if !options.test_backends.is_empty() {
        options.test_backends.clone()
    } else if options.backend_check {
        all_execution_backends()
    } else {
        vec![selected_backend]
    };

    let is_serial = options.serial || options.test_threads == Some(1);

    if is_serial {
        for test in tests {
            let status =
                execute_single_test_case(&test, &backends, options, executor.clone(), true);
            summary.record(status.effective_status);
            let stop = matches!(
                status.effective_status,
                TestStatus::Fail | TestStatus::Panic | TestStatus::Timeout
            ) && options.fail_fast;
            all_results.push(status);
            if stop {
                break;
            }
        }
    } else {
        use rayon::prelude::*;

        let num_threads = options.test_threads.unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
        });

        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build();

        all_results = if let Ok(tp) = pool {
            tp.install(|| {
                tests
                    .par_iter()
                    .map(|t| {
                        let exec = executor.clone();
                        execute_single_test_case(t, &backends, options, exec, false)
                    })
                    .collect()
            })
        } else {
            tests
                .par_iter()
                .map(|t| {
                    let exec = executor.clone();
                    execute_single_test_case(t, &backends, options, exec, false)
                })
                .collect()
        };

        for res in &all_results {
            summary.record(res.effective_status);
        }
    }

    summary.duration = started.elapsed();
    let matrix = if backends.len() > 1 {
        Some(build_backend_matrix(&all_results, &backends))
    } else {
        None
    };

    (all_results, summary, matrix)
}

fn execute_single_test_case<E: BackendTestExecutor>(
    test: &TestInfo,
    backends: &[ExecutionBackend],
    options: &TestRunOptions,
    mut executor: E,
    serial_mode: bool,
) -> TestCaseResult {
    let is_ignored = test.should_ignore && !options.run_ignored_only && !options.include_ignored;
    if is_ignored || !filter_by_tags(test, &options.include_tags) {
        let mut map = BTreeMap::new();
        for backend in backends {
            map.insert(
                *backend,
                BackendExecutionResult {
                    status: TestStatus::Ignored,
                    message: Some("ignored by attribute/filter".to_string()),
                    duration: Duration::from_millis(0),
                    stack_trace: None,
                    output: String::new(),
                },
            );
        }
        TestCaseResult {
            name: test.name.clone(),
            effective_status: TestStatus::Ignored,
            expected_fail_applied: false,
            backend_results: map,
        }
    } else {
        let timeout = test.timeout.unwrap_or(options.default_timeout);
        let mut backend_results = BTreeMap::new();
        let mut canonical = TestStatus::Pass;

        if backends.len() > 1 && !serial_mode {
            use rayon::prelude::*;
            let results: Vec<(ExecutionBackend, BackendExecutionResult)> = backends
                .par_iter()
                .map(|backend| {
                    let mut exec = executor.clone();
                    let r = exec.execute_test(*backend, test, timeout, options);
                    (*backend, r)
                })
                .collect();

            for (backend, r) in results {
                canonical = first_non_pass(canonical, r.status);
                backend_results.insert(backend, r);
            }
        } else {
            for backend in backends {
                let r = executor.execute_test(*backend, test, timeout, options);
                canonical = first_non_pass(canonical, r.status);
                backend_results.insert(*backend, r);
            }
        }

        let mut effective_status = canonical;
        let mut expected_fail_applied = false;
        if test.expect_fail && canonical == TestStatus::Fail {
            effective_status = TestStatus::Pass;
            expected_fail_applied = true;
        }

        TestCaseResult {
            name: test.name.clone(),
            effective_status,
            expected_fail_applied,
            backend_results,
        }
    }
}

fn first_non_pass(current: TestStatus, next: TestStatus) -> TestStatus {
    if current != TestStatus::Pass {
        return current;
    }
    next
}

pub fn build_backend_matrix(
    results: &[TestCaseResult],
    backends: &[ExecutionBackend],
) -> BackendMatrix {
    let mut rows = BTreeMap::new();
    let mut inconsistencies = Vec::new();

    for tr in results {
        let mut row = BTreeMap::new();
        for b in backends {
            let status = tr
                .backend_results
                .get(b)
                .map(|r| r.status)
                .unwrap_or(TestStatus::Ignored);
            row.insert(*b, status);
        }

        let unique: BTreeSet<_> = row.values().copied().collect();
        if unique.len() > 1 {
            inconsistencies.push(format!(
                "{} => {}",
                tr.name,
                backends
                    .iter()
                    .map(|b| format!(
                        "{:?}:{}",
                        b,
                        row.get(b).copied().unwrap_or(TestStatus::Ignored).as_str()
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        rows.insert(tr.name.clone(), row);
    }

    BackendMatrix {
        backends: backends.to_vec(),
        rows,
        inconsistencies,
    }
}

pub fn render_result_line(
    name: &str,
    status: TestStatus,
    no_color: bool,
    message: Option<&str>,
    duration: Option<Duration>,
    _output: Option<&str>, // Reserved for future use if needed in output
) -> String {
    let display_name = name.replace("__", "::");
    let prefix = match status {
        TestStatus::Pass => colorize("PASS", "\x1b[32m", no_color),
        TestStatus::Fail => colorize("FAIL", "\x1b[31m", no_color),
        TestStatus::Panic => colorize("PANIC", "\x1b[35m", no_color),
        TestStatus::Timeout => colorize("TIMEOUT", "\x1b[33m", no_color),
        TestStatus::Ignored => "IGNORED".to_string(),
    };

    let timing = duration.map(|d| {
        if d.as_secs() > 0 {
            format!("{:.2}s", d.as_secs_f64())
        } else if d.as_millis() > 0 {
            format!("{:.2}ms", d.as_secs_f64() * 1000.0)
        } else if d.as_micros() > 0 {
            format!("{:.0}µs", d.as_secs_f64() * 1_000_000.0)
        } else {
            "0.00ms".to_string()
        }
    });

    let suffix = match (status, timing) {
        (TestStatus::Pass, Some(t)) => format!(" ... ok ({})", t),
        (TestStatus::Pass, None) => " ... ok".to_string(),
        (_, Some(t)) => format!(" ... ({})", t),
        (_, None) => String::new(),
    };

    if let Some(msg) = message {
        format!("{} {}{} - {}", prefix, display_name, suffix, msg)
    } else {
        format!("{} {}{}", prefix, display_name, suffix)
    }
}

fn colorize(s: &str, ansi: &str, no_color: bool) -> String {
    if no_color {
        s.to_string()
    } else {
        format!("{}{}\x1b[0m", ansi, s)
    }
}

pub fn format_test_summary(summary: &TestSummary, no_color: bool) -> String {
    let header = colorize("Test Summary:", "\x1b[1;37m", no_color); // Bold white
    let passed = if summary.passed > 0 {
        colorize(&format!("{} passed", summary.passed), "\x1b[32m", no_color) // Green
    } else {
        format!("{} passed", summary.passed)
    };
    let failed = if summary.failed > 0 {
        colorize(&format!("{} failed", summary.failed), "\x1b[31m", no_color) // Red
    } else {
        format!("{} failed", summary.failed)
    };
    let panics = if summary.panics > 0 {
        colorize(&format!("{} panic", summary.panics), "\x1b[35m", no_color) // Magenta
    } else {
        format!("{} panic", summary.panics)
    };
    let timeout = if summary.timeout > 0 {
        colorize(
            &format!("{} timeout", summary.timeout),
            "\x1b[33m",
            no_color,
        ) // Yellow
    } else {
        format!("{} timeout", summary.timeout)
    };

    let result_line = if summary.is_success() {
        colorize(
            &format!("All tests passed: {}", passed),
            "\x1b[1;32m",
            no_color,
        ) // Bold green
    } else {
        colorize(
            &format!("Tests failed: {}, {}, {}", failed, panics, timeout),
            "\x1b[1;31m",
            no_color,
        ) // Bold red
    };

    format!(
        "\n{}\n  Total: {}\n  Passed: {}\n  Failed: {}\n  Panics: {}\n  Ignored: {}\n  Timeout: {}\n  Duration: {:.2}ms\n{}",
        header,
        summary.total,
        summary.passed,
        summary.failed,
        summary.panics,
        summary.ignored,
        summary.timeout,
        summary.duration.as_secs_f64() * 1000.0,
        result_line
    )
}

pub fn render_backend_matrix_ascii(matrix: &BackendMatrix) -> String {
    let mut out = String::new();
    out.push_str("\nBackend Result Matrix\n");
    out.push_str("+------------------------------+");
    for _ in &matrix.backends {
        out.push_str("-----------+");
    }
    out.push('\n');
    out.push_str("| Test                         |");
    for b in &matrix.backends {
        out.push_str(&format!(" {:^9} |", format!("{:?}", b)));
    }
    out.push('\n');
    out.push_str("+------------------------------+");
    for _ in &matrix.backends {
        out.push_str("-----------+");
    }
    out.push('\n');

    for (name, row) in &matrix.rows {
        out.push_str(&format!("| {:<28} |", truncate(name, 28)));
        for b in &matrix.backends {
            let status = row.get(b).copied().unwrap_or(TestStatus::Ignored).as_str();
            out.push_str(&format!(" {:<9} |", status));
        }
        out.push('\n');
    }

    out.push_str("+------------------------------+");
    for _ in &matrix.backends {
        out.push_str("-----------+");
    }
    out.push('\n');

    if !matrix.inconsistencies.is_empty() {
        out.push_str("Backend Inconsistencies:\n");
        for i in &matrix.inconsistencies {
            out.push_str("  - ");
            out.push_str(i);
            out.push('\n');
        }
    }

    out
}

fn truncate(input: &str, max: usize) -> String {
    if input.len() <= max {
        input.to_string()
    } else {
        format!("{}…", &input[..max.saturating_sub(1)])
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Per-backend summary helper
// ─────────────────────────────────────────────────────────────────────────────

/// Aggregated pass/fail numbers for a single backend across all test cases.
#[derive(Debug, Clone, Default)]
pub struct PerBackendSummary {
    pub backend: ExecutionBackend,
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub panics: usize,
    pub timeout: usize,
    pub ignored: usize,
}

impl PerBackendSummary {
    pub fn is_clean(&self) -> bool {
        self.failed == 0 && self.panics == 0 && self.timeout == 0
    }
}

/// Build per-backend summary stats from test results.
pub fn build_per_backend_summaries(
    results: &[TestCaseResult],
    backends: &[ExecutionBackend],
) -> Vec<PerBackendSummary> {
    let mut summaries: Vec<PerBackendSummary> = backends
        .iter()
        .map(|b| PerBackendSummary {
            backend: *b,
            ..Default::default()
        })
        .collect();

    for result in results {
        for s in summaries.iter_mut() {
            let status = result
                .backend_results
                .get(&s.backend)
                .map(|r| r.status)
                .unwrap_or(TestStatus::Ignored);
            s.total += 1;
            match status {
                TestStatus::Pass => s.passed += 1,
                TestStatus::Fail => s.failed += 1,
                TestStatus::Panic => s.panics += 1,
                TestStatus::Timeout => s.timeout += 1,
                TestStatus::Ignored => s.ignored += 1,
            }
        }
    }
    summaries
}

// ─────────────────────────────────────────────────────────────────────────────
// Rich multi-backend report renderer
// ─────────────────────────────────────────────────────────────────────────────

/// Short display name for a backend (used in column headers).
fn backend_short_name(b: ExecutionBackend) -> &'static str {
    match b {
        ExecutionBackend::Interpreter => "interp",
        ExecutionBackend::Jit => "jit",
        ExecutionBackend::NativeJit => "njit",
        ExecutionBackend::Mixed => "mixed",
        ExecutionBackend::Safe => "safe",
        ExecutionBackend::Bytecode => "vm",
        ExecutionBackend::AdaptiveJit => "ajit",
        ExecutionBackend::TieredJit => "tjit",
        ExecutionBackend::Aot => "aot",
        ExecutionBackend::Wasm => "wasm",
        #[cfg(debug_assertions)]
        ExecutionBackend::Gpu => "gpu",
    }
}

/// Public alias for use in backends.rs / CLI layer.
pub fn backend_short_name_pub(b: ExecutionBackend) -> &'static str {
    backend_short_name(b)
}

/// Render a coloured status cell (8-char wide).
fn status_cell(status: TestStatus, no_color: bool) -> String {
    let (text, ansi) = match status {
        TestStatus::Pass => ("  PASS  ", "\x1b[32m"),  // green
        TestStatus::Fail => ("  FAIL  ", "\x1b[31m"),  // red
        TestStatus::Panic => (" PANIC  ", "\x1b[35m"), // magenta
        TestStatus::Timeout => (" TIMEOUT", "\x1b[33m"), // yellow
        TestStatus::Ignored => (" IGNORE ", ""),
    };
    if no_color || ansi.is_empty() {
        format!("{}", text)
    } else {
        format!("{}{}\x1b[0m", ansi, text)
    }
}

/// Render the detailed multi-backend result grid.
///
/// Shows one row per test, one column per backend, with colour-coded cells.
/// Below the grid: per-backend summary bars and an overall overview.
pub fn render_multi_backend_report(
    results: &[TestCaseResult],
    backends: &[ExecutionBackend],
    summaries: &[PerBackendSummary],
    no_color: bool,
) -> String {
    use std::fmt::Write as _;

    let bold = if no_color { "" } else { "\x1b[1m" };
    let dim = if no_color { "" } else { "\x1b[2m" };
    let reset = if no_color { "" } else { "\x1b[0m" };
    let cyan = if no_color { "" } else { "\x1b[36m" };
    let green = if no_color { "" } else { "\x1b[32m" };
    let red = if no_color { "" } else { "\x1b[31m" };
    let yellow = if no_color { "" } else { "\x1b[33m" };

    let mut out = String::new();

    // ── Header ───────────────────────────────────────────────────────────────
    let _ = writeln!(
        out,
        "\n{bold}{cyan}╔══════════════════════════════════════════════════════════════╗{reset}"
    );
    let _ = writeln!(
        out,
        "{bold}{cyan}║          Multi-Backend Test Report                           ║{reset}"
    );
    let _ = writeln!(
        out,
        "{bold}{cyan}╚══════════════════════════════════════════════════════════════╝{reset}"
    );
    let _ = writeln!(
        out,
        "{dim}Backends tested: {}{reset}",
        backends
            .iter()
            .map(|b| backend_short_name(*b))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = writeln!(out);

    // ── Per-test grid ────────────────────────────────────────────────────────
    // Column widths
    let name_w = results
        .iter()
        .map(|r| r.name.replace("__", "::").len())
        .max()
        .unwrap_or(20)
        .max(20)
        .min(50);
    let cell_w: usize = 8; // fixed cell width

    // Header row
    let _ = write!(out, "  {bold}{:<name_w$}{reset}", "Test", name_w = name_w);
    for b in backends {
        let label = backend_short_name(*b);
        let _ = write!(out, "  {:^cell_w$}", label, cell_w = cell_w);
    }
    let _ = writeln!(out);

    // Separator
    let _ = write!(out, "  {}", "─".repeat(name_w));
    for _ in backends {
        let _ = write!(out, "  {}", "─".repeat(cell_w));
    }
    let _ = writeln!(out);

    // Test rows
    for result in results {
        let display_name = result.name.replace("__", "::");
        let truncated = if display_name.len() > name_w {
            format!("{}…", &display_name[..name_w.saturating_sub(1)])
        } else {
            display_name.clone()
        };

        // Color the test name based on effective status
        let name_color = match result.effective_status {
            TestStatus::Pass => green,
            TestStatus::Fail | TestStatus::Panic | TestStatus::Timeout => red,
            TestStatus::Ignored => dim,
        };
        let _ = write!(
            out,
            "  {name_color}{:<name_w$}{reset}",
            truncated,
            name_w = name_w
        );

        // Status cells per backend
        for b in backends {
            let status = result
                .backend_results
                .get(b)
                .map(|r| r.status)
                .unwrap_or(TestStatus::Ignored);
            let cell = status_cell(status, no_color);
            let _ = write!(out, "  {}", cell);
        }

        // Append timing from first available backend
        let timing = result
            .backend_results
            .values()
            .next()
            .map(|r| {
                let d = r.duration;
                if d.as_secs() > 0 {
                    format!(" {:.2}s", d.as_secs_f64())
                } else if d.as_millis() > 0 {
                    format!(" {:.1}ms", d.as_secs_f64() * 1000.0)
                } else {
                    format!(" {:.0}µs", d.as_secs_f64() * 1_000_000.0)
                }
            })
            .unwrap_or_default();
        let _ = writeln!(out, "  {dim}{}{reset}", timing);
    }

    // ── Per-backend summary table ─────────────────────────────────────────────
    let _ = writeln!(out);
    let _ = writeln!(out, "{bold}Per-Backend Summary{reset}");
    let _ = write!(out, "  {:<10}", "Backend");
    let _ = write!(out, "  {:>5}", "Total");
    let _ = write!(out, "  {:>6}", "Passed");
    let _ = write!(out, "  {:>6}", "Failed");
    let _ = write!(out, "  {:>6}", "Panics");
    let _ = write!(out, "  {:>7}", "Timeout");
    let _ = write!(out, "  {:>7}", "Ignored");
    let _ = writeln!(out, "  Result");
    let _ = write!(out, "  {}", "─".repeat(10));
    let _ = write!(out, "  {}", "─".repeat(5));
    let _ = write!(out, "  {}", "─".repeat(6));
    let _ = write!(out, "  {}", "─".repeat(6));
    let _ = write!(out, "  {}", "─".repeat(6));
    let _ = write!(out, "  {}", "─".repeat(7));
    let _ = write!(out, "  {}", "─".repeat(7));
    let _ = writeln!(out, "  {}", "─".repeat(8));

    let mut all_clean = true;
    for s in summaries {
        let pass_color = if s.is_clean() { green } else { "" };
        let fail_color = if s.failed > 0 { red } else { "" };
        let panic_color = if s.panics > 0 { red } else { "" };
        let timeout_color = if s.timeout > 0 { yellow } else { "" };
        let result_str = if s.is_clean() {
            format!("{green}✓ PASS{reset}")
        } else {
            all_clean = false;
            format!("{red}✗ FAIL{reset}")
        };

        let _ = write!(out, "  {:<10}", backend_short_name(s.backend));
        let _ = write!(out, "  {:>5}", s.total);
        let _ = write!(out, "  {pass_color}{:>6}{reset}", s.passed);
        let _ = write!(out, "  {fail_color}{:>6}{reset}", s.failed);
        let _ = write!(out, "  {panic_color}{:>6}{reset}", s.panics);
        let _ = write!(out, "  {timeout_color}{:>7}{reset}", s.timeout);
        let _ = write!(out, "  {:>7}", s.ignored);
        let _ = writeln!(out, "  {}", result_str);
    }

    // ── Failure details section ──────────────────────────────────────────────
    let failing_items: Vec<(&TestCaseResult, Vec<(&str, &str)>)> = results
        .iter()
        .filter_map(|r| {
            let mut errs = Vec::new();
            for b in backends {
                if let Some(br) = r.backend_results.get(b) {
                    if matches!(
                        br.status,
                        TestStatus::Fail | TestStatus::Panic | TestStatus::Timeout
                    ) {
                        if let Some(ref msg) = br.message {
                            errs.push((backend_short_name(*b), msg.as_str()));
                        }
                    }
                }
            }
            if !errs.is_empty() {
                Some((r, errs))
            } else {
                None
            }
        })
        .collect();

    if !failing_items.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "{bold}{red}Failure Details ({} failed test{}):{reset}",
            failing_items.len(),
            if failing_items.len() == 1 { "" } else { "s" }
        );
        for (r, errs) in failing_items {
            let display_name = r.name.replace("__", "::");
            let _ = writeln!(out, "  {red}✗{reset} {bold}{}{reset}", display_name);
            for (backend_name, msg) in errs {
                let first_line = msg.lines().next().unwrap_or(msg).trim();
                let _ = writeln!(
                    out,
                    "    {dim}[{}]{reset} {red}{}{reset}",
                    backend_name, first_line
                );
            }
        }
    }

    // ── Inconsistency report ─────────────────────────────────────────────────
    let inconsistent: Vec<&TestCaseResult> = results
        .iter()
        .filter(|r| {
            let statuses: std::collections::BTreeSet<_> = backends
                .iter()
                .filter_map(|b| r.backend_results.get(b))
                .map(|br| br.status)
                .collect();
            statuses.len() > 1
        })
        .collect();

    if !inconsistent.is_empty() {
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "{bold}{yellow}⚠  Backend Inconsistencies ({} test{}):{reset}",
            inconsistent.len(),
            if inconsistent.len() == 1 { "" } else { "s" }
        );
        let _ = writeln!(
            out,
            "{dim}   These tests produce different outcomes across backends{reset}"
        );
        for r in &inconsistent {
            let _ = write!(out, "   {yellow}▶{reset} {}", r.name.replace("__", "::"));
            for b in backends {
                let status = r
                    .backend_results
                    .get(b)
                    .map(|br| br.status)
                    .unwrap_or(TestStatus::Ignored);
                let cell = match status {
                    TestStatus::Pass => format!("{green}PASS{reset}"),
                    TestStatus::Fail => format!("{red}FAIL{reset}"),
                    TestStatus::Panic => {
                        format!("{}PANIC{reset}", if no_color { "" } else { "\x1b[35m" })
                    }
                    TestStatus::Timeout => format!("{yellow}TIME{reset}"),
                    TestStatus::Ignored => format!("{dim}SKIP{reset}"),
                };
                let _ = write!(out, "  {}:{}", backend_short_name(*b), cell);
            }
            let _ = writeln!(out);
        }
    }

    // ── Overall result ────────────────────────────────────────────────────────
    let _ = writeln!(out);
    let _ = write!(out, "  {}", "═".repeat(60));
    let _ = writeln!(out);
    if all_clean && inconsistent.is_empty() {
        let _ = writeln!(
            out,
            "  {bold}{green}✓ All backends passed — output is consistent across runtimes{reset}"
        );
    } else {
        if !all_clean {
            let failing: Vec<&str> = summaries
                .iter()
                .filter(|s| !s.is_clean())
                .map(|s| backend_short_name(s.backend))
                .collect();
            let _ = writeln!(
                out,
                "  {bold}{red}✗ Failures on: {}{reset}",
                failing.join(", ")
            );
        }
        if !inconsistent.is_empty() {
            let _ = writeln!(
                out,
                "  {bold}{yellow}⚠  {} test(s) behave differently across backends{reset}",
                inconsistent.len()
            );
        }
    }
    let _ = writeln!(out);

    out
}

/// Render a progress banner printed before multi-backend tests start.
pub fn render_multi_backend_header(backends: &[ExecutionBackend], no_color: bool) -> String {
    let bold = if no_color { "" } else { "\x1b[1m" };
    let cyan = if no_color { "" } else { "\x1b[36m" };
    let reset = if no_color { "" } else { "\x1b[0m" };
    let dim = if no_color { "" } else { "\x1b[2m" };

    let names: Vec<&str> = backends.iter().map(|b| backend_short_name(*b)).collect();
    format!(
        "{bold}{cyan}Running tests across {} backend(s):{reset} {dim}{}{reset}\n",
        backends.len(),
        names.join(", ")
    )
}

pub fn to_json_report(
    results: &[TestCaseResult],
    summary: &TestSummary,
    matrix: Option<&BackendMatrix>,
) -> String {
    let tests = results
        .iter()
        .map(|r| {
            let backends = r
                .backend_results
                .iter()
                .map(|(b, br)| {
                    (
                        format!("{:?}", b),
                        json!({
                            "status": br.status.as_str(),
                            "duration_ms": br.duration.as_millis(),
                            "message": br.message,
                            "stack_trace": br.stack_trace,
                        }),
                    )
                })
                .collect::<serde_json::Map<String, serde_json::Value>>();

            json!({
                "name": r.name,
                "status": r.effective_status.as_str(),
                "expect_fail_applied": r.expected_fail_applied,
                "backends": backends,
            })
        })
        .collect::<Vec<_>>();

    let payload = json!({
        "summary": {
            "total": summary.total,
            "passed": summary.passed,
            "failed": summary.failed,
            "panics": summary.panics,
            "ignored": summary.ignored,
            "timeout": summary.timeout,
            "duration_ms": summary.duration.as_millis(),
        },
        "tests": tests,
        "matrix": matrix.map(|m| {
            let rows = m.rows.iter().map(|(name, row)| {
                let statuses = row.iter().map(|(b,s)| (format!("{:?}", b), json!(s.as_str()))).collect::<serde_json::Map<_,_>>();
                (name.clone(), json!(statuses))
            }).collect::<serde_json::Map<_,_>>();
            json!({
                "backends": m.backends.iter().map(|b| format!("{:?}", b)).collect::<Vec<_>>(),
                "rows": rows,
                "inconsistencies": m.inconsistencies,
            })
        })
    });

    serde_json::to_string_pretty(&payload)
        .unwrap_or_else(|_| "{\"error\":\"json-serialize-failed\"}".to_string())
}

pub fn run_tests_with_interpreter(ast: &[Stmt]) -> TestSummary {
    #[derive(Clone, Copy)]
    struct SmokeExecutor;
    impl BackendTestExecutor for SmokeExecutor {
        fn execute_test(
            &mut self,
            _backend: ExecutionBackend,
            _test: &TestInfo,
            _timeout: Duration,
            _options: &TestRunOptions,
        ) -> BackendExecutionResult {
            BackendExecutionResult {
                status: TestStatus::Pass,
                message: None,
                duration: Duration::from_millis(0),
                stack_trace: None,
                output: String::new(),
            }
        }
    }

    let mut executor = SmokeExecutor;
    let (results, summary, _) = run_tests(
        ast,
        &mut executor,
        &TestRunOptions::default(),
        ExecutionBackend::Interpreter,
    );

    for result in &results {
        let backend_result = result.backend_results.get(&ExecutionBackend::Interpreter);
        let msg = backend_result.and_then(|r| r.message.as_deref());
        let duration = backend_result.map(|r| r.duration).unwrap_or_default();
        let output = backend_result.map(|r| r.output.clone()).unwrap_or_default();
        println!(
            "{}",
            render_result_line(
                &result.name,
                result.effective_status,
                false,
                msg,
                Some(duration),
                if output.is_empty() {
                    None
                } else {
                    Some(&output)
                }
            )
        );
        // Print captured output only on failures/panics/timeouts
        let show_output = matches!(
            result.effective_status,
            TestStatus::Fail | TestStatus::Panic | TestStatus::Timeout
        );
        if show_output && !output.is_empty() {
            for line in output.lines() {
                println!("    {}", line);
            }
        }
    }
    println!("{}", format_test_summary(&summary, false));
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_detects_inconsistency() {
        let mut row = BTreeMap::new();
        row.insert(
            ExecutionBackend::Interpreter,
            BackendExecutionResult {
                status: TestStatus::Pass,
                message: None,
                duration: Duration::from_millis(1),
                stack_trace: None,
                output: String::new(),
            },
        );
        row.insert(
            ExecutionBackend::Jit,
            BackendExecutionResult {
                status: TestStatus::Fail,
                message: None,
                duration: Duration::from_millis(1),
                stack_trace: None,
                output: String::new(),
            },
        );
        let case = TestCaseResult {
            name: "t".into(),
            effective_status: TestStatus::Fail,
            expected_fail_applied: false,
            backend_results: row,
        };
        let m = build_backend_matrix(
            &[case],
            &[ExecutionBackend::Interpreter, ExecutionBackend::Jit],
        );
        assert_eq!(m.inconsistencies.len(), 1);
    }
}

pub mod executor;
pub mod stdout_capture;
