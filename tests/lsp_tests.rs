use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

#[test]
fn test_lsp_initialize() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_verify"))
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn vera lsp");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("Failed to open stdout"));

    // Send initialize request
    let init_req = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}"#;
    write!(stdin, "Content-Length: {}\r\n\r\n{}", init_req.len(), init_req).unwrap();
    stdin.flush().unwrap();

    // Read response
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    assert!(line.starts_with("Content-Length: "));
    
    let len_str = line.trim_start_matches("Content-Length: ").trim();
    let len: usize = len_str.parse().unwrap();
    
    let mut empty = String::new();
    stdout.read_line(&mut empty).unwrap(); // empty line
    
    let mut buf = vec![0; len];
    std::io::Read::read_exact(&mut stdout, &mut buf).unwrap();
    let response: serde_json::Value = serde_json::from_slice(&buf).unwrap();
    
    assert_eq!(response["id"], 1);
    assert!(response["result"]["capabilities"].is_object());
    
    // Send initialized notification
    let init_notif = r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#;
    write!(stdin, "Content-Length: {}\r\n\r\n{}", init_notif.len(), init_notif).unwrap();
    stdin.flush().unwrap();
    
    // Send didOpen notification
    let did_open = r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///test.vera","languageId":"vera","version":1,"text":"func main(): i32 { return 0; }"}}}"#;
    write!(stdin, "Content-Length: {}\r\n\r\n{}", did_open.len(), did_open).unwrap();
    stdin.flush().unwrap();

    // Send shutdown request
    let shutdown_req = r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#;
    write!(stdin, "Content-Length: {}\r\n\r\n{}", shutdown_req.len(), shutdown_req).unwrap();
    stdin.flush().unwrap();
    
    // Read shutdown response
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let len_str = line.trim_start_matches("Content-Length: ").trim();
    let len: usize = len_str.parse().unwrap();
    let mut empty = String::new();
    stdout.read_line(&mut empty).unwrap(); // empty line
    let mut buf = vec![0; len];
    std::io::Read::read_exact(&mut stdout, &mut buf).unwrap();

    // Send exit notification
    let exit_notif = r#"{"jsonrpc":"2.0","method":"exit","params":null}"#;
    write!(stdin, "Content-Length: {}\r\n\r\n{}", exit_notif.len(), exit_notif).unwrap();
    stdin.flush().unwrap();

    let status = child.wait().unwrap();
    assert!(status.success());
}

#[test]
fn test_lsp_diagnostics() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_verify"))
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn vera lsp");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("Failed to open stdout"));

    // Send initialize request
    let init_req = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}"#;
    write!(stdin, "Content-Length: {}\r\n\r\n{}", init_req.len(), init_req).unwrap();
    stdin.flush().unwrap();

    // Read initialize response
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let len_str = line.trim_start_matches("Content-Length: ").trim();
    let len: usize = len_str.parse().unwrap();
    stdout.read_line(&mut String::new()).unwrap();
    let mut buf = vec![0; len];
    std::io::Read::read_exact(&mut stdout, &mut buf).unwrap();
    
    // Send initialized notification
    let init_notif = r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#;
    write!(stdin, "Content-Length: {}\r\n\r\n{}", init_notif.len(), init_notif).unwrap();
    
    // Send didOpen notification with a parse error
    // "func main() { return 1"
    let did_open = r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///test_diag.vera","languageId":"vera","version":1,"text":"func main() { return 1"}}}"#;
    write!(stdin, "Content-Length: {}\r\n\r\n{}", did_open.len(), did_open).unwrap();
    stdin.flush().unwrap();

    // Read publishDiagnostics notification
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    assert!(line.starts_with("Content-Length: "));
    let len_str = line.trim_start_matches("Content-Length: ").trim();
    let len: usize = len_str.parse().unwrap();
    stdout.read_line(&mut String::new()).unwrap();
    let mut buf = vec![0; len];
    std::io::Read::read_exact(&mut stdout, &mut buf).unwrap();
    
    let response: serde_json::Value = serde_json::from_slice(&buf).unwrap();
    assert_eq!(response["method"], "textDocument/publishDiagnostics");
    
    let diagnostics = response["params"]["diagnostics"].as_array().unwrap();
    assert!(!diagnostics.is_empty(), "Expected diagnostics to be reported");

    
    let diag = &diagnostics[0];
    let msg = diag["message"].as_str().unwrap();
    
    // Send shutdown request
    let shutdown_req = r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#;
    write!(stdin, "Content-Length: {}\r\n\r\n{}", shutdown_req.len(), shutdown_req).unwrap();
    stdin.flush().unwrap();

    let _ = child.kill();
}

#[test]
fn test_lsp_inlay_hints() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_verify"))
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to spawn vera lsp");

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("Failed to open stdout"));

    // Send initialize request
    let init_req = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}"#;
    write!(stdin, "Content-Length: {}\r\n\r\n{}", init_req.len(), init_req).unwrap();
    stdin.flush().unwrap();

    // Read initialize response
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let len_str = line.trim_start_matches("Content-Length: ").trim();
    let len: usize = len_str.parse().unwrap();
    stdout.read_line(&mut String::new()).unwrap();
    let mut buf = vec![0; len];
    std::io::Read::read_exact(&mut stdout, &mut buf).unwrap();
    
    // Send initialized notification
    let init_notif = r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#;
    write!(stdin, "Content-Length: {}\r\n\r\n{}", init_notif.len(), init_notif).unwrap();
    
    // Send didOpen notification with a valid program
    let did_open = r#"{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"file:///test_hints.vera","languageId":"vera","version":1,"text":"func main(): i32 {\n    return 0;\n}"}}}"#;
    write!(stdin, "Content-Length: {}\r\n\r\n{}", did_open.len(), did_open).unwrap();
    stdin.flush().unwrap();

    // The server sends publishDiagnostics (empty)
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let len_str = line.trim_start_matches("Content-Length: ").trim();
    let len: usize = len_str.parse().unwrap();
    stdout.read_line(&mut String::new()).unwrap();
    let mut buf = vec![0; len];
    std::io::Read::read_exact(&mut stdout, &mut buf).unwrap();
    
    // Send inlayHint request
    let hints_req = r#"{"jsonrpc":"2.0","id":2,"method":"textDocument/inlayHint","params":{"textDocument":{"uri":"file:///test_hints.vera"},"range":{"start":{"line":0,"character":0},"end":{"line":2,"character":1}}}}"#;
    write!(stdin, "Content-Length: {}\r\n\r\n{}", hints_req.len(), hints_req).unwrap();
    stdin.flush().unwrap();

    // Read inlayHint response
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let len_str = line.trim_start_matches("Content-Length: ").trim();
    let len: usize = len_str.parse().unwrap();
    stdout.read_line(&mut String::new()).unwrap();
    let mut buf = vec![0; len];
    std::io::Read::read_exact(&mut stdout, &mut buf).unwrap();
    
    let response: serde_json::Value = serde_json::from_slice(&buf).unwrap();
    assert_eq!(response["id"], 2);
    
    let hints = response["result"].as_array().unwrap();
    assert!(!hints.is_empty(), "Expected inlay hints for verified function");
    assert_eq!(hints[0]["label"], "✓ verified");

    // Send shutdown request
    let shutdown_req = r#"{"jsonrpc":"2.0","id":3,"method":"shutdown","params":null}"#;
    write!(stdin, "Content-Length: {}\r\n\r\n{}", shutdown_req.len(), shutdown_req).unwrap();
    stdin.flush().unwrap();

    let _ = child.kill();
}
