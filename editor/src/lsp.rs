use lsp_types::*;
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Mutex};

use crate::completion::{CandidateKind, CompletionCandidate};
use crate::discovery::find_executable;

#[derive(Debug, Clone)]
pub struct LspDiagnostic {
    pub line: usize,
    pub col: usize,
    pub message: String,
    pub severity: DiagnosticSeverity,
}

#[derive(Debug, Clone)]
pub struct DocumentSymbolInfo {
    pub name: String,
    pub kind: String,
    pub line: usize,
    pub col: usize,
}

pub struct LspClient {
    process: Option<Child>,
    next_id: AtomicI32,
    pub diagnostics: Arc<Mutex<Vec<LspDiagnostic>>>,
    pub completions: Arc<Mutex<Vec<CompletionCandidate>>>,
    pub hover_text: Arc<Mutex<Option<String>>>,
    pub symbols: Arc<Mutex<Vec<DocumentSymbolInfo>>>,
    pub active: bool,
}

impl LspClient {
    pub fn new() -> Self {
        Self {
            process: None,
            next_id: AtomicI32::new(1),
            diagnostics: Arc::new(Mutex::new(Vec::new())),
            completions: Arc::new(Mutex::new(Vec::new())),
            hover_text: Arc::new(Mutex::new(None)),
            symbols: Arc::new(Mutex::new(Vec::new())),
            active: false,
        }
    }

    pub fn start(&mut self, workspace_root: &Path) -> bool {
        let als_bin = match find_executable("als").or_else(|| find_executable("adesh-language-server")) {
            Some(bin) => bin,
            None => return false,
        };

        let mut child = match Command::new(als_bin)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(_) => return false,
        };

        let stdout = child.stdout.take();
        self.process = Some(child);
        self.active = true;

