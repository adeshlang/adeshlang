use als::AdeshLanguageServer;
use lsp_server::{Notification, Request, RequestId};
use serde_json::{json, Value};

const URI: &str = "file:///C:/protocol.adesh";

fn server(source: &str) -> AdeshLanguageServer {
    let mut server = AdeshLanguageServer::new();
    notify(
        &mut server,
        "workspace/didChangeConfiguration",
        json!({"settings":{"diagnostics":{"enabled":false}}}),
    );
    open(&mut server, URI, source);
    server
}

fn notify(server: &mut AdeshLanguageServer, method: &str, params: Value) -> Vec<Notification> {
    server.handle_notification(Notification {
        method: method.into(),
        params,
    })
}

fn open(server: &mut AdeshLanguageServer, uri: &str, source: &str) {
    notify(
        server,
        "textDocument/didOpen",
        json!({"textDocument":{"uri":uri,"languageId":"adesh","version":1,"text":source}}),
    );
}

fn request(server: &mut AdeshLanguageServer, method: &str, params: Value) -> Value {
    let response = server
        .handle_request(Request {
            id: RequestId::from(1),
            method: method.into(),
            params,
        })
        .expect("response");
    assert!(response.error.is_none(), "{method}: {:?}", response.error);
    response.result.expect("result")
}

fn position(line: u32, character: u32) -> Value {
    json!({"textDocument":{"uri":URI},"position":{"line":line,"character":character}})
}

fn labels(items: &Value) -> Vec<&str> {
    items
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i["label"].as_str())
        .collect()
}

#[test]
fn chained_completion_and_member_hover() {
    let source = "class Address {\n    city: string;\n}\nclass User {\n    address: Address;\n}\nlet u: User = User();\nu.address.ci\n";
    let mut s = server(source);
    let result = request(&mut s, "textDocument/completion", position(7, 12));
    assert!(labels(&result).contains(&"city"), "{result}");
    let result = request(&mut s, "textDocument/hover", position(7, 4));
    assert!(
        result["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("Address"),
        "{result}"
    );
    assert!(result["range"].is_object());
}

#[test]
fn namespace_and_postfix_completion_replaces_receiver() {
    let mut s = server("let ready = true;\nready.if\nJSON.pa\n");
    let result = request(&mut s, "textDocument/completion", position(1, 8));
    let item = result
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["label"] == "if")
        .unwrap();
    assert_eq!(item["textEdit"]["range"]["start"]["character"], 0);
    assert_eq!(item["textEdit"]["range"]["end"]["character"], 8);
    assert!(item["textEdit"]["newText"]
        .as_str()
        .unwrap()
        .starts_with("if (ready)"));
    let result = request(&mut s, "textDocument/completion", position(2, 7));
    assert!(labels(&result).contains(&"parse"));
}

#[test]
fn rename_returns_workspace_edit_and_rejects_invalid_names() {
    let mut s = server("let value = 1;\nprint(value);\n");
    let mut params = position(0, 5);
    params["newName"] = json!("count");
    let edit = request(&mut s, "textDocument/rename", params.clone());
    assert_eq!(edit["changes"][URI].as_array().unwrap().len(), 2);
    assert_eq!(edit["changes"][URI][0]["newText"], "count");
    params["newName"] = json!("not valid");
    let response = s
        .handle_request(Request {
            id: 2.into(),
            method: "textDocument/rename".into(),
            params,
        })
        .unwrap();
    assert_eq!(response.error.unwrap().code, -32602);
}

