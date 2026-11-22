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

use crate::completion::get_completions_with_config;
use crate::config::AlsConfig;
use crate::diagnostics::compute_diagnostics_semantic_with_config;
use crate::document::DocumentStore;
use crate::formatting::format_document_with_config;
use crate::hover::get_hover_semantic;
use crate::inlay_hints::get_inlay_hints_semantic;
use crate::semantic_tokens::{get_semantic_tokens_range_semantic, get_semantic_tokens_semantic};
use crate::symbols::{
    do_rename_semantic, find_definition_semantic, find_references_semantic,
    get_document_symbols_semantic, prepare_rename_semantic,
};
use crate::workspace::WorkspaceIndex;

/// Adesh Language Server
pub struct AdeshLanguageServer {
    config: AlsConfig,
    documents: DocumentStore,
    workspace: WorkspaceIndex,
    /// Cached diagnostics per document
    diagnostics_cache: HashMap<Url, Vec<Diagnostic>>,
}

impl AdeshLanguageServer {
    pub fn new() -> Self {
        Self {
            config: AlsConfig::default(),
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

    /// Handle an LSP request. Per JSON-RPC 2.0, every request receives a response
    /// (never silently dropped as `None`).
    pub fn handle_request(&mut self, req: Request) -> Option<Response> {
        let id = req.id.clone();
        let method = req.method.clone();

        let response = match req.method.as_str() {
            "textDocument/completion" => self.handle_completion(id.clone(), req.params),
            "textDocument/hover" => self.handle_hover(id.clone(), req.params),
            "textDocument/definition" => self.handle_definition(id.clone(), req.params),
            "textDocument/references" => self.handle_references(id.clone(), req.params),
            "textDocument/documentSymbol" => self.handle_document_symbol(id.clone(), req.params),
            "workspace/symbol" => self.handle_workspace_symbol(id.clone(), req.params),
            "textDocument/formatting" => self.handle_formatting(id.clone(), req.params),
            "textDocument/prepareRename" => self.handle_prepare_rename(id.clone(), req.params),
            "textDocument/rename" => self.handle_rename(id.clone(), req.params),
            "textDocument/codeAction" => self.handle_code_action(id.clone(), req.params),
            "textDocument/signatureHelp" => self.handle_signature_help(id.clone(), req.params),
            "completionItem/resolve" => self.handle_completion_resolve(id.clone(), req.params),
            "textDocument/semanticTokens/full" => {
                self.handle_semantic_tokens_full(id.clone(), req.params)
            }
            "textDocument/semanticTokens/range" => {
                self.handle_semantic_tokens_range(id.clone(), req.params)
            }
            "textDocument/inlayHint" => self.handle_inlay_hint(id.clone(), req.params),
            "textDocument/declaration" => self.handle_declaration(id.clone(), req.params),
            "textDocument/typeDefinition" => self.handle_type_definition(id.clone(), req.params),
            "textDocument/documentHighlight" => {
                self.handle_document_highlight(id.clone(), req.params)
            }
            "textDocument/foldingRange" => self.handle_folding_range(id.clone(), req.params),
            "textDocument/documentLink" => self.handle_document_link(id.clone(), req.params),
            "textDocument/selectionRange" => self.handle_selection_range(id.clone(), req.params),
            "textDocument/implementation"
            | "textDocument/codeLens"
            | "textDocument/prepareCallHierarchy"
            | "callHierarchy/incomingCalls"
            | "callHierarchy/outgoingCalls"
            | "textDocument/prepareTypeHierarchy"
            | "typeHierarchy/supertypes"
            | "typeHierarchy/subtypes"
            | "textDocument/linkedEditingRange" => {
                self.handle_structural(id.clone(), &method, req.params)
            }
            _ => {
                warn!("Unhandled request: {}", req.method);
                Some(Response::new_err(
                    id.clone(),
                    lsp_server::ErrorCode::MethodNotFound as i32,
                    format!("Method not found: {}", req.method),
                ))
            }
        };

        Some(response.unwrap_or_else(|| {
            Response::new_err(
                id,
                lsp_server::ErrorCode::InvalidParams as i32,
                format!("Invalid parameters or unknown document for request `{method}`"),
            )
        }))
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
                let uri = notif
                    .params
                    .get("textDocument")
                    .and_then(|d| d.get("uri"))
                    .and_then(|u| serde_json::from_value::<Url>(u.clone()).ok());
                self.handle_did_close(notif.params);
                if let Some(uri) = uri {
                    outgoing.push(Self::build_publish_diagnostics(uri, Vec::new(), None));
                }
            }
            "textDocument/didSave" => {
                if let Some(diag_notif) = self.handle_did_save(notif.params) {
                    outgoing.push(diag_notif);
                }
            }
            "workspace/didChangeConfiguration" => {
                if let Ok(params) =
                    serde_json::from_value::<DidChangeConfigurationParams>(notif.params)
                {
                    self.config.update_from_value(&params.settings);
                    outgoing.extend(self.refresh_diagnostics());
                }
            }
            "workspace/didChangeWatchedFiles" => {
                if let Ok(params) =
                    serde_json::from_value::<DidChangeWatchedFilesParams>(notif.params)
                {
                    for change in params.changes {
                        if self.documents.get(&change.uri).is_some() {
                            continue;
                        }
                        if change.typ == FileChangeType::DELETED {
                            self.workspace.remove_document(&change.uri);
                        } else {
                            self.workspace.index_file_from_disk(&change.uri);
                        }
                    }
                    outgoing.extend(self.refresh_diagnostics());
                }
            }
            "workspace/didChangeWorkspaceFolders" => {
                if let Ok(params) =
                    serde_json::from_value::<DidChangeWorkspaceFoldersParams>(notif.params)
                {
                    for folder in params.event.removed {
                        if let Ok(path) = folder.uri.to_file_path() {
                            self.workspace.remove_workspace_root(&path);
                        }
                    }
                    for folder in params.event.added {
                        if let Ok(path) = folder.uri.to_file_path() {
                            self.workspace.add_workspace_root(path);
                        }
                    }
                    outgoing.extend(self.refresh_diagnostics());
                }
            }
            "initialized" => info!("Client initialized"),
            "$/cancelRequest" => {}
            _ => warn!("Unhandled notification: {}", notif.method),
        }
        outgoing
    }

