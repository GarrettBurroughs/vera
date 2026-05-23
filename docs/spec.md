# Vera Language Specification
**Version 1.0**

This document defines the formal syntax, type system, execution semantics, and verification calculus of the **Vera** programming language. Vera is a systems-level, verification-first programming language designed to compile directly to platform-compatible C ABI binary code, guaranteeing zero-cost abstraction, memory safety, and mathematical correctness via native SMT-backed proofs.

---

## 1. Introduction & Language Philosophy

Vera treats verification not as an external annotation layer (like ACSL or JML comments), but as a core language construct. The compiler consists of a dual pipeline:
1. **Compilation Pipeline**: Parses, type-checks, borrow-checks, and lowers executable code blocks to C or LLVM IR, erasing all specifications and logic assertions.
2. **Verification Pipeline**: Translates type-checked AST nodes and their adjacent `spec` blocks into Verification Conditions (VCs) using a Weakest Precondition (WP) calculus, verifying their mathematical validity using SMT solvers (Z3/CVC5).

### Core Goals:
* **Zero Runtime Cost**: All verification constructs (refinements, invariants, assertions, ghost variables) are statically checked and compile to zero-overhead runtime layouts.
* **C ABI Compatibility**: The memory layout of all structures matches the platform C ABI. Vera can call and be called by foreign C code directly.
* **Sound Memory Safety**: Hybrid model combining a compile-time static borrow checker with SMT-verified pointer arithmetic inside `unsafe` blocks.
* **Mathematical Soundness**: Automatic checks to prevent vacuous proofs, quantifier matching loops, and unsound memory alias assumptions.

---

## 2. Lexical Grammar

### 2.1 Identifiers
Identifiers are sequences of one or more alphabetic characters, digits, and underscores, starting with an alphabetic character or underscore:
`IDENTIFIER ::= [a-zA-Z_][a-zA-Z0-9_]*`

### 2.2 Keywords
The following tokens are reserved keywords in Vera and cannot be used as identifiers:
* **Execution**: `const`, `var`, `func`, `pure`, `return`, `if`, `else`, `while`, `for`, `in`, `match`, `case`, `struct`, `variant`, `enum`, `unsafe`, `impl`, `type`
* **Verification**: `spec`, `requires`, `ensures`, `assigns`, `invariant`, `decreases`, `forall`, `exists`, `choose`, `assert`, `assume`, `check`, `ghost`, `self`, `old`, `result`, `valid`, `valid_read`, `separated`

### 2.3 Operators
* **Arithmetic**: `+`, `-`, `*`, `/`, `%`
* **Bitwise / Logical**: `&`, `|`, `^`, `<<`, `>>`, `~`
* **Boolean**: `&&`, `||`, `!`
* **Verification Logic**: `==>` (implication), `<==>` (equivalence)
* **Comparison**: `==`, `!=`, `<`, `<=`, `>`, `>=`
* **Assignment**: `=`

---

## 3. Type System

Vera's type system consists of primitives, compounds, references, raw pointers, variants, enums, and refinement types.

### 3.1 Primitive Types
| Type | Representation | Bit Width | Semantics |
|---|---|---|---|
| `bool` | Boolean | 8 | `true` or `false` |
| `i8`, `i16`, `i32`, `i64` | Signed Integer | 8, 16, 32, 64 | Checked overflow at runtime |
| `u8`, `u16`, `u32`, `u64` | Unsigned Integer | 8, 16, 32, 64 | Checked overflow at runtime |
| `w8`, `w16`, `w32`, `w64` | Wrapping Integer | 8, 16, 32, 64 | Wrapping (modular) arithmetic |
| `f32`, `f64` | IEEE 754 Float | 32, 64 | Standard floating-point |
| `char` | ASCII Character | 8 | Numeric ASCII value |
| `void` | Unit Type | 0 | Empty return value |

*Note: In specifications, all arithmetic on signed and unsigned integers is modeled using infinite mathematical integers ($\mathbb{Z}$), while wrapping integers `wN` are modeled using bitvector arithmetic.*

### 3.2 Compound Types

#### 3.2.1 Structs
Structs are nominal product types with named fields. Struct layouts conform exactly to the host platform's C struct packing and alignment rules.
```vera
@abi(C)
struct Point {
    x: f64,
    y: f64,
}
```

#### 3.2.2 Arrays
Fixed-size arrays are declared as `array[T, N]`, where `T` is the type and `N` is a compile-time constant u64. Their runtime representation is a contiguous sequence of `N` elements of type `T`.
```vera
const data: array[i32, 5] = [1, 2, 3, 4, 5];
```

