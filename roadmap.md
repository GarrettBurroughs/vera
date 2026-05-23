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

## Phase 5: LLVM Backend CodeGen
- [ ] Translate typed HIR into LLVM IR using `inkwell`.
- [ ] Generate proper struct memory layouts, including C-ABI compatible fat-pointers for closures.
- [ ] Emit final optimized executable binaries (`.exe` / ELF).