    pub fn initialize(&mut self, params: &InitializeParams) {
        if let Some(options) = &params.initialization_options {
            self.config.update_from_value(options);
        }
        if let Some(folders) = &params.workspace_folders {
            for folder in folders {
                if let Ok(path) = folder.uri.to_file_path() {
                    self.workspace.add_workspace_root(path);
                }
            }
        } else {
            #[allow(deprecated)]
            if let Some(uri) = &params.root_uri {
                if let Ok(path) = uri.to_file_path() {
                    self.workspace.add_workspace_root(path);
                }
            }
        }
    }

    fn refresh_diagnostics(&mut self) -> Vec<Notification> {
        let mut outgoing = Vec::new();
        for doc in self.documents.iter() {
            let diagnostics = if crate::adl::is_adl_uri(&doc.uri) {
                crate::adl::compute_adl_diagnostics(&doc.content, &doc.uri)
            } else if let Some(index) = self.workspace.get_index(&doc.uri) {
                compute_diagnostics_semantic_with_config(index, &self.workspace, doc, &self.config)
            } else {
                continue;
            };
            self.diagnostics_cache
                .insert(doc.uri.clone(), diagnostics.clone());
            outgoing.push(Self::build_publish_diagnostics(
                doc.uri.clone(),
                diagnostics,
                Some(doc.version),
            ));
        }
        outgoing
    }

