//! Inlay hints provider for type and ownership information
//!
//! Shows inline hints for types, ownership states, and borrow information.

use lsp_types::*;
use crate::analysis::{SymbolInfo, SymbolKind};
use crate::document::Document;

/// Generate inlay hints for a document range
pub fn get_inlay_hints(
    _doc: &Document,
    symbols: &[SymbolInfo],
    range: Range,
) -> Vec<InlayHint> {
    let mut hints = Vec::new();
    
    // Filter symbols in range
    let filtered_symbols: Vec<_> = symbols
        .iter()
        .filter(|s| {
            let line_0 = s.line.saturating_sub(1) as u32;
            let col_0 = s.col.saturating_sub(1) as u32;
            let pos = Position::new(line_0, col_0);
            pos >= range.start && pos <= range.end
        })
        .collect();
    
    for symbol in filtered_symbols {
        // Add type hints for variables and parameters
        if matches!(symbol.kind, SymbolKind::Variable | SymbolKind::Parameter) {
            if let Some(hint) = create_type_hint(symbol) {
                hints.push(hint);
            }
        }
        
        // Add ownership hints
        if let Some(hint) = create_ownership_hint(symbol) {
            hints.push(hint);
        }
    }
    
    hints
}

/// Create a type hint for a symbol
fn create_type_hint(symbol: &SymbolInfo) -> Option<InlayHint> {
    // Infer type from symbol information
    let type_str = infer_symbol_type(symbol)?;
    
    let line_0 = symbol.line.saturating_sub(1) as u32;
    let col_0 = symbol.col.saturating_sub(1) as u32;
    let position = Position::new(
        line_0,
        col_0 + symbol.name.len() as u32,
    );
    
    Some(InlayHint {
        position,
        label: InlayHintLabel::String(format!(": {}", type_str)),
        kind: Some(InlayHintKind::TYPE),
        text_edits: None,
        tooltip: Some(InlayHintTooltip::String(
            format!("Inferred type: {}", type_str)
        )),
        padding_left: Some(true),
        padding_right: Some(false),
        data: None,
    })
}

/// Infer the type of a symbol from context
fn infer_symbol_type(symbol: &SymbolInfo) -> Option<String> {
    // Check if we have type annotation in the symbol info
    if let Some(ref type_info) = symbol.type_name {
        if !type_info.is_empty() && type_info != "unknown" {
            return Some(type_info.clone());
        }
    }

    // Try extracting from signature if formatted like "let x: type = ..." or "x: type"
    if let Some(signature) = symbol.signature.as_deref() {
        if let Some(colon_idx) = signature.find(':') {
            let after_colon = &signature[colon_idx + 1..];
            let type_part = after_colon.split('=').next().unwrap_or("").trim();
            if !type_part.is_empty() {
                return Some(type_part.to_string());
            }
        }
    }
    
    None
}

/// Create an ownership hint for a symbol
fn create_ownership_hint(symbol: &SymbolInfo) -> Option<InlayHint> {
    // Check if this is a variable that might benefit from ownership hints
    if !matches!(symbol.kind, SymbolKind::Variable | SymbolKind::Parameter) {
        return None;
    }
    
    let ownership_state = infer_ownership_state(symbol);
    if let Some(state) = ownership_state {
        let line_0 = symbol.line.saturating_sub(1) as u32;
        let col_0 = symbol.col.saturating_sub(1) as u32;
        let position = Position::new(
            line_0,
            col_0 + symbol.name.len() as u32,
        );
        
        Some(InlayHint {
            position,
            label: InlayHintLabel::String(state),
            kind: Some(InlayHintKind::TYPE),
            text_edits: None,
            tooltip: Some(InlayHintTooltip::String(
                "Variable ownership state".to_string()
            )),
            padding_left: Some(true),
            padding_right: Some(false),
            data: None,
        })
    } else {
        None
    }
}

/// Infer ownership state from symbol information
/// This is a placeholder - real implementation would use borrow checker
fn infer_ownership_state(_symbol: &SymbolInfo) -> Option<String> {
    // In a real implementation, we'd:
    // 1. Check if variable is borrowed (& or &mut)
    // 2. Check if variable was moved
    // 3. Check if variable is in a region
    // 4. Check if variable uses explicit ARC (share/weak)
    
    // For now, return None (no hint)
    // This could be enhanced by analyzing the actual code
    None
}

/// Create parameter hints for function calls
pub fn get_parameter_hints(
    _doc: &Document,
    _line: u32,
    _character: u32,
) -> Vec<InlayHint> {
    // In a full implementation:
    // 1. Find the function call at the position
    // 2. Get the function signature
    // 3. Create hints for each parameter
    
    // This is a placeholder for future enhancement
    Vec::new()
}

/// Create return type hints for functions without explicit return types
pub fn get_return_type_hints(
    _doc: &Document,
    symbols: &[SymbolInfo],
) -> Vec<InlayHint> {
    let hints = Vec::new();
    
    for _symbol in symbols {
        // In a full implementation:
        // 1. Check if function has explicit return type
        // 2. If not, infer it and show as hint
        // 3. Position hint after function signature
    }
    
    hints
}
