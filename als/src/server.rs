//! Adesh Language Server implementation
//!
//! Integrates the semantic engine from the compiler crate with the LSP protocol.
//! All IDE features (completion, hover, definition, references, diagnostics, etc.)
//! use the shared semantic engine rather than duplicating compiler logic.

use log::{info, warn};
use lsp_server::{Notification, Request, RequestId, Response};
use lsp_types::*;
use serde_json::Value;
use std::collections::HashMap;

use crate::analysis::{analyze, extract_symbols_with_positions};
use crate::completion::get_completions_semantic;
use crate::diagnostics::compute_diagnostics_semantic;
use crate::document::DocumentStore;
use crate::formatting::format_document;
use crate::hover::get_hover_semantic;
use crate::symbols::{
    do_rename_semantic, find_definition_semantic, find_references_semantic,
    get_document_symbols_semantic, prepare_rename_semantic,
};
use crate::semantic_tokens::{get_semantic_tokens, get_semantic_tokens_range};
use crate::inlay_hints::get_inlay_hints;
use crate::workspace::WorkspaceIndex;

/// Adesh Language Server
pub struct AdeshLanguageServer {
    documents: DocumentStore,
    workspace: WorkspaceIndex,
    /// Cached diagnostics per document
    diagnostics_cache: HashMap<Url, Vec<Diagnostic>>,
}

impl AdeshLanguageServer {
    pub fn new() -> Self {
        Self {
            documents: DocumentStore::new(),
            workspace: WorkspaceIndex::new(),
            diagnostics_cache: HashMap::new(),
        }
    }

    /// Get the semantic index for a document (re-index if needed).
    #[allow(dead_code)]
    fn get_or_index(&mut self, uri: &Url) -> Option<&adeshlang::semantics::SemanticIndex> {
        // Index if not already indexed
        if self.workspace.get_index(uri).is_none() {
            if let Some(doc) = self.documents.get(uri) {
                self.workspace.index_document(uri, &doc.content);
            }
        }
        self.workspace.get_index(uri)
    }

    /// Handle an LSP request
    pub fn handle_request(&mut self, req: Request) -> Option<Response> {
        let id = req.id.clone();

        match req.method.as_str() {
            "textDocument/completion" => self.handle_completion(id, req.params),
            "textDocument/hover" => self.handle_hover(id, req.params),
            "textDocument/definition" => self.handle_definition(id, req.params),
            "textDocument/references" => self.handle_references(id, req.params),
            "textDocument/documentSymbol" => self.handle_document_symbol(id, req.params),
            "textDocument/workspaceSymbol" => self.handle_workspace_symbol(id, req.params),
            "textDocument/formatting" => self.handle_formatting(id, req.params),
            "textDocument/prepareRename" => self.handle_prepare_rename(id, req.params),
            "textDocument/rename" => self.handle_rename(id, req.params),
            "textDocument/codeAction" => self.handle_code_action(id, req.params),
            "textDocument/signatureHelp" => self.handle_signature_help(id, req.params),
            "completionItem/resolve" => self.handle_completion_resolve(id, req.params),
            "textDocument/semanticTokens/full" => self.handle_semantic_tokens_full(id, req.params),
            "textDocument/semanticTokens/range" => self.handle_semantic_tokens_range(id, req.params),
            "textDocument/inlayHint" => self.handle_inlay_hint(id, req.params),
            "textDocument/declaration" => self.handle_declaration(id, req.params),
            "textDocument/typeDefinition" => self.handle_type_definition(id, req.params),
            "textDocument/documentHighlight" => self.handle_document_highlight(id, req.params),
            "textDocument/foldingRange" => self.handle_folding_range(id, req.params),
            "textDocument/documentLink" => self.handle_document_link(id, req.params),
            "textDocument/selectionRange" => self.handle_selection_range(id, req.params),
            _ => {
                warn!("Unhandled request: {}", req.method);
                Some(Response::new_err(
                    id,
                    lsp_server::ErrorCode::MethodNotFound as i32,
                    format!("Method not found: {}", req.method),
                ))
            }
        }
    }

    /// Handle an LSP notification
    pub fn handle_notification(&mut self, notif: Notification) -> Vec<Notification> {
        let mut outgoing = Vec::new();
        match notif.method.as_str() {
            "textDocument/didOpen" => {
                if let Some(diag_notif) = self.handle_did_open(notif.params) {
                    outgoing.push(diag_notif);
                }
            }
            "textDocument/didChange" => {
                if let Some(diag_notif) = self.handle_did_change(notif.params) {
                    outgoing.push(diag_notif);
                }
            }
            "textDocument/didClose" => {
                self.handle_did_close(notif.params);
            }
            "textDocument/didSave" => {
                if let Some(diag_notif) = self.handle_did_save(notif.params) {
                    outgoing.push(diag_notif);
                }
            }
            "initialized" => info!("Client initialized"),
            "$/cancelRequest" => {}
            _ => warn!("Unhandled notification: {}", notif.method),
        }
        outgoing
    }

