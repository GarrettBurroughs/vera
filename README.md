# Vera Compiler

Vera is a systems programming language focused on formal verification, C-ABI compatibility, and developer ergonomics. 

The compiler incorporates an incremental query-driven architecture, a lossless CST parser for excellent Language Server Protocol (LSP) support, and a robust SMT-based Verification Pipeline (WP generation) that sits alongside the LLVM lowering backend.

## Quick Links

- **[Development Guide](development.md)**: Start here if you are contributing! It outlines the project structure, TDD rules, and the required workflow for proposing language changes.
- **[Compiler Roadmap](roadmap.md)**: Macro-level tracking of the compiler development phases.
- **[Formal Grammar](grammar.ebnf)**: The exact lexical and syntactic rules of the language.
- **[Language Specification](docs/spec.md)**: Formal semantics, memory model, and Verification calculus.
- **[Language Syntax](docs/syntax.md)**: Developer-friendly guide to Vera's syntax and features.

## Example Usage

Vera emphasizes reliability through formal verification. You can embed proof requirements directly into the function signature:

```vera
// A mathematically verified addition function
pub pure func add_positive(a: i32, b: i32): i32 
spec {
    requires a > 0 && b > 0;
    ensures result > 0;
} {
    return a + b;
}
```

## CLI Commands

The compiler CLI (`vera`) provides standard developer tooling:

- **Build an executable**: `vera build src/main.vera -o app`
- **Build and execute immediately**: `vera run src/main.vera`
- **Verify without compiling**: `vera check src/main.vera`
- **Start the Language Server**: `vera lsp`
- **Dump internal stages for debugging**: `vera build src/main.vera --emit=hir`

## Testing Strategy

We heavily utilize Test-Driven Development (TDD). Before implementing a new feature or fixing a bug, an executable test must be written.

To run the entire test suite, including the end-to-end `selftests` runner that executes actual Vera programs:
```bash
cargo test
```

## Editor Tooling

Vera includes a basic VS Code extension for syntax highlighting. See [tools/vscode/README.md](tools/vscode/README.md) for packaging and installation instructions.
