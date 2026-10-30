use super::*;
use tempfile::tempdir;

#[test]
fn collects_modifiers_consecutive_comments_and_full_source() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("api.adesh");
    fs::write(
        &path,
        r#"/** First paragraph. */
/** Second paragraph.
 * @param {number} x Input.
 */
export async fn compute(x: number): number {
    if (x > 0) { return x + 1; }
    return 0;
}
/** Version. */
export const VERSION = "1.0";
/** Shape. */
type Point = { x: number; y: number; };
/** Orphan. */ export
"#,
    )
    .unwrap();
    let (items, _) = collect_docs_from_file(&path, "api.adesh").unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].name, "compute");
    assert_eq!(items[0].line, 5);
    assert!(items[0].exported);
    assert_eq!(
        items[0].signature,
        "export async fn compute(x: number): number"
    );
    assert!(items[0].source.ends_with("return 0;\n}"));
    assert!(items[0].doc.contains("First paragraph."));
    assert!(items[0].doc.contains("Second paragraph."));
    assert_eq!(items[1].kind, "constant");
    assert_eq!(items[1].signature, "export const VERSION = \"1.0\";");
    assert_eq!(items[2].source, "type Point = { x: number; y: number; };");
}

#[test]
fn resolves_adesh_and_legacy_imports_and_deduplicates_cycles() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("main.adesh");
    fs::write(&input, "import \"./helper\" as h;\nimport \"./helper.adesh\" as same;\nimport \"./old\" as old;\n/** Entry. */ fn Main() {}").unwrap();
    fs::write(
        dir.path().join("helper.adesh"),
        "import \"./main.adesh\" as main;\n/** Helper. */ fn help() {}",
    )
    .unwrap();
    fs::write(dir.path().join("old.ind"), "/** Legacy. */ fn legacy() {}").unwrap();
    let out = dir.path().join("site");
    generate_docs(&input, &out).unwrap();
    let index: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(out.join("search-index.json")).unwrap()).unwrap();
    assert_eq!(index.as_array().unwrap().len(), 3);
    let html = fs::read_to_string(out.join("index.html")).unwrap();
    assert!(html.contains("3</strong><span>source modules"));
    assert!(!html.contains(&dir.path().to_string_lossy().to_string()));
    for asset in ["logo.svg", "style.css", "app.js", "search-index.json"] {
        assert!(out.join(asset).is_file());
    }
    let first = html;
    generate_docs(&input, &out).unwrap();
    assert_eq!(first, fs::read_to_string(out.join("index.html")).unwrap());
}

#[test]
fn extracts_named_imports_and_all_supported_declarations() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("api.adesh");
    fs::write(
        &path,
        r#"import { helper } from "./helper.adesh";
/** A */ class A {}
/** B */ struct B {}
/** C */ enum C { One, Two }
/** D */ interface D { name: string; }
/** E */ type E = number;
/** F */ let f = 1;
/** G */ const g = 2;
/** H */ public static fn h() {}
"#,
    )
    .unwrap();
    let (items, imports) = collect_docs_from_file(&path, "api.adesh").unwrap();
    assert_eq!(imports, vec!["./helper.adesh"]);
    assert_eq!(
        items.iter().map(|i| i.kind.as_str()).collect::<Vec<_>>(),
        vec![
            "class",
            "struct",
            "enum",
            "interface",
            "type",
            "variable",
            "constant",
            "function"
        ]
    );
    assert!(items[7].exported);
}

