use super::{ItemDoc, escape, markup};
use std::collections::BTreeSet;

fn tags(item: &ItemDoc, items: &[ItemDoc]) -> String {
    let comment = markup::comment(&item.doc);
    let mut out = markup::markdown(&comment.description, items);
    for (_, description) in comment.tags.iter().filter(|(tag, _)| tag == "description") {
        out.push_str(&markup::markdown(description, items));
    }
    let params: Vec<_> = comment
        .tags
        .iter()
        .filter(|(tag, _)| {
            matches!(
                tag.as_str(),
                "param" | "arg" | "argument" | "property" | "prop"
            )
        })
        .collect();
    if !params.is_empty() {
        out.push_str("<h3>Parameters &amp; properties</h3><div class=\"table-wrap\"><table><thead><tr><th scope=\"col\">Name</th><th scope=\"col\">Type</th><th scope=\"col\">Description</th></tr></thead><tbody>");
        for (_, value) in params {
            let (ty, rest) = markup::typed_value(value);
            let (name, description) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            out.push_str(&format!(
                "<tr><td><code>{}</code></td><td><code>{}</code></td><td>{}</td></tr>",
                escape(name),
                escape(if ty.is_empty() { "—" } else { ty }),
                markup::inline(description.trim().trim_start_matches('-').trim(), items)
            ));
        }
        out.push_str("</tbody></table></div>");
    }
    let mut example = 0;
    for (tag, value) in &comment.tags {
        match tag.as_str() {
            "param" | "arg" | "argument" | "property" | "prop" | "description" => {}
            "example" => {
                example += 1;
                out.push_str(&format!("<h3>Example {example}</h3>"));
                if value.trim_start().starts_with("```") {
                    out.push_str(&markup::markdown(value, items));
                } else {
                    out.push_str(&markup::code_block(value.trim(), "adesh"));
                }
            }
            "returns" | "return" | "throws" | "throw" | "type" => {
                let title = match tag.as_str() {
                    "returns" | "return" => "Returns",
                    "type" => "Type",
                    _ => "Throws",
                };
                let (ty, description) = markup::typed_value(value);
                out.push_str(&format!("<h3>{title}</h3><div class=\"return-value\">"));
                if !ty.is_empty() {
                    out.push_str(&format!(
                        "<code class=\"type-label\">{}</code>",
                        markup::inline(ty, items)
                    ));
                }
                out.push_str(&markup::markdown(description, items));
                out.push_str("</div>");
            }
            "deprecated" | "warning" | "note" | "since" | "see" => {
                let title = match tag.as_str() {
                    "deprecated" => "Deprecated",
                    "warning" => "Warning",
                    "note" => "Note",
                    "since" => "Since",
                    _ => "See also",
                };
                out.push_str(&format!(
                    "<aside class=\"callout {tag}\"><strong>{title}</strong>{}</aside>",
                    markup::markdown(value, items)
                ));
            }
            _ => {
                out.push_str(&format!(
                    "<h3>{}</h3>{}",
                    escape(tag),
                    markup::markdown(value, items)
                ));
            }
        }
    }
    out
}

