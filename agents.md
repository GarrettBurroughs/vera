# AI Agent Guidelines

**CRITICAL:** If you are an AI agent, assistant, or tool interacting with this repository, you **MUST** read and adhere to these rules before taking any action.

## 1. Tool Usage and Editing Files

* **DO NOT use command line tools to edit files.** Never use `sed`, `awk`, `echo >`, `cat >`, or similar bash commands to modify code or text files.
* **DO NOT write Python or bash scripts to edit files.** 
* **USE PROVIDED IDE TOOLS:** You must exclusively use your built-in tools (e.g., `replace_file_content`, `multi_replace_file_content`, `write_to_file`) to edit files.
* Only run shell commands when explicitly necessary (e.g., `cargo test`, `git status`, or interacting with build systems).

## 2. Documentation is the Source of Truth

Before making any changes to the codebase, you must review the existing documentation to understand the project architecture and roadmap:
* Read **`development.md`**: The absolute source of truth for coding standards, the testing infrastructure, and workflows.
* Read **`roadmap.md`**: Outlines the current progress. Ensure your work aligns with the current phases.
* If making language design changes, you MUST read and update **`docs/spec.md`**, **`docs/syntax.md`**, and **`grammar.ebnf`**. Do not write code for a new language feature until the documentation is written and updated.

## 3. Test-Driven Development (TDD) Workflow

* **Write the Test First:** Before writing any compiler Rust code, or fixing a bug, you must write a failing test.
* For compiler components, write a unit test (`#[cfg(test)]`).
* For language features, write a `.vera` integration test inside the `selftests/` directory. Use the appropriate test directive (e.g., `// run-pass`, `// build-fail`, `// verify-fail`).
* Verify the test fails by running `cargo test`. Only then should you implement the feature/fix.

## 4. Determinism in the Compiler

* **DO NOT use `std::collections::HashMap` or `HashSet`** if the iteration order affects the generated code, Verification Conditions, or LLVM IR. 
* Use `std::collections::BTreeMap` or `BTreeSet` to ensure the compiler remains strictly deterministic.

## 5. Token & Context Window Management (Efficiency)

To decrease token usage and speed up your workflow:
* **Targeted Reading**: DO NOT read massive files in their entirety unless necessary. Use `grep_search` to find specific functions, structs, or symbols, and then use `view_file` with `StartLine` and `EndLine` to read only the relevant chunks.
* **Control Terminal Output**: `cargo test` and `cargo build` can produce massive logs. When running commands you suspect will be verbose, pipe the output to a temporary file (e.g., `> output.log`) and read snippets of it, or use `head`/`tail`.
* **Targeted Testing**: During the TDD loop, run ONLY the specific test you are working on (e.g., `cargo test --test self_tests -- 016_for_loop`) instead of the entire test suite, to avoid polluting the context window with hundreds of passing test logs.
* **Distill Documentation Early**: At the beginning of a task, read all relevant documentation (`development.md`, `roadmap.md`, etc.) to understand the project architecture and what needs to be done. Once you understand the task, distill the relevant information and focus purely on execution. Only cross-reference the original, large documentation files again if you encounter uncertainty or ambiguity.

## 6. Skills

We have established specific skills to help you interact with this project. Look for them in `.gemini/skills/` or use the `view_file` tool to inspect them:
* `run-selftests`: Instructions on executing and debugging the `selftests/` suite.
* `vera-tdd`: The Test-Driven Development workflow you must follow.
