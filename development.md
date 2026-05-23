# Vera Development Guide

**Welcome to the Vera compiler project.** This document is the **absolute source of truth** and the main entry point for all development work. Whenever you are starting a new task, adding a feature, or fixing a bug, **refer to this document FIRST**.

---

## 1. Project Navigation and Key Documentation

Before modifying the compiler or the language, ensure you understand the architecture and formal semantics. The project is divided into several documentation realms:

### Language Specification (`docs/`)
* **[spec.md](docs/spec.md)**: The formal rules of the language, memory model, and Weakest Precondition (WP) verification calculus.
* **[syntax.md](docs/syntax.md)**: A developer-friendly guide to Vera's syntax, primitive types, and control flow.

### Compiler Design (`design/`)
* **[architecture.md](design/architecture.md)**: High-level compiler pipeline overview and Backend abstraction (LLVM).
* **[lsp_and_parsing.md](design/lsp_and_parsing.md)**: Custom incremental compilation, lossless parsing (Concrete Syntax Tree), and Diagnostic Intermediary Representation.
* **[verification_pipeline.md](design/verification_pipeline.md)**: How the compiler interacts with SMT solvers (e.g., Z3) and lowers Verification IR to SMT-LIB2.
* **[cli_and_logging.md](design/cli_and_logging.md)**: Logging architecture, CLI subcommands, and `--emit` options for debugging.

### Formal Grammar
* **[grammar.ebnf](grammar.ebnf)**: The Extended Backus-Naur Form grammar mapping out the exact lexical and syntactic rules of the language.

### Project Roadmap
* **[roadmap.md](roadmap.md)**: Outlines the high-level phases of the compiler development. **You must review and update this document as major milestones are completed.**

---

## 2. Coding Standards and Conventions

### Rust Compiler Implementation
* **Edition**: The compiler is written in Rust 2021.
* **Formatting and Linting**: All code must pass `cargo fmt` and `cargo clippy`. Treat clippy warnings as errors during development.
* **Error Handling**: Do **not** use `.unwrap()` or `panic!()` in the compiler unless representing a true Internal Compiler Error (ICE). Always bubble up errors using `Result` and convert them into the `Diagnostic` IR described in the LSP design docs.
* **Determinism**: The compiler must be strictly deterministic. Avoid iterating over standard `HashMap` or `HashSet` if the iteration order affects the generated code, Verification Conditions, or LLVM IR. Use `BTreeMap` or an index-mapped map instead.

### Vera Standard Library & Examples
* All new language features must be accompanied by a demonstration in the `examples/` directory.
* Verification assertions (`requires`, `ensures`) should be as tight as possible.

### Test-Driven Development (TDD)
* We strictly adhere to **Test-Driven Development (TDD)**. 
* **Write Tests First**: Before writing any new compiler code or fixing a bug, you must first write a failing test (either a unit test in the Rust code or a new `.vera` file in the `selftests/` directory) that reproduces the missing feature or bug. Only proceed to compiler implementation once the test is explicitly defined.

---

## 3. Workflow for Language Design Changes

If you are modifying the design of the Vera language itself (e.g., adding a new keyword, changing a type rule, or altering verification semantics), you **MUST** follow this exact workflow to keep the repository consistent:

1. **Update `docs/spec.md`**: Define the formal semantics, memory impact, and SMT proof obligations of the new feature.
2. **Update `docs/syntax.md`**: Provide user-facing examples and syntax definitions.
3. **Update `grammar.ebnf`**: Ensure the new syntax is accurately reflected in the formal grammar.
4. **Update `tools/vscode/syntaxes/vera.tmLanguage.json`**: Add the new keywords or syntax rules to the TextMate grammar so syntax highlighting works correctly.
5. **Update Examples**: Modify or add examples in `examples/` to demonstrate the new feature.

*Do not write compiler Rust code for a new language feature until the above 5 steps are fully completed and documented.*

---

## 4. Building the VS Code Extension

To ensure a good developer experience, the repository includes a basic VS Code extension providing syntax highlighting.
1. Navigate to the extension folder: `cd tools/vscode`
2. Install dependencies: `npm install`
3. Package the extension: `npx vsce package`
4. Install it in VS Code via: `code --install-extension vera-0.1.0.vsix`
*(See `tools/vscode/README.md` for more details)*.

---

## 5. Testing Infrastructure

A compiler's reliability is paramount. The testing infrastructure is broken down into three tiers, located across the repository:

### Unit Tests
* Run via `cargo test`.
* Reside inline within the `src/` modules (e.g., `#[cfg(test)]` blocks for parsing rules, incremental cache behaviors, internal logic functions).

### Integration Tests
* Reside in the `tests/integration_tests.rs` file.
* These test the compiler components working together (e.g., feeding a string of code into the parser, running semantic analysis, and verifying that the expected HIR or diagnostics are produced).

### Self Tests (Executable TDD Programs)
* Reside in the `selftests/` directory.
* These are standalone `.vera` programs that test themselves at runtime (e.g., asserting `1 + 1 == 2` and returning an exit code of `0` on success). 
* **Runner**: We manage these inside Rust using a dedicated test runner (`tests/self_tests.rs`). When you run `cargo test`, the runner automatically walks the `selftests/` directory, spawns the compiler (`vera run`), and verifies the exit codes match the expected directives (e.g., `// run-pass`, `// build-fail`, `// verify-fail`).

### Example Integration (End-to-End)
* Located in `examples/`.
* The `self_tests.rs` runner also verifies that all full-scale examples (like `http_server.vera`) compile and pass SMT verification, ensuring we never break real-world programs.
