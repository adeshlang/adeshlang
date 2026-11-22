//! Exercises the shipped executable, including framing and the initialize/shutdown handshake.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn send(server: &mut Server, value: Value) {
    let body = serde_json::to_vec(&value).unwrap();
    let stdin = server.0.stdin.as_mut().unwrap();
    write!(stdin, "Content-Length: {}\r\n\r\n", body.len()).unwrap();
    stdin.write_all(&body).unwrap();
    stdin.flush().unwrap();
}

fn response(messages: &mpsc::Receiver<Value>, id: i32) -> Value {
    loop {
        let message = messages
            .recv_timeout(Duration::from_secs(15))
            .expect("LSP response within 15 seconds");
        if message["id"] == id {
            assert!(message.get("error").is_none(), "{message}");
            return message["result"].clone();
        }
    }
}

#[test]
fn stdio_initialize_edit_request_shutdown() {
    let mut server = Server(
        Command::new(env!("CARGO_BIN_EXE_als"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let stdout = server.0.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let mut length = None;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 {
                    let _ = done_tx.send(());
                    return;
                }
                if header == "\r\n" || header == "\n" {
                    break;
                }
                if let Some(value) = header.strip_prefix("Content-Length:") {
                    length = value.trim().parse::<usize>().ok();
                }
            }
            let Some(length) = length else {
                return;
            };
            let mut body = vec![0; length];
            if reader.read_exact(&mut body).is_err() {
                return;
            }
            if let Ok(message) = serde_json::from_slice::<Value>(&body) {
                if tx.send(message).is_err() {
                    return;
                }
            }
        }
    });
    send(
        &mut server,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "processId":null,"capabilities":{},"initializationOptions":{"codeLens":{"enabled":false}}
        }}),
    );
    let result = response(&rx, 1);
    for capability in [
        "declarationProvider",
        "typeDefinitionProvider",
        "implementationProvider",
        "documentHighlightProvider",
        "callHierarchyProvider",
        "typeHierarchyProvider",
        "linkedEditingRangeProvider",
    ] {
        assert_eq!(result["capabilities"][capability], true, "{capability}");
    }
    send(
        &mut server,
        json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    );
    let uri = "file:///C:/stdio.adesh";
    send(
        &mut server,
        json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{
            "textDocument":{"uri":uri,"languageId":"adesh","version":1,"text":"let value = 42;\nprint(value);\n"}
        }}),
    );
    send(
        &mut server,
        json!({"jsonrpc":"2.0","id":2,"method":"textDocument/hover","params":{
            "textDocument":{"uri":uri},"position":{"line":1,"character":7}
        }}),
    );
    assert!(response(&rx, 2)["contents"]["value"]
        .as_str()
        .unwrap()
        .contains("value"));
    send(
        &mut server,
        json!({"jsonrpc":"2.0","id":3,"method":"textDocument/codeLens","params":{"textDocument":{"uri":uri}}}),
    );
    assert_eq!(response(&rx, 3), json!([]));
    send(
        &mut server,
        json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{
            "textDocument":{"uri":uri,"version":2},"contentChanges":[{"text":"JSON.pa"}]
        }}),
    );
    send(
        &mut server,
        json!({"jsonrpc":"2.0","id":4,"method":"textDocument/completion","params":{
            "textDocument":{"uri":uri},"position":{"line":0,"character":7}
        }}),
    );
    assert!(response(&rx, 4)
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["label"] == "parse"));
    send(
        &mut server,
        json!({"jsonrpc":"2.0","id":5,"method":"shutdown","params":null}),
    );
    assert_eq!(response(&rx, 5), Value::Null);
    send(
        &mut server,
        json!({"jsonrpc":"2.0","method":"exit","params":null}),
    );
    drop(server.0.stdin.take());
    done_rx
        .recv_timeout(Duration::from_secs(15))
        .expect("server closes stdout after exit");
    assert!(server.0.wait().unwrap().success());
}