#### 3.2.3 Slices
Slices represent a borrow-checked, read-only or mutable view into a contiguous sequence of elements. They are written as `slice[T]` and `mut slice[T]`. At runtime, a slice is represented as a C-compatible fat pointer struct:
```c
struct Slice {
    void* ptr;
    uint64_t len;
};
```

### 3.3 References vs. Raw Pointers
Vera strictly delineates borrow-checked safe references from unsafe raw pointers:

* `ref T`: Safe read-only reference. Guaranteed non-null, properly aligned, and active within the borrow-checker lifetime.
* `mut ref T`: Safe mutable reference. Guaranteed non-null, properly aligned, and exclusive (no aliasing).
* `ptr T`: Raw read-only pointer. Equivalent to `const T*` in C. May be null, dangling, or unaligned.
* `mut ptr T`: Raw mutable pointer. Equivalent to `T*` in C. May be null, dangling, or unaligned.

```vera
func read_val(r: ref i32): i32 {
    return *r;
}
```

### 3.4 Variants and Enums

#### 3.4.1 Enums
Enums are C-like enumerations mapping directly to numeric integer tags:
```vera
enum State {
    Idle,
    Running,
    Stopped
}
```

#### 3.4.2 Variants (Tagged Unions)
Variants represent algebraic sum types. They use square brackets `[T]` for generics. When decorated with `@abi(C)`, they are represented as a C structure containing a tag integer followed by a C union of payloads:
```vera
@abi(C)
variant Option[T] {
    None,
    Some(T)
}
```
Binary layout in C equivalent:
```c
struct Option {
    uint32_t tag;
    union {
        T Some;
    } payload;
};
```

### 3.4.3 The `Result` Type
For error handling, the standard library provides the `Result[T, E]` variant, which is also laid out as a C-compatible tagged union:
```vera
@abi(C)
variant Result[T, E] {
    Ok(T),
    Err(E)
}
```

### 3.5 Refinement Types
Refinement types restrict a base type by attaching a logic constraint that must always evaluate to `true` for values of this type. The constraint is declared using `T where (Predicate)` where `self` is an implicit identifier referencing the underlying value.

```vera
type Nat = i32 where (self >= 0);
type NonZero = i32 where (self != 0);

// Built-in string type is a refinement of a byte slice, guaranteeing valid UTF-8
type string = slice[u8] where (std.spec.is_utf8(self));
```

#### Verification and ABI:
* **Layout**: Refinement types are completely erased to their base type during code generation.
* **Proof Obligations**: Assigning a value to a variable of a refinement type, or passing an argument to a function parameter of a refinement type, generates an SMT assertion that the value satisfies the refinement predicate.

### 3.6 Traits and Generic Constraints
Vera uses **Traits** to define shared behavior and constrain generic type parameters.
```vera
trait Comparable {
    pure func less_than(self: ref Self, other: ref Self): bool;
}

// Constraining the generic parameter T
func is_sorted[T: Comparable](arr: slice[T]): bool
```
Operators like `<`, `>`, and `==` on generic types are syntactic sugar for these core trait methods. Traits are resolved at monomorphization time and compile to direct function calls with zero overhead.

---

## 4. Execution Semantics

### 4.1 Variables and Mutability
Variables are declared using `const` (immutable) or `var` (mutable):
```vera
const a = 10; // Immutable
var b = 20;   // Mutable
b = b + 5;
```

### 4.2 Functions
Functions are declared using the `func` keyword. Return types are declared after a colon `:`. 
```vera
func calculate(x: i32): i32 {
    return x * 2;
}
```
#### Pure Functions:
Functions marked `pure func` are mathematical and deterministic. They are restricted from:
1. Having side effects (no `assigns` clauses on caller memory).
2. Containing `unsafe` blocks or raw pointer dereferences.
3. Invoking any non-`pure` functions.
Pure functions are promoted into mathematical logic functions in the SMT context.

#### 4.2.1 Function Pointers and Closures
Vera supports passing functions as arguments using a **C ABI compatible fat pointer** design for closures.

* **Function Pointers** (no captured state): `func(i32): bool`. Maps directly to a C function pointer `bool (*)(int32_t)`.
* **Closures** (capturing state): To maintain C compatibility, closures are represented as a pair of pointers (a code pointer and an opaque environment pointer).
  ```c
  struct Closure_i32_bool {
      bool (*fn_ptr)(void* env, int32_t arg);
      void* env;
  };
  ```
