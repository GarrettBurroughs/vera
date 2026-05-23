# Compiler CLI and Logging Architecture

To ensure the Vera compiler is observable, debuggable, and extensible, it relies on a highly structured CLI and an asynchronous logging system.

## 1. Command-Line Interface (CLI)

The CLI uses specific subcommands to distinguish the mode of operation. 

### Subcommands
* `vera build [entry_file]` - Compiles the project into an executable binary.
* `vera run [entry_file]` - Compiles and immediately executes the binary.
* `vera check [entry_file]` - Runs parsing, semantic analysis, borrow checking, and verification. Emits errors/diagnostics but skips LLVM lowering.
* `vera lsp` - Starts the Language Server Protocol over stdin/stdout.

### Standard Flags
* `--log-level <LEVEL>` - Sets the logging verbosity (`error`, `warn`, `info`, `debug`, `trace`). Default is `warn`.
* `--log-file <FILE>` - Diverts log output from `stdout` directly to the specified file (useful for large `TRACE` dumps).
* `-O0, -O1, -O2, -O3` - LLVM optimization levels.
* `--no-verify` - (Unsafe) Skips the SMT verification stage for rapid iteration.
* `--solver <NAME>` - Selects the backend SMT solver (e.g., `z3`, `cvc5`).
* `--target <TARGET>` - Specifies the compilation target architecture.

### Emitting Intermediate Stages
To aid in compiler development and to allow external tooling, the compiler can dump its intermediate states using the `--emit` flag:
* `--emit=tokens` - Token stream from Lexer.
* `--emit=cst` - Concrete Syntax Tree.
* `--emit=hir` - Typed High-Level Intermediate Representation.
* `--emit=vir` - Verification Intermediate Representation.
* `--emit=smt` - Raw SMT-LIB2 queries.
* `--emit=llvm` - Unoptimized/Optimized LLVM IR.

#### Output Formatting
By default, stages that produce data structures (like `tokens`, `cst`, `hir`, `vir`) are dumped to `stdout` as **JSON**. This structured output is machine-readable for automated tooling. Text-based intermediate representations (like `smt` or `llvm`) are dumped directly as raw text.

* `--pretty` - When provided, data structures are rendered as beautiful, human-readable ASCII-art trees rather than JSON.
* `--emit-out <FILE>` - Redirects the emitted output to a file instead of `stdout`. If the file lacks an extension, the compiler will automatically deduce it (e.g., `.ast`, `.json`, `.smt2`, `.ll`).

---

## 2. Logging Architecture

The compiler uses structured, asynchronous logging (via the `tracing` crate). By default, logs write to `stdout` alongside standard output, but they can be redirected using the `--log-file` flag to prevent terminal freezing when generating massive traces.

### Log Levels

**`ERROR` (Unrecoverable System Failures)**
* Internal Compiler Errors (ICE - compiler bugs).
* File system I/O errors (e.g., failed to read source file, solver not found).

**`WARN` (Anomalies & Fallbacks)**
* Cache invalidation thrashing in the incremental engine.
* SMT solver timeouts or "unknown" outcomes.
* Deprecated CLI flag usage.

**`INFO` (High-Level Stage Transitions)**
* Time profiling for each stage (e.g., `[INFO] Parsing finished in 2.1ms`).
* High-level verification summaries (e.g., `[INFO] Verified 45 functions, 120 assertions`).
* Overall cache hit/miss rates.

**`DEBUG` (Granular Stage Details)**
* **Incremental Engine**: Lists of specific `FileId`s marked dirty.
* **Parsing**: Locations where error-recovery nodes were injected.
* **Semantic**: Failed trait resolutions triggering fallback searches.
* **Verification**: Block-level Weakest Precondition formulas before solver dispatch.
* **LLVM**: Specific LLVM optimization passes being applied.

**`TRACE` (Maximum Verbosity - Compiler Development)**
*This level tracks the microscopic behavior of the compiler, meant strictly for debugging compiler internals.*
* **Lexing**: Byte-by-byte tokenization state machine transitions.
* **Parsing**: Recursive descent entry/exit traces, token consumption, and lookahead buffering.
* **Semantic**: Step-by-step type unification details, variable scoping stack states, and borrow checker graph mutations.
* **Incremental Engine**: Complete evaluation traces of every single query (hit/miss, dependency edge updates).
* **Verification**: Step-by-step backward propagation of the WP calculus, showing the exact formula mutations statement by statement.
* **SMT**: The exact raw string payloads piped to and from the SMT solver process.
