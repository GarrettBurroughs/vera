use std::path::PathBuf;
use std::collections::BTreeSet;
use std::process::Command;
use super::VerificationError;

/// Returns the path to the z3 binary.
///
/// Resolution order:
///   1. `<project root>/tools/z3/bin/z3`  (installed via scripts/install_z3.sh)
///   2. `z3` on the system PATH
fn z3_path() -> PathBuf {
    // Walk up from the compiled binary's location to find the repo root.
    // At test/run time this is typically the cargo workspace root.
    let local = {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.push("tools/z3/bin/z3");
        p
    };
    if local.exists() {
        return local;
    }
    PathBuf::from("z3")
}

#[derive(Debug, Clone)]
pub enum SmtExpr {
    BoolConst(bool),
    IntConst(i64),
    Var(String),
    Add(Box<SmtExpr>, Box<SmtExpr>),
    Sub(Box<SmtExpr>, Box<SmtExpr>),
    Mul(Box<SmtExpr>, Box<SmtExpr>),
    Div(Box<SmtExpr>, Box<SmtExpr>),
    Eq(Box<SmtExpr>, Box<SmtExpr>),
    Lt(Box<SmtExpr>, Box<SmtExpr>),
    Gt(Box<SmtExpr>, Box<SmtExpr>),
    Le(Box<SmtExpr>, Box<SmtExpr>),
    Ge(Box<SmtExpr>, Box<SmtExpr>),
    And(Box<SmtExpr>, Box<SmtExpr>),
    Or(Box<SmtExpr>, Box<SmtExpr>),
    Implies(Box<SmtExpr>, Box<SmtExpr>),
    Not(Box<SmtExpr>),
    /// Universal quantifier: `(forall ((var Int)) body)`
    #[allow(dead_code)] // Scaffolded for loop invariant quantifier support (Phase 2)
    Forall(String, Box<SmtExpr>),
    /// Existential quantifier: `(exists ((var Int)) body)`
    #[allow(dead_code)] // Scaffolded for loop invariant quantifier support (Phase 2)
    Exists(String, Box<SmtExpr>),
}

impl SmtExpr {
    pub fn substitute(&self, var_name: &str, replacement: &SmtExpr) -> SmtExpr {
        match self {
            SmtExpr::BoolConst(_) | SmtExpr::IntConst(_) => self.clone(),
            SmtExpr::Var(name) => {
                if name == var_name {
                    replacement.clone()
                } else {
                    self.clone()
                }
            }
            SmtExpr::Add(l, r) => SmtExpr::Add(Box::new(l.substitute(var_name, replacement)), Box::new(r.substitute(var_name, replacement))),
            SmtExpr::Sub(l, r) => SmtExpr::Sub(Box::new(l.substitute(var_name, replacement)), Box::new(r.substitute(var_name, replacement))),
            SmtExpr::Mul(l, r) => SmtExpr::Mul(Box::new(l.substitute(var_name, replacement)), Box::new(r.substitute(var_name, replacement))),
            SmtExpr::Div(l, r) => SmtExpr::Div(Box::new(l.substitute(var_name, replacement)), Box::new(r.substitute(var_name, replacement))),
            SmtExpr::Eq(l, r) => SmtExpr::Eq(Box::new(l.substitute(var_name, replacement)), Box::new(r.substitute(var_name, replacement))),
            SmtExpr::Lt(l, r) => SmtExpr::Lt(Box::new(l.substitute(var_name, replacement)), Box::new(r.substitute(var_name, replacement))),
            SmtExpr::Gt(l, r) => SmtExpr::Gt(Box::new(l.substitute(var_name, replacement)), Box::new(r.substitute(var_name, replacement))),
            SmtExpr::Le(l, r) => SmtExpr::Le(Box::new(l.substitute(var_name, replacement)), Box::new(r.substitute(var_name, replacement))),
            SmtExpr::Ge(l, r) => SmtExpr::Ge(Box::new(l.substitute(var_name, replacement)), Box::new(r.substitute(var_name, replacement))),
            SmtExpr::And(l, r) => SmtExpr::And(Box::new(l.substitute(var_name, replacement)), Box::new(r.substitute(var_name, replacement))),
            SmtExpr::Or(l, r) => SmtExpr::Or(Box::new(l.substitute(var_name, replacement)), Box::new(r.substitute(var_name, replacement))),
            SmtExpr::Implies(l, r) => SmtExpr::Implies(Box::new(l.substitute(var_name, replacement)), Box::new(r.substitute(var_name, replacement))),
            SmtExpr::Not(inner) => SmtExpr::Not(Box::new(inner.substitute(var_name, replacement))),
            // Quantifiers: do not substitute the bound variable.
            SmtExpr::Forall(bound, body) => {
                if bound == var_name {
                    self.clone() // bound variable shadows; no substitution inside
                } else {
                    SmtExpr::Forall(bound.clone(), Box::new(body.substitute(var_name, replacement)))
                }
            }
            SmtExpr::Exists(bound, body) => {
                if bound == var_name {
                    self.clone()
                } else {
                    SmtExpr::Exists(bound.clone(), Box::new(body.substitute(var_name, replacement)))
                }
            }
        }
    }