    // ===== Request Handlers =====

    fn handle_completion(&mut self, id: RequestId, params: Value) -> Option<Response> {
        let params: CompletionParams = serde_json::from_value(params).ok()?;
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;

        let doc = self.documents.get(uri)?;

        // Route ADL files to the ADL completion provider
        if crate::adl::is_adl_uri(uri) {
            let items = crate::adl::get_adl_completions(&doc.content, pos.line, pos.character);
            let response = CompletionResponse::Array(items);
            return Some(Response::new_ok(id, response));
        }

        let trigger = params.context.and_then(|ctx| ctx.trigger_character);

        // Get the line text for context analysis
        let line_text = doc.get_line(pos.line as usize).unwrap_or("");

        // Get the word before cursor
        let word = doc.get_word_at(pos.line, pos.character.saturating_sub(1));

        // Re-index the document to get the semantic index
        self.workspace.index_document(uri, &doc.content);
        let index = self.workspace.get_index(uri)?;

        let items = get_completions_semantic(index, trigger.as_deref(), word.as_deref(), line_text, pos.line, pos.character);

        let response = CompletionResponse::Array(items);
        Some(Response::new_ok(id, response))
    }

    fn handle_hover(&mut self, id: RequestId, params: Value) -> Option<Response> {
        let params: HoverParams = serde_json::from_value(params).ok()?;
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let doc = self.documents.get(uri)?;

        // Route ADL files to the ADL hover provider
        if crate::adl::is_adl_uri(uri) {
            let hover = crate::adl::get_adl_hover(&doc.content, pos.line, pos.character);
            return Some(Response::new_ok(id, hover));
        }

        // Re-index to get the semantic index
        self.workspace.index_document(uri, &doc.content);
        let index = self.workspace.get_index(uri)?;

        let hover = get_hover_semantic(index, &self.workspace, doc, pos.line, pos.character);

        Some(Response::new_ok(id, hover))
    }

    fn handle_definition(&mut self, id: RequestId, params: Value) -> Option<Response> {
        let params: GotoDefinitionParams = serde_json::from_value(params).ok()?;
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let doc = self.documents.get(uri)?;

        // Re-index to get the semantic index
        self.workspace.index_document(uri, &doc.content);
        let index = self.workspace.get_index(uri)?;

        let location = find_definition_semantic(index, &self.workspace, doc, pos.line, pos.character);

        let response: Option<GotoDefinitionResponse> = location.map(GotoDefinitionResponse::Scalar);
        Some(Response::new_ok(id, response))
    }

    fn handle_references(&mut self, id: RequestId, params: Value) -> Option<Response> {
        let params: ReferenceParams = serde_json::from_value(params).ok()?;
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let include_declaration = params.context.include_declaration;

        let doc = self.documents.get(uri)?;

        // Re-index to get the semantic index
        self.workspace.index_document(uri, &doc.content);
        let index = self.workspace.get_index(uri)?;

        let locations = find_references_semantic(index, &self.workspace, doc, pos.line, pos.character, include_declaration);

        Some(Response::new_ok(id, locations))
    }

    fn handle_document_symbol(&mut self, id: RequestId, params: Value) -> Option<Response> {
        let params: DocumentSymbolParams = serde_json::from_value(params).ok()?;
        let uri = &params.text_document.uri;

        let doc = self.documents.get(uri)?;

        // Route ADL files to the ADL document symbol provider
        if crate::adl::is_adl_uri(uri) {
            let symbols = crate::adl::get_adl_document_symbols(&doc.content, uri);
            let response = DocumentSymbolResponse::Nested(symbols);
            return Some(Response::new_ok(id, response));
        }

        // Re-index to get the semantic index
        self.workspace.index_document(uri, &doc.content);
        let index = self.workspace.get_index(uri)?;

        let symbols = get_document_symbols_semantic(index);

        let response = DocumentSymbolResponse::Nested(symbols);
        Some(Response::new_ok(id, response))
    }

