# Vera Verification Specifications

Verification in Vera is a first-class language feature. The compiler parses contract specifications alongside executable code, performs type checking, generates Verification Conditions (VCs) using a Weakest Precondition (WP) calculus, and passes them to an SMT solver (such as Z3 or CVC5 via Why3) to mathematically prove the code's correctness.

---

## 1. Native Contract Clauses

Functions can be annotated with contracts that form the boundaries of the function's behavior.

```vera
func swap(a: mut ptr i32, b: mut ptr i32)
spec {
    requires valid(a) && valid(b);
    requires separated(a, b);
    assigns *a, *b;
    ensures *a == old(*b) && *b == old(*a);
}
{
    unsafe {
        const temp = *a;
        *a = *b;
        *b = temp;
    }
}
```

### Preconditions (`requires`)
- Defines what must be true before calling the function.
- The caller is responsible for proving that the preconditions are satisfied at the call site.
- If multiple `requires` clauses are present, they are implicitly conjoined (`&&`).

### Postconditions (`ensures`)
- Defines what is guaranteed to be true when the function returns.
- The function implementation must satisfy these conditions.
- Inside `ensures`, you can use:
  - `result`: Refers to the returned value.
  - `old(expr)`: Evaluates the expression in the state before the function execution started.

### Frame Conditions (`assigns` / `writes`)
- Lists the memory locations that the function is allowed to modify.
- Anything not listed in `assigns` is guaranteed to be unchanged when the function returns.
- If a function has no `assigns` clause, it is treated as **pure** (cannot modify any heap memory).

### Termination (`decreases`)
- Used for recursive functions to prove termination.
- Must evaluate to an expression that strictly decreases on each recursive call according to a well-founded relation (typically a non-negative integer that decreases toward 0).

### Struct Invariants
In addition to function-level contracts, structs can define native invariants that must hold for all instances of the struct at public boundaries:
* **Syntax**: Declared using the `invariant <expr>` clause directly inside the struct body.
* **Verification Rule**: For any public or exported function taking `ref Self`, `mut ref Self`, or returning `Self`:
  1. The struct invariant is automatically assumed as a **precondition** on entry.
  2. The struct invariant must be proved as a **postcondition** on exit (for return values or mutated parameters).
* **Temporary Violations**: Private internal helper functions do not automatically enforce struct invariants, allowing the implementation to temporarily violate invariants during intermediate steps as long as the invariant is restored before returning to the caller.

```vera
struct Counter {
    value: i32,
    limit: i32,

    // Struct invariant guaranteed at public boundaries
    invariant value <= limit;
}
```

---

## 2. Logic Language Expressions

Vera's contracts support a first-order logic language that extends standard boolean expressions:

* **Implication (`==>`)**: `A ==> B` is true if `A` implies `B`.
* **Equivalence (`<==>`)**: `A <==> B` is true if `A` is equivalent to `B`.
* **Universal Quantifier (`forall`)**: `forall(x: T) { P(x) }` or range-bounded `forall(x in start..end) { P(x) }`.
* **Existential Quantifier (`exists`)**: `exists(x: T) { P(x) }` or range-bounded `exists(x in start..end) { P(x) }`.
* **Choice Binder (`choose`)**: `choose(x: T) { P(x) }` or range-bounded `choose(x in start..end) { P(x) }` (evaluates to a unique value of type `T` that satisfies `P(x)`, assuming one exists).
* **Integer Arithmetic**: Inside contracts, standard operators `+`, `-`, `*`, `/` represent mathematical operations on mathematical integers ($\mathbb{Z}$) rather than machine integers. This eliminates the need to worry about overflow in proofs, while the compiler verifies that the implementation does not overflow.

Example of quantification:
```vera
func find_zero(arr: slice[i32]): Option[u64]
spec {
    ensures match result {
        case Some(idx) => idx < arr.len() && arr[idx] == 0,
        case None => forall(i in 0..arr.len()) { arr[i] != 0 },
    };
}
```

---

## 3. Memory Verification Predicates

When interacting with C ABI code and raw pointers, Vera provides predicates to reason about raw memory:

* `valid(ptr)`: Evaluates to `true` if the memory pointed to by `ptr` is valid for both reading and writing.
* `valid_read(ptr)`: Evaluates to `true` if the memory is valid for reading only.
* `separated(ptr1, ptr2)`: Evaluates to `true` if the memory block pointed to by `ptr1` and the block pointed to by `ptr2` are completely disjoint (i.e. no aliasing).