    pub fn to_smtlib2(&self) -> String {
        match self {
            SmtExpr::BoolConst(true) => "true".to_string(),
            SmtExpr::BoolConst(false) => "false".to_string(),
            SmtExpr::IntConst(v) => v.to_string(),
            SmtExpr::Var(name) => name.clone(),
            SmtExpr::Add(l, r) => format!("(+ {} {})", l.to_smtlib2(), r.to_smtlib2()),
            SmtExpr::Sub(l, r) => format!("(- {} {})", l.to_smtlib2(), r.to_smtlib2()),
            SmtExpr::Mul(l, r) => format!("(* {} {})", l.to_smtlib2(), r.to_smtlib2()),
            SmtExpr::Div(l, r) => format!("(div {} {})", l.to_smtlib2(), r.to_smtlib2()),
            SmtExpr::Eq(l, r) => format!("(= {} {})", l.to_smtlib2(), r.to_smtlib2()),
            SmtExpr::Lt(l, r) => format!("(< {} {})", l.to_smtlib2(), r.to_smtlib2()),
            SmtExpr::Gt(l, r) => format!("(> {} {})", l.to_smtlib2(), r.to_smtlib2()),
            SmtExpr::Le(l, r) => format!("(<= {} {})", l.to_smtlib2(), r.to_smtlib2()),
            SmtExpr::Ge(l, r) => format!("(>= {} {})", l.to_smtlib2(), r.to_smtlib2()),
            SmtExpr::And(l, r) => format!("(and {} {})", l.to_smtlib2(), r.to_smtlib2()),
            SmtExpr::Or(l, r) => format!("(or {} {})", l.to_smtlib2(), r.to_smtlib2()),
            SmtExpr::Implies(l, r) => format!("(=> {} {})", l.to_smtlib2(), r.to_smtlib2()),
            SmtExpr::Not(inner) => format!("(not {})", inner.to_smtlib2()),
            SmtExpr::Forall(var, body) => format!("(forall (({} Int)) {})", var, body.to_smtlib2()),
            SmtExpr::Exists(var, body) => format!("(exists (({} Int)) {})", var, body.to_smtlib2()),
        }
    }
}

/// Checks satisfiability of the given SMT expression by shelling out to Z3.
/// Returns Ok(true) if SAT, Ok(false) if UNSAT.
pub fn check_sat(expr: &SmtExpr) -> Result<bool, VerificationError> {
    // Collect variables to declare them; use BTreeSet for deterministic ordering.
    let mut vars = BTreeSet::new();
    collect_vars(expr, &mut vars);

    let mut smt_script = String::new();
    
    // For now, we assume all variables are integers. We would need type tracking in SmtExpr
    // to do this properly.
    for var in vars {
        smt_script.push_str(&format!("(declare-const {} Int)\n", var));
    }

    smt_script.push_str(&format!("(assert {})\n", expr.to_smtlib2()));
    smt_script.push_str("(check-sat)\n");

    // Write to a temporary file, or use stdin.
    // For simplicity, we can pass it via stdin to `z3 -in`.
    use std::io::Write;
    let mut child = Command::new(z3_path())
        .arg("-in")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| VerificationError::Z3Error(format!("Failed to spawn z3: {}", e)))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(smt_script.as_bytes())
             .map_err(|e| VerificationError::Z3Error(format!("Failed to write to z3 stdin: {}", e)))?;
    }

    let output = child.wait_with_output()
        .map_err(|e| VerificationError::Z3Error(format!("Failed to read z3 output: {}", e)))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    
    if stdout.contains("unsat") {
        Ok(false)
    } else if stdout.contains("sat") {
        Ok(true)
    } else {
        Err(VerificationError::Z3Error(format!("Unexpected Z3 output: {}", stdout)))
    }
}

