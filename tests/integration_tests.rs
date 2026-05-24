/// Integration tests that verify the compiler pipeline end-to-end by
/// invoking the compiled `verify` binary directly.
///
/// These complement the unit tests in `src/` and the self-tests in `selftests/`.
/// The binary is built once via `cargo test` and then exercised through
/// the `check` subcommand, which runs the full front-end pipeline without
/// requiring Z3 or LLVM code generation.
#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::path::PathBuf;

    /// Returns the path to the compiled `verify` binary.
    fn vera_bin() -> PathBuf {
        let manifest = std::env::var("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|_| ".".to_string());
        PathBuf::from(manifest).join("target/debug/verify")
    }

    /// Writes `src` to a uniquely named temp file inside `target/` and invokes `vera check`.
    /// Using unique names avoids races when tests run in parallel.
    /// Returns `(exit_success, combined_output)`.
    fn vera_check(test_name: &str, src: &str) -> (bool, String) {
        let manifest = std::env::var("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|_| ".".to_string());
        let path = PathBuf::from(manifest)
            .join(format!("target/integration_{}.vera", test_name));

        std::fs::write(&path, src).expect("failed to write temp vera file");

        let out = Command::new(vera_bin())
            .args(["check", path.to_str().unwrap()])
            .output()
            .expect("failed to spawn vera binary — run `cargo build` first");

        let _ = std::fs::remove_file(&path);

        let combined = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        (out.status.success(), combined)
    }

    /// A syntactically valid, type-correct program exits successfully under `check`.
    #[test]
    fn test_valid_program_check_passes() {
        let src = r#"
func add(a: i32, b: i32): i32 {
    return a + b;
}
func main(): i32 {
    const res: i32 = add(1, 2);
    return res;
}
"#;
        let (ok, output) = vera_check("valid_program", src);
        assert!(ok, "Expected check to pass. output:\n{}", output);
    }

    /// A program with a return type mismatch exits with non-zero and reports a Semantic Error.
    #[test]
    fn test_return_type_mismatch_check_fails() {
        let src = "func main(): i32 { return true; }\n";
        let (ok, output) = vera_check("type_mismatch", src);
        assert!(!ok, "Expected check to fail for type mismatch. output:\n{}", output);
        assert!(
            output.contains("Semantic Error") || output.contains("Type"),
            "Expected type error in output, got:\n{}", output
        );
    }

    /// A program with an undefined variable exits with non-zero and reports a Semantic Error.
    #[test]
    fn test_undefined_variable_check_fails() {
        let src = "func main(): i32 { return undefined_var; }\n";
        let (ok, output) = vera_check("undefined_var", src);
        assert!(!ok, "Expected check to fail for undefined variable. output:\n{}", output);
        assert!(
            output.contains("Semantic Error") || output.contains("Undefined"),
            "Expected undefined variable error in output, got:\n{}", output
        );
    }

    /// A struct definition and instantiation pass `check` without errors.
    #[test]
    fn test_struct_check_passes() {
        let src = r#"
struct Vec2 {
    x: i32,
    y: i32,
}
func main(): i32 {
    const v: Vec2 = Vec2 { x: 3, y: 4 };
    return v.x;
}
"#;
        let (ok, output) = vera_check("struct_check", src);
        assert!(ok, "Expected struct check to pass. output:\n{}", output);
    }

    /// A program with a spec block (using parameters in requires) passes `check`.
    /// This exercises the bug fix where spec clauses must be lowered after
    /// the function scope is entered and parameters are declared.
    #[test]
    fn test_spec_block_check_passes() {
        let src = r#"
func positive(x: i32): i32
spec {
    requires x > 0;
    ensures true;
}
{
    return x;
}
func main(): i32 {
    return 0;
}
"#;
        let (ok, output) = vera_check("spec_block", src);
        assert!(ok, "Expected spec block to pass check. output:\n{}", output);
    }
}