```vera
func copy_ints(dest: mut ptr i32, src: ptr i32, count: u64)
spec {
    requires valid(dest..dest.add(count));
    requires valid_read(src..src.add(count));
    requires separated(dest..dest.add(count), src..src.add(count));
    assigns dest..dest.add(count);
}
```

---

## 4. Loop Invariants and Variants

Loops must be verified to ensure they are correct and terminate. Vera uses three loop annotations:

```vera
var i = 0;
while i < len
spec {
    invariant i <= len;
    invariant forall(j in 0..i) { arr[j] != value };
    decreases len - i;
}
{
    if arr[i] == value {
        break;
    }
    i = i + 1;
}
```

1. `invariant <expr>`: A condition that holds:
   - Before entering the loop.
   - At the beginning of each iteration.
   - Immediately after the loop terminates.
2. `assigns <lvalue-list>`: Declares which variables or memory locations are mutated inside the loop.
3. `decreases <expr>`: A loop variant used to prove that the loop terminates. The expression must be a non-negative integer that decreases with every iteration.


### Simplifying Loop Verification: Auto-Inference and Slice Framing

Writing loop invariants is often the most tedious part of formal verification. To make verification as easy as possible, the Vera compiler employs two automated reasoning techniques: **Range Invariant Auto-Inference** and **Slice Framing Invariants**.

#### 1. Range Invariant Auto-Inference
When writing standard loops (e.g., iterating over a index range), the compiler runs a lightweight abstract interpreter using interval domains to automatically compute the bounds of the loop counter.
For a loop using `for i in 0..len` or a `while i < len` loop incremented by `1`, the compiler automatically injects the range invariants:
* `i >= 0`
* `i <= len`

This eliminates the need to write redundant invariants like `invariant i <= len` manually.

#### 2. Automatic Slice Framing Invariants
When modifying a slice `arr` inside a loop at the current index `i`, you normally have to write a framing invariant specifying that elements at indices `j >= i` have not yet changed from their pre-loop values (e.g., `invariant forall(j in i..arr.len()) { arr[j] == old(arr[j]) }`).

In Vera, the compiler automatically generates and inserts this framing invariant under strict **Loop Induction Variable Analysis** to ensure proof soundness:
1. **Strict Monotonicity**: The loop counter must be verified by the compiler as a strictly monotonic loop induction variable (incremented exactly once by a constant `1` at the end of the loop block).
2. **No Reassignment**: The loop counter cannot be reassigned or modified anywhere else inside the loop body.
3. **No Early Exit**: The loop must not contain early `break` or `return` statements (unless an explicit loop exit contract is manually provided).

If these checks are satisfied, the compiler automatically inserts:
* `invariant forall(j in i..arr.len()) { arr[j] == old(arr[j]) };`

If any check fails, auto-framing is disabled, and the compiler requires the programmer to specify the framing invariant manually.

---

### Comparison: Manual vs. Automated Verification

Here is a side-by-side look at a function that sets all elements of a slice to 0.

#### Verbose Traditional Verification (Standard WP)
In traditional systems (such as Frama-C or raw SMT condition generators), the programmer is forced to specify every trivial fact:

```vera
func zero_out_verbose(arr: mut slice[i32])
spec {
    ensures forall(k in 0..arr.len()) { arr[k] == 0 };
}
{
    var i: u64 = 0;
    while i < arr.len()
    spec {
        // Manually declaring range bounds
        invariant i <= arr.len();
        // Manually declaring frame stability of unvisited indices
        invariant forall(j in i..arr.len()) { arr[j] == old(arr[j]) };
        // Manually declaring progression progress
        invariant forall(j in 0..i) { arr[j] == 0 };
        decreases arr.len() - i;
    }
    {
        arr[i] = 0;
        i = i + 1;
    }
}
```

#### Simplified Vera Verification
By utilizing **Range Invariant Auto-Inference** and **Automatic Slice Framing**, the programmer only needs to write the core loop invariant (the actual work done by the loop):

```vera
func zero_out_vera(arr: mut slice[i32])
spec {
    ensures forall(k in 0..arr.len()) { arr[k] == 0 };
}
{
    for i in 0..arr.len()
    spec {
        // Programmer only specifies the loop progress invariant:
        invariant forall(j in 0..i) { arr[j] == 0 };
    }
    {
        arr[i] = 0;
    }
}
```

