use crate::hir::{HirFunc, HirStmt, HirExpr, HirType, BinaryOp};
use super::smt::{SmtExpr, check_sat};
use super::VerificationError;

pub fn verify_func(func: &HirFunc) -> Result<(), VerificationError> {
    // 1. Generate Weakest Precondition for the function body
    let mut current_wp = SmtExpr::BoolConst(true); // Base case for end of function

    // If there are ensures clauses, the end of the function must satisfy them.
    // For now, let's just AND them together.
    for ens in &func.ensures {
        current_wp = SmtExpr::And(Box::new(current_wp), Box::new(hir_to_smt(ens)));
    }

    // 2. Compute WP backwards through statements
    for stmt in func.body.statements.iter().rev() {
        current_wp = compute_wp(stmt, current_wp);
    }

    // 3. Add requires clauses as preconditions
    let mut precondition = SmtExpr::BoolConst(true);
    for req in &func.requires {
        precondition = SmtExpr::And(Box::new(precondition), Box::new(hir_to_smt(req)));
    }

    // 4. Final Verification Condition (VC): Requires => WP
    let vc = SmtExpr::Implies(Box::new(precondition), Box::new(current_wp));

    // 5. To prove VC is valid, we check if (NOT VC) is satisfiable
    let query = SmtExpr::Not(Box::new(vc));

    // 6. Invoke Z3
    let is_sat = check_sat(&query)?;

    if is_sat {
        Err(VerificationError::ProofFailed(format!("Function '{}' failed verification.", func.name)))
    } else {
        Ok(()) // Unsat means valid
    }
}

fn compute_wp(stmt: &HirStmt, post: SmtExpr) -> SmtExpr {
    match stmt {
        HirStmt::Assert(expr) => {
            // WP(assert Q, P) = Q && P
            SmtExpr::And(Box::new(hir_to_smt(expr)), Box::new(post))
        }
        HirStmt::Assume(expr) => {
            // WP(assume Q, P) = Q => P
            SmtExpr::Implies(Box::new(hir_to_smt(expr)), Box::new(post))
        }
        HirStmt::Let(name, _, _, init) => {
            // WP(x = E, P) = P[E/x]
            post.substitute(name, &hir_to_smt(init))
        }
        HirStmt::Expr(expr) => {
            // Handle reassignment: `x = E` in HIR is Expr(BinaryOp(Assign, VarRef(x), E))
            // WP(x = E, P) = P[E/x]
            if let HirExpr::BinaryOp(BinaryOp::Assign, lhs, rhs, _) = expr {
                if let HirExpr::VarRef(name, _) = lhs.as_ref() {
                    return post.substitute(name, &hir_to_smt(rhs));
                }
            }
            // Non-assignment expressions (e.g., side-effect-free calls) don't affect WP.
            post
        }
        HirStmt::Return(_opt_expr) => {
            // TODO: handle return values in WP
            post
        }
        HirStmt::Error => post,
    }
}

