# Vera Syntax Reference

This document describes the syntax, grammar rules, and core constructs of the Vera programming language. 

Vera uses a modern, expression-oriented syntax (similar to Rust) but keeps its execution model and memory layout aligned with C to ensure zero-cost abstractions and C ABI compatibility.

---

## 1. Types

Vera divides types into **primitives**, **compounds**, **pointers/references**, and **enumerations**.

| Vera Type | Description | C Equivalent | Width (bits) |
|---|---|---|---|
| `bool` | Boolean value (`true` or `false`) | `_Bool` (C99) / `bool` | 8 |
| `i8` | Signed 8-bit integer | `int8_t` | 8 |
| `i16` | Signed 16-bit integer | `int16_t` | 16 |
| `i32` | Signed 32-bit integer | `int32_t` | 32 |
| `i64` | Signed 64-bit integer | `int64_t` | 64 |
| `u8` | Unsigned 8-bit integer | `uint8_t` | 8 |
| `u16` | Unsigned 16-bit integer | `uint16_t` | 16 |
| `u32` | Unsigned 32-bit integer | `uint32_t` | 32 |
| `u64` | Unsigned 64-bit integer | `uint64_t` | 64 |
| `w8` | Wrapping unsigned 8-bit integer | `uint8_t` | 8 |
| `w16` | Wrapping unsigned 16-bit integer | `uint16_t` | 16 |
| `w32` | Wrapping unsigned 32-bit integer | `uint32_t` | 32 |
| `w64` | Wrapping unsigned 64-bit integer | `uint64_t` | 64 |
| `f32` | Single-precision float | `float` | 32 |
| `f64` | Double-precision float | `double` | 64 |
| `char` | ASCII character | `char` | 8 |
| `void` | Empty/unit type | `void` | 0 |


### Structs
Structs are nominal types with named fields. By default, structs are laid out in memory exactly like C structs (obeying field alignment and padding).

```vera
struct Point {
    x: f64,
    y: f64,
}

struct Node {
    value: i32,
    next: *mut Node,
}
```

### Arrays & Slices
- **Fixed-size Arrays**: Written as `[T; N]` where `N` is a compile-time constant. They are contiguous in memory.
- **Slices**: Written as `&[T]` or `&mut [T]`. Slices are fat pointers containing a pointer to the start element and a `u64` length.

```vera
let arr: [i32; 5] = [1, 2, 3, 4, 5];
let slice: &[i32] = &arr[1..4]; // Pointer to arr[1], length 3
```

### Enums & Algebraic Data Types (ADTs)
Vera supports both traditional C-like enums and tagged unions (ADTs).

#### C-Like Enums
These map directly to integer types under the C ABI.
```vera
enum Color {
    Red,
    Green,
    Blue,
}
```

#### Algebraic Data Types (Tagged Unions)
These carry payload data. To guarantee compatibility with external C/C++ libraries, the compiler compiles them using a strict layout mapping. By applying `#[repr(C)]`, they are laid out as a C struct containing an integer tag followed by a union of all possible payloads:

```vera
#[repr(C)]
enum Option<T> {
    None,
    Some(T),
}

#[repr(C)]
enum Shape {
    Circle(f64), // Radius
    Rectangle { width: f64, height: f64 },
}
```

##### C ABI Binary Equivalence
The compiler translates the `Shape` ADT into the following platform-compatible C structure:
```c
struct Shape {
    uint32_t tag;
    union {
        double Circle;
        struct {
            double width;
            double height;
        } Rectangle;
    } payload;
};
```
This guarantees that tag alignments, union sizes, and inner padding match the platform's native C compiler conventions exactly.

#### Refinement Types
Vera supports **Refinement Types**, which constrain a base type by a logical predicate that must always hold for values of that type. Refinement types allow shifting verification requirements directly into the type system.

##### Syntax
Refinement types are declared using the `{ x: T where P(x) }` syntax:
* `x` is a temporary variable representing the value.
* `T` is the base type (such as `i32` or a struct).
* `P(x)` is a boolean logic expression.

You can define refinement types inline or create type aliases using the `type` keyword:
```vera
// Nominal refinement type definitions
type Nat = { x: i32 where x >= 0 };
type NonZero = { x: i32 where x != 0 };

// An inline refinement type used in a function parameter
fn safe_divide(num: i32, den: { d: i32 where d != 0 }) -> i32 {
    return num / den; // No division-by-zero check needed
}
```

