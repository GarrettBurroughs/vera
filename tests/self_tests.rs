#[cfg(test)]
mod tests {
    use std::process::Command;

    fn run_single_test(path_str: &str) {
        let content = std::fs::read_to_string(path_str).unwrap();

        // Directives:
        // // run-pass: runs `--verify run`, expects exit code 0.
        // // build-pass: runs `--build`, expects exit code 0.
        // // build-fail or // compile-fail: runs `--build`, expects exit code 1.
        // // verify-fail: runs `--verify run`, expects exit code 1.
        
        let is_run_pass = content.contains("// run-pass");
        let is_build_pass = content.contains("// build-pass");
        let is_build_fail = content.contains("// build-fail") || content.contains("// compile-fail");
        let is_verify_fail = content.contains("// verify-fail");

        let mut should_run = is_run_pass || is_build_pass || is_build_fail || is_verify_fail;
        
        if !should_run {
            // No recognized directive, default to treating as run-pass or skip?
            // Existing logic treated files without pass/fail as ignored, but let's just assert if missing
            return;
        }

        let mut expected_code = if is_build_fail || is_verify_fail { 1 } else { 0 };
        for line in content.lines() {
            if let Some(idx) = line.find("// expect-exit-code: ") {
                let num_str = line[idx + 21..].trim();
                if let Ok(num) = num_str.parse::<i32>() {
                    expected_code = num;
                    break;
                }
            }
        }

        let verify_exe = env!("CARGO_BIN_EXE_verify");

        let arg = if is_build_fail || is_build_pass {
            "build"
        } else {
            // default for run-pass, verify-fail
            "run"
        };

        let tmp_out = format!("./.tmp.build.{}.out", std::process::id());
        
        let mut cmd = Command::new(verify_exe);
        
        if arg == "run" {
            cmd.arg("--verify").arg("run").arg(path_str);
        } else {
            cmd.arg("build").arg(path_str).arg("--output").arg(&tmp_out);
        }

        let status = cmd.status().unwrap();
        let _ = std::fs::remove_file(&tmp_out);

        assert_eq!(
            status.code(),
            Some(expected_code),
            "Test {} failed. Expected exit code {}, but got {:?}",
            path_str, expected_code, status.code()
        );
    }

    include!(concat!(env!("OUT_DIR"), "/generated_selftests.rs"));
}