When you pass a closure in Vera, the compiler automatically generates this C-compatible struct and passes the captured environment as the first `void*` argument, allowing Vera closures to be used as callbacks in foreign C code.

### 4.3 Control Flow

#### 4.3.1 Conditionally Branching (`if`-`else`)
`if`-`else` branches are expressions that can return a value. Both branches must evaluate to the same type.
```vera
const max = if a > b { a } else { b };
```

#### 4.3.2 Pattern Matching (`match`)
Pattern matching is used to destructure enums and variants. Pattern matching must be exhaustive.
```vera
const val = Option.Some(42);
const num = match val {
    case Some(x) => x,
    case None => 0,
};
```

#### 4.3.3 Loops (`while` and `for`)
Loops require loop condition bounds checks and loop invariants to prove correctness. Specifications are placed in an external `spec` block between the header and loop body:
```vera
for i in 0..10
spec {
    invariant i <= 10;
}
{
    print_int(i);
}
```

#### 4.3.4 Error Propagation (`?`)
The `?` operator is used to propagate errors from `Result[T, E]` types. If the expression evaluates to `Ok(val)`, it unwraps `val`. If it evaluates to `Err(e)`, the current function immediately returns `Err(e)`.
```vera
func read_config(): Result[Config, Error] {
    const file = open("config.toml")?; // Returns early if open() fails
    return parse_config(file);
}
```
*Verification Semantics*: The WP engine automatically generates verification paths for both the success branch and the early-return branch (checking the `ensures` clause for the `Err` return).

### 4.4 Monomorphization-Time Verification of Generics
When verifying code with generic type parameters (e.g., `Option[T]`), the verification conditions (VCs) are generated and dispatched at **monomorphization time** for each instantiated concrete type. This allows the SMT solver to reason precisely about the bit widths, properties, and invariants of concrete types.

---

## 5. Verification Calculus & Memory Model

Vera generates SMT verification conditions using Weakest Precondition (WP) calculus.

### 5.1 Weakest Precondition (WP) Propagation Rules
Let $WP(S, P)$ denote the weakest precondition for statement $S$ to terminate in a state satisfying postcondition $P$.

* **Assignment**:
  $$WP(x = e, P) = P[e / x]$$
* **Sequence**:
  $$WP(S_1; S_2, P) = WP(S_1, WP(S_2, P))$$
* **Conditional**:
  $$WP(\text{if } c \{ S_1 \} \text{ else } S_2, P) = (c \implies WP(S_1, P)) \land (\neg c \implies WP(S_2, P))$$
* **Assertions & Assumptions**:
  $$WP(\text{assert } Q, P) = Q \land P$$
  $$WP(\text{assume } Q, P) = Q \implies P$$

### 5.2 Loop Invariants with Early Exits (`break` / `continue` / `return`)
Let a loop have invariant $I$ and termination variant $V$.
1. **Initiation**: The invariant $I$ must hold before the loop starts.
2. **Preservation**: Under the assumption that $I$ holds and the loop condition $c$ is true, the execution of the loop body must preserve $I$.
3. **Early Exit Invariant Preservation**:
   * If a loop contains a `break` statement, the loop invariant $I$ must hold immediately prior to the break. At the target block of the break, the loop invariant $I$ is assumed.
   * If a loop contains a `continue` statement, the loop invariant $I$ must hold before jumping back, and the termination measure $V$ must have strictly decreased.
   * If a loop contains a `return` statement, the function's overall postcondition (`ensures` clause) must be proved immediately at the return site.

### 5.3 Memory Safety and Borrow Checker
The borrow checker statically verifies that:
1. Safe references (`ref T`) never outlive the scope of their referents.
2. Mutable references (`mut ref T`) have exclusive access and cannot be aliased.

#### Safe Reference Assumption:
For any safe reference `r: ref T`, the verification engine automatically assumes the predicate `valid(r)` inside all contracts, eliminating the need to write redundant memory safety preconditions.

#### Raw Pointer Transition:
Converting safe references to raw pointers is always safe (e.g. `ref T` to `ptr T`). 
Converting raw pointers to safe references or dereferencing raw pointers is `unsafe` and requires generating explicit memory proof obligations:
```vera
func deref_int(p: ptr i32): i32
spec {
    requires p != null && valid(p);
}
{
    unsafe {
        return *p;
    }
}
```

