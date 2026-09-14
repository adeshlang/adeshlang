//! Doc Generator (HTML)
//!
//! Walks Adesh source files, collects doc-comments adjacent to declarations,
//! and renders a single-page HTML document with navigation and basic styling.
//! Also follows relative imports to traverse a small program graph.
use crate::parsing::ast::TokenKind;
use crate::parsing::lexer::Lexer;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone)]
struct ItemDoc {
    kind: String,
    name: String,
    doc: String,
    file: String,
}

fn parse_jsdoc(doc: &str) -> String {
    doc.to_string()
}

fn collect_docs_from_file(path: &Path) -> Result<(Vec<ItemDoc>, Vec<String>), String> {
    let src = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut lx = Lexer::new(&src);
    let toks = lx.tokenize().map_err(|e| e.to_string())?;
    let mut out: Vec<ItemDoc> = Vec::new();
    let mut imports: Vec<String> = Vec::new();
    let mut i = 0usize;
    let file = path.to_string_lossy().to_string();
    while i < toks.len() {
        let tk = &toks[i];
        match tk.kind {
            TokenKind::DocComment => {
                let doc = parse_jsdoc(&tk.lexeme);
                let mut j = i + 1;
                while j < toks.len()
                    && matches!(toks[j].kind, TokenKind::Semicolon | TokenKind::DocComment)
                {
                    j += 1;
                }
                if j < toks.len() {
                    let mut k = j;
                    if matches!(toks[k].kind, TokenKind::Export) {
                        k += 1;
                        if k < toks.len() && matches!(toks[k].kind, TokenKind::Default) {
                            k += 1;
                        }
                    }
                    match toks[k].kind {
                        TokenKind::Fn => {
                            if k + 1 < toks.len()
                                && matches!(toks[k + 1].kind, TokenKind::Identifier)
                            {
                                let name = toks[k + 1].lexeme.clone();
                                out.push(ItemDoc {
                                    kind: "function".into(),
                                    name,
                                    doc: doc.clone(),
                                    file: file.clone(),
                                });
                            }
                        }
                        TokenKind::Class => {
                            if k + 1 < toks.len()
                                && matches!(toks[k + 1].kind, TokenKind::Identifier)
                            {
                                let name = toks[k + 1].lexeme.clone();
                                out.push(ItemDoc {
                                    kind: "class".into(),
                                    name,
                                    doc: doc.clone(),
                                    file: file.clone(),
                                });
                            }
                        }
                        TokenKind::Struct => {
                            if k + 1 < toks.len()
                                && matches!(toks[k + 1].kind, TokenKind::Identifier)
                            {
                                let name = toks[k + 1].lexeme.clone();
                                out.push(ItemDoc {
                                    kind: "struct".into(),
                                    name,
                                    doc: doc.clone(),
                                    file: file.clone(),
                                });
                            }
                        }
                        TokenKind::Enum => {
                            if k + 1 < toks.len()
                                && matches!(toks[k + 1].kind, TokenKind::Identifier)
                            {
                                let name = toks[k + 1].lexeme.clone();
                                out.push(ItemDoc {
                                    kind: "enum".into(),
                                    name,
                                    doc: doc.clone(),
                                    file: file.clone(),
                                });
                            }
                        }
                        TokenKind::Let | TokenKind::Const => {
                            if k + 1 < toks.len()
                                && matches!(toks[k + 1].kind, TokenKind::Identifier)
                            {
                                let name = toks[k + 1].lexeme.clone();
                                out.push(ItemDoc {
                                    kind: "variable".into(),
                                    name,
                                    doc: doc.clone(),
                                    file: file.clone(),
                                });
                            }
                        }
                        TokenKind::Interface => {
                            if k + 1 < toks.len()
                                && matches!(toks[k + 1].kind, TokenKind::Identifier)
                            {
                                let name = toks[k + 1].lexeme.clone();
                                out.push(ItemDoc {
                                    kind: "interface".into(),
                                    name,
                                    doc: doc.clone(),
                                    file: file.clone(),
                                });
                            }
                        }
                        TokenKind::Type => {
                            if k + 1 < toks.len()
                                && matches!(toks[k + 1].kind, TokenKind::Identifier)
                            {
                                let name = toks[k + 1].lexeme.clone();
                                out.push(ItemDoc {
                                    kind: "type".into(),
                                    name,
                                    doc: doc.clone(),
                                    file: file.clone(),
                                });
                            }
                        }
                        _ => {}
                    }
                }
                i = j;
                continue;
            }
            TokenKind::Import => {
                let j = i + 1;
                if j < toks.len() && matches!(toks[j].kind, TokenKind::String) {
                    imports.push(toks[j].lexeme.clone());
                }
            }
            _ => {}
        }
        i += 1;
    }
    Ok((out, imports))
}

