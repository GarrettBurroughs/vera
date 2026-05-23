# Verification Pipeline

The verification pipeline is the heart of the Vera language. It runs concurrently with or immediately after the standard semantic analysis and borrow checking stages, ensuring mathematical correctness before lowering to LLVM IR.

## 1. Lowering to Verification IR

The compiler does not pass the raw AST to the verification engine. Instead, the type-checked High-Level Intermediate Representation (HIR) is lowered into a specialized **Verification IR (VIR)**. 

### VIR Characteristics:
* **Control Flow Graph (CFG)**: Loops, branches, and early returns (`break`, `continue`, `return`, `?`) are explicitly modeled as a graph of basic blocks.
* **Erased Lifetimes**: Because the borrow checker has already guaranteed memory alias safety, lifetimes are erased. Safe references (`ref T`) are treated as mathematical values or raw pointers with guaranteed `valid` predicates.
* **Desugared Traits**: Generic type constraints are monomorphized, and trait method calls are resolved to their specific concrete function instances.

## 2. Weakest Precondition (WP) Generation

The compiler iterates backward through the VIR Control Flow Graph to generate Weakest Preconditions.

1. **Postconditions**: The generator starts at the exit blocks of a function, using the `ensures` clauses as the initial postcondition.
2. **Backward Propagation**: It steps backward through every statement, applying WP rules:
   * **Assignments**: `x = E` substitutes `E` for `x` in the condition.
   * **Branches**: `if c { A } else { B }` splits the condition into `(c ==> WP(A)) && (!c ==> WP(B))`.
3. **Function Preconditions**: By the time it reaches the entry block of the function, the resulting formula represents everything that must be true for the function to safely execute and satisfy its postconditions.
4. **Contract Verification**: The compiler asserts that the function's explicit `requires` clauses logically imply this generated formula.

## 3. SMT-LIB2 Lowering and Solver Interaction

The generated WP formulas, along with the `std.spec` logical axioms, are translated into the **SMT-LIB2** standard format.

### Interaction:
* The compiler invokes an external SMT solver (like Z3 or CVC5) via standard I/O streams or C API bindings.
* The solver attempts to prove the negation of the theorem (i.e., it looks for a counter-example where the code fails its contract).
* **Sat**: If the solver finds a counter-example, the code contains a bug.
* **Unsat**: If the solver proves no counter-example can exist, the code is mathematically verified.

## 4. Verification Diagnostics

Mapping SMT solver output back to user-friendly errors is notoriously difficult. Vera handles this by:
1. **Assert Labeling**: Every WP generation step tags formulas with specific assert IDs linking back to exact CST spans.
2. **Model Extraction**: When the solver returns `sat` (a bug), the compiler extracts the solver's model (the specific variable values that trigger the bug).
3. **Diagnostic IR Generation**: The extracted model and failing assert ID are bundled into the compiler's `Diagnostic` intermediary representation (see `lsp_and_parsing.md`), pointing the user directly to the failing line with the exact variables that caused the failure.
