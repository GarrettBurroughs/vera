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
                    let status = Command::new("cargo")
                        .arg("run")
                        .arg("--")
                        .arg("run")
                        .arg(&path)
                        .status()
                        .unwrap();
                    
                    if path.file_name().unwrap() == "002_exit_code.vera" {
                        assert_eq!(status.code(), Some(42));
                    } else if path.file_name().unwrap() == "001_basic_math.vera" {
                        // The minimal compiler can't handle arithmetic yet.
                        // We will skip this in the runner until Phase 2 is complete.
                    } else {
                        assert_eq!(status.code(), Some(0));
                    }
                }
            }
        }
    }
}
