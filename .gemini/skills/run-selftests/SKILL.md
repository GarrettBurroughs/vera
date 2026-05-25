---
name: run-selftests
description: Instructions for running and debugging the Vera self-test suite (TDD executable programs).
---

# Running Vera Self-Tests

The Vera compiler project uses a robust integration test suite called `selftests`. These are standalone `.vera` programs that test the entire compilation and execution (or verification) lifecycle.

## 1. Test Directives
Every file in `selftests/` MUST start with a directive that tells the test runner what is expected to happen.

* `// run-pass`: The file should compile successfully and return an exit code of `0` when executed.
* `// build-fail`: The file should fail to compile (e.g., due to a syntax error or type-checking error).
* `// verify-fail`: The file should compile, but the SMT verification phase should fail (e.g., an `ensures` clause is violated).

## 2. How to Run the Tests
To run the self-test suite, use the following cargo command from the workspace root:

```bash
cargo test --test self_tests
```

If you want to run a specific test file (e.g., `016_for_loop.vera`), you can filter the test runner by passing a substring:

```bash
cargo test --test self_tests -- 016_for_loop
```

## 3. Debugging Failures
If a test fails, you can run the compiler manually to see the exact output and diagnostic errors:

```bash
cargo run -- run selftests/016_for_loop.vera
```

Use the `--emit` flags to see the intermediate representations if you are debugging a compiler issue:

```bash
cargo run -- run selftests/016_for_loop.vera --emit hir
cargo run -- run selftests/016_for_loop.vera --emit llvm-ir
cargo run -- run selftests/016_for_loop.vera --emit smt
```