pub(crate) fn hir_to_smt(expr: &HirExpr) -> SmtExpr {
    match expr {
        HirExpr::IntLiteral(v, _) => SmtExpr::IntConst(*v),
        HirExpr::BoolLiteral(v, _) => SmtExpr::BoolConst(*v),
        HirExpr::VarRef(name, _) => SmtExpr::Var(name.clone()),
        HirExpr::Call(name, _, ty) => {
            // For Phase 5, we treat function calls as opaque values in WP.
            // In the future, we will use uninterpreted functions or inline contracts.
            if ty == &HirType::I32 {
                SmtExpr::Var(format!("__call_{}", name)) // Treat as an arbitrary variable
            } else {
                SmtExpr::BoolConst(false)
            }
        }
        HirExpr::BinaryOp(op, lhs, rhs, _) => {
            let lhs_smt = Box::new(hir_to_smt(lhs));
            let rhs_smt = Box::new(hir_to_smt(rhs));
            match op {
                BinaryOp::Add => SmtExpr::Add(lhs_smt, rhs_smt),
                BinaryOp::Sub => SmtExpr::Sub(lhs_smt, rhs_smt),
                BinaryOp::Mul => SmtExpr::Mul(lhs_smt, rhs_smt),
                BinaryOp::Eq => SmtExpr::Eq(lhs_smt, rhs_smt),
                BinaryOp::Neq => SmtExpr::Not(Box::new(SmtExpr::Eq(lhs_smt, rhs_smt))),
                BinaryOp::Lt => SmtExpr::Lt(lhs_smt, rhs_smt),
                BinaryOp::Gt => SmtExpr::Gt(lhs_smt, rhs_smt),
                BinaryOp::Le => SmtExpr::Le(lhs_smt, rhs_smt),
                BinaryOp::Ge => SmtExpr::Ge(lhs_smt, rhs_smt),
                BinaryOp::And => SmtExpr::And(lhs_smt, rhs_smt),
                BinaryOp::Or => SmtExpr::Or(lhs_smt, rhs_smt),
                _ => SmtExpr::BoolConst(false), // fallback
            }
        }
        _ => SmtExpr::BoolConst(false), // fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::{HirType, HirExpr, HirStmt, BinaryOp};
    use crate::verification::smt::SmtExpr;

    // ------------------------------------------------------------------
    // compute_wp
    // ------------------------------------------------------------------

    /// WP(assert Q, P) = Q && P
    #[test]
    fn test_wp_assert() {
        // WP(assert x > 0, true) = (x > 0) && true
        let q = HirExpr::BinaryOp(
            BinaryOp::Gt,
            Box::new(HirExpr::VarRef("x".into(), HirType::I32)),
            Box::new(HirExpr::IntLiteral(0, HirType::I32)),
            HirType::Bool,
        );
        let post = SmtExpr::BoolConst(true);
        let wp = compute_wp(&HirStmt::Assert(q), post);
        assert_eq!(wp.to_smtlib2(), "(and (> x 0) true)");
    }

    /// WP(assume Q, P) = Q => P
    #[test]
    fn test_wp_assume() {
        // WP(assume x > 0, true) = (x > 0) => true
        let q = HirExpr::BinaryOp(
            BinaryOp::Gt,
            Box::new(HirExpr::VarRef("x".into(), HirType::I32)),
            Box::new(HirExpr::IntLiteral(0, HirType::I32)),
            HirType::Bool,
        );
        let post = SmtExpr::BoolConst(true);
        let wp = compute_wp(&HirStmt::Assume(q), post);
        assert_eq!(wp.to_smtlib2(), "(=> (> x 0) true)");
    }

    /// WP(let x = E, P) = P[E/x]
    #[test]
    fn test_wp_let_substitution() {
        // WP(const x = 5, x > 0) = 5 > 0
        let init = HirExpr::IntLiteral(5, HirType::I32);
        let post = SmtExpr::Gt(
            Box::new(SmtExpr::Var("x".into())),
            Box::new(SmtExpr::IntConst(0)),
        );
        let wp = compute_wp(&HirStmt::Let("x".into(), true, HirType::I32, init), post);
        assert_eq!(wp.to_smtlib2(), "(> 5 0)");
    }

    /// WP(x = E, P) = P[E/x] — exercises the bug fix for assignment via HirStmt::Expr.
    #[test]
    fn test_wp_assignment_substitution() {
        // WP(x = 10, x == 10) should give: 10 == 10  (i.e., true)
        let assign = HirExpr::BinaryOp(
            BinaryOp::Assign,
            Box::new(HirExpr::VarRef("x".into(), HirType::I32)),
            Box::new(HirExpr::IntLiteral(10, HirType::I32)),
            HirType::I32,
        );
        let post = SmtExpr::Eq(
            Box::new(SmtExpr::Var("x".into())),
            Box::new(SmtExpr::IntConst(10)),
        );
        let wp = compute_wp(&HirStmt::Expr(assign), post);
        // After substitution x -> 10 in (= x 10): (= 10 10)
        assert_eq!(wp.to_smtlib2(), "(= 10 10)");
    }

    // ------------------------------------------------------------------
    // hir_to_smt
    // ------------------------------------------------------------------

    /// IntLiteral lowers to IntConst.
    #[test]
    fn test_hir_to_smt_int_literal() {
        let expr = HirExpr::IntLiteral(42, HirType::I32);
        assert_eq!(hir_to_smt(&expr).to_smtlib2(), "42");
    }

    /// BoolLiteral lowers to BoolConst.
    #[test]
    fn test_hir_to_smt_bool_literal() {
        assert_eq!(hir_to_smt(&HirExpr::BoolLiteral(true, HirType::Bool)).to_smtlib2(), "true");
        assert_eq!(hir_to_smt(&HirExpr::BoolLiteral(false, HirType::Bool)).to_smtlib2(), "false");
    }

    /// VarRef lowers to Var with the same name.
    #[test]
    fn test_hir_to_smt_var_ref() {
        let expr = HirExpr::VarRef("my_var".into(), HirType::I32);
        assert_eq!(hir_to_smt(&expr).to_smtlib2(), "my_var");
    }

    /// BinaryOp::Add lowers to SmtExpr::Add.
    #[test]
    fn test_hir_to_smt_add() {
        let expr = HirExpr::BinaryOp(
            BinaryOp::Add,
            Box::new(HirExpr::IntLiteral(1, HirType::I32)),
            Box::new(HirExpr::IntLiteral(2, HirType::I32)),
            HirType::I32,
        );
        assert_eq!(hir_to_smt(&expr).to_smtlib2(), "(+ 1 2)");
    }

    /// BinaryOp::Lt lowers to SmtExpr::Lt.
    #[test]
    fn test_hir_to_smt_comparison() {
        let expr = HirExpr::BinaryOp(
            BinaryOp::Lt,
            Box::new(HirExpr::VarRef("x".into(), HirType::I32)),
            Box::new(HirExpr::IntLiteral(5, HirType::I32)),
            HirType::Bool,
        );
        assert_eq!(hir_to_smt(&expr).to_smtlib2(), "(< x 5)");
    }
}