### 5.4 Field-Level Framing Analysis
An `assigns` clause restricts which memory locations a function or loop is permitted to write to.
* **Struct Fields**: An assigns clause like `assigns self.offset` indicates that only the `offset` field of struct `self` may change. The verification engine automatically generates the frame condition:
  $$\forall \text{ field } f \neq \text{offset} :: \text{self}.f == \text{old}(\text{self}.f)$$
* **Full Struct**: Writing `assigns self[..]` or `assigns self` indicates all fields within the struct can be mutated.

### 5.5 Precondition Vacuity Checking
To prevent programmers from writing conflicting preconditions that make the function entry logically equivalent to `false` (which would vacuously validate any postcondition), the compiler enforces a **Vacuity Check**:
$$\exists x_1, x_2, \dots, x_n :: \text{Precondition}(x_1, \dots, x_n)$$
The compiler dispatches this query to the SMT solver. If the solver returns `unsat` (unsatisfiable), compilation is aborted with a `Vacuous Precondition Error`.

---

## 6. Ghost & Logic Sublanguage

### 6.1 Ghost Code
Ghost variables, parameters, and expressions are used exclusively for verification. The compiler completely erases ghost code during lowering to C/LLVM, guaranteeing zero runtime overhead.
```vera
func add_element(arr: mut ref LinkedList, val: i32, ghost expected_len: u64)
```

#### Ghost Blocks:
You can declare a block of ghost code using the `ghost { ... }` block inside normal functions:
```vera
ghost {
    var ghost_sum = 0;
    for i in 0..len {
        ghost_sum = ghost_sum + arr[i];
    }
    assert ghost_sum == mathematical_sum(len);
}
```

### 6.2 Logic Expressions and Quantifiers
Vera supports standard first-order logic operations:
* `==>`: Logical implication.
* `<==>`: Logical equivalence.
* `forall(x: T) { P(x) }` and range-bounded `forall(x in start..end) { P(x) }`.
* `exists(x: T) { P(x) }` and range-bounded `exists(x in start..end) { P(x) }`.
* `choose(x: T) { P(x) }`: Evaluates to a unique value satisfying $P(x)$, assuming one exists.

### 6.3 Mandatory Logic Termination (`decreases`)
Unbounded recursion in logic functions can trigger infinite matching loops in SMT solvers, hanging verification.
**Rule**: Any recursive logic or ghost function must specify a `decreases` clause containing a well-founded variant expression (an expression that decreases toward a lower bound, e.g. toward 0):
```vera
ghost func power(base: u64, exp: u64): u64
spec {
    decreases exp;
}
{
    if exp == 0 { 1 } else { base * power(base, exp - 1) }
}
```

### 6.4 Automatic Contract Predicates
For any function `f`, the compiler automatically generates two logic namespace predicates representing its contract:
1. `f.requires(args)`: The preconditions of `f`.
2. `f.ensures(args, result)`: The postconditions of `f`.
These predicates can be reused in other contracts for contract inheritance and composition.

---

## 7. Standard Specification Library (`std.spec`)

The `std.spec` module provides predefined logic helper functions and predicates for common mathematical constraints:

### 7.1 Array and Slice Predicates
* `std.spec.is_sorted[T](slice: slice[T]): bool`
  * Returns true if all elements are sorted in non-decreasing order.
* `std.spec.permutation[T](a: slice[T], b: slice[T]): bool`
  * Returns true if slice `a` is a mathematical permutation of slice `b`.
* `std.spec.all_distinct[T](slice: slice[T]): bool`
  * Returns true if no two elements in the slice are equal.
* `std.spec.contains[T](slice: slice[T], value: T): bool`
  * Returns true if `value` exists in the slice.

### 7.2 Memory Helper Predicates
* `valid[T](ptr: ptr T)`: Returns true if pointer is valid for reading and writing.
* `valid_read[T](ptr: ptr T)`: Returns true if pointer is valid for reading.
* `separated[T](ptr1: ptr T, ptr2: ptr T)`: Returns true if the memory ranges do not overlap.

---

## 8. Module and Import System

Vera organizes code into modules based on the file system. Every `.vera` file implicitly defines a module.

### 8.1 Visibility
By default, all functions, types, and variables are **private** to their module. To expose them, use the `pub` keyword:
```vera
pub struct Node {
    pub value: i32,
    pub next: mut ptr Node,
}

pub func create_node(): Node { ... }
```

### 8.2 Importing
Use the `import` keyword to bring items into scope.
```vera
// Import an entire module
import std.spec;

// Import specific items
import std.collections.{LinkedList, HashMap};

// Import with an alias
import std.io as io;
```