    fn handle_structural(
        &mut self,
        id: RequestId,
        method: &str,
        params: Value,
    ) -> Option<Response> {
        use crate::hierarchy::*;
        match method {
            "callHierarchy/incomingCalls" | "callHierarchy/outgoingCalls" => {
                let item: CallHierarchyItem =
                    serde_json::from_value(params.get("item")?.clone()).ok()?;
                self.workspace.index_file_from_disk(&item.uri);
                return Some(if method.ends_with("incomingCalls") {
                    Response::new_ok(id, call_hierarchy_incoming(&self.workspace, &item))
                } else {
                    Response::new_ok(id, call_hierarchy_outgoing(&self.workspace, &item))
                });
            }
            "typeHierarchy/supertypes" | "typeHierarchy/subtypes" => {
                let item: TypeHierarchyItem =
                    serde_json::from_value(params.get("item")?.clone()).ok()?;
                self.workspace.index_file_from_disk(&item.uri);
                return Some(if method.ends_with("supertypes") {
                    Response::new_ok(id, type_hierarchy_supertypes(&self.workspace, &item))
                } else {
                    Response::new_ok(id, type_hierarchy_subtypes(&self.workspace, &item))
                });
            }
            _ => {}
        }
        let uri: Url =
            serde_json::from_value(params.get("textDocument")?.get("uri")?.clone()).ok()?;
        let doc = self.documents.get(&uri)?;
        self.workspace.index_document(&uri, &doc.content);
        let index = self.workspace.get_index(&uri)?;
        if method == "textDocument/codeLens" {
            return Some(Response::new_ok(
                id,
                crate::code_lens::get_code_lenses(index, &self.workspace, doc, &self.config),
            ));
        }
        let pos: Position = serde_json::from_value(params.get("position")?.clone()).ok()?;
        Some(match method {
            "textDocument/implementation" => Response::new_ok(
                id,
                crate::symbols::find_implementations_semantic(
                    index,
                    &self.workspace,
                    doc,
                    pos.line,
                    pos.character,
                ),
            ),
            "textDocument/prepareCallHierarchy" => Response::new_ok(
                id,
                prepare_call_hierarchy(index, &self.workspace, doc, pos.line, pos.character),
            ),
            "textDocument/prepareTypeHierarchy" => Response::new_ok(
                id,
                prepare_type_hierarchy(index, &self.workspace, doc, pos.line, pos.character),
            ),
            "textDocument/linkedEditingRange" => Response::new_ok(
                id,
                get_linked_editing_ranges(index, pos.line, pos.character),
            ),
            _ => return None,
        })
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

        self.workspace.index_document(uri, &doc.content);
        let index = self.workspace.get_index(uri)?;
        let items = get_completions_with_config(
            index,
            &self.workspace,
            doc,
            pos.line,
            pos.character,
            trigger.as_deref(),
            &self.config,
        );

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

        let location =
            find_definition_semantic(index, &self.workspace, doc, pos.line, pos.character);

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

        let locations = find_references_semantic(
            index,
            &self.workspace,
            doc,
            pos.line,
            pos.character,
            include_declaration,
        );

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
                    adeshlang::semantics::SemanticSymbolKind::TypeAlias => {
                        SymbolKind::TYPE_PARAMETER
                    }
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

        let edits = format_document_with_config(doc, &self.config, Some(&params.options));

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

        let workspace_edit = do_rename_semantic(
            index,
            &self.workspace,
            doc,
            pos.line,
            pos.character,
            new_name,
        );

        Some(match workspace_edit {
            Ok(edit) => Response::new_ok(id, edit),
            Err(message) => {
                Response::new_err(id, lsp_server::ErrorCode::InvalidParams as i32, message)
            }
        })
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

        self.workspace.index_document(uri, &doc.content);
        let index = self.workspace.get_index(uri)?;
        let actions = crate::code_actions::get_code_actions(
            index,
            &self.workspace,
            doc,
            params.range,
            diagnostics,
            params.context.only.as_deref(),
        );

        Some(Response::new_ok(id, actions))
    }

    fn handle_signature_help(&mut self, id: RequestId, params: Value) -> Option<Response> {
        let params: SignatureHelpParams = serde_json::from_value(params).ok()?;
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let doc = self.documents.get(uri)?;

        self.workspace.index_document(uri, &doc.content);
        let index = self.workspace.get_index(uri)?;
        let signature_help = crate::signature_help::get_signature_help(
            index,
            &self.workspace,
            doc,
            pos.line,
            pos.character,
        );

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
                self.diagnostics_cache
                    .insert(uri.clone(), diagnostics.clone());
                return Some(Self::build_publish_diagnostics(
                    uri,
                    diagnostics,
                    Some(version),
                ));
            }

            self.workspace.open_document(&uri, &content);

            if let Some(doc) = self.documents.get(&uri) {
                self.workspace.index_document(&uri, &doc.content);
                let index = self.workspace.get_index(&uri)?;
                let diagnostics = compute_diagnostics_semantic_with_config(
                    index,
                    &self.workspace,
                    doc,
                    &self.config,
                );
                self.diagnostics_cache
                    .insert(uri.clone(), diagnostics.clone());
                return Some(Self::build_publish_diagnostics(
                    uri,
                    diagnostics,
                    Some(version),
                ));
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
                    self.diagnostics_cache
                        .insert(uri.clone(), diagnostics.clone());
                    return Some(Self::build_publish_diagnostics(
                        uri,
                        diagnostics,
                        Some(version),
                    ));
                }

                self.workspace.index_document(&uri, &change.text);

