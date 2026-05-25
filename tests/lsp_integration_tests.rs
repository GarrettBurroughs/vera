use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use serde_json::Value;

fn json_subset_match(expected: &Value, actual: &Value) -> Result<(), String> {
    match (expected, actual) {
        (Value::Object(exp_map), Value::Object(act_map)) => {
            for (k, v) in exp_map {
                if let Some(act_v) = act_map.get(k) {
                    json_subset_match(v, act_v).map_err(|e| format!("{}.{}", k, e))?;
                } else {
                    return Err(format!("missing key: {}", k));
                }
            }
            Ok(())
        }
        (Value::Array(exp_arr), Value::Array(act_arr)) => {
            if exp_arr.len() > act_arr.len() {
                return Err(format!("expected array length >= {}, got {}", exp_arr.len(), act_arr.len()));
            }
            for (i, v) in exp_arr.iter().enumerate() {
                json_subset_match(v, &act_arr[i]).map_err(|e| format!("[{}].{}", i, e))?;
            }
            Ok(())
        }
        (exp, act) => {
            if exp == act {
                Ok(())
            } else {
                Err(format!("expected {}, got {}", exp, act))
            }
        }
    }
}

#[test]
fn test_lsp_transcripts() {
    let mut failures = Vec::new();
    let paths = std::fs::read_dir("selftests/lsp").unwrap();
    
    let mut manifest_dir = env!("CARGO_MANIFEST_DIR").to_string();
    if manifest_dir.contains("\\") {
        manifest_dir = manifest_dir.replace("\\", "/");
    }
    let workspace_root_url = format!("file://{}", manifest_dir);
    
    for path in paths {
        let path = path.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("transcript") {
            continue;
        }
        
        println!("Running LSP transcript: {}", path.display());
        let content = std::fs::read_to_string(&path).unwrap();
        let content = content.replace("$WORKSPACE_ROOT", &workspace_root_url);
        
        let mut child = Command::new(env!("CARGO_BIN_EXE_verify"))
            .arg("lsp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("Failed to spawn vera lsp");
            
        let mut stdin = child.stdin.take().expect("Failed to open stdin");
        let mut stdout = BufReader::new(child.stdout.take().expect("Failed to open stdout"));
        
        for (line_idx, line) in content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") { continue; }
            
            eprintln!("PROCESSING LINE: {}", line);
            
            if line.starts_with("SEND: ") {
                let msg = &line[6..];
                write!(stdin, "Content-Length: {}\r\n\r\n{}", msg.len(), msg).unwrap();
                stdin.flush().unwrap();
            } else if line.starts_with("EXPECT: ") {
                let expected_str = &line[8..];
                let expected_json: Value = serde_json::from_str(expected_str).unwrap();
                
                let mut out_line = String::new();
                if let Err(e) = stdout.read_line(&mut out_line) {
                    failures.push(format!("{}:{} Failed to read response: {}", path.display(), line_idx+1, e));
                    break;
                }
                
                eprintln!("RECEIVED HEAD: {:?}", out_line);
                
                if out_line.is_empty() {
                    failures.push(format!("{}:{} Unexpected EOF from server", path.display(), line_idx+1));
                    break;
                }
                
                if !out_line.starts_with("Content-Length: ") {
                    failures.push(format!("{}:{} Expected Content-Length, got {}", path.display(), line_idx+1, out_line));
                    break;
                }
                
                let len_str = out_line.trim_start_matches("Content-Length: ").trim();
                let len: usize = len_str.parse().unwrap();
                stdout.read_line(&mut String::new()).unwrap(); // empty line
                let mut buf = vec![0; len];
                std::io::Read::read_exact(&mut stdout, &mut buf).unwrap();
                
                let actual_json: Value = serde_json::from_slice(&buf).unwrap();
                eprintln!("RECEIVED JSON: {}", actual_json);
                if let Err(e) = json_subset_match(&expected_json, &actual_json) {
                    failures.push(format!("{}:{} Match failed: {}\nExpected: {}\nActual: {}", path.display(), line_idx+1, e, expected_json, actual_json));
                    break;
                }
            }
        }
        let _ = child.kill();
    }
    
    if !failures.is_empty() {
        panic!("LSP Integration tests failed:\n{}", failures.join("\n\n"));
    }
}
