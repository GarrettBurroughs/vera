use crate::hir::{HirFunc, HirStmt, HirExpr, HirType, BinaryOp};
use super::smt::{SmtExpr, check_sat};
use super::VerificationError;

pub fn verify_func(func: &HirFunc) -> Result<(), VerificationError> {
    // 1. Generate Weakest Precondition for the function body
    let mut current_wp = SmtExpr::BoolConst(true); // Base case for end of function

    // If there are ensures clauses, the end of the function must satisfy them.
    for ens in &func.ensures {
        current_wp = SmtExpr::And(Box::new(current_wp), Box::new(hir_to_smt(ens)));
    }
    
    let ensures_wp = current_wp.clone();

    // 2. Compute WP backwards through statements
    for stmt in func.body.statements.iter().rev() {
        current_wp = compute_wp(stmt, current_wp, &ensures_wp);
    }

    // 3. Add requires clauses as preconditions
    let mut precondition = SmtExpr::BoolConst(true);
    for req in &func.requires {
        precondition = SmtExpr::And(Box::new(precondition), Box::new(hir_to_smt(req)));
    }

    // 3.5. Add parameter refinement constraints as preconditions
    for (p_name, p_ty) in &func.params {
        if let HirType::Refinement(_, cond) = p_ty {
            let cond_smt = hir_to_smt(cond).substitute("self", &SmtExpr::Var(p_name.clone()));
            precondition = SmtExpr::And(Box::new(precondition), Box::new(cond_smt));
        }
    }

    // 3.7. Precondition Vacuity Checking
    // Disallow contradictory preconditions that make the function trivially verifiable
    if !check_sat(&precondition)? {
        return Err(VerificationError::VacuousPrecondition(format!("Function '{}' has vacuous (unsatisfiable) preconditions.", func.name)));
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

fn compute_wp(stmt: &HirStmt, post: SmtExpr, ensures_wp: &SmtExpr) -> SmtExpr {
    match stmt {
        HirStmt::Assert(expr) => {
            wp_eval_expr(expr, "__dummy_assert", SmtExpr::And(Box::new(SmtExpr::Var("__dummy_assert".into())), Box::new(post)), ensures_wp)
        }
        HirStmt::Assume(expr) => {
            wp_eval_expr(expr, "__dummy_assume", SmtExpr::Implies(Box::new(SmtExpr::Var("__dummy_assume".into())), Box::new(post)), ensures_wp)
        }
        HirStmt::Let(name, _, ty, init) => {
            let mut post_sub = wp_eval_expr(init, name, post, ensures_wp);
            if let HirType::Refinement(_, cond) = ty {
                let cond_smt = hir_to_smt(cond).substitute("self", &SmtExpr::Var(name.clone()));
                post_sub = SmtExpr::And(Box::new(cond_smt), Box::new(post_sub));
            }
            post_sub
        }
        HirStmt::Expr(expr) => {
            if let HirExpr::BinaryOp(BinaryOp::Assign, lhs, rhs, _) = expr
                && let HirExpr::VarRef(name, ty) = lhs.as_ref() {
                    let mut post_sub = wp_eval_expr(rhs, name, post, ensures_wp);
                    if let HirType::Refinement(_, cond) = ty {
                        let cond_smt = hir_to_smt(cond).substitute("self", &SmtExpr::Var(name.clone()));
                        post_sub = SmtExpr::And(Box::new(cond_smt), Box::new(post_sub));
                    }
                    return post_sub;
                }
            wp_eval_expr(expr, "__dummy_expr", post, ensures_wp)
        }
        HirStmt::While(cond, body, invariants, decreases) => {
            let mut i_expr = SmtExpr::BoolConst(true);
            for inv in invariants {
                i_expr = SmtExpr::And(Box::new(i_expr), Box::new(hir_to_smt(inv)));
            }
            
            let mut mod_vars = std::collections::HashSet::new();
            for s in &body.statements {
                collect_modified_vars_stmt(s, &mut mod_vars);
            }
            
            let b_expr = hir_to_smt(cond);
            
            // compute_block_wp backwards
            let mut body_wp = i_expr.clone();
            
            if let Some(dec) = decreases {
                let d_expr = hir_to_smt(dec);
                let d0_var = SmtExpr::Var("___d0".into());
                let dec_cond = SmtExpr::FuncCall("<".into(), vec![d_expr.clone(), d0_var.clone()]);
                body_wp = SmtExpr::And(Box::new(body_wp), Box::new(dec_cond));
            }
            
            for s in body.statements.iter().rev() {
                body_wp = compute_wp(s, body_wp, ensures_wp);
            }
            
            if let Some(dec) = decreases {
                let d_expr = hir_to_smt(dec);
                let d0_eq = SmtExpr::FuncCall("=".into(), vec![SmtExpr::Var("___d0".into()), d_expr.clone()]);
                let d_pos = SmtExpr::FuncCall(">=".into(), vec![d_expr.clone(), SmtExpr::IntConst(0)]);
                
                body_wp = SmtExpr::And(
                    Box::new(d_pos),
                    Box::new(SmtExpr::Forall(vec![("___d0".into(), "Int".into())], Box::new(SmtExpr::Implies(Box::new(d0_eq), Box::new(body_wp)))))
                );
            }
            
            let preservation = SmtExpr::Implies(Box::new(b_expr.clone()), Box::new(body_wp));
            let exit = SmtExpr::Implies(Box::new(SmtExpr::Not(Box::new(b_expr))), Box::new(post));
            
            let mut quantified_body = SmtExpr::Implies(Box::new(i_expr.clone()), Box::new(SmtExpr::And(Box::new(preservation), Box::new(exit))));
            
            // Sort to ensure determinism in tests and SMT output
            let mut mod_vars_sorted: Vec<_> = mod_vars.into_iter().collect();
            mod_vars_sorted.sort();
            if !mod_vars_sorted.is_empty() {
                let bounds: Vec<(String, String)> = mod_vars_sorted.into_iter().map(|v| (v, "Int".to_string())).collect();
                quantified_body = SmtExpr::Forall(bounds, Box::new(quantified_body));
            }
            
            SmtExpr::And(Box::new(i_expr), Box::new(quantified_body))
        }
        HirStmt::For(_, _, _) => post,
        HirStmt::Break => post,
        HirStmt::Continue => post,
        HirStmt::Return(opt_expr) => {
            if let Some(expr) = opt_expr {
                wp_eval_expr(expr, "result", ensures_wp.clone(), ensures_wp)
            } else {
                ensures_wp.clone()
            }
        }
        HirStmt::Error => post,
        HirStmt::GhostBlock(block) => {
            let mut ghost_post = post;
            for s in block.statements.iter().rev() {
                ghost_post = compute_wp(s, ghost_post, ensures_wp);
            }
            ghost_post
        }
    }
}

fn wp_eval_expr(expr: &HirExpr, var_name: &str, post: SmtExpr, ensures_wp: &SmtExpr) -> SmtExpr {
    match expr {
        HirExpr::Try(inner, _) => {
            let tmp_name = format!("__try_tmp_{}", var_name);
            let tmp_var = SmtExpr::Var(tmp_name.clone());
            let is_ok = SmtExpr::FuncCall("is_ok".into(), vec![tmp_var.clone()]);
            let unwrap_ok = SmtExpr::FuncCall("unwrap_ok".into(), vec![tmp_var.clone()]);
            let unwrap_err = SmtExpr::FuncCall("unwrap_err".into(), vec![tmp_var.clone()]);
            let mk_err = SmtExpr::FuncCall("mk_err".into(), vec![unwrap_err]);
            
            let ok_branch = post.substitute(var_name, &unwrap_ok);
            let err_branch = ensures_wp.substitute("result", &mk_err);
            
            let branches = SmtExpr::And(
                Box::new(SmtExpr::Implies(Box::new(is_ok.clone()), Box::new(ok_branch))),
                Box::new(SmtExpr::Implies(Box::new(SmtExpr::Not(Box::new(is_ok))), Box::new(err_branch)))
            );
            
            wp_eval_expr(inner, &tmp_name, branches, ensures_wp)
        }
        HirExpr::Match(cond, arms, _) => {
            let tmp_cond = format!("__match_cond_{}", var_name);
            let cond_var = SmtExpr::Var(tmp_cond.clone());
            
            let mut final_wp = SmtExpr::BoolConst(true);
            for (pat, arm_expr) in arms {
                let (arm_cond, bindings) = pattern_to_smt(pat, &cond_var);
                let mut arm_post = wp_eval_expr(arm_expr, var_name, post.clone(), ensures_wp);
                for (b_name, b_val) in bindings {
                    arm_post = arm_post.substitute(&b_name, &b_val);
                }
                final_wp = SmtExpr::And(
                    Box::new(final_wp),
                    Box::new(SmtExpr::Implies(Box::new(arm_cond), Box::new(arm_post)))
                );
            }
            
            wp_eval_expr(cond, &tmp_cond, final_wp, ensures_wp)
        }
        HirExpr::ResultOk(inner, _) => {
            let tmp_name = format!("__ok_{}", var_name);
            let tmp_var = SmtExpr::Var(tmp_name.clone());
            let mk_ok = SmtExpr::FuncCall("mk_ok".into(), vec![tmp_var]);
            let post_sub = post.substitute(var_name, &mk_ok);
            wp_eval_expr(inner, &tmp_name, post_sub, ensures_wp)
        }
        HirExpr::ResultErr(inner, _) => {
            let tmp_name = format!("__err_{}", var_name);
            let tmp_var = SmtExpr::Var(tmp_name.clone());
            let mk_err = SmtExpr::FuncCall("mk_err".into(), vec![tmp_var]);
            let post_sub = post.substitute(var_name, &mk_err);
            wp_eval_expr(inner, &tmp_name, post_sub, ensures_wp)
        }
        HirExpr::VariantConstructor(_, case, args, _) => {
            if case == "Some" && args.len() == 1 {
                let tmp_name = format!("__some_{}", var_name);
                let tmp_var = SmtExpr::Var(tmp_name.clone());
                let mk_ok = SmtExpr::FuncCall("mk_ok".into(), vec![tmp_var]);
                let post_sub = post.substitute(var_name, &mk_ok);
                wp_eval_expr(&args[0], &tmp_name, post_sub, ensures_wp)
            } else if case == "None" {
                let mk_err = SmtExpr::FuncCall("mk_err".into(), vec![SmtExpr::IntConst(0)]);
                post.substitute(var_name, &mk_err)
            } else {
                post.substitute(var_name, &SmtExpr::BoolConst(false))
            }
        }
        HirExpr::BinaryOp(op, l, r, _) => {
            let tmp_l = format!("{}_l", var_name);
            let tmp_r = format!("{}_r", var_name);
            
            let op_smt = match op {
                BinaryOp::Add => SmtExpr::Add(Box::new(SmtExpr::Var(tmp_l.clone())), Box::new(SmtExpr::Var(tmp_r.clone()))),
                BinaryOp::Sub => SmtExpr::Sub(Box::new(SmtExpr::Var(tmp_l.clone())), Box::new(SmtExpr::Var(tmp_r.clone()))),
                BinaryOp::Mul => SmtExpr::Mul(Box::new(SmtExpr::Var(tmp_l.clone())), Box::new(SmtExpr::Var(tmp_r.clone()))),
                BinaryOp::Div => SmtExpr::Div(Box::new(SmtExpr::Var(tmp_l.clone())), Box::new(SmtExpr::Var(tmp_r.clone()))),
                BinaryOp::Eq => SmtExpr::Eq(Box::new(SmtExpr::Var(tmp_l.clone())), Box::new(SmtExpr::Var(tmp_r.clone()))),
                BinaryOp::Neq => SmtExpr::Not(Box::new(SmtExpr::Eq(Box::new(SmtExpr::Var(tmp_l.clone())), Box::new(SmtExpr::Var(tmp_r.clone()))))),
                BinaryOp::Lt => SmtExpr::Lt(Box::new(SmtExpr::Var(tmp_l.clone())), Box::new(SmtExpr::Var(tmp_r.clone()))),
                BinaryOp::Gt => SmtExpr::Gt(Box::new(SmtExpr::Var(tmp_l.clone())), Box::new(SmtExpr::Var(tmp_r.clone()))),
                BinaryOp::Le => SmtExpr::Le(Box::new(SmtExpr::Var(tmp_l.clone())), Box::new(SmtExpr::Var(tmp_r.clone()))),
                BinaryOp::Ge => SmtExpr::Ge(Box::new(SmtExpr::Var(tmp_l.clone())), Box::new(SmtExpr::Var(tmp_r.clone()))),
                BinaryOp::And => SmtExpr::And(Box::new(SmtExpr::Var(tmp_l.clone())), Box::new(SmtExpr::Var(tmp_r.clone()))),
                BinaryOp::Or => SmtExpr::Or(Box::new(SmtExpr::Var(tmp_l.clone())), Box::new(SmtExpr::Var(tmp_r.clone()))),
                _ => SmtExpr::BoolConst(false),
            };
            
            let p_sub = post.substitute(var_name, &op_smt);
            let wp_r = wp_eval_expr(r, &tmp_r, p_sub, ensures_wp);
            wp_eval_expr(l, &tmp_l, wp_r, ensures_wp)
        }
        HirExpr::Call(func, args, ty) => {
            let mut current_post = post;
            let mut arg_vars = Vec::new();
            for i in 0..args.len() {
                arg_vars.push(format!("{}_arg_{}", var_name, i));
            }
            
            let call_smt = if func == "valid" || func == "valid_read" || func == "separated" {
                let smt_args: Vec<SmtExpr> = arg_vars.iter().map(|n| SmtExpr::Var(n.clone())).collect();
                SmtExpr::FuncCall(func.clone(), smt_args)
            } else if ty == &HirType::I32 {
                SmtExpr::Var(format!("__call_{}", func))
            } else {
                SmtExpr::BoolConst(false)
            };
            current_post = current_post.substitute(var_name, &call_smt);
            
            for i in (0..args.len()).rev() {
                current_post = wp_eval_expr(&args[i], &arg_vars[i], current_post, ensures_wp);
            }
            current_post
        }
        _ => {
            post.substitute(var_name, &hir_to_smt(expr))
        }
    }
}

fn pattern_to_smt(pat: &crate::hir::HirPattern, val: &SmtExpr) -> (SmtExpr, Vec<(String, SmtExpr)>) {
    use crate::hir::HirPattern;
    match pat {
        HirPattern::Wildcard => (SmtExpr::BoolConst(true), vec![]),
        HirPattern::Binding(name) => (SmtExpr::BoolConst(true), vec![(name.clone(), val.clone())]),
        HirPattern::VariantCase(name, bindings) => {
            if name == "Ok" || name == "Some" {
                let is_ok = SmtExpr::FuncCall("is_ok".into(), vec![val.clone()]);
                let mut binds = vec![];
                if bindings.len() == 1 {
                    binds.push((bindings[0].clone(), SmtExpr::FuncCall("unwrap_ok".into(), vec![val.clone()])));
                }
                (is_ok, binds)
            } else if name == "Err" || name == "None" {
                let is_err = SmtExpr::Not(Box::new(SmtExpr::FuncCall("is_ok".into(), vec![val.clone()])));
                let mut binds = vec![];
                if bindings.len() == 1 {
                    binds.push((bindings[0].clone(), SmtExpr::FuncCall("unwrap_err".into(), vec![val.clone()])));
                }
                (is_err, binds)
            } else {
                (SmtExpr::BoolConst(true), vec![])
            }
        }
        HirPattern::Literal(lit) => {
            (SmtExpr::Eq(Box::new(val.clone()), Box::new(hir_to_smt(lit))), vec![])
        }
    }
}

fn hir_type_to_smt_sort(ty: &crate::hir::HirType) -> String {
    match ty {
        crate::hir::HirType::Bool => "Bool".to_string(),
        _ => "Int".to_string(), // I32 and fallback
    }
}

pub(crate) fn hir_to_smt(expr: &HirExpr) -> SmtExpr {
    match expr {
        HirExpr::IntLiteral(v, _) => SmtExpr::IntConst(*v),
        HirExpr::BoolLiteral(v, _) => SmtExpr::BoolConst(*v),
        HirExpr::VarRef(name, _) => SmtExpr::Var(name.clone()),
        HirExpr::EnumVariant(_, _, val, _) => SmtExpr::IntConst(*val as i64),
        HirExpr::Quantifier(kind, params, body, _) => {
            let body_smt = Box::new(hir_to_smt(body));
            let bounds: Vec<(String, String)> = params.iter().map(|(n, t)| (n.clone(), hir_type_to_smt_sort(t))).collect();
            match kind {
                crate::hir::QuantifierKind::Forall => SmtExpr::Forall(bounds, body_smt),
                crate::hir::QuantifierKind::Exists => SmtExpr::Exists(bounds, body_smt),
                crate::hir::QuantifierKind::Choose => {
                    SmtExpr::Var(format!("__choose_{}", params.first().map(|(n, _)| n.clone()).unwrap_or_default()))
                }
            }
        }
        HirExpr::Call(name, args, ty) => {
            if name == "valid" || name == "valid_read" || name == "separated" {
                let smt_args: Vec<SmtExpr> = args.iter().map(|a| hir_to_smt(a)).collect();
                SmtExpr::FuncCall(name.clone(), smt_args)
            } else if ty == &HirType::I32 {
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
                BinaryOp::Div => SmtExpr::Div(lhs_smt, rhs_smt),
                BinaryOp::Eq => SmtExpr::Eq(lhs_smt, rhs_smt),
                BinaryOp::Neq => SmtExpr::Not(Box::new(SmtExpr::Eq(lhs_smt, rhs_smt))),
                BinaryOp::Lt => SmtExpr::Lt(lhs_smt, rhs_smt),
                BinaryOp::Gt => SmtExpr::Gt(lhs_smt, rhs_smt),
                BinaryOp::Le => SmtExpr::Le(lhs_smt, rhs_smt),
                BinaryOp::Ge => SmtExpr::Ge(lhs_smt, rhs_smt),
                BinaryOp::And => SmtExpr::And(lhs_smt, rhs_smt),
                BinaryOp::Or => SmtExpr::Or(lhs_smt, rhs_smt),
                BinaryOp::Implies => SmtExpr::Implies(lhs_smt, rhs_smt),
                BinaryOp::Iff => SmtExpr::Eq(lhs_smt, rhs_smt), // Iff maps to Eq in SMT for booleans
                _ => SmtExpr::BoolConst(false), // fallback
            }
        }
        HirExpr::ResultOk(inner, _) => SmtExpr::FuncCall("mk_ok".to_string(), vec![hir_to_smt(inner)]),
        HirExpr::ResultErr(inner, _) => SmtExpr::FuncCall("mk_err".to_string(), vec![hir_to_smt(inner)]),
        HirExpr::Block(block, _) => {
            if let Some(crate::hir::HirStmt::Expr(e)) = block.statements.last() {
                hir_to_smt(e)
            } else if let Some(crate::hir::HirStmt::Return(Some(e))) = block.statements.last() {
                hir_to_smt(e)
            } else {
                SmtExpr::BoolConst(false)
            }
        }
        _ => SmtExpr::BoolConst(false), // fallback
    }
}

fn collect_modified_vars_stmt(stmt: &HirStmt, vars: &mut std::collections::HashSet<String>) {
    match stmt {
        HirStmt::Expr(expr) => collect_modified_vars_expr(expr, vars),
        HirStmt::While(_, body, _, _) | HirStmt::For(_, _, body) | HirStmt::GhostBlock(body) => {
            for s in &body.statements {
                collect_modified_vars_stmt(s, vars);
            }
        }
        HirStmt::Let(_, _, _, init) => collect_modified_vars_expr(init, vars),
        HirStmt::Return(Some(e)) => collect_modified_vars_expr(e, vars),
        HirStmt::Assert(e) | HirStmt::Assume(e) => collect_modified_vars_expr(e, vars),
        _ => {}
    }
}

fn collect_modified_vars_expr(expr: &HirExpr, vars: &mut std::collections::HashSet<String>) {
    match expr {
        HirExpr::BinaryOp(BinaryOp::Assign, lhs, rhs, _) => {
            if let HirExpr::VarRef(name, _) = lhs.as_ref() {
                vars.insert(name.clone());
            }
            collect_modified_vars_expr(rhs, vars);
        }
        HirExpr::BinaryOp(_, lhs, rhs, _) => {
            collect_modified_vars_expr(lhs, vars);
            collect_modified_vars_expr(rhs, vars);
        }
        HirExpr::UnaryOp(_, inner, _) | HirExpr::Ref(inner, _, _) | HirExpr::Deref(inner, _) |
        HirExpr::Try(inner, _) | HirExpr::ResultOk(inner, _) | HirExpr::ResultErr(inner, _) |
        HirExpr::FieldAccess(inner, _, _) => {
            collect_modified_vars_expr(inner, vars);
        }
        HirExpr::Call(_, args, _) | HirExpr::VariantConstructor(_, _, args, _) | HirExpr::ArrayExpr(args, _) => {
            for arg in args {
                collect_modified_vars_expr(arg, vars);
            }
        }
        HirExpr::CallIndirect(callee, args, _) => {
            collect_modified_vars_expr(callee, vars);
            for arg in args {
                collect_modified_vars_expr(arg, vars);
            }
        }
        HirExpr::If(cond, thn, els, _) => {
            collect_modified_vars_expr(cond, vars);
            for s in &thn.statements { collect_modified_vars_stmt(s, vars); }
            if let Some(e) = els {
                for s in &e.statements { collect_modified_vars_stmt(s, vars); }
            }
        }
        HirExpr::Match(cond, arms, _) => {
            collect_modified_vars_expr(cond, vars);
            for (_, arm) in arms {
                collect_modified_vars_expr(arm, vars);
            }
        }
        HirExpr::Block(block, _) => {
            for s in &block.statements { collect_modified_vars_stmt(s, vars); }
        }
        HirExpr::IndexExpr(base, idx, _) => {
            collect_modified_vars_expr(base, vars);
            collect_modified_vars_expr(idx, vars);
        }
        HirExpr::SliceExpr(base, start, end, _) => {
            collect_modified_vars_expr(base, vars);
            collect_modified_vars_expr(start, vars);
            collect_modified_vars_expr(end, vars);
        }
        HirExpr::StructExpr(_, fields, _) => {
            for (_, f) in fields {
                collect_modified_vars_expr(f, vars);
            }
        }
        _ => {}
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
        let ensures_wp = SmtExpr::BoolConst(true);
        let wp = compute_wp(&HirStmt::Assert(q), post, &ensures_wp);
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
        let ensures_wp = SmtExpr::BoolConst(true);
        let wp = compute_wp(&HirStmt::Assume(q), post, &ensures_wp);
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
        let ensures_wp = SmtExpr::BoolConst(true);
        let wp = compute_wp(&HirStmt::Let("x".into(), true, HirType::I32, init), post, &ensures_wp);
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
        let ensures_wp = SmtExpr::BoolConst(true);
        let wp = compute_wp(&HirStmt::Expr(assign), post, &ensures_wp);
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
