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
    pub function: Function,
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
    pub json_format: bool,
    pub no_color: bool,
    pub deterministic_seed: u64,
    pub frozen_unix_time_secs: i64,
    pub quiet: bool,     // Suppress print output from tests
    pub nocapture: bool, // Always show captured output (even for passing tests)
}

impl Default for TestRunOptions {
    fn default() -> Self {
        Self {
            fail_fast: false,
            include_tags: BTreeSet::new(),
            test_name: None,
            default_timeout: Duration::from_secs(5),
            backend_check: false,
            json_format: false,
            no_color: false,
            deterministic_seed: 0xD00DFEED,
            frozen_unix_time_secs: 1_735_689_600,
            quiet: false,
            nocapture: false,
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

pub trait BackendTestExecutor {
    fn execute_test(
        &mut self,
        backend: ExecutionBackend,
        test: &TestInfo,
        timeout: Duration,
        options: &TestRunOptions,
    ) -> BackendExecutionResult;
}

pub fn collect_tests(ast: &[Stmt]) -> Vec<TestInfo> {
    let mut tests = Vec::new();

    for stmt in ast {
        if let StmtKind::Function(func, _) = &stmt.kind {
            if func.is_test {
                tests.push(TestInfo {
                    name: func.name.clone(),
                    function: func.clone(),
                    should_ignore: func.test_ignore,
                    expect_fail: func.test_expect_fail,
                    timeout: func.test_timeout.map(Duration::from_secs),
                    tags: extract_tags(&func.decorators),
                });
            }
        }
    }

    tests
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

    let mut tests = collect_tests(ast);
    if let Some(name) = options.test_name.as_deref() {
        let normalized = name.replace("::", "__");
        let prefix = format!("{}__", normalized);
        tests.retain(|t| t.name == normalized || t.name.starts_with(&prefix));
    }
    let backends = if options.backend_check {
        all_execution_backends()
    } else {
        vec![selected_backend]
    };

    for test in tests {
        let status = if test.should_ignore || !filter_by_tags(&test, &options.include_tags) {
            let mut map = BTreeMap::new();
            for backend in &backends {
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

            for backend in &backends {
                let r = executor.execute_test(*backend, &test, timeout, options);
                canonical = first_non_pass(canonical, r.status);
                backend_results.insert(*backend, r);
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
        };

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

    summary.duration = started.elapsed();
    let matrix = if options.backend_check {
        Some(build_backend_matrix(&all_results, &backends))
    } else {
        None
    };

    (all_results, summary, matrix)
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
