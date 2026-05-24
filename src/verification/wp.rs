use crate::hir::{HirFunc, HirBlock, HirStmt, HirExpr, HirType, BinaryOp};
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
        HirStmt::Expr(_expr) => {
            // Reassignment is represented as Expr(Assign(name, val)) in HIR?
            // Actually, HIR doesn't have assignment yet in phase 3.5, only 'let'.
            // So expressions don't affect WP.
            post
        }
        HirStmt::Return(_opt_expr) => {
            // TODO: handle return values in WP
            post
        }
        HirStmt::Error => post,
    }
}

fn hir_to_smt(expr: &HirExpr) -> SmtExpr {
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