fn write_assets(out_dir: &Path) -> Result<(), String> {
    let css = r#"body{font-family:system-ui,Segoe UI,Arial;margin:0;background:#0b0c10;color:#c5c8c6}header{padding:16px;background:#1f2833;color:#66fcf1;font-weight:600}main{display:flex}nav{width:280px;border-right:1px solid #222;padding:12px}nav a{display:block;color:#c5c8c6;text-decoration:none;padding:6px;border-radius:4px}nav a:hover{background:#1f2833}section{flex:1;padding:16px}h2{color:#66fcf1;margin:0 0 8px}pre{background:#1f2833;padding:12px;border-radius:6px;overflow:auto}code{color:#c5c8c6} .kind{font-size:12px;color:#aaa;margin-left:8px}"#;
    let js = r#"document.querySelectorAll('nav a').forEach(a=>{a.addEventListener('click',e=>{e.preventDefault();const id=a.getAttribute('href').substring(1);document.querySelectorAll('section article').forEach(el=>el.style.display='none');const el=document.getElementById(id);if(el) el.style.display='block';});});"#;
    fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    fs::write(out_dir.join("style.css"), css).map_err(|e| e.to_string())?;
    fs::write(out_dir.join("app.js"), js).map_err(|e| e.to_string())?;
    Ok(())
}

fn escape(s: &str) -> String {
    let mut o = String::new();
    for ch in s.chars() {
        match ch {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            _ => o.push(ch),
        }
    }
    o
}

fn render_html(items: &[ItemDoc]) -> String {
    let mut nav = String::new();
    let mut body = String::new();
    for (idx, it) in items.iter().enumerate() {
        let id = format!("doc{}", idx);
        nav.push_str(&format!("<a href=\"#{id}\">{}</a>", escape(&it.name)));
        body.push_str(&format!("<article id=\"{id}\" style=\"display:{}\"><h2>{} <span class=\"kind\">{}</span></h2><pre><code>{}</code></pre><p><em>{}</em></p></article>", if idx==0 {"block"} else {"none"}, escape(&it.name), escape(&it.kind), escape(&it.doc), escape(&it.file)));
    }
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Docs</title><link rel=\"stylesheet\" href=\"style.css\"></head><body><header>AdeshLang Docs</header><main><nav>{}</nav><section>{}</section></main><script src=\"app.js\"></script></body></html>",
        nav, body
    )
}

pub fn generate_docs(entry: &Path, out_dir: &Path) -> Result<(), String> {
    let mut pending: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    pending.push(entry.to_path_buf());
    let mut items: Vec<ItemDoc> = Vec::new();
    while let Some(p) = pending.pop() {
        if !seen.insert(p.clone()) {
            continue;
        }
        let (docs, imps) = collect_docs_from_file(&p)?;
        items.extend(docs);
        let base = p.parent().unwrap_or(Path::new("."));
        for imp in imps {
            let path = base.join(imp);
            let pp = if path.extension().is_some() {
                path
            } else {
                path.with_extension("ind")
            };
            if pp.exists() {
                pending.push(pp);
            }
        }
    }
    write_assets(out_dir)?;
    let html = render_html(&items);
    fs::write(out_dir.join("index.html"), html).map_err(|e| e.to_string())?;
    Ok(())
}
