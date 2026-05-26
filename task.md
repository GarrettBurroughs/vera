# Vera Compiler Task Tracker

## Phase 3 — Module System & Binary Output

### Task 1: Module System (Spec Files & Visibility) — COMPLETE ✅
- [x] `HirFunc.body` changed to `Option<HirBlock>` to support bodyless spec functions
- [x] Backend handles `None` bodies (declares as external LLVM symbols)
- [x] Verification pipeline skips functions with no body
- [x] Integration: `selftests/043_spec_file.vera` passes (`std.exit` calls libc)
- [x] Fixed unit tests in `src/hir/lower.rs` to use `.as_ref().unwrap()` on `body`
- [x] Fixed backend call resolution to fall back to short name for spec functions

### Task 2: Binary Output (Linking & Object Files) — COMPLETE ✅
- [x] Added `CompileOptions` struct to backend (`target: Option<&str>`, `emit_obj_only: bool`)
- [x] Extracted shared codegen logic into `build_codegen` helper
- [x] Added `compile_with_options` that supports `emit_obj_only` and cross-compilation target triples
- [x] `compile_to_binary` now delegates to `compile_with_options` (backward-compatible)
- [x] Wired `--emit=obj` and `--target` in `run_compiler_pipeline`
- [x] Integration tests: `test_emit_obj_produces_object_file`, `test_target_host_triple_build_succeeds`
- [x] All 122 tests pass (68 unit, 7 integration, 47 self-tests)

### Task 3: Incremental Query Engine — NOT STARTED
Integrate `salsa` for incremental parsing, type-checking, and isolated background verification.
