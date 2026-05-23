use std::process::Command;
use super::VerificationError;

#[derive(Debug, Clone)]
pub enum SmtExpr {
    BoolConst(bool),
    IntConst(i64),
    Var(String),
    Add(Box<SmtExpr>, Box<SmtExpr>),
    Sub(Box<SmtExpr>, Box<SmtExpr>),
    Mul(Box<SmtExpr>, Box<SmtExpr>),
    Eq(Box<SmtExpr>, Box<SmtExpr>),
    Lt(Box<SmtExpr>, Box<SmtExpr>),
    Gt(Box<SmtExpr>, Box<SmtExpr>),
    Le(Box<SmtExpr>, Box<SmtExpr>),
    Ge(Box<SmtExpr>, Box<SmtExpr>),
    And(Box<SmtExpr>, Box<SmtExpr>),
    Or(Box<SmtExpr>, Box<SmtExpr>),
    Implies(Box<SmtExpr>, Box<SmtExpr>),
    Not(Box<SmtExpr>),
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
            SmtExpr::Eq(l, r) => SmtExpr::Eq(Box::new(l.substitute(var_name, replacement)), Box::new(r.substitute(var_name, replacement))),
            SmtExpr::Lt(l, r) => SmtExpr::Lt(Box::new(l.substitute(var_name, replacement)), Box::new(r.substitute(var_name, replacement))),
            SmtExpr::Gt(l, r) => SmtExpr::Gt(Box::new(l.substitute(var_name, replacement)), Box::new(r.substitute(var_name, replacement))),
            SmtExpr::Le(l, r) => SmtExpr::Le(Box::new(l.substitute(var_name, replacement)), Box::new(r.substitute(var_name, replacement))),
            SmtExpr::Ge(l, r) => SmtExpr::Ge(Box::new(l.substitute(var_name, replacement)), Box::new(r.substitute(var_name, replacement))),
            SmtExpr::And(l, r) => SmtExpr::And(Box::new(l.substitute(var_name, replacement)), Box::new(r.substitute(var_name, replacement))),
            SmtExpr::Or(l, r) => SmtExpr::Or(Box::new(l.substitute(var_name, replacement)), Box::new(r.substitute(var_name, replacement))),
            SmtExpr::Implies(l, r) => SmtExpr::Implies(Box::new(l.substitute(var_name, replacement)), Box::new(r.substitute(var_name, replacement))),
            SmtExpr::Not(inner) => SmtExpr::Not(Box::new(inner.substitute(var_name, replacement))),
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
            SmtExpr::Eq(l, r) => format!("(= {} {})", l.to_smtlib2(), r.to_smtlib2()),
            SmtExpr::Lt(l, r) => format!("(< {} {})", l.to_smtlib2(), r.to_smtlib2()),
            SmtExpr::Gt(l, r) => format!("(> {} {})", l.to_smtlib2(), r.to_smtlib2()),
            SmtExpr::Le(l, r) => format!("(<= {} {})", l.to_smtlib2(), r.to_smtlib2()),
            SmtExpr::Ge(l, r) => format!("(>= {} {})", l.to_smtlib2(), r.to_smtlib2()),
            SmtExpr::And(l, r) => format!("(and {} {})", l.to_smtlib2(), r.to_smtlib2()),
            SmtExpr::Or(l, r) => format!("(or {} {})", l.to_smtlib2(), r.to_smtlib2()),
            SmtExpr::Implies(l, r) => format!("(=> {} {})", l.to_smtlib2(), r.to_smtlib2()),
            SmtExpr::Not(inner) => format!("(not {})", inner.to_smtlib2()),
        }
    }
}

/// Checks satisfiability of the given SMT expression by shelling out to Z3.
/// Returns Ok(true) if SAT, Ok(false) if UNSAT.
pub fn check_sat(expr: &SmtExpr) -> Result<bool, VerificationError> {
    // Collect variables to declare them
    let mut vars = std::collections::HashSet::new();
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
    let mut child = Command::new("z3")
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

fn collect_vars(expr: &SmtExpr, vars: &mut std::collections::HashSet<String>) {
    match expr {
        SmtExpr::Var(v) => { vars.insert(v.clone()); },
        SmtExpr::Add(l, r) | SmtExpr::Sub(l, r) | SmtExpr::Mul(l, r) |
        SmtExpr::Eq(l, r) | SmtExpr::Lt(l, r) | SmtExpr::Gt(l, r) |
        SmtExpr::Le(l, r) | SmtExpr::Ge(l, r) | SmtExpr::And(l, r) |
        SmtExpr::Or(l, r) | SmtExpr::Implies(l, r) => {
            collect_vars(l, vars);
            collect_vars(r, vars);
        }
        SmtExpr::Not(inner) => collect_vars(inner, vars),
        _ => {}
    }
}