    fn handle_workspace_symbol(&mut self, id: RequestId, params: Value) -> Option<Response> {
        let params: WorkspaceSymbolParams = serde_json::from_value(params).ok()?;
        let query = &params.query;

        let results = self.workspace.workspace_symbols(query);

        let symbols: Vec<SymbolInformation> = results
            .into_iter()
            .map(|(uri, sym)| {
                let start = Position {
                    line: (sym.line.saturating_sub(1)) as u32,
                    character: (sym.col.saturating_sub(1)) as u32,
                };
                let end = Position {
                    line: start.line,
                    character: start.character + sym.name.len() as u32,
                };

                let kind = match sym.kind {
                    adeshlang::semantics::SemanticSymbolKind::Function => SymbolKind::FUNCTION,
                    adeshlang::semantics::SemanticSymbolKind::Class => SymbolKind::CLASS,
                    adeshlang::semantics::SemanticSymbolKind::Variable => SymbolKind::VARIABLE,
                    adeshlang::semantics::SemanticSymbolKind::Constant => SymbolKind::CONSTANT,
                    adeshlang::semantics::SemanticSymbolKind::Method => SymbolKind::METHOD,
                    adeshlang::semantics::SemanticSymbolKind::Field => SymbolKind::FIELD,
                    adeshlang::semantics::SemanticSymbolKind::Struct => SymbolKind::STRUCT,
                    adeshlang::semantics::SemanticSymbolKind::Enum => SymbolKind::ENUM,
                    adeshlang::semantics::SemanticSymbolKind::Interface => SymbolKind::INTERFACE,
                    adeshlang::semantics::SemanticSymbolKind::TypeAlias => SymbolKind::TYPE_PARAMETER,
                    _ => SymbolKind::VARIABLE,
                };

                SymbolInformation {
                    name: sym.name.clone(),
                    kind,
                    tags: None,
                    #[allow(deprecated)]
                    deprecated: None,
                    location: Location {
                        uri: uri.clone(),
                        range: Range { start, end },
                    },
                    container_name: sym.parent_type.clone(),
                }
            })
            .collect();

        Some(Response::new_ok(id, symbols))
    }

    fn handle_formatting(&mut self, id: RequestId, params: Value) -> Option<Response> {
        let params: DocumentFormattingParams = serde_json::from_value(params).ok()?;
        let uri = &params.text_document.uri;

        let doc = self.documents.get(uri)?;

        // ADL files use their own formatter
        if crate::adl::is_adl_uri(uri) {
            let edits = crate::adl::format_adl_document(&doc.content);
            return Some(Response::new_ok(id, edits));
        }

        let edits = format_document(doc);

        Some(Response::new_ok(id, edits))
    }

    fn handle_prepare_rename(&mut self, id: RequestId, params: Value) -> Option<Response> {
        let params: TextDocumentPositionParams = serde_json::from_value(params).ok()?;
        let uri = &params.text_document.uri;
        let pos = params.position;

        let doc = self.documents.get(uri)?;

        self.workspace.index_document(uri, &doc.content);
        let index = self.workspace.get_index(uri)?;

        let range = prepare_rename_semantic(index, doc, pos.line, pos.character);

        let response: Option<PrepareRenameResponse> = range.map(PrepareRenameResponse::Range);
        Some(Response::new_ok(id, response))
    }

    fn handle_rename(&mut self, id: RequestId, params: Value) -> Option<Response> {
        let params: RenameParams = serde_json::from_value(params).ok()?;
        let uri = &params.text_document_position.text_document.uri;
        let pos = params.text_document_position.position;
        let new_name = &params.new_name;

        let doc = self.documents.get(uri)?;

        self.workspace.index_document(uri, &doc.content);
        let index = self.workspace.get_index(uri)?;

        let workspace_edit = do_rename_semantic(index, &self.workspace, doc, pos.line, pos.character, new_name);

        Some(Response::new_ok(id, workspace_edit))
    }

    fn handle_code_action(&mut self, id: RequestId, params: Value) -> Option<Response> {
        let params: CodeActionParams = serde_json::from_value(params).ok()?;
        let uri = &params.text_document.uri;

        let doc = self.documents.get(uri)?;
        let diagnostics = &params.context.diagnostics;

        // ADL files use schema-driven code actions
        if crate::adl::is_adl_uri(uri) {
            let actions = crate::adl::get_adl_code_actions(&doc.content, uri, diagnostics);
            return Some(Response::new_ok(id, actions));
        }

        let mut actions = Vec::new();

        for diag in diagnostics {
            if let Some(action) = self.generate_quick_fix(doc, diag) {
                actions.push(CodeActionOrCommand::CodeAction(action));
            }
        }

        Some(Response::new_ok(id, actions))
    }