pub(crate) fn collect_vars(expr: &SmtExpr, vars: &mut BTreeSet<String>) {
    match expr {
        SmtExpr::Var(v) => { vars.insert(v.clone()); },
        SmtExpr::Add(l, r) | SmtExpr::Sub(l, r) | SmtExpr::Mul(l, r) | SmtExpr::Div(l, r) |
        SmtExpr::Eq(l, r) | SmtExpr::Lt(l, r) | SmtExpr::Gt(l, r) |
        SmtExpr::Le(l, r) | SmtExpr::Ge(l, r) | SmtExpr::And(l, r) |
        SmtExpr::Or(l, r) | SmtExpr::Implies(l, r) => {
            collect_vars(l, vars);
            collect_vars(r, vars);
        }
        SmtExpr::Not(inner) => collect_vars(inner, vars),
        SmtExpr::Forall(bound, body) | SmtExpr::Exists(bound, body) => {
            // The bound variable is declared by the quantifier itself; don't add it as a free var.
            let mut inner_vars = vars.clone();
            collect_vars(body, &mut inner_vars);
            for v in inner_vars {
                if &v != bound {
                    vars.insert(v);
                }
            }
        }
        _ => {}
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // to_smtlib2 serialization
    // ------------------------------------------------------------------

    /// Boolean constants serialize to `"true"` and `"false"`.
    #[test]
    fn test_smt_bool_const() {
        assert_eq!(SmtExpr::BoolConst(true).to_smtlib2(), "true");
        assert_eq!(SmtExpr::BoolConst(false).to_smtlib2(), "false");
    }

    /// Integer constants serialize to their decimal string.
    #[test]
    fn test_smt_int_const() {
        assert_eq!(SmtExpr::IntConst(42).to_smtlib2(), "42");
        assert_eq!(SmtExpr::IntConst(0).to_smtlib2(), "0");
        assert_eq!(SmtExpr::IntConst(-7).to_smtlib2(), "-7");
    }

    /// Variables serialize to their name.
    #[test]
    fn test_smt_var() {
        assert_eq!(SmtExpr::Var("x".to_string()).to_smtlib2(), "x");
    }

    /// Arithmetic operators produce the expected S-expression forms.
    #[test]
    fn test_smt_arithmetic() {
        let add = SmtExpr::Add(Box::new(SmtExpr::IntConst(1)), Box::new(SmtExpr::IntConst(2)));
        assert_eq!(add.to_smtlib2(), "(+ 1 2)");

        let sub = SmtExpr::Sub(Box::new(SmtExpr::Var("a".into())), Box::new(SmtExpr::IntConst(3)));
        assert_eq!(sub.to_smtlib2(), "(- a 3)");

        let mul = SmtExpr::Mul(Box::new(SmtExpr::IntConst(2)), Box::new(SmtExpr::IntConst(3)));
        assert_eq!(mul.to_smtlib2(), "(* 2 3)");
    }

    /// Comparison operators produce the expected S-expression forms.
    #[test]
    fn test_smt_comparisons() {
        let lt = SmtExpr::Lt(Box::new(SmtExpr::Var("x".into())), Box::new(SmtExpr::IntConst(5)));
        assert_eq!(lt.to_smtlib2(), "(< x 5)");

        let ge = SmtExpr::Ge(Box::new(SmtExpr::IntConst(10)), Box::new(SmtExpr::Var("y".into())));
        assert_eq!(ge.to_smtlib2(), "(>= 10 y)");

        let eq = SmtExpr::Eq(Box::new(SmtExpr::Var("a".into())), Box::new(SmtExpr::Var("b".into())));
        assert_eq!(eq.to_smtlib2(), "(= a b)");
    }

    /// Logical connectives produce the expected S-expression forms.
    #[test]
    fn test_smt_logical() {
        let and = SmtExpr::And(
            Box::new(SmtExpr::BoolConst(true)),
            Box::new(SmtExpr::BoolConst(false)),
        );
        assert_eq!(and.to_smtlib2(), "(and true false)");

        let or = SmtExpr::Or(
            Box::new(SmtExpr::Var("p".into())),
            Box::new(SmtExpr::Var("q".into())),
        );
        assert_eq!(or.to_smtlib2(), "(or p q)");

        let not = SmtExpr::Not(Box::new(SmtExpr::BoolConst(true)));
        assert_eq!(not.to_smtlib2(), "(not true)");

        let imp = SmtExpr::Implies(
            Box::new(SmtExpr::Var("pre".into())),
            Box::new(SmtExpr::Var("post".into())),
        );
        assert_eq!(imp.to_smtlib2(), "(=> pre post)");
    }

    /// A compound expression serializes correctly (nested S-expressions).
    #[test]
    fn test_smt_compound() {
        // (not (= x (+ y 1)))
        let expr = SmtExpr::Not(Box::new(SmtExpr::Eq(
            Box::new(SmtExpr::Var("x".into())),
            Box::new(SmtExpr::Add(
                Box::new(SmtExpr::Var("y".into())),
                Box::new(SmtExpr::IntConst(1)),
            )),
        )));
        assert_eq!(expr.to_smtlib2(), "(not (= x (+ y 1)))");
    }

    // ------------------------------------------------------------------
    // substitute
    // ------------------------------------------------------------------

    /// Substituting a variable for an integer constant replaces the var.
    #[test]
    fn test_substitute_simple_var() {
        let expr = SmtExpr::Var("x".to_string());
        let result = expr.substitute("x", &SmtExpr::IntConst(5));
        assert_eq!(result.to_smtlib2(), "5");
    }

    /// Substituting into an expression leaves non-matching variables unchanged.
    #[test]
    fn test_substitute_leaves_others() {
        // (+ x y)[x := 3] = (+ 3 y)
        let expr = SmtExpr::Add(
            Box::new(SmtExpr::Var("x".into())),
            Box::new(SmtExpr::Var("y".into())),
        );
        let result = expr.substitute("x", &SmtExpr::IntConst(3));
        assert_eq!(result.to_smtlib2(), "(+ 3 y)");
    }

    /// Substitution propagates into nested sub-expressions.
    #[test]
    fn test_substitute_nested() {
        // (not (= x (+ x 1)))[x := 10] = (not (= 10 (+ 10 1)))
        let expr = SmtExpr::Not(Box::new(SmtExpr::Eq(
            Box::new(SmtExpr::Var("x".into())),
            Box::new(SmtExpr::Add(
                Box::new(SmtExpr::Var("x".into())),
                Box::new(SmtExpr::IntConst(1)),
            )),
        )));
        let result = expr.substitute("x", &SmtExpr::IntConst(10));
        assert_eq!(result.to_smtlib2(), "(not (= 10 (+ 10 1)))");
    }

    /// Constants are unchanged by substitution.
    #[test]
    fn test_substitute_on_const_is_noop() {
        let expr = SmtExpr::IntConst(42);
        let result = expr.substitute("x", &SmtExpr::IntConst(0));
        assert_eq!(result.to_smtlib2(), "42");
    }

    // ------------------------------------------------------------------
    // collect_vars
    // ------------------------------------------------------------------

    /// collect_vars finds all variable names in an expression.
    #[test]
    fn test_collect_vars() {
        // (and (< x y) (= z 0))
        let expr = SmtExpr::And(
            Box::new(SmtExpr::Lt(
                Box::new(SmtExpr::Var("x".into())),
                Box::new(SmtExpr::Var("y".into())),
            )),
            Box::new(SmtExpr::Eq(
                Box::new(SmtExpr::Var("z".into())),
                Box::new(SmtExpr::IntConst(0)),
            )),
        );
        let mut vars = BTreeSet::new();
        collect_vars(&expr, &mut vars);
        assert!(vars.contains("x"), "expected x in vars");
        assert!(vars.contains("y"), "expected y in vars");
        assert!(vars.contains("z"), "expected z in vars");
        assert_eq!(vars.len(), 3);
    }
}