#[test]
fn configuration_updates_refresh_diagnostics_and_gate_features() {
    let mut s = server("let unused = 42;\n");
    let notifications = notify(
        &mut s,
        "workspace/didChangeConfiguration",
        json!({"settings":{"adesh":{
            "diagnostics":{"enabled":true},"completion":{"enabled":false},"inlayHints":{"enabled":false}
        }}}),
    );
    assert!(!notifications[0].params["diagnostics"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        request(&mut s, "textDocument/completion", position(0, 0)),
        json!([])
    );
    assert_eq!(
        request(
            &mut s,
            "textDocument/inlayHint",
            json!({"textDocument":{"uri":URI},"range":{"start":{"line":0,"character":0},"end":{"line":1,"character":0}}})
        ),
        json!([])
    );
    let notifications = notify(
        &mut s,
        "workspace/didChangeConfiguration",
        json!({"settings":{"diagnostics":{"enabled":false}}}),
    );
    assert_eq!(notifications[0].params["diagnostics"], json!([]));
    let closed = notify(
        &mut s,
        "textDocument/didClose",
        json!({"textDocument":{"uri":URI}}),
    );
    assert_eq!(closed[0].params["diagnostics"], json!([]));
}

#[test]
fn hierarchy_and_implementation_requests_are_routed() {
    let mut s = server("fn leaf(): i32 { return 1; }\nfn main() {\n    leaf();\n}\ninterface Shape {\n    fn draw();\n}\nclass Box implements Shape {\n    fn draw() {}\n}\n");
    let calls = request(&mut s, "textDocument/prepareCallHierarchy", position(0, 4));
    assert_eq!(calls[0]["name"], "leaf");
    let incoming = request(
        &mut s,
        "callHierarchy/incomingCalls",
        json!({"item":calls[0]}),
    );
    assert!(
        incoming
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["from"]["name"] == "main"),
        "{incoming}"
    );
    let main = request(&mut s, "textDocument/prepareCallHierarchy", position(1, 4));
    let outgoing = request(
        &mut s,
        "callHierarchy/outgoingCalls",
        json!({"item":main[0]}),
    );
    assert!(
        outgoing
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["to"]["name"] == "leaf"),
        "{outgoing}"
    );
    let types = request(&mut s, "textDocument/prepareTypeHierarchy", position(4, 11));
    let subtypes = request(&mut s, "typeHierarchy/subtypes", json!({"item":types[0]}));
    assert_eq!(subtypes[0]["name"], "Box");
    let parents = request(
        &mut s,
        "typeHierarchy/supertypes",
        json!({"item":subtypes[0]}),
    );
    assert_eq!(parents[0]["name"], "Shape");
    let implementations = request(&mut s, "textDocument/implementation", position(4, 11));
    assert_eq!(implementations.as_array().unwrap().len(), 1);
    let lenses = request(
        &mut s,
        "textDocument/codeLens",
        json!({"textDocument":{"uri":URI}}),
    );
    assert!(lenses
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["command"]["command"] == "adesh.runFile"));
}

#[test]
fn formatting_and_selection_ranges_stay_in_document() {
    let source = "fn main(){\nlet value=1;\nprint(value);\n}";
    let mut s = server(source);
    let edits = request(
        &mut s,
        "textDocument/formatting",
        json!({"textDocument":{"uri":URI},"options":{"tabSize":2,"insertSpaces":true}}),
    );
    assert_eq!(edits[0]["range"]["end"], json!({"line":3,"character":1}));
    assert!(edits[0]["newText"]
        .as_str()
        .unwrap()
        .contains("  let value = 1;"));
    let ranges = request(
        &mut s,
        "textDocument/selectionRange",
        json!({"textDocument":{"uri":URI},"positions":[{"line":2,"character":7}]}),
    );
    assert_eq!(ranges.as_array().unwrap().len(), 1);
    assert!(ranges[0]["parent"].is_object());
    let folding = request(
        &mut s,
        "textDocument/foldingRange",
        json!({"textDocument":{"uri":URI}}),
    );
    assert!(folding
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["startLine"] == 0 && r["endLine"] == 3));
}

#[test]
fn workspace_auto_imports_and_type_definition() {
    let mut s = server("let u: User = User();\nu\n");
    open(
        &mut s,
        "file:///C:/user.adesh",
        "export class User {\n    name: string;\n}\nexport fn unique_function(): void {}\n",
    );
    let ty = request(&mut s, "textDocument/typeDefinition", position(1, 0));
    assert_eq!(ty["uri"], "file:///C:/user.adesh");
    notify(
        &mut s,
        "textDocument/didChange",
        json!({"textDocument":{"uri":URI,"version":2},"contentChanges":[{"text":"unique_f"}]}),
    );
    let items = request(&mut s, "textDocument/completion", position(0, 8));
    let item = items
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["label"] == "unique_function")
        .unwrap();
    assert!(item["additionalTextEdits"][0]["newText"]
        .as_str()
        .unwrap()
        .contains("unique_function"));
    let symbols = request(&mut s, "workspace/symbol", json!({"query":"unique"}));
    assert!(symbols
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["name"] == "unique_function"));
}

