//! ALS Main Entry Point
//! 
//! Starts the Adesh Language Server and listens for LSP messages over stdio.

use std::error::Error;
use log::info;
use lsp_server::{Connection, Message};
use lsp_types::{
    InitializeParams, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    CompletionOptions, HoverProviderCapability, OneOf, SignatureHelpOptions,
    WorkDoneProgressOptions, DocumentFormattingOptions, DocumentSymbolOptions,
    CodeActionProviderCapability, CodeActionOptions, RenameOptions,
    SemanticTokensServerCapabilities, SemanticTokensLegend, SemanticTokensOptions,
    SemanticTokensFullOptions, DocumentLinkOptions,
    SelectionRangeProviderCapability, FoldingRangeProviderCapability,
};

// Import from the lib crate
use als::{AdeshLanguageServer, semantic_tokens};

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    // Initialize logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp(None)
        .init();

    info!("Starting Adesh Language Server (ALS) v0.1.0");

    // Create LSP connection over stdio
    let (connection, io_threads) = Connection::stdio();

    // Initialize server capabilities
    let server_capabilities = serde_json::to_value(ServerCapabilities {
        // Text document sync - full sync for simplicity, can be incremental later
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        
        // Completion support — triggers on member access, type annotations,
        // function calls, and decorators for TypeScript-like IntelliSense.
        // VSCode's editor.quickSuggestions setting triggers completions on every keystroke.
        completion_provider: Some(CompletionOptions {
            resolve_provider: Some(true),
            trigger_characters: Some(vec![
                ".".to_string(),   // member access: user.
                ":".to_string(),   // type annotation: let x: int, fn foo(n: i32): string
                "(".to_string(),  // function call: foo(
                "@".to_string(),  // decorator: @decorator
            ]),
            work_done_progress_options: WorkDoneProgressOptions::default(),
            all_commit_characters: Some(vec![
                ".".to_string(),
                ",".to_string(),
                ";".to_string(),
                "(".to_string(),
                ")".to_string(),
                "[".to_string(),
                "]".to_string(),
                "{".to_string(),
                "}".to_string(),
                ":".to_string(),
            ]),
            completion_item: None,
        }),
        
        // Hover support
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        
        // Signature help
        signature_help_provider: Some(SignatureHelpOptions {
            trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
            retrigger_characters: None,
            work_done_progress_options: WorkDoneProgressOptions::default(),
        }),
        
        // Go-to-definition
        definition_provider: Some(OneOf::Left(true)),
        
        // Find references
        references_provider: Some(OneOf::Left(true)),
        
        // Document symbols (outline)
        document_symbol_provider: Some(OneOf::Right(DocumentSymbolOptions {
            label: Some("Adesh".to_string()),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        })),
        
        // Workspace symbols
        workspace_symbol_provider: Some(OneOf::Left(true)),
        
        // Code formatting
        document_formatting_provider: Some(OneOf::Right(DocumentFormattingOptions {
            work_done_progress_options: WorkDoneProgressOptions::default(),
        })),
        
        // Code actions (quick fixes)
        code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
            code_action_kinds: Some(vec![
                lsp_types::CodeActionKind::QUICKFIX,
                lsp_types::CodeActionKind::REFACTOR,
            ]),
            work_done_progress_options: WorkDoneProgressOptions::default(),
            resolve_provider: Some(false),
        })),
        
        // Rename support
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        })),
        
        // Semantic tokens support
        semantic_tokens_provider: Some(
            SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                legend: SemanticTokensLegend {
                    token_types: semantic_tokens::SEMANTIC_TOKEN_TYPES.to_vec(),
                    token_modifiers: semantic_tokens::SEMANTIC_TOKEN_MODIFIERS.to_vec(),
                },
                range: Some(true),
                full: Some(SemanticTokensFullOptions::Bool(true)),
                work_done_progress_options: WorkDoneProgressOptions::default(),
            })
        ),
        
        // Inlay hints support
        inlay_hint_provider: Some(OneOf::Left(true)),

        // Document link support (clickable paths/URLs in ADL files)
        document_link_provider: Some(DocumentLinkOptions {
            work_done_progress_options: WorkDoneProgressOptions::default(),
            resolve_provider: Some(false),
        }),

        // Selection range support (smart selection expansion)
        selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),

        // Folding range support
        folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),

        ..Default::default()
    })?;

    // Handle initialize handshake
    let (initialize_id, _initialize_params) = match connection.initialize_start() {
        Ok(v) => v,
        Err(e) => {
            eprintln!(
                "ALS: No LSP client connected on stdio ({}). Launch from an editor or pipe JSON-RPC messages.",
                e
            );
            return Ok(());
        }
    };
    let initialize_params: InitializeParams = serde_json::from_value(_initialize_params)?;
    
    info!("Client: {:?}", initialize_params.client_info);
    
    let initialize_result = lsp_types::InitializeResult {
        capabilities: serde_json::from_value(server_capabilities)?,
        server_info: Some(lsp_types::ServerInfo {
            name: "Adesh Language Server".to_string(),
            version: Some("0.1.0".to_string()),
        }),
    };
    
    connection.initialize_finish(initialize_id, serde_json::to_value(initialize_result)?)?;
    info!("ALS initialized successfully");

    // Create and run the language server
    let mut server = AdeshLanguageServer::new();
    
    // Main message loop
    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    info!("Shutdown request received");
                    break;
                }
                
                // Handle request
                if let Some(response) = server.handle_request(req) {
                    connection.sender.send(Message::Response(response))?;
                }
            }
            Message::Notification(notif) => {
                let outgoing_notifs = server.handle_notification(notif);
                for outgoing in outgoing_notifs {
                    connection.sender.send(Message::Notification(outgoing))?;
                }
            }
            Message::Response(_) => {
                // We don't send requests, so no responses expected
            }
        }
    }

    io_threads.join()?;
    info!("ALS shutdown complete");
    Ok(())
}