#[test]
fn structured_docs_render_tables_examples_and_safe_markdown() {
    let item = ItemDoc {
        id: "sum-id".into(),
        kind: "function".into(),
        name: "sum".into(),
        doc: r#"Add **two** values with `sum`.

- Fast
- Predictable

| Value | Meaning |
| --- | --- |
| 0 | Zero |

@param {number} a - First value.
@param {number} b Second value.
@returns {number} The total.
@throws {Error} On overflow.
@deprecated Use {@link sum}.
@since 1.0
@example
sum(1, 2); // 3
@example
```adesh
sum(3, 4);
```
"#
        .into(),
        file: "api.adesh".into(),
        line: 10,
        signature: "fn sum(a: number, b: number): number".into(),
        source: "fn sum(a: number, b: number): number { return a + b; }".into(),
        exported: true,
    };
    let html = render::html("demo", &[item], &["api.adesh".into()]);
    for expected in [
        "<strong>two</strong>",
        "<ul>",
        "<th scope=\"col\">Meaning</th>",
        "Parameters &amp; properties",
        "<h3>Returns</h3>",
        "<h3>Throws</h3>",
        "Example 2",
        "tok-number",
        "tok-comment",
        "class=\"badge deprecated\"",
        "href=\"#sum-id\"",
        "logo.svg",
        "View source",
    ] {
        assert!(html.contains(expected), "Missing: {expected}");
    }
}

#[test]
fn escapes_untrusted_comments_code_titles_and_links() {
    let html = markup::markdown(
        "<script>alert(1)</script>\n\n[bad](javascript:alert)\n\n[good](https://example.com)\n\n```adesh\nprint(\"<img onerror=bad>\");\n```",
        &[],
    );
    assert!(!html.contains("<script>"));
    assert!(!html.contains("<img"));
    assert!(!html.contains("href=\"javascript:"));
    assert!(html.contains("href=\"https://example.com\""));
    assert!(html.contains("&lt;script&gt;"));
    assert!(html.contains("&lt;img"));
    assert!(!markup::inline("[bad](JaVaScRiPt:alert)", &[]).contains("<a "));
    assert!(!markup::inline("[bad](//example.com)", &[]).contains("<a "));
    assert!(render::html("<script>", &[], &[]).contains("&lt;script&gt;"));
}

#[test]
fn highlighting_preserves_whitespace_unicode_and_escaped_code() {
    let code = "fn café(x: number) {\n\treturn \"<tag>\" + 42; // comment\n}";
    let html = markup::highlight(code);
    assert!(html.contains("tok-keyword"));
    assert!(html.contains("tok-function"));
    assert!(html.contains("café"));
    assert!(html.contains("\n\t"));
    assert!(html.contains("&lt;tag&gt;"));
    assert!(html.contains("tok-comment"));
}

#[test]
fn empty_docs_have_guidance_and_missing_inputs_return_errors() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("empty.adesh");
    fs::write(&path, "fn Main() {}").unwrap();
    let out = dir.path().join("site");
    generate_docs(&path, &out).unwrap();
    let html = fs::read_to_string(out.join("index.html")).unwrap();
    assert!(html.contains("No documented declarations yet"));
    assert!(html.contains("lang=\"en\""));
    assert!(!html.contains("style=\"display:none"));
    assert!(generate_docs(&dir.path().join("missing.adesh"), &out).is_err());
}

#[test]
fn ids_are_unique_stable_and_do_not_depend_on_item_order() {
    assert_eq!(
        item_id("a.adesh", "function", "run", 3),
        item_id("a.adesh", "function", "run", 3)
    );
    assert_ne!(
        item_id("a.adesh", "function", "run", 3),
        item_id("b.adesh", "function", "run", 3)
    );
    assert_ne!(
        item_id("a.adesh", "function", "run", 3),
        item_id("a.adesh", "function", "run", 9)
    );
}

#[test]
fn tags_inside_fenced_examples_are_not_interpreted_as_doc_tags() {
    let comment = markup::comment(
        "Description.\n@example\n```adesh\n@decorator\nfn example() {}\n```\n@returns {number} A value.",
    );
    assert_eq!(comment.tags.len(), 2);
    assert!(comment.tags[0].1.contains("@decorator"));
}

#[test]
fn optional_type_semicolons_do_not_swallow_following_declarations() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("types.adesh");
    fs::write(
        &path,
        "/** Alias. */ type Point = { x: number; y: number; }\n/** Next. */ export fn next() {}",
    )
    .unwrap();
    let (items, _) = collect_docs_from_file(&path, "types.adesh").unwrap();
    assert_eq!(items[0].signature, "type Point = { x: number; y: number; }");
    assert!(!items[0].source.contains("Next"));
    assert_eq!(items[1].name, "next");
}