#[test]
fn semantic_tokens_use_utf16_and_include_keywords() {
    let source = "let face = \"😀\"; let value = 1; print(value);\n";
    let mut s = server(source);
    let result = request(
        &mut s,
        "textDocument/semanticTokens/full",
        json!({"textDocument":{"uri":URI}}),
    );
    let data = result["data"].as_array().unwrap();
    let mut line = 0;
    let mut col = 0;
    let mut positions = Vec::new();
    for token in data.as_chunks::<5>().0 {
        let delta = token[0].as_u64().unwrap();
        line += delta;
        col = if delta == 0 {
            col + token[1].as_u64().unwrap()
        } else {
            token[1].as_u64().unwrap()
        };
        positions.push((line, col, token[3].as_u64().unwrap()));
    }
    let expected = source[..source.find("value").unwrap()]
        .encode_utf16()
        .count() as u64;
    assert!(
        positions
            .iter()
            .any(|p| p.0 == 0 && p.1 == expected && p.2 == 8),
        "{positions:?}"
    );
    assert!(
        positions.iter().any(|p| p.0 == 0 && p.1 == 0 && p.2 == 13),
        "{positions:?}"
    );
}
#[test]
fn code_actions_preserve_other_imports_and_comments() {
    let source = "import { used, unused } from \"./tools\";\nprint(used);\n";
    let mut s = server(source);
    let actions = request(
        &mut s,
        "textDocument/codeAction",
        json!({
            "textDocument":{"uri":URI},"range":{"start":{"line":0,"character":15},"end":{"line":0,"character":21}},
            "context":{"only":["quickfix"],"diagnostics":[{
                "range":{"start":{"line":0,"character":15},"end":{"line":0,"character":21}},
                "code":"unused-import","message":"unused import: `unused`"
            }]}
        }),
    );
    let remove = actions
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["title"] == "Remove unused import")
        .unwrap();
    let edit = &remove["edit"]["changes"][URI][0];
    assert_eq!(edit["newText"], " used ");
    assert_eq!(edit["range"]["start"]["character"], 8);
    let mut s = server("import \"z\";\n// Keep this documentation.\nimport \"a\";\n");
    let actions = request(
        &mut s,
        "textDocument/codeAction",
        json!({
            "textDocument":{"uri":URI},"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":0}},
            "context":{"only":["source.organizeImports"],"diagnostics":[]}
        }),
    );
    assert_eq!(actions, json!([]), "Do not sort across and erase comments");
}

#[test]
fn imported_modules_are_linked_and_locals_have_linked_edits() {
    let mut s = server("fn main() {\n    let local = 1;\n    print(local);\n}\n");
    let linked = request(&mut s, "textDocument/linkedEditingRange", position(1, 9));
    assert_eq!(linked["ranges"].as_array().unwrap().len(), 2);
    let highlights = request(&mut s, "textDocument/documentHighlight", position(1, 9));
    assert!(highlights
        .as_array()
        .unwrap()
        .iter()
        .any(|h| h["kind"] == 3));
    assert!(highlights
        .as_array()
        .unwrap()
        .iter()
        .any(|h| h["kind"] == 2));
    let signature_source = "fn add(a: i32, b: i32): i32 { return a + b; }\nadd(1, );";
    notify(
        &mut s,
        "textDocument/didChange",
        json!({"textDocument":{"uri":URI,"version":2},"contentChanges":[{"text":signature_source}]}),
    );
    let signature = request(&mut s, "textDocument/signatureHelp", position(1, 7));
    assert_eq!(signature["activeParameter"], 1);
}

#[test]
fn workspace_disk_lifecycle_preserves_open_buffers() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Temp(std::path::PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let dir = Temp(std::env::temp_dir().join(format!(
        "als-protocol-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir_all(&dir.0).unwrap();
    let path = dir.0.join("disk.adesh");
    std::fs::write(&path, "export fn disk_original() {}").unwrap();
    let uri = lsp_types::Url::from_file_path(&path).unwrap().to_string();
    let root = lsp_types::Url::from_directory_path(&dir.0).unwrap();
    let mut s = AdeshLanguageServer::new();
    let params: lsp_types::InitializeParams = serde_json::from_value(json!({
        "capabilities":{},"workspaceFolders":[{"uri":root,"name":"temporary"}],
        "initializationOptions":{"diagnostics":{"enabled":false}}
    }))
    .unwrap();
    s.initialize(&params);
    assert_eq!(
        request(&mut s, "workspace/symbol", json!({"query":"disk_original"}))
            .as_array()
            .unwrap()
            .len(),
        1
    );
    open(&mut s, &uri, "export fn buffer_only() {}");
    std::fs::write(&path, "export fn disk_changed() {}").unwrap();
    notify(
        &mut s,
        "workspace/didChangeWatchedFiles",
        json!({"changes":[{"uri":uri,"type":2}]}),
    );
    assert_eq!(
        request(&mut s, "workspace/symbol", json!({"query":"buffer_only"}))
            .as_array()
            .unwrap()
            .len(),
        1
    );
    notify(
        &mut s,
        "textDocument/didClose",
        json!({"textDocument":{"uri":uri}}),
    );
    assert_eq!(
        request(&mut s, "workspace/symbol", json!({"query":"disk_changed"}))
            .as_array()
            .unwrap()
            .len(),
        1
    );
    notify(
        &mut s,
        "workspace/didChangeWorkspaceFolders",
        json!({"event":{"added":[],"removed":[{"uri":root,"name":"temporary"}]}}),
    );
    assert_eq!(
        request(&mut s, "workspace/symbol", json!({"query":"disk_changed"})),
        json!([])
    );
}
