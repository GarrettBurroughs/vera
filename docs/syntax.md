# Vera Syntax Reference

This document describes the syntax, grammar rules, and core constructs of the Vera programming language. 

Vera uses a modern, expression-oriented syntax but keeps its execution model and memory layout aligned with C to ensure zero-cost abstractions and C ABI compatibility.

---

## 1. Types

Vera divides types into **primitives**, **compounds**, **pointers/references**, and **enumerations/variants**.

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
    next: mut ptr Node,
}
```

### Arrays & Slices
- **Fixed-size Arrays**: Written as `array[T, N]` where `T` is the type and `N` is a compile-time constant size. They are contiguous in memory.
- **Slices**: Written as `slice[T]` or `mut slice[T]`. Slices are fat pointers containing a raw pointer to the start element and a `u64` length.

```vera
const arr: array[i32, 5] = [1, 2, 3, 4, 5];
const view: slice[i32] = arr[1..4]; // Slice of elements 1, 2, 3
```

### Enums & Variants (Algebraic Data Types)
Vera distinguishes between simple integer-mapped enumerations and rich tagged unions (variants).

#### Enums
These map directly to integer types under the C ABI.
```vera
enum Color {
    Red,
    Green,
    Blue
}
```

#### Variants (Tagged Unions)
These carry payload data. To guarantee compatibility with external C/C++ libraries, the compiler compiles them using a strict layout mapping. By applying the `@abi(C)` attribute, they are laid out as a C struct containing an integer tag followed by a union of all possible payloads:

```vera
@abi(C)
variant Option[T] {
    None,
    Some(T)
}

@abi(C)
variant Shape {
    Circle(f64), // Radius
    Rectangle { width: f64, height: f64 }
}

@abi(C)
variant Result[T, E] {
    Ok(T),
    Err(E)
}
```

##### C ABI Binary Equivalence
The compiler translates the `Shape` variant into the following platform-compatible C structure:
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
Refinement types are declared using the `T where (Predicate)` syntax, where `self` is an implicit identifier referencing the underlying value:

```vera
// Nominal refinement type definitions
type Nat = i32 where (self >= 0);
type NonZero = i32 where (self != 0);
type string = slice[u8] where (std.spec.is_utf8(self));

// An inline refinement type used in a function parameter
func safe_divide(num: i32, den: i32 where (self != 0)): i32 {
    return num / den; // No division-by-zero check needed
}
```

##### Array and Slice Bounds
Refinement types are especially powerful for array and slice indexing, eliminating bounds-checking contracts:
```vera
func get_element(items: slice[i32], idx: u64 where (self < items.len())): i32 {
    return items[idx]; // Guaranteed safe, no manual 'requires idx < items.len()' contract needed!
}
```

##### Compilation and Verification Semantics
* **C ABI Layout**: Refinement types are completely erased to their base type during compilation. At runtime, `Nat` is just a standard 32-bit signed C integer (`int32_t`). It has zero memory or execution overhead.
* **Proof Obligations**: The compiler generates an SMT verification condition (VC) at every assignment or function call site where a value is cast to a refinement type.
  ```vera
  const a: i32 = get_input();
  // const b: Nat = a; // Verification error: cannot prove a >= 0
  
  if a >= 0 {
      const b: Nat = a; // Compiles successfully: compiler proves a >= 0 holds here
  }
  ```

#### Traits and Constraints
Generic types can be constrained using traits:
```vera
trait Display {
    func to_string(self: ref Self): string;
}

func print_item[T: Display](item: ref T) {
    const s = item.to_string();
    print_str(s);
}
```

---

## 2. Variables and Mutability

By default, variables in Vera are **immutable** and declared with `const`. To allow mutation, you must use `var`. This makes code easier to analyze for verification since most variables behave like static single assignments (SSA).

```vera
const x: i32 = 42; // Immutable
// x = 10; -> Compile error: cannot assign to immutable variable 'x'

var y: i32 = 10; // Mutable
y = y + 5; // Valid
```

Type inference is supported:
```vera
const a = 5; // Inferred as default integer type i32
const b = true; // Inferred as bool
```

---

## 3. Functions

Functions are defined with the `func` keyword. Parameters require explicit types. The return type is specified after a colon `:` (defaults to `void` if omitted).

```vera
func add(a: i32, b: i32): i32 {
    return a + b;
}
```

### C ABI Compatibility and Externs
To interact with C libraries, you can declare external functions using the `extern` block and annotate Vera functions to export them to C.

```vera
// Declaring a C function to be called from Vera
extern "C" {
    func malloc(size: u64): mut ptr u8;
    func free(p: mut ptr u8);
}

// Exporting a Vera function to C
@export
func vera_sum(arr: ptr i32, len: u64): i32 {
    var sum = 0;
    var i = 0;
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

### Closures and Function Pointers
Vera supports closures and function pointers. Closures are implemented as C-compatible "fat pointers" (a pair of a function pointer and an environment pointer).
```vera
func map(arr: slice[i32], f: func(i32): i32) {
    for i in 0..arr.len() {
        arr[i] = f(arr[i]);
    }
}

// Usage with a closure capturing environment
const multiplier = 5;
map(arr, |x| x * multiplier);
```

---

## 4. Control Flow

Vera is expression-oriented. Blocks, `if`-`else` branches, and `match` statements can return values.

### If / Else
```vera
const condition = true;
const x = if condition {
    10
} else {
    20
};
```

### While Loop
Loop conditions must be of type `bool`.
```vera
var i = 0;
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
Match statements are used to destructure enums and variants. The compiler enforces exhaustiveness checking.
```vera
const val = Option.Some(42);

const result = match val {
    case Some(x) => x,
    case None => 0,
};
```

You can match on ranges, wildcards (`_`), and bind variables:
```vera
const score = 85;
match score {
    case 90..=100 => print_str("Grade: A"),
    case 80..90   => print_str("Grade: B"),
    case _        => print_str("Other"),
}
```

### Error Propagation (`?`)
The `?` operator unwraps a `Result::Ok` or propagates an `Err`.
```vera
func parse_file(path: string): Result[Data, Error] {
    const file = open_file(path)?; // Returns early with Err if it fails
    return Ok(parse(file));
}
```

---

## 5. Pointer Types: Borrowed vs. Raw

To maintain compatibility with the C memory layout while keeping proofs simple, Vera has two pointer categories:

1. **References (`ref T` and `mut ref T`)**: 
   - Managed by the compiler's static borrow checker.
   - Guaranteed to be non-null and not aliased (if mutable).
   - In contracts, references are assumed to always be valid, eliminating the need to write `valid(...)` assertions.
   - Representation: Compiles down to standard C pointers (`T*`).

2. **Raw Pointers (`ptr T` and `mut ptr T`)**:
   - Equivalent to C pointers (`const T*` and `T*`).
   - Can be null, unaligned, or dangling.
   - Operations on raw pointers (like dereferencing or arithmetic) must reside inside an `unsafe` block.
   - In contracts, they require explicit verification assertions (e.g., `valid(ptr)` and `separated(ptr1, ptr2)`).

```vera
func read_value(r: ref i32): i32 {
    return *r; // Safe: guaranteed valid by the compiler
}

func unsafe_read_value(p: ptr i32): i32
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

---

## 6. Modules and Imports
Every file is a module. Use `import` to bring items into scope and `pub` to expose them.

```vera
import std.fs.File;
import std.io as io;

pub struct Config {
    pub path: string,
}

pub func load_config(): Result[Config, io.Error] { ... }
```