                if let Some(doc) = self.documents.get(&uri) {
                    let index = self.workspace.get_index(&uri)?;
                    let diagnostics = compute_diagnostics_semantic_with_config(
                        index,
                        &self.workspace,
                        doc,
                        &self.config,
                    );
                    self.diagnostics_cache
                        .insert(uri.clone(), diagnostics.clone());
                    return Some(Self::build_publish_diagnostics(
                        uri,
                        diagnostics,
                        Some(version),
                    ));
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
            self.workspace.close_document(&uri);
            self.diagnostics_cache.remove(&uri);
        }
    }

    fn handle_did_save(&mut self, params: Value) -> Option<Notification> {
        if let Ok(params) = serde_json::from_value::<DidSaveTextDocumentParams>(params) {
            let uri = params.text_document.uri;
            info!("Document saved: {}", uri);
            if !self.config.diagnostics.refresh_on_save {
                return None;
            }
            if let Some(doc) = self.documents.get(&uri) {
                if crate::adl::is_adl_uri(&uri) {
                    let diagnostics = crate::adl::compute_adl_diagnostics(&doc.content, &uri);
                    self.diagnostics_cache
                        .insert(uri.clone(), diagnostics.clone());
                    return Some(Self::build_publish_diagnostics(
                        uri,
                        diagnostics,
                        Some(doc.version),
                    ));
                }
                self.workspace.index_document(&uri, &doc.content);
                let index = self.workspace.get_index(&uri)?;
                let diagnostics = compute_diagnostics_semantic_with_config(
                    index,
                    &self.workspace,
                    doc,
                    &self.config,
                );
                self.diagnostics_cache
                    .insert(uri.clone(), diagnostics.clone());
                return Some(Self::build_publish_diagnostics(
                    uri,
                    diagnostics,
                    Some(doc.version),
                ));
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

    fn handle_semantic_tokens_full(&mut self, id: RequestId, params: Value) -> Option<Response> {
        let params: SemanticTokensParams = serde_json::from_value(params).ok()?;
        let uri = &params.text_document.uri;

        let doc = self.documents.get(uri)?;

        // ADL files use their own semantic tokens
        if crate::adl::is_adl_uri(uri) {
            let tokens = crate::semantic_tokens::get_adl_tokens(&doc.content, None);
            let result = SemanticTokensResult::Tokens(tokens);
            return Some(Response::new_ok(id, result));
        }

        self.workspace.index_document(uri, &doc.content);
        let index = self.workspace.get_index(uri)?;

        let tokens = get_semantic_tokens_semantic(doc, index);
        Some(Response::new_ok(id, tokens))
    }

    fn handle_semantic_tokens_range(&mut self, id: RequestId, params: Value) -> Option<Response> {
        let params: SemanticTokensRangeParams = serde_json::from_value(params).ok()?;
        let uri = &params.text_document.uri;
        let range = params.range;

        let doc = self.documents.get(uri)?;

        // ADL files use their own semantic tokens
        if crate::adl::is_adl_uri(uri) {
            let tokens = crate::semantic_tokens::get_adl_tokens(&doc.content, Some(range));
            let result = SemanticTokensResult::Tokens(tokens);
            return Some(Response::new_ok(id, result));
        }

        self.workspace.index_document(uri, &doc.content);
        let index = self.workspace.get_index(uri)?;

        let tokens = get_semantic_tokens_range_semantic(doc, index, range);
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

        self.workspace.index_document(uri, &doc.content);
        let index = self.workspace.get_index(uri)?;

        let hints = get_inlay_hints_semantic(index, &self.workspace, doc, range, &self.config);
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

        let location = crate::symbols::find_type_definition_semantic(
            index,
            &self.workspace,
            doc,
            pos.line,
            pos.character,
        );
        Some(Response::new_ok(
            id,
            location.map(GotoDefinitionResponse::Scalar),
        ))
    }

    fn handle_document_highlight(&mut self, id: RequestId, params: Value) -> Option<Response> {
        let params: DocumentHighlightParams = serde_json::from_value(params).ok()?;
        let uri = &params.text_document_position_params.text_document.uri;
        let pos = params.text_document_position_params.position;

        let doc = self.documents.get(uri)?;
        self.workspace.index_document(uri, &doc.content);
        let index = self.workspace.get_index(uri)?;

        Some(Response::new_ok(
            id,
            crate::symbols::find_document_highlights_semantic(index, pos.line, pos.character),
        ))
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

        Some(Response::new_ok(
            id,
            crate::hierarchy::get_folding_ranges(&doc.content),
        ))
    }

    fn handle_document_link(&mut self, id: RequestId, params: Value) -> Option<Response> {
        let params: DocumentLinkParams = serde_json::from_value(params).ok()?;
        let uri = &params.text_document.uri;

        let doc = self.documents.get(uri)?;

        if crate::adl::is_adl_uri(uri) {
            let links = crate::adl::get_adl_document_links(&doc.content, uri);
            return Some(Response::new_ok(id, links));
        }

        self.workspace.index_document(uri, &doc.content);
        let index = self.workspace.get_index(uri)?;
        Some(Response::new_ok(
            id,
            crate::hierarchy::get_adesh_document_links(index, &self.workspace, doc),
        ))
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

        Some(Response::new_ok(
            id,
            crate::hierarchy::get_selection_ranges(doc, positions),
        ))
    }
}

impl Default for AdeshLanguageServer {
    fn default() -> Self {
        Self::new()
    }
}
