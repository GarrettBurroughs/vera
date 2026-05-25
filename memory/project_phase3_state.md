---
name: project-phase3-state
description: Current state of Phase 3 in the Vera compiler — which tasks are complete and what's next
metadata:
  type: project
---

Phase 3 of the Vera compiler roadmap (Compiler Architecture & Tools) as of 2026-05-25:

- **Module System** (Task 1) — COMPLETE. `HirFunc.body` is `Option<HirBlock>` to support bodyless spec functions. Backend declares spec functions as external LLVM symbols; call resolution falls back to short name (last `::` segment) when full path not found. Tests 040–043 all pass.

- **Binary Output** (Task 2) — COMPLETE. Added `CompileOptions { target, emit_obj_only }` to backend. `--emit=obj` produces a `.o` object file without linking; `--target=<triple>` cross-compiles via LLVM. 122 total tests pass.

- **Incremental Query Engine** — NOT STARTED. Next task: integrate `salsa` for incremental parsing, type-checking, and isolated background verification queries.

**Why:** Staying in Phase 3 of the roadmap before moving to LSP features.
**How to apply:** When a new agent picks up work, they should proceed with the Incremental Query Engine as the next task per `task.md` and `roadmap.md`.
