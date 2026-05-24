# Vera Compiler Roadmap

This document outlines the macro-level phases required to build the complete Vera compiler.

## Phase 1: Foundation and Vertical Slice
- [x] Scaffolding: Setup workspace, dependency graph, CLI structure, and test runner.
- [x] Vertical Slice: Parse a minimal `func main(): i32 { return 42; }` program and lower it completely to an executable LLVM binary that returns `42`.

## Phase 2: Lexer and Parser
- [x] Build `vera_lexer` using `logos` for lightning-fast tokenization of all Vera keywords and literals.
- [x] Build `vera_parser` using `rowan` to support lossless Concrete Syntax Trees (CST), preserving whitespace and invalid syntax for LSP.
- [x] Plumb errors into a centralized `miette` diagnostic structure.

## Phase 3: Semantic Analysis and HIR
- [x] Abstract Syntax Tree (AST) lowering from CST.
- [x] Name Resolution: Resolve paths, modules, visibility, and trait names.
- [x] Type Checking: Unify types, enforce borrow checker rules, and produce the typed High-Level Intermediate Representation (HIR).

## Phase 3.5: Advanced Expressions and Statements
- [x] Add parsing for math (`+`, `-`, `*`, `/`), logic (`&&`, `||`, `!`), and comparisons (`==`, `<`, etc).
- [x] Add parsing for `let` statements (`const`/`var`), identifier lookups, and `if/else` expressions.
- [x] Implement AST wrappers and HIR lowering (with type unification) for the new AST nodes.
- [x] Update LLVM backend to generate instructions for advanced expressions and local variables using `inkwell`.

## Phase 4: Verification Pipeline
- [x] Integrate `z3` binary execution for SMT solving.
- [x] Parse `spec` blocks and `requires`/`ensures` contracts.
- [x] Implement Weakest Precondition (WP) generation module (`src/verification/wp.rs`).
- [x] Generate SMT-LIB2 queries and shell out to `z3` to verify them (`src/verification/smt.rs`).
- [x] Validate valid asserts and fail compilation on invalid asserts.

## Phase 5: LLVM Backend CodeGen & Function Calls (Completed)
- [x] Translate typed HIR into LLVM IR using `inkwell`.
- [x] Implement variables (`alloca`), assignment (`store`/`load`), and control flow (`br`).
- [x] Implement function signatures, arguments, and recursive calls.

---

## Comprehensive Todo List

### 1. Language Constructs & Type System
- [x] **Structs**: Memory layout rules (C ABI), field access, instantiation.
- [x] **Enums & Variants**: Tagged unions, C ABI `@abi(C)` generation, `match` expressions.
- [x] **Arrays & Slices**: Compile-time arrays (`array[T, N]`), slices (`slice[T]`, `mut slice[T]`), fat pointer layouts.
- [ ] **Pointers & References**: Safe references (`ref T`, `mut ref T`), raw pointers (`ptr T`, `mut ptr T`), dereferencing (`*x`).
- [ ] **Borrow Checker**: Lifetime tracking, exclusive mutability analysis, non-lexical lifetimes.
- [ ] **Refinement Types**: Types bounded by predicates (`T where P`), erasure during code generation, type-checking assertion obligations.
- [ ] **Generics & Traits**: Monomorphization, generic type bounds, traits (`trait`, `impl`).
- [ ] **Closures & Function Pointers**: `func(...)` types, C ABI fat pointer closure representation (`fn_ptr`, `env`), anonymous closure expressions `|x| expr`.
- [x] **Error Handling (`?`)**: Desugaring the `?` operator for `Result[T, E]`, early return paths.
- [x] **Loops & Iteration (Part 1)**: `while`, `break`, `continue`.
- [x] **Loops & Iteration (Part 2)**: `for` loops (requires arrays/slices first).
- [ ] **Unsafe Blocks**: `unsafe { ... }` scopes for raw pointer manipulations.

### 2. Verification & SMT Integration
- [ ] **Loop Verification**: `invariant` and `decreases` parsing, induction proofs, termination proofs.
- [ ] **Ghost Code**: `ghost { ... }` blocks, ghost variables/parameters, ensuring complete erasure in LLVM backend.
- [ ] **Logic Quantifiers**: `forall`, `exists`, `choose` implementations in WP and SMT-LIB2 backend.
- [ ] **Memory Verification Models**: Heap modeling in SMT, `valid(r)`, `valid_read(r)`, `separated(p1, p2)` intrinsic predicates.
- [ ] **Framing Analysis**: `assigns` clauses parsing and semantics, enforcing immutability of unassigned memory in loops and functions.
- [ ] **Precondition Vacuity Checking**: Automated `unsat` check for preconditions before function verification.
- [ ] **`std.spec` Core Library**: Implement `is_sorted`, `permutation`, `all_distinct`, `contains` natively in WP logic.

### 3. Compiler Architecture & Tools
- [ ] **Module System**: `import`, path resolution, file-system mapping, `pub` visibility scopes.
- [ ] **Incremental Query Engine**: Integrate `salsa` for incremental parsing, type-checking, and isolated background verification queries.
- [ ] **Self-Hosting Optimizations**: Implement `Strip Mode` in parser to discard CST metadata (comments/whitespace) during CLI builds for memory efficiency.
- [ ] **Binary Output**: Expand `compile_to_binary` to cross-compile executable ELF/PE formats and object files properly linking system `libc`.

### 4. Language Server Protocol (LSP) Features
- [ ] **LSP Server Backbone**: Basic text document sync, initialization, and client-server JSON-RPC communication.
- [ ] **Diagnostic Syncing**: Publish parsing, semantic, and verification errors to the editor.
- [ ] **Inline Proof Status**: Visual checkmarks and feedback for verified functions and assertions.
- [ ] **Visual Counterexample Debugging**: Parse Z3 models, filter to local scope, and project inline virtual text (inlay hints) on assertion failures.
- [ ] **Incremental Asynchronous Proofs**: Background thread execution for verification queries, non-blocking UI, configurable solver timeouts.
- [ ] **Refactoring & IDE Intelligence**: Rename symbols, formatting (using the lossless CST), Go-to Definition, Auto-completion.