pub(super) fn html(project: &str, items: &[ItemDoc], files: &[String]) -> String {
    let kinds: BTreeSet<_> = items.iter().map(|i| i.kind.as_str()).collect();
    let mut nav = String::new();
    for file in files {
        let members: Vec<_> = items.iter().filter(|i| &i.file == file).collect();
        if members.is_empty() {
            continue;
        }
        nav.push_str(&format!(
            "<div class=\"nav-group\"><h3 title=\"{}\">{}</h3>",
            escape(file),
            escape(file)
        ));
        for item in members {
            nav.push_str(&format!("<a class=\"symbol-link\" href=\"#{}\" data-id=\"{}\"><span class=\"kind-dot {}\" aria-hidden=\"true\"></span><span>{}</span><small>{}</small></a>",
                item.id, item.id, item.kind, escape(&item.name), item.kind));
        }
        nav.push_str("</div>");
    }
    let mut filters = String::from("<option value=\"\">All kinds</option>");
    for kind in &kinds {
        filters.push_str(&format!("<option value=\"{kind}\">{kind}</option>"));
    }
    let mut module_filters = String::from("<option value=\"\">All modules</option>");
    for file in files {
        module_filters.push_str(&format!(
            "<option value=\"{}\">{}</option>",
            escape(file),
            escape(file)
        ));
    }
    let mut cards = String::new();
    let mut toc = String::new();
    for item in items {
        let comment = markup::comment(&item.doc);
        let deprecated = comment.tags.iter().any(|(tag, _)| tag == "deprecated");
        let search = format!(
            "{} {} {} {} {}",
            item.name, item.kind, item.file, item.signature, item.doc
        )
        .to_lowercase();
        cards.push_str(&format!(
            "<article class=\"api-card\" id=\"{id}\" data-search=\"{search}\" data-kind=\"{kind}\" data-file=\"{file}\" aria-labelledby=\"{id}-title\">\
            <div class=\"item-meta\"><span class=\"badge {kind}\">{kind}</span>{export}{deprecated}<span class=\"location\">{file}:{line}</span></div>\
            <div class=\"item-title\"><h2 id=\"{id}-title\">{name}</h2><a class=\"permalink\" href=\"#{id}\" aria-label=\"Permalink to {name}\">#</a></div>\
            <div class=\"signature\">{signature}</div><div class=\"prose\">{description}</div>\
            <details class=\"source-view\"><summary>View source <span>{file}:{line}</span></summary>{source}</details></article>",
            id=item.id, search=escape(&search), kind=item.kind, file=escape(&item.file), line=item.line,
            name=escape(&item.name), export=if item.exported { "<span class=\"badge exported\">exported</span>" } else { "" },
            deprecated=if deprecated { "<span class=\"badge deprecated\">deprecated</span>" } else { "" },
            signature=markup::code_block(&item.signature, "adesh"), description=tags(item, items),
            source=markup::code_block(&item.source, "adesh")
        ));
        toc.push_str(&format!(
            "<a href=\"#{}\" data-id=\"{}\">{}</a>",
            item.id,
            item.id,
            escape(&item.name)
        ));
    }
    if items.is_empty() {
        cards.push_str("<div class=\"empty-state\"><h2>No documented declarations yet</h2><p>Add a <code>/** ... */</code> comment directly before a function, class, struct, enum, interface, type, constant, or variable, then run <code>adesh docs</code> again.</p>");
        cards.push_str(&markup::code_block("/**\n * Add two numbers.\n * @param {number} a First value.\n * @param {number} b Second value.\n * @returns {number} The sum.\n * @example\n * add(2, 3); // 5\n */\nexport fn add(a: number, b: number): number {\n    return a + b;\n}", "adesh"));
        cards.push_str("</div>");
    }
    let exported = items.iter().filter(|i| i.exported).count();
    format!(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="color-scheme" content="dark light">
<meta name="description" content="Searchable API reference for {project}, generated by AdeshLang.">
<title>{project} · AdeshLang API reference</title>
<link rel="icon" type="image/svg+xml" href="logo.svg">
<link rel="stylesheet" href="style.css">
<script src="app.js" defer></script>
</head>
<body>
<a class="skip-link" href="#main">Skip to content</a>
<header class="topbar">
  <a class="brand" href="#overview"><img src="logo.svg" width="42" height="42" alt="AdeshLang logo"><span>Adesh<span class="brand-muted">Lang</span><small>DOCUMENTATION</small></span></a>
  <div class="topbar-context"><span class="status-dot"></span> API reference <span class="divider">/</span> <strong>{project}</strong></div>
  <div class="header-actions"><button id="menu-toggle" type="button" aria-expanded="false" aria-controls="sidebar" hidden>Menu</button><button id="theme-toggle" type="button" aria-label="Switch color theme" hidden>Theme</button><a class="download-index" href="search-index.json" download>JSON index <span aria-hidden="true">↗</span></a></div>
</header>
<div class="layout">
<aside class="sidebar" id="sidebar" aria-label="API navigation">
  <a class="overview-link" href="#overview"><span aria-hidden="true">◈</span> Overview <span class="nav-count">{count}</span></a>
  <div class="sidebar-label">WORKSPACE MODULES</div>
  <nav aria-label="Symbols">{nav}</nav>
  <div class="sidebar-footer"><img src="logo.svg" alt="" width="24" height="24"><span>Built with AdeshLang<small>Offline-ready documentation</small></span></div>
</aside>
<main id="main">
  <section class="hero" id="overview" aria-labelledby="overview-title">
    <div class="eyebrow"><span></span> DEVELOPER REFERENCE</div>
    <h1 id="overview-title">{project}<span>API documentation.</span></h1>
    <p>Explore the building blocks. Read the signatures, follow the examples, and find exactly what you need.</p>
    <div class="stats"><div><strong>{count}</strong><span>documented symbols</span></div><div><strong>{modules}</strong><span>source modules</span></div><div><strong>{exported}</strong><span>public exports</span></div></div>
  </section>
  <section class="reference" aria-labelledby="reference-title">
    <div class="section-heading"><h2 id="reference-title">API reference</h2><span id="result-count" role="status" aria-live="polite">{count} symbols</span></div>
    <div class="search-tools" id="search-tools" hidden>
      <label class="search-box"><span class="sr-only">Search names, descriptions, and signatures</span><span aria-hidden="true">⌕</span><input id="search" type="search" placeholder="Search the documentation…" autocomplete="off" aria-keyshortcuts="/ Control+k Meta+k"><kbd>/</kbd></label>
      <div class="filters"><label><span class="sr-only">Filter by kind</span><select id="kind-filter">{filters}</select></label><label><span class="sr-only">Filter by module</span><select id="module-filter">{module_filters}</select></label><button type="button" id="clear-filters">Reset</button></div>
    </div>
    <noscript><p class="callout">Search and theme controls need JavaScript. All documentation and source code are available below without it.</p></noscript>
    <div id="no-results" class="empty-state" hidden><h2>No matching symbols</h2><p>Try another name, description, or signature, or reset the filters.</p></div>
    <div class="api-list">{cards}</div>
  </section>
  <footer class="page-footer"><span>Generated by <strong>AdeshLang</strong> · API reference</span><a href="#overview">Back to top ↑</a></footer>
</main>
<aside class="page-toc" aria-label="On this page"><div class="sidebar-label">ON THIS PAGE</div><a href="#overview">Overview</a>{toc}<div class="toc-note">Read. Build. Create.<small>Your source, beautifully documented.</small></div></aside>
</div>
<div id="announcement" class="sr-only" role="status" aria-live="polite"></div>
</body></html>"##,
        project = escape(project),
        count = items.len(),
        modules = files.len(),
        exported = exported,
        nav = nav,
        filters = filters,
        module_filters = module_filters,
        cards = cards,
        toc = toc
    )
}