    fn handle_signature_help(&mut self, id: RequestId, params: Value) -> Option<Response> {
        let params: SignatureHelpParams = serde_json::from_value(params).ok()?;
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let doc = self.documents.get(uri)?;

        // Get function name being called (simple heuristic)
        let line = doc.get_line(pos.line as usize)?;
        let before_cursor = &line[..(pos.character as usize).min(line.len())];

        let fn_name = before_cursor.rfind('(').and_then(|paren_pos| {
            let before_paren = &before_cursor[..paren_pos];
            let start = before_paren
                .rfind(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
                .map(|i| i + 1)
                .unwrap_or(0);
            Some(before_paren[start..].trim())
        });

        let signature_help = fn_name.and_then(|name| self.get_signature_for(name));

        Some(Response::new_ok(id, signature_help))
    }

    fn handle_completion_resolve(&mut self, id: RequestId, params: Value) -> Option<Response> {
        let item: CompletionItem = serde_json::from_value(params).ok()?;
        Some(Response::new_ok(id, item))
    }

    // ===== Notification Handlers =====

    fn handle_did_open(&mut self, params: Value) -> Option<Notification> {
        if let Ok(params) = serde_json::from_value::<DidOpenTextDocumentParams>(params) {
            let uri = params.text_document.uri;
            let content = params.text_document.text;
            let version = params.text_document.version;

            info!("Document opened: {}", uri);
            self.documents.open(uri.clone(), content.clone(), version);

            // ADL files use schema-driven validation diagnostics
            if crate::adl::is_adl_uri(&uri) {
                let diagnostics = crate::adl::compute_adl_diagnostics(&content, &uri);
                self.diagnostics_cache.insert(uri.clone(), diagnostics.clone());
                return Some(Self::build_publish_diagnostics(uri, diagnostics, Some(version)));
            }

            self.workspace.index_document(&uri, &content);

            if let Some(doc) = self.documents.get(&uri) {
                self.workspace.index_document(&uri, &doc.content);
                let index = self.workspace.get_index(&uri)?;
                let diagnostics = compute_diagnostics_semantic(index, doc);
                self.diagnostics_cache.insert(uri.clone(), diagnostics.clone());
                return Some(Self::build_publish_diagnostics(uri, diagnostics, Some(version)));
            }
        }
        None
    }

    fn handle_did_change(&mut self, params: Value) -> Option<Notification> {
        if let Ok(params) = serde_json::from_value::<DidChangeTextDocumentParams>(params) {
            let uri = params.text_document.uri;
            let version = params.text_document.version;

            if let Some(change) = params.content_changes.into_iter().last() {
                self.documents.update(&uri, change.text.clone(), version);

                // ADL files use schema-driven validation diagnostics
                if crate::adl::is_adl_uri(&uri) {
                    let diagnostics = crate::adl::compute_adl_diagnostics(&change.text, &uri);
                    self.diagnostics_cache.insert(uri.clone(), diagnostics.clone());
                    return Some(Self::build_publish_diagnostics(uri, diagnostics, Some(version)));
                }

                self.workspace.index_document(&uri, &change.text);

                if let Some(doc) = self.documents.get(&uri) {
                    let index = self.workspace.get_index(&uri)?;
                    let diagnostics = compute_diagnostics_semantic(index, doc);
                    self.diagnostics_cache.insert(uri.clone(), diagnostics.clone());
                    return Some(Self::build_publish_diagnostics(uri, diagnostics, Some(version)));
                }
            }
        }
        None
    }

    fn handle_did_close(&mut self, params: Value) {
        if let Ok(params) = serde_json::from_value::<DidCloseTextDocumentParams>(params) {
            let uri = params.text_document.uri;
            info!("Document closed: {}", uri);
            self.documents.close(&uri);
            self.workspace.remove_document(&uri);
            self.diagnostics_cache.remove(&uri);
        }
    }

    fn handle_did_save(&mut self, params: Value) -> Option<Notification> {
        if let Ok(params) = serde_json::from_value::<DidSaveTextDocumentParams>(params) {
            let uri = params.text_document.uri;
            info!("Document saved: {}", uri);
            if let Some(doc) = self.documents.get(&uri) {
                self.workspace.index_document(&uri, &doc.content);
                let index = self.workspace.get_index(&uri)?;
                let diagnostics = compute_diagnostics_semantic(index, doc);
                self.diagnostics_cache.insert(uri.clone(), diagnostics.clone());
                return Some(Self::build_publish_diagnostics(uri, diagnostics, Some(doc.version)));
            }
        }
        None
    }

    fn build_publish_diagnostics(
        uri: Url,
        diagnostics: Vec<Diagnostic>,
        version: Option<i32>,
    ) -> Notification {
        let params = PublishDiagnosticsParams {
            uri,
            diagnostics,
            version,
        };
        Notification {
            method: "textDocument/publishDiagnostics".to_string(),
            params: serde_json::to_value(params).unwrap(),
        }
    }

    // ===== Quick Fix Generation =====

    fn generate_quick_fix(
        &self,
        doc: &crate::document::Document,
        diag: &Diagnostic,
    ) -> Option<CodeAction> {
        let message = diag.message.as_str();

        // Add missing import
        if message.contains("not found") || message.contains("undefined") {
            if let Some(word) = self.extract_identifier(message) {
                return Some(self.create_add_import_action(doc, diag, &word));
            }
        }

        // Remove unused variable
        if message.contains("unused variable") || message.contains("never used") {
            if let Some(var_name) = self.extract_identifier(message) {
                return Some(self.create_remove_unused_action(doc, diag, &var_name));
            }
        }

        None
    }

    fn extract_identifier(&self, message: &str) -> Option<String> {
        if let Some(start) = message.find('`') {
            if let Some(end) = message[start + 1..].find('`') {
                return Some(message[start + 1..start + 1 + end].to_string());
            }
        }
        None
    }

    fn create_add_import_action(
        &self,
        doc: &crate::document::Document,
        diag: &Diagnostic,
        identifier: &str,
    ) -> CodeAction {
        let edit = WorkspaceEdit {
            changes: Some({
                let mut changes = std::collections::HashMap::new();
                changes.insert(
                    doc.uri.clone(),
                    vec![TextEdit {
                        range: Range::new(Position::new(0, 0), Position::new(0, 0)),
                        new_text: format!("import {{ {} }} from \"{}\";\n", identifier, identifier),
                    }],
                );
                changes
            }),
            document_changes: None,
            change_annotations: None,
        };

        CodeAction {
            title: format!("Add import for '{}'", identifier),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![diag.clone()]),
            edit: Some(edit),
            command: None,
            is_preferred: Some(true),
            disabled: None,
            data: None,
        }
    }

