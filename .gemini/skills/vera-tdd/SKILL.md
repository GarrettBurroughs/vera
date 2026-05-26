---
name: vera-tdd
description: The Test-Driven Development workflow that MUST be followed when making changes to the Vera compiler.
---

# Vera Test-Driven Development (TDD) Workflow

The Vera compiler strictly adheres to Test-Driven Development (TDD). Agents MUST follow these steps before writing any Rust code for the compiler.

## Step 1: Write a Failing Test

Before implementing a new feature, fixing a bug, or changing the language semantics, you MUST write a test that reproduces the missing feature or bug.

**For Language Features (Integration Tests):**
1. Create a new `.vera` file in the `selftests/` directory (e.g., `selftests/050_new_feature.vera`).
2. Add the appropriate directive at the top (`// run-pass`, `// build-fail`, or `// verify-fail`).
3. Write the minimal Vera code needed to test the behavior.

**For Internal Compiler Logic (Unit Tests):**
1. Locate the appropriate Rust module (e.g., `src/hir/typecheck.rs`).
2. Add a `#[test]` function in the `mod tests { ... }` block at the bottom of the file.

## Step 2: Verify the Test Fails

Run the test suite to ensure the test fails as expected. This proves that your test is accurately capturing the missing behavior.

* **For self-tests:** `cargo test --test self_tests`
* **For unit tests:** `cargo test`

*Do not proceed to Step 3 until you have a failing test.*

## Step 3: Implement the Fix

Now you can write the Rust code in the compiler to implement the feature or fix the bug.

* Ensure your code is deterministic (e.g., use `BTreeMap` instead of `HashMap`).
* Ensure you add proper error diagnostics using `miette`.

## Step 4: Verify the Test Passes

Run the test suite again. If it passes, you have successfully completed the TDD loop!

```bash
cargo test
cargo clippy
```

Fix any clippy warnings before considering the task complete.