        if let Some(stdout) = stdout {
            let diag_arc = Arc::clone(&self.diagnostics);
            let comp_arc = Arc::clone(&self.completions);
            let hover_arc = Arc::clone(&self.hover_text);
            let sym_arc = Arc::clone(&self.symbols);

            tokio::spawn(async move {
                let mut reader = BufReader::new(stdout);
                loop {
                    let mut header_line = String::new();
                    if reader.read_line(&mut header_line).unwrap_or(0) == 0 {
                        break;
                    }
                    if header_line.starts_with("Content-Length:") {
                        let len_str = header_line.trim_start_matches("Content-Length:").trim();
                        if let Ok(content_len) = len_str.parse::<usize>() {
                            let mut empty_line = String::new();
                            let _ = reader.read_line(&mut empty_line);

                            let mut body_buf = vec![0u8; content_len];
                            if std::io::Read::read_exact(&mut reader, &mut body_buf).is_ok() {
                                if let Ok(json_val) = serde_json::from_slice::<Value>(&body_buf) {
                                    // Diagnostics notification
                                    if json_val.get("method").and_then(|m| m.as_str()) == Some("textDocument/publishDiagnostics") {
                                        if let Some(params) = json_val.get("params") {
                                            if let Some(diags_arr) = params.get("diagnostics").and_then(|d| d.as_array()) {
                                                let mut parsed_list = Vec::new();
                                                for d in diags_arr {
                                                    let line = d.pointer("/range/start/line").and_then(|l| l.as_u64()).unwrap_or(0) as usize;
                                                    let col = d.pointer("/range/start/character").and_then(|c| c.as_u64()).unwrap_or(0) as usize;
                                                    let msg = d.get("message").and_then(|m| m.as_str()).unwrap_or("Syntax error").to_string();
                                                    let sev_num = d.get("severity").and_then(|s| s.as_u64()).unwrap_or(1);
                                                    let severity = match sev_num {
                                                        2 => DiagnosticSeverity::WARNING,
                                                        3 => DiagnosticSeverity::INFORMATION,
                                                        4 => DiagnosticSeverity::HINT,
                                                        _ => DiagnosticSeverity::ERROR,
                                                     };
                                                    parsed_list.push(LspDiagnostic { line, col, message: msg, severity });
                                                }
                                                if let Ok(mut diag_lock) = diag_arc.lock() {
                                                    *diag_lock = parsed_list;
                                                }
                                            }
                                        }
                                    }

                                    // Completion / Hover / Symbols Response
                                    if let Some(result) = json_val.get("result") {
                                        if let Some(items) = result.get("items").and_then(|i| i.as_array()).or_else(|| result.as_array()) {
                                            let mut list = Vec::new();
                                            for item in items {
                                                if let Some(label) = item.get("label").and_then(|l| l.as_str()) {
                                                    let kind_num = item.get("kind").and_then(|k| k.as_u64()).unwrap_or(1) as u8;
                                                    let kind = match kind_num {
                                                        2 | 3 => CandidateKind::Function,
                                                        6 => CandidateKind::Variable,
                                                        7 => CandidateKind::Struct,
                                                        13 => CandidateKind::Enum,
                                                        9 => CandidateKind::Module,
                                                        15 => CandidateKind::Snippet,
                                                        14 => CandidateKind::Keyword,
                                                        _ => CandidateKind::Keyword,
                                                    };
                                                    let detail = item.get("detail").and_then(|d| d.as_str()).map(ToString::to_string);
                                                    let doc = item.get("documentation").and_then(|d| {
                                                        if let Some(s) = d.as_str() {
                                                            Some(s.to_string())
                                                        } else {
                                                            d.get("value").and_then(|v| v.as_str()).map(ToString::to_string)
                                                        }
                                                    });
                                                    list.push(CompletionCandidate {
                                                        label: label.to_string(),
                                                        insert_text: item.get("insertText").and_then(|it| it.as_str()).map(ToString::to_string),
                                                        kind,
                                                        detail,
                                                        documentation: doc,
                                                    });
                                                }
                                            }
                                            if !list.is_empty() {
                                                if let Ok(mut comp_lock) = comp_arc.lock() {
                                                    *comp_lock = list;
                                                }
                                            }
                                        }

                                        // Hover
                                        if let Some(contents) = result.get("contents") {
                                            let text = if let Some(s) = contents.as_str() {
                                                s.to_string()
                                            } else if let Some(val) = contents.get("value").and_then(|v| v.as_str()) {
                                                val.to_string()
                                            } else {
                                                contents.to_string()
                                            };
                                            if let Ok(mut hover_lock) = hover_arc.lock() {
                                                *hover_lock = Some(text);
                                            }
                                        }

                                        // Document Symbols
                                        if let Some(arr) = result.as_array() {
                                            let mut sym_list = Vec::new();
                                            for item in arr {
                                                if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                                                    let kind_str = item.get("kind").map(|k| k.to_string()).unwrap_or_else(|| "Symbol".to_string());
                                                    let line = item.pointer("/range/start/line").and_then(|l| l.as_u64()).unwrap_or(0) as usize;
                                                    let col = item.pointer("/range/start/character").and_then(|c| c.as_u64()).unwrap_or(0) as usize;
                                                    sym_list.push(DocumentSymbolInfo {
                                                        name: name.to_string(),
                                                        kind: kind_str,
                                                        line,
                                                        col,
                                                    });
                                                }
                                            }
                                            if !sym_list.is_empty() {
                                                if let Ok(mut sym_lock) = sym_arc.lock() {
                                                    *sym_lock = sym_list;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            });
        }

        let root_uri = Url::from_directory_path(workspace_root).ok();
        #[allow(deprecated)]
        let init_params = InitializeParams {
            root_uri,
            capabilities: ClientCapabilities::default(),
            ..Default::default()
        };

        self.send_request("initialize", serde_json::to_value(init_params).unwrap_or_default());
        true
    }

    pub fn send_did_open(&mut self, path: &Path, content: &str) {
        if !self.active {
            return;
        }
        if let Ok(uri) = Url::from_file_path(path) {
            let params = DidOpenTextDocumentParams {
                text_document: TextDocumentItem {
                    uri,
                    language_id: "adesh".to_string(),
                    version: 1,
                    text: content.to_string(),
                },
            };
            self.send_notification("textDocument/didOpen", serde_json::to_value(params).unwrap_or_default());
        }
    }

    pub fn send_did_change(&mut self, path: &Path, version: i32, content: &str) {
        if !self.active {
            return;
        }
        if let Ok(uri) = Url::from_file_path(path) {
            let params = DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier { uri, version },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: content.to_string(),
                }],
            };
            self.send_notification("textDocument/didChange", serde_json::to_value(params).unwrap_or_default());
        }
    }

    pub fn request_completion(&mut self, path: &Path, line: usize, col: usize) {
        if !self.active {
            return;
        }
        if let Ok(uri) = Url::from_file_path(path) {
            let params = CompletionParams {
                text_document_position: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri },
                    position: Position {
                        line: line as u32,
                        character: col as u32,
                    },
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
                context: None,
            };
            self.send_request("textDocument/completion", serde_json::to_value(params).unwrap_or_default());
        }
    }

    pub fn request_hover(&mut self, path: &Path, line: usize, col: usize) {
        if !self.active {
            return;
        }
        if let Ok(uri) = Url::from_file_path(path) {
            let params = HoverParams {
                text_document_position_params: TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri },
                    position: Position {
                        line: line as u32,
                        character: col as u32,
                    },
                },
                work_done_progress_params: WorkDoneProgressParams::default(),
            };
            self.send_request("textDocument/hover", serde_json::to_value(params).unwrap_or_default());
        }
    }

    pub fn request_document_symbols(&mut self, path: &Path) {
        if !self.active {
            return;
        }
        if let Ok(uri) = Url::from_file_path(path) {
            let params = DocumentSymbolParams {
                text_document: TextDocumentIdentifier { uri },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            };
            self.send_request("textDocument/documentSymbol", serde_json::to_value(params).unwrap_or_default());
        }
    }

    fn send_request(&mut self, method: &str, params: Value) -> i32 {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.write_message(&req.to_string());
        id
    }

    fn send_notification(&mut self, method: &str, params: Value) {
        let notif = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.write_message(&notif.to_string());
    }

    fn write_message(&mut self, content: &str) {
        if let Some(ref mut child) = self.process {
            if let Some(ref mut stdin) = child.stdin {
                let msg = format!("Content-Length: {}\r\n\r\n{}", content.len(), content);
                let _ = stdin.write_all(msg.as_bytes());
                let _ = stdin.flush();
            }
        }
    }

    pub fn stop(&mut self) {
        if self.active {
            self.send_notification("shutdown", serde_json::Value::Null);
            self.send_notification("exit", serde_json::Value::Null);
            if let Some(mut child) = self.process.take() {
                let _ = child.kill();
            }
            self.active = false;
        }
    }
}

impl Drop for LspClient {
    fn drop(&mut self) {
        self.stop();
    }
}