##### Array and Slice Bounds
Refinement types are especially powerful for array and slice indexing, eliminating bounds-checking contracts:
```vera
fn get_element(slice: &[i32], idx: { i: u64 where i < slice.len() }) -> i32 {
    return slice[idx]; // Guaranteed safe, no manual 'requires idx < slice.len()' contract needed!
}
```

##### Compilation and Verification Semantics
* **C ABI Layout**: Refinement types are completely erased to their base type during compilation. At runtime, `Nat` is just a standard 32-bit signed C integer (`int32_t`). It has zero memory or execution overhead.
* **Proof Obligations**: The compiler generates an SMT verification condition (VC) at every assignment or function call site where a value is cast to a refinement type.
  ```vera
  let a: i32 = get_input();
  // let b: Nat = a; // Compilation/Verification error: cannot prove a >= 0
  
  if a >= 0 {
      let b: Nat = a; // Compiles successfully: compiler proves a >= 0 holds here
  }
  ```

---

## 2. Variables and Mutability

By default, variables in Vera are **immutable**. To allow mutation, you must use the `mut` keyword. This makes code easier to analyze for verification since most variables behave like static single assignments (SSA).

```vera
let x: i32 = 42; // Immutable
// x = 10; -> Compile error: cannot assign to immutable variable 'x'

let mut y: i32 = 10; // Mutable
y = y + 5; // Valid
```

Type inference is supported:
```vera
let a = 5; // Inferred as default integer type i32
let b = true; // Inferred as bool
```

---

## 3. Functions

Functions are defined with the `fn` keyword. Parameters require explicit types. The return type is specified after `->` (defaults to `void` if omitted).

```vera
fn add(a: i32, b: i32) -> i32 {
    return a + b;
}
```

### C ABI Compatibility and Externs
To interact with C libraries, you can declare external functions using the `extern` block and annotate Vera functions to export them to C.

```vera
// Declaring a C function to be called from Vera
extern "C" {
    fn malloc(size: u64) -> *mut u8;
    fn free(ptr: *mut u8);
}

// Exporting a Vera function to C
#[no_mangle]
pub extern "C" fn vera_sum(arr: *const i32, len: u64) -> i32 {
    let mut sum = 0;
    let mut i = 0;
    while i < len {
        // Unsafe block is required to dereference raw pointers
        unsafe {
            sum = sum + *arr.add(i);
        }
        i = i + 1;
    }
    return sum;
}
```

---

## 4. Control Flow

Vera is expression-oriented. Blocks, `if`-`else` branches, and `match` statements can return values.

### If / Else
```vera
let condition = true;
let x = if condition {
    10
} else {
    20
};
```

### While Loop
Loop conditions must be of type `bool`.
```vera
let mut i = 0;
while i < 10 {
    i = i + 1;
}
```

### For Loop
For loops iterate over ranges or slices.
```vera
// Loop over 0 to 9 inclusive (0..10 is exclusive, 0..=10 is inclusive)
for i in 0..10 {
    print_int(i);
}
```

### Match (Pattern Matching)
Match statements are used to destructure enums and ADTs. The compiler enforces exhaustiveness checking.
```vera
let val = Option::Some(42);

let result = match val {
    Option::Some(x) => x,
    Option::None => 0,
};
```

You can match on ranges, wildcards (`_`), and bind variables:
```vera
let score = 85;
match score {
    90..=100 => print_str("Grade: A"),
    80..90   => print_str("Grade: B"),
    _        => print_str("Other"),
}
```

---

## 5. Pointer Types: Borrowed vs. Raw

To maintain compatibility with the C memory layout while keeping proofs simple, Vera has two pointer types:

1. **References (`&T` and `&mut T`)**: 
   - Managed by the compiler's static borrow checker.
   - Guaranteed to be non-null and not aliased (if mutable).
   - In contracts, references are assumed to always be valid, eliminating the need to write `valid(...)` assertions.
   - Representation: Compiles down to standard C pointers (`T*`).

2. **Raw Pointers (`*const T` and `*mut T`)**:
   - Equivalent to C pointers (`const T*` and `T*`).
   - Can be null, unaligned, or dangling.
   - Operations on raw pointers (like dereferencing or arithmetic) must reside inside an `unsafe` block.
   - In contracts, they require explicit verification assertions (e.g., `valid(ptr)` and `separated(ptr1, ptr2)`).

```vera
fn read_value(r: &i32) -> i32 {
    return *r; // Safe: guaranteed valid by the compiler
}

fn unsafe_read_value(p: *const i32) -> i32
spec {
    requires p != null;
    requires valid(p);
}
{
    unsafe {
        return *p; // Safe because of contracts and unsafe block
    }
}

```
