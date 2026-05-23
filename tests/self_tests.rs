#[cfg(test)]
mod tests {
    use std::process::Command;

    #[test]
    fn run_all_selftests() {
        let entries = std::fs::read_dir("selftests").unwrap();
        
        let build_status = Command::new("cargo")
            .arg("build")
            .status()
            .unwrap();
        assert!(build_status.success(), "Failed to build the compiler");
            
        for entry in entries {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("vera") {
                let content = std::fs::read_to_string(&path).unwrap();
                if content.contains("// run-pass") {
                    let mut expected_code = 0;
                    for line in content.lines() {
                        if let Some(idx) = line.find("// expect-exit-code: ") {
                            let num_str = &line[idx + 21..].trim();
                            if let Ok(num) = num_str.parse::<i32>() {
                                expected_code = num;
                                break;
                            }
                        }
                    }

                    let status = Command::new("cargo")
                        .arg("run")
                        .arg("--")
                        .arg("run")
                        .arg(&path)
                        .status()
                        .unwrap();
                    
                    assert_eq!(
                        status.code(), 
                        Some(expected_code), 
                        "Test {} failed. Expected exit code {}, but got {:?}", 
                        path.display(), expected_code, status.code()
                    );
                }
            }
        }
    }
}
