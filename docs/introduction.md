# Introduction to Vera

**Vera** is a verification-driven systems programming language designed to build safe, high-performance, and mathematically correct software. It combines the low-level efficiency and C ABI compatibility of C with the formal verification power of ACSL (ANSI/ISO C Specification Language) and Frama-C/WP, but with contracts natively integrated as first-class, elegant constructs of the language.

Vera is designed to be **self-hosting** and supports a robust **Language Server Protocol (LSP)** out of the box to enable seamless developer environments.

---

## Core Philosophy

1. **Native Contracts (Verification-First)**: Unlike tools like Frama-C where specifications are written inside comments (`/*@ ... */`), Vera treats contracts, invariants, preconditions, and postconditions as core language statements. They are checked by the compiler during the verification phase and compiled out in release builds.
2. **C ABI Compatibility**: Vera compiles directly to machine code or C, conforming fully to the C ABI. It can call and be called by C code without marshalling overhead.
3. **Sound Memory Safety**: Vera implements a hybrid memory safety model. It offers a static borrow-checker for safe reference types (enabling highly automated SMT proofs) and raw pointers with ACSL-like separation logic constraints for low-level system interactions.
4. **Self-Hostable**: The language features are rich enough to write its own parser, typechecker, SMT translator, and compiler.
5. **IDE-First**: The syntax, parser, and type-checker are designed with error recovery and incremental compilation in mind, ensuring an active and helpful Language Server.

---

## Hello, Vera!

Here is a quick look at Vera. This example calculates the greatest common divisor (GCD) of two integers, with native verification contracts guaranteeing that the returned value is a common divisor.

```vera
/// Computes the Greatest Common Divisor of two non-negative integers.
func gcd(a: u32, b: u32): u32
spec {
    requires a > 0 || b > 0;
    ensures result > 0;
    ensures a % result == 0 && b % result == 0;
}
{
    var x = a;
    var y = b;

    while y != 0
    spec {
        // Loop invariants ensure that the SMT solver can prove the postconditions
        invariant x > 0 || y > 0;
        invariant forall(d: u32) { 
            (d > 0 && a % d == 0 && b % d == 0) ==> (x % d == 0 && y % d == 0) 
        };
        decreases y;
    }
    {
        const temp = y;
        y = x % y;
        x = temp;
    }

    return x;
}
```

### Why Native Contracts Matter

In traditional languages, specifications are either written in separate files or as structured comments. This leads to several problems:
- **Syntax Highlighting & Formatting**: IDEs treat comments as plain text or require custom plugins. In Vera, contract expressions are part of the grammar, sharing the same parser and syntax highlighter.
- **Type Checking**: Vera's type checker validates specification expressions. If you change a variable name, the compiler will catch references to the old name in the contracts.
- **Refactoring**: Standard LSP rename/refactor operations automatically update contracts.

---

## Core Targets of the Vera Language

- **Preconditions (`requires`)**: Statements that must be true before a function is invoked. The caller must prove them.
- **Postconditions (`ensures`)**: Statements that are guaranteed to be true when the function returns. The function implementation must prove them.
- **Frame Conditions (`assigns` / `writes`)**: Declares which memory locations a function is allowed to modify, preventing unexpected side effects from breaking caller proofs.
- **Loop Invariants (`invariant`)**: Properties that hold true at the entry of a loop, after every iteration, and upon loop termination.
- **Loop Termination (`decreases`)**: A variant expression that strictly decreases with every iteration under a well-founded relation, proving the loop cannot run forever.