---

## 5. Ghost Code and Logic Functions

Sometimes, verification requires tracking helper state that is not needed in the final executable, or writing complex mathematical functions. Vera supports this via **Ghost Code**.

### Ghost Variables and Parameters
Ghost variables and parameters are used only for verification. The compiler guarantees that they are completely erased during the code-generation phase and have no impact on runtime performance or layout.

```vera
func compute_sum(arr: slice[i32], ghost size: u64): i32
spec {
    requires arr.len() == size;
}
```

### Logic/Ghost Functions
Logic functions are pure, mathematical functions. They cannot modify state, and they can only be used in contracts or other ghost code. While they are not compiled to machine code and can use infinite mathematical datatypes, unbounded recursion can cause the SMT solver to hang or loop infinitely during quantifier instantiation (quantifier matching loops). 

Therefore, the compiler enforces that **all recursive logic and ghost functions must provide a `decreases` clause** and prove termination:

```vera
ghost func mathematical_sum(n: u64): u64 
spec {
    decreases n;
}
{
    if n == 0 {
        0
    } else {
        n + mathematical_sum(n - 1)
    }
}
```

### Pure Function Reflection
In many verification tools, programmers must duplicate logic by writing a mathematical ghost function (e.g., `ghost func logical_is_sorted`) and a matching executable function (e.g., `func is_sorted`). 

To eliminate this overhead, Vera introduces **Pure Function Reflection**. Any runtime function marked with the `pure` keyword is automatically promoted by the type-checker into a logic predicate that can be used directly inside contracts, invariants, and assert statements.

#### Purity Constraints
To be marked `pure`, a function must satisfy the following strict compile-time checks:
1. **No Side Effects**: It cannot mutate any caller-allocated memory (no `assigns` clauses other than writing to its own local variables).
2. **Deterministic Execution**: It can only call other `pure` functions.
3. **No Unsafe Operations**: It cannot dereference raw pointers or contain `unsafe` blocks.
4. **Guaranteed Termination**: The compiler must be able to prove termination (e.g. via bounded loop constructs or explicit termination measures).

#### Example
```vera
/// A pure executable function that checks if a slice is sorted.
pure func is_sorted(arr: slice[i32]): bool {
    for i in 0..arr.len() - 1
    spec {
        // Program loop invariant
        invariant forall(j in 0..i) { arr[j] <= arr[j + 1] };
    }
    {
        if arr[i] > arr[i + 1] {
            return false;
        }
    }
    return true;
}

/// is_sorted can now be used as a predicate in contracts!
func binary_search(arr: slice[i32], target: i32): Option[u64]
spec {
    requires is_sorted(arr);
    ensures match result {
        case Some(idx) => arr[idx] == target,
        case None => forall(i in 0..arr.len()) { arr[i] != target },
    };
}
```

##### Under the Hood (SMT Generation)
The compiler translates `is_sorted` into a mathematical function in the SMT-LIB2 theory context:
```smt2
(define-fun-rec is_sorted ((arr (Array Int Int)) (len Int)) Bool
    ...)
```
This enables zero-overhead executable code at runtime (compiling to a simple C-compatible function) while offering mathematical reflection for correctness proofs.

---

## 6. Inline Statements

To aid the SMT solver in complex proofs, Vera allows assertions and assumptions directly in function bodies:

* `assert <expr>;`: Commands the compiler to prove that `<expr>` holds at this exact point. If it cannot prove it, compilation fails.
* `assume <expr>;`: Instructs the verification engine to treat `<expr>` as true at this point without proving it. This is useful for dealing with third-party libraries or hardware inputs where verification is impossible.
* `check <expr>;`: Similar to `assert`, but does not add the fact to the solver context afterwards (used to verify intermediate goals without cluttering the proof environment).

```vera
const x = get_sensor_value();
assume x >= 0; // External hardware constraint
const y = x + 1;
assert y > 0; // Proven by solver
```

---

## 7. Precondition Vacuity Checking

A common and dangerous pitfall in formal verification is the **vacuous proof**. If a bug in a function precondition, macro definition, or type constraint makes the precondition logically equivalent to `false` (or otherwise impossible to satisfy), the SMT solver will successfully "prove" any postcondition vacuously. This occurs because in first-order logic, `false ==> Post` is always true. 