    fn create_remove_unused_action(
        &self,
        doc: &crate::document::Document,
        diag: &Diagnostic,
        var_name: &str,
    ) -> CodeAction {
        let edit = WorkspaceEdit {
            changes: Some({
                let mut changes = std::collections::HashMap::new();
                changes.insert(
                    doc.uri.clone(),
                    vec![TextEdit {
                        range: diag.range,
                        new_text: String::new(),
                    }],
                );
                changes
            }),
            document_changes: None,
            change_annotations: None,
        };

        CodeAction {
            title: format!("Remove unused variable '{}'", var_name),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![diag.clone()]),
            edit: Some(edit),
            command: None,
            is_preferred: Some(false),
            disabled: None,
            data: None,
        }
    }

    fn get_signature_for(&self, name: &str) -> Option<SignatureHelp> {
        // Built-in function signatures
        let (label, doc, params) = match name {
            "print" => (
                "fn print(...args, options?)",
                "Print values to stdout",
                vec![
                    ParameterInformation {
                        label: ParameterLabel::Simple("...args".to_string()),
                        documentation: Some(Documentation::String("Values to print".to_string())),
                    },
                    ParameterInformation {
                        label: ParameterLabel::Simple("options?".to_string()),
                        documentation: Some(Documentation::String("Print options (color, sep, end, etc.)".to_string())),
                    },
                ],
            ),
            "println" => (
                "fn println(...args)",
                "Print values with newline",
                vec![ParameterInformation {
                    label: ParameterLabel::Simple("...args".to_string()),
                    documentation: Some(Documentation::String("Values to print".to_string())),
                }],
            ),
            "len" => (
                "fn len(collection): number",
                "Get length of array/string/collection",
                vec![ParameterInformation {
                    label: ParameterLabel::Simple("collection".to_string()),
                    documentation: Some(Documentation::String("Array, string, or collection".to_string())),
                }],
            ),
            "range" => (
                "fn range(start: number, end: number, step?: number): array",
                "Create a numeric range",
                vec![
                    ParameterInformation {
                        label: ParameterLabel::Simple("start".to_string()),
                        documentation: Some(Documentation::String("Start value (inclusive)".to_string())),
                    },
                    ParameterInformation {
                        label: ParameterLabel::Simple("end".to_string()),
                        documentation: Some(Documentation::String("End value (exclusive)".to_string())),
                    },
                    ParameterInformation {
                        label: ParameterLabel::Simple("step?".to_string()),
                        documentation: Some(Documentation::String("Step increment (default: 1)".to_string())),
                    },
                ],
            ),
            "map" => (
                "fn map(array, fn): array",
                "Transform each element of an array",
                vec![
                    ParameterInformation {
                        label: ParameterLabel::Simple("array".to_string()),
                        documentation: Some(Documentation::String("Array to transform".to_string())),
                    },
                    ParameterInformation {
                        label: ParameterLabel::Simple("fn".to_string()),
                        documentation: Some(Documentation::String("Transformation function".to_string())),
                    },
                ],
            ),
            "filter" => (
                "fn filter(array, fn): array",
                "Filter elements by predicate",
                vec![
                    ParameterInformation {
                        label: ParameterLabel::Simple("array".to_string()),
                        documentation: Some(Documentation::String("Array to filter".to_string())),
                    },
                    ParameterInformation {
                        label: ParameterLabel::Simple("fn".to_string()),
                        documentation: Some(Documentation::String("Predicate function".to_string())),
                    },
                ],
            ),
            "reduce" => (
                "fn reduce(array, fn, initial): any",
                "Reduce array to single value",
                vec![
                    ParameterInformation {
                        label: ParameterLabel::Simple("array".to_string()),
                        documentation: Some(Documentation::String("Array to reduce".to_string())),
                    },
                    ParameterInformation {
                        label: ParameterLabel::Simple("fn".to_string()),
                        documentation: Some(Documentation::String("Reducer function (acc, item) -> acc".to_string())),
                    },
                    ParameterInformation {
                        label: ParameterLabel::Simple("initial".to_string()),
                        documentation: Some(Documentation::String("Initial accumulator value".to_string())),
                    },
                ],
            ),
            "assert" => (
                "fn assert(condition: bool, message?: string)",
                "Fail if condition is false",
                vec![
                    ParameterInformation {
                        label: ParameterLabel::Simple("condition".to_string()),
                        documentation: Some(Documentation::String("Condition to check".to_string())),
                    },
                    ParameterInformation {
                        label: ParameterLabel::Simple("message?".to_string()),
                        documentation: Some(Documentation::String("Optional error message".to_string())),
                    },
                ],
            ),
            _ => return None,
        };

        Some(SignatureHelp {
            signatures: vec![SignatureInformation {
                label: label.to_string(),
                documentation: Some(Documentation::String(doc.to_string())),
                parameters: Some(params),
                active_parameter: None,
            }],
            active_signature: Some(0),
            active_parameter: None,
        })
    }

    fn handle_semantic_tokens_full(&mut self, id: RequestId, params: Value) -> Option<Response> {
        let params: SemanticTokensParams = serde_json::from_value(params).ok()?;
        let uri = &params.text_document.uri;

        let doc = self.documents.get(uri)?;

        // ADL files use their own semantic tokens
        if crate::adl::is_adl_uri(uri) {
            let tokens = crate::adl::get_adl_semantic_tokens(&doc.content);
            let result = SemanticTokensResult::Tokens(tokens);
            return Some(Response::new_ok(id, result));
        }

        let result = analyze(&doc.content);
        let symbols = extract_symbols_with_positions(&result.statements, &result.tokens);

        let tokens = get_semantic_tokens(doc, &symbols);
        Some(Response::new_ok(id, tokens))
    }

    fn handle_semantic_tokens_range(&mut self, id: RequestId, params: Value) -> Option<Response> {
        let params: SemanticTokensRangeParams = serde_json::from_value(params).ok()?;
        let uri = &params.text_document.uri;
        let range = params.range;

        let doc = self.documents.get(uri)?;

        // ADL files use their own semantic tokens
        if crate::adl::is_adl_uri(uri) {
            let tokens = crate::adl::get_adl_semantic_tokens(&doc.content);
            let result = SemanticTokensResult::Tokens(tokens);
            return Some(Response::new_ok(id, result));
        }

        let result = analyze(&doc.content);
        let symbols = extract_symbols_with_positions(&result.statements, &result.tokens);

        let tokens = get_semantic_tokens_range(doc, &symbols, range);
        Some(Response::new_ok(id, tokens))
    }

    fn handle_inlay_hint(&mut self, id: RequestId, params: Value) -> Option<Response> {
        let params: InlayHintParams = serde_json::from_value(params).ok()?;
        let uri = &params.text_document.uri;
        let range = params.range;

        let doc = self.documents.get(uri)?;

        // ADL files don't use Adesh inlay hints
        if crate::adl::is_adl_uri(uri) {
            return Some(Response::new_ok(id, Vec::<InlayHint>::new()));
        }

        let result = analyze(&doc.content);
        let symbols = extract_symbols_with_positions(&result.statements, &result.tokens);

        let hints = get_inlay_hints(doc, &symbols, range);
        Some(Response::new_ok(id, hints))
    }

    fn handle_declaration(&mut self, id: RequestId, params: Value) -> Option<Response> {
        // Declaration is the same as definition for Adesh
        self.handle_definition(id, params)
    }

    fn handle_type_definition(&mut self, id: RequestId, params: Value) -> Option<Response> {
        let params: GotoDefinitionParams = serde_json::from_value(params).ok()?;
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let doc = self.documents.get(uri)?;
        self.workspace.index_document(uri, &doc.content);
        let index = self.workspace.get_index(uri)?;

        // Get the identifier at the cursor position
        let internal_line = (pos.line + 1) as usize;
        let internal_col = (pos.character + 1) as usize;
        let identifier = index.identifier_at(internal_line, internal_col);

        if let Some(name) = identifier {
            // If it's a variable, find its type and go to the type definition
            if let Some(sym) = index.find_declaration(name) {
                if let Some(type_str) = &sym.type_annotation {
                    if let Some(td) = index.get_type(type_str) {
                        let start = Position {
                            line: (td.line.saturating_sub(1)) as u32,
                            character: (td.col.saturating_sub(1)) as u32,
                        };
                        let end = Position {
                            line: start.line,
                            character: start.character + td.name.len() as u32,
                        };
                        let location = Location {
                            uri: uri.clone(),
                            range: Range { start, end },
                        };
                        let response: Option<GotoDefinitionResponse> =
                            Some(GotoDefinitionResponse::Scalar(location));
                        return Some(Response::new_ok(id, response));
                    }
                }
            }
        }

        // Fall back to regular definition
        self.handle_definition(id, serde_json::to_value(params).unwrap_or_default())
    }

    fn handle_document_highlight(
        &mut self,
        id: RequestId,
        params: Value,
    ) -> Option<Response> {
        let params: DocumentHighlightParams = serde_json::from_value(params).ok()?;
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let doc = self.documents.get(uri)?;
        self.workspace.index_document(uri, &doc.content);
        let index = self.workspace.get_index(uri)?;

        let internal_line = (pos.line + 1) as usize;
        let internal_col = (pos.character + 1) as usize;
        let identifier = index.identifier_at(internal_line, internal_col);

        if let Some(name) = identifier {
            let refs = index.find_references_by_name(name);
            let highlights: Vec<DocumentHighlight> = refs
                .into_iter()
                .map(|(line, col, end_col)| {
                    let start = Position {
                        line: (line.saturating_sub(1)) as u32,
                        character: (col.saturating_sub(1)) as u32,
                    };
                    let end = Position {
                        line: start.line,
                        character: (end_col.saturating_sub(1)) as u32,
                    };
                    DocumentHighlight {
                        range: Range { start, end },
                        kind: Some(DocumentHighlightKind::READ),
                    }
                })
                .collect();
            return Some(Response::new_ok(id, highlights));
        }

        Some(Response::new_ok(id, Vec::<DocumentHighlight>::new()))
    }

    fn handle_folding_range(&mut self, id: RequestId, params: Value) -> Option<Response> {
        let params: FoldingRangeParams = serde_json::from_value(params).ok()?;
        let uri = &params.text_document.uri;

        let doc = self.documents.get(uri)?;

        // ADL files: use the ADL folding range provider
        if crate::adl::is_adl_uri(uri) {
            let ranges = crate::adl::get_adl_folding_ranges(&doc.content);
            return Some(Response::new_ok(id, ranges));
        }

        self.workspace.index_document(uri, &doc.content);
        let index = self.workspace.get_index(uri)?;

        let mut ranges = Vec::new();

        // Generate folding ranges from the AST structure
        for stmt in &index.statements {
            self.collect_folding_ranges(stmt, &mut ranges);
        }

        Some(Response::new_ok(id, ranges))
    }

    /// Compute folding ranges for ADL files using simple brace/bracket matching.
    #[allow(dead_code)]
    fn compute_adl_folding_ranges(&self, content: &str) -> Vec<FoldingRange> {
        let lines: Vec<&str> = content.lines().collect();
        let mut ranges = Vec::new();
        let mut stack: Vec<(u32, u32, char)> = Vec::new(); // (line, col, open_char)

        for (line_idx, line) in lines.iter().enumerate() {
            for (col_idx, ch) in line.chars().enumerate() {
                match ch {
                    '{' | '[' => stack.push((line_idx as u32, col_idx as u32, ch)),
                    '}' | ']' => {
                        if let Some((start_line, _, open_ch)) = stack.pop() {
                            let matching = match open_ch {
                                '{' => '}',
                                '[' => ']',
                                _ => continue,
                            };
                            if matching == ch && line_idx as u32 > start_line {
                                let kind = Some(FoldingRangeKind::Region);
                                ranges.push(FoldingRange {
                                    start_line,
                                    end_line: line_idx as u32,
                                    start_character: None,
                                    end_character: None,
                                    kind,
                                    collapsed_text: None,
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        ranges
    }

    fn collect_folding_ranges(&self, stmt: &adeshlang::parsing::ast::Stmt, ranges: &mut Vec<FoldingRange>) {
        use adeshlang::parsing::ast::StmtKind;

        match &stmt.kind {
            StmtKind::Function(func, _) => {
                if let Some(first) = func.body.first() {
                    let start_line = (first.span.line.saturating_sub(1)) as u32;
                    let end_line = (func.body.last().map(|s| s.span.line).unwrap_or(start_line as usize + 1).saturating_sub(1)) as u32;
                    if end_line > start_line {
                        ranges.push(FoldingRange {
                            start_line,
                            end_line,
                            start_character: None,
                            end_character: None,
                            kind: Some(FoldingRangeKind::Region),
                            collapsed_text: None,
                        });
                    }
                }
                for s in func.body.iter() {
                    self.collect_folding_ranges(s, ranges);
                }
            }
            StmtKind::Class(class, _) => {
                let start_line = (stmt.span.line.saturating_sub(1)) as u32;
                let end_line = start_line + 10; // approximate
                ranges.push(FoldingRange {
                    start_line,
                    end_line,
                    start_character: None,
                    end_character: None,
                    kind: Some(FoldingRangeKind::Region),
                    collapsed_text: Some(format!("class {}", class.name)),
                });
            }
            StmtKind::Block(stmts) => {
                if stmts.len() > 2 {
                    let start_line = (stmts.first().map(|s| s.span.line).unwrap_or(0).saturating_sub(1)) as u32;
                    let end_line = (stmts.last().map(|s| s.span.line).unwrap_or(0).saturating_sub(1)) as u32;
                    if end_line > start_line {
                        ranges.push(FoldingRange {
                            start_line,
                            end_line,
                            start_character: None,
                            end_character: None,
                            kind: Some(FoldingRangeKind::Region),
                            collapsed_text: None,
                        });
                    }
                }
                for s in stmts {
                    self.collect_folding_ranges(s, ranges);
                }
            }
            StmtKind::If { then_branch, else_branch, .. } => {
                self.collect_folding_ranges(then_branch, ranges);
                if let Some(e) = else_branch {
                    self.collect_folding_ranges(e, ranges);
                }
            }
            StmtKind::While { body, .. } => self.collect_folding_ranges(body, ranges),
            StmtKind::ForIn { body, .. } => self.collect_folding_ranges(body, ranges),
            StmtKind::TryCatch { try_block, catch_block, .. } => {
                self.collect_folding_ranges(try_block, ranges);
                self.collect_folding_ranges(catch_block, ranges);
            }
            _ => {}
        }
    }

    fn handle_document_link(&mut self, id: RequestId, params: Value) -> Option<Response> {
        let params: DocumentLinkParams = serde_json::from_value(params).ok()?;
        let uri = &params.text_document.uri;

        let doc = self.documents.get(uri)?;

        if crate::adl::is_adl_uri(uri) {
            let links = crate::adl::get_adl_document_links(&doc.content, uri);
            return Some(Response::new_ok(id, links));
        }

        // Non-ADL files: no document links
        Some(Response::new_ok(id, Vec::<DocumentLink>::new()))
    }

    fn handle_selection_range(&mut self, id: RequestId, params: Value) -> Option<Response> {
        let params: SelectionRangeParams = serde_json::from_value(params).ok()?;
        let uri = &params.text_document.uri;
        let positions = &params.positions;

        let doc = self.documents.get(uri)?;

        if crate::adl::is_adl_uri(uri) {
            let ranges = crate::adl::get_adl_selection_ranges(&doc.content, positions);
            return Some(Response::new_ok(id, ranges));
        }

        // Non-ADL files: no selection ranges
        Some(Response::new_ok(id, Vec::<Option<SelectionRange>>::new()))
    }
}

impl Default for AdeshLanguageServer {
    fn default() -> Self {
        Self::new()
    }
}
