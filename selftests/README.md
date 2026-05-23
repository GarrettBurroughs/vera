# Vera Selftests

This directory contains executable `.vera` programs used for Test-Driven Development (TDD) of the compiler.

Each file in this directory is a complete program that tests a specific feature of the language. When executed, the program should return a status code of `0` (success). Any non-zero exit code indicates a failure.

## Directives
The test runner (invoked via `cargo test`) will scan these files. You can add directives at the top of the file as comments to tell the runner what to expect:
* `// run-pass`: The compiler should successfully build the file, and running the resulting binary should exit with code `0`.
* `// build-fail`: The compiler should intentionally fail to build this file (useful for testing borrow checker violations or syntax errors).
* `// verify-fail`: The compiler should build the AST but fail during the SMT verification stage.