To solve this, Vera performs automatic **Precondition Vacuity Checking** at compile time:
* **The Check**: For every function `f` with precondition $Pre(x_1, x_2, ...)$, the verification engine generates an SMT query:
  $$\exists x_1, x_2, ... :: Pre(x_1, x_2, ...)$$
* **Behavior**: If this query is **unsatisfiable (`unsat`)**, meaning there is no set of arguments that can satisfy the function's preconditions, the compiler immediately halts compilation and issues a **Vacuous Precondition Error**.
* **Integration**: This checking also happens in real-time within the Language Server Protocol (LSP) to warn the developer of unsatisfiable contracts immediately in the editor.

```vera
// Example of a bug that triggers a Vacuous Precondition Error:
const MAX_SAFE_INT: i32 = -1; // Type bug: should be (unsigned int)-1 >> 1

func process(x: i32)
spec {
    requires x > 0 && x < MAX_SAFE_INT;
}
// Compiler runs: exists x :: x > 0 && x < -1 -> UNSATISFIABLE!
// Result: Compile error: "Function 'process' has a vacuously false precondition"
```

---

## 8. Automatic Contract Predicates (Contract Reuse)

In traditional verification, calling a function `bar` inside `foo` forces `foo` to duplicate the preconditions and postconditions of `bar` in its own contracts to verify correctly.

To prevent this duplication, the Vera compiler automatically generates two logic namespace predicates for every function `f(x: A) -> B`:
1. `f.requires(x)`: Resolves to `f`'s preconditions.
2. `f.ensures(x, result)`: Resolves to `f`'s postconditions.

These predicates can be reused in any contract, allowing contract inheritance and abstraction.

```vera
func sort(arr: mut slice[i32])
spec {
    ensures is_sorted(arr);
    ensures permutation(arr, old(arr));
}

// Callers can inherit sort's contract automatically
func sort_and_process(arr: mut slice[i32])
spec {
    requires sort.requires(arr);
    assigns arr[..];
    ensures sort.ensures(old(arr), arr);
}
{
    sort(arr);
}
```

---

## 9. Nested and Pattern-Matched Behaviors

Branching functions (like comparisons or parsers) often require verbose, repetitive flat behaviors. To keep specifications concise, Vera supports **Nested Behaviors** and **Pattern-Matched Contracts**.

### Pattern-Matched Contracts
You can use `match` directly in postconditions to align the contract structure with the implementation's logical branches:

```vera
func semver_compare(x: semver_t, y: semver_t): i32
spec {
    ensures match (x.major.compare(y.major)) {
        case Ordering.Greater => result == 1,
        case Ordering.Less => result == -1,
        case Ordering.Equal => match (x.minor.compare(y.minor)) {
            case Ordering.Greater => result == 1,
            case Ordering.Less => result == -1,
            case Ordering.Equal => result == x.patch.compare(y.patch),
        }
    };
}
```

### Nested Behaviors
Alternatively, behaviors can be nested hierarchically. A nested behavior inherits the assumptions of its parent, eliminating the need to repeat common predicates.

```vera
func semver_compare(x: semver_t, y: semver_t): i32
spec {
    behavior major_gt:
        assumes x.major > y.major
        ensures result == 1;
    behavior major_eq:
        assumes x.major == y.major
        behavior minor_gt:
            assumes x.minor > y.minor
            ensures result == 1;
        behavior minor_eq:
            assumes x.minor == y.minor
            ensures result == x.patch.compare(y.patch);
}
```

---

## 10. Standard Specification Library (`std.spec`)

To provide developers with a robust toolbox for writing specifications, Vera compiles with `std.spec`, a standard library of mathematical predicates and logic helpers:

* `std.spec.is_sorted[T](slice: slice[T]): bool`: Evaluates to `true` if the slice elements are sorted in non-decreasing order.
* `std.spec.permutation[T](a: slice[T], b: slice[T]): bool`: Evaluates to `true` if `a` is a mathematical permutation of `b`.
* `std.spec.all_distinct[T](slice: slice[T]): bool`: Evaluates to `true` if no two elements in the slice are equal.
* `std.spec.contains[T](slice: slice[T], val: T): bool`: Evaluates to `true` if `val` is present in `slice`.

By using `std.spec`, developers can write concise, high-level contracts (e.g. `ensures std.spec.permutation(arr, old(arr))`) without having to manually define complex inductive or recursive predicates.
