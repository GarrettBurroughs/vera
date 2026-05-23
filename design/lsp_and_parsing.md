# LSP, Parsing, and Incremental Compilation

To provide a world-class developer experience, the Vera compiler is designed around three core pillars: custom incremental compilation, lossless error-resilient parsing, and an agnostic intermediary diagnostic representation.

## 1. Custom Incremental Compilation Architecture

Instead of relying on heavy frameworks like `salsa`, the Vera compiler implements a bespoke, lightweight incremental query engine from scratch. This allows for tighter control over memory and compilation phases.

### The Query Context
The compiler state is held in a centralized `QueryContext`. The context maintains a memoization cache and a dependency graph of files and AST queries.
* **Queries**: Every compiler stage is structured as a pure function (a "query") that takes the `QueryContext` and a set of inputs (e.g., `TypeCheck(FileId)`).
* **Caching**: The result of a query is cached. 
* **Invalidation**: When a user modifies a file via the LSP, the `QueryContext` marks that file's specific `FileId` as dirty. Only the queries that transitively depend on that file are invalidated, ensuring blazing fast re-compilation.

## 2. Lossless Error-Resilient Parsing

Traditional parsers halt or fail wildly upon encountering syntax errors, which ruins autocomplete in IDEs. 

### Concrete Syntax Tree (CST)
Vera parses code into a **Concrete Syntax Tree**. Unlike an AST, a CST is entirely lossless:
* It stores every single token from the lexer, including whitespaces and comments.
* When the parser encounters unexpected syntax, it does not abort. Instead, it wraps the problematic tokens in an `ErrorNode` and attempts to resynchronize at the next recognizable delimiter (like a semicolon `;` or a closing brace `}`).

This ensures that even if a function is syntactically broken, the LSP can still understand the struct definitions above it and provide accurate autocomplete.

## 3. Diagnostic Intermediary Representation

Error reporting is centralized through an intermediary representation (IR) to ensure consistency across the CLI and LSP.

### The `Diagnostic` Struct
When any stage of the compiler encounters an issue (syntax error, type mismatch, borrow checker violation, or verification failure), it emits a `Diagnostic` object rather than printing directly to standard output.

```rust
pub struct Diagnostic {
    pub severity: Severity,        // Error, Warning, Info
    pub code: String,              // e.g., "E001"
    pub message: String,           // High-level description
    pub primary_span: Span,        // The exact location of the error
    pub secondary_spans: Vec<(Span, String)>, // Contextual hints
    pub help: Option<String>,      // Actionable advice
}
```

### Dual Rendering
This intermediary representation is completely agnostic to how it is displayed:
1. **CLI Mode**: The compiler passes the `Diagnostic` IR to a terminal renderer (mimicking the beautiful error outputs of tools like `ariadne` or `miette`), printing colored, formatted ASCII art pointing to the problematic source lines.
2. **LSP Mode**: The language server simply serializes the exact same `Diagnostic` IR into the JSON payload expected by the LSP protocol (the `PublishDiagnostics` notification), ensuring the editor underlines the exact same spans with the exact same help messages.
