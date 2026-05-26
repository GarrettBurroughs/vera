use crate::hir::{HirExprKind, HirStmtKind, HirFunc, HirStmt, HirExpr, HirType, BinaryOp};
use super::smt::{SmtExpr, check_sat};
use super::VerificationError;

pub fn verify_func(func: &HirFunc) -> Result<(), VerificationError> {
    // 1. Convert ensures clauses to SMT
    let mut current_wp = SmtExpr::BoolConst(true);
    for ens in &func.ensures {
        current_wp = SmtExpr::And(Box::new(current_wp), Box::new(hir_to_smt(ens)));
    }
    
    if let Some(sym_id) = func.ret_sym_id {
        current_wp = current_wp.substitute(&format!("result_{}", sym_id.0), &SmtExpr::Var("result".into(), "Int".into()));
    }
    
    let ensures_wp = current_wp.clone();

    // 2. Compute WP backwards through statements
    if let Some(body) = &func.body {
        for stmt in body.statements.iter().rev() {
            current_wp = compute_wp(stmt, current_wp, &ensures_wp, &func.assigns);
        }
    }

    // 3. Add requires clauses as preconditions
    let mut precondition = SmtExpr::BoolConst(true);
    for req in &func.requires {
        precondition = SmtExpr::And(Box::new(precondition), Box::new(hir_to_smt(req)));
    }

    // 3.5. Add parameter refinement constraints as preconditions
    for (p_name, _, p_ty) in &func.params {
        if let HirType::Refinement(_, cond) = p_ty {
            let cond_smt = hir_to_smt(&cond).substitute("self", &SmtExpr::Var(p_name.clone(), "Int".into()));
            precondition = SmtExpr::And(Box::new(precondition), Box::new(cond_smt));
        }
    }

    // 3.7. Precondition Vacuity Checking
    // Disallow contradictory preconditions that make the function trivially verifiable
    if check_sat(&precondition, func.span)?.is_none() {
        return Err(VerificationError::VacuousPrecondition { message: format!("Function '{}' has vacuous (unsatisfiable) preconditions.", func.name), span: func.span });
    }

    // 4. Final Verification Condition (VC): Requires => WP
    let vc = SmtExpr::Implies(Box::new(precondition), Box::new(current_wp));

    let query = SmtExpr::Not(Box::new(vc));

    // 6. Invoke Z3
    let is_sat = check_sat(&query, func.span)?;

    if let Some(model) = is_sat {
        Err(VerificationError::ProofFailed { message: format!("Function '{}' failed verification.", func.name), span: func.span, counterexample: Some(model) })
    } else {
        Ok(()) // Unsat means valid
    }
}

fn compute_wp(stmt: &HirStmt, post: SmtExpr, ensures_wp: &SmtExpr, assigns: &[HirExpr]) -> SmtExpr {
    match &stmt.kind {
        HirStmtKind::Assert(expr) => {
            wp_eval_expr(expr, "__dummy_assert", SmtExpr::And(Box::new(SmtExpr::Var("__dummy_assert".into(), "Int".into())), Box::new(post)), ensures_wp, assigns)
        }
        HirStmtKind::Assume(expr) => {
            wp_eval_expr(expr, "__dummy_assume", SmtExpr::Implies(Box::new(SmtExpr::Var("__dummy_assume".into(), "Int".into())), Box::new(post)), ensures_wp, assigns)
        }
        HirStmtKind::Let(name, sym_id, _, ty, init) => {
            let smt_name = format!("{}_{}", name, sym_id.0);
            let mut post_sub = wp_eval_expr(init, &smt_name, post, ensures_wp, assigns);
            if let HirType::Refinement(_, cond) = ty {
                let cond_smt = hir_to_smt(&cond).substitute("self", &SmtExpr::Var(smt_name, "Int".into()));
                post_sub = SmtExpr::And(Box::new(cond_smt), Box::new(post_sub));
            }
            post_sub
        }
        HirStmtKind::Expr(expr) => {
            if let HirExprKind::BinaryOp(BinaryOp::Assign, lhs, rhs, _) = &expr.kind {
                if let HirExprKind::VarRef(name, sym_id, ty) = &lhs.kind {
                    let smt_name = format!("{}_{}", name.as_str(), sym_id.0);
                    let mut post_sub = wp_eval_expr(rhs, &smt_name, post, ensures_wp, assigns);
                    if let HirType::Refinement(_, cond) = ty {
                        let cond_smt = hir_to_smt(&cond).substitute("self", &SmtExpr::Var(smt_name, "Int".into()));
                        post_sub = SmtExpr::And(Box::new(cond_smt), Box::new(post_sub));
                    }
                    return post_sub;
                } else if let HirExprKind::Deref(ptr, _) = &lhs.kind {
                    let mut frame_cond = SmtExpr::BoolConst(false);
                    for a in assigns {
                        if let HirExprKind::Deref(a_ptr, _) = &a.kind {
                            frame_cond = SmtExpr::Or(Box::new(frame_cond), Box::new(SmtExpr::Eq(Box::new(hir_to_smt(ptr)), Box::new(hir_to_smt(a_ptr)))));
                        }
                    }
                    if frame_cond.to_smtlib2() == "false" {
                        // Empty assigns clause or no derefs in assigns
                    }
                    let eval_rhs = wp_eval_expr(rhs, "__dummy_rhs", post, ensures_wp, assigns);
                    return SmtExpr::And(Box::new(frame_cond), Box::new(eval_rhs));
                }
            }
            wp_eval_expr(expr, "__dummy_expr", post, ensures_wp, assigns)
        }
        HirStmtKind::While(cond, body, invariants, decreases, loop_assigns) => {
            let mut i_expr = SmtExpr::BoolConst(true);
            for inv in invariants {
                i_expr = SmtExpr::And(Box::new(i_expr), Box::new(hir_to_smt(inv)));
            }
            
            let mut mod_vars = std::collections::HashSet::new();
            for s in &body.statements {
                collect_modified_vars_stmt(s, &mut mod_vars);
            }
            
            let b_expr = hir_to_smt(&cond);
            
            // compute_block_wp backwards
            let mut body_wp = i_expr.clone();
            
            if let Some(dec) = decreases {
                let d_expr = hir_to_smt(dec);
                let d0_var = SmtExpr::Var("___d0".into(), "Int".into());
                let dec_cond = SmtExpr::FuncCall("<".into(), vec![d_expr.clone(), d0_var.clone()]);
                body_wp = SmtExpr::And(Box::new(body_wp), Box::new(dec_cond));
            }
            
            for s in body.statements.iter().rev() {
                body_wp = compute_wp(s, body_wp, ensures_wp, loop_assigns);
            }
            
            if let Some(dec) = decreases {
                let d_expr = hir_to_smt(dec);
                let d0_eq = SmtExpr::FuncCall("=".into(), vec![SmtExpr::Var("___d0".into(), "Int".into()), d_expr.clone()]);
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
        HirStmtKind::For(_, _, _, _, _) => post,
        HirStmtKind::Break => post,
        HirStmtKind::Continue => post,
        HirStmtKind::Return(opt_expr) => {
            if let Some(expr) = opt_expr {
                wp_eval_expr(expr, "result", ensures_wp.clone(), ensures_wp, assigns)
            } else {
                ensures_wp.clone()
            }
        }
        HirStmtKind::Error => post,
        HirStmtKind::GhostBlock(block) => {
            let mut ghost_post = post;
            for s in block.statements.iter().rev() {
                ghost_post = compute_wp(s, ghost_post, ensures_wp, assigns);
            }
            ghost_post
        }
    }
}

fn wp_eval_expr(expr: &HirExpr, var_name: &str, post: SmtExpr, ensures_wp: &SmtExpr, assigns: &[HirExpr]) -> SmtExpr {
    match &expr.kind {
        HirExprKind::Block(block, _) => {
            let mut block_wp = post;
            for s in block.statements.iter().rev() {
                block_wp = compute_wp(s, block_wp, ensures_wp, assigns);
            }
            block_wp
        }
        HirExprKind::Try(inner, _) => {
            let tmp_name = format!("__try_tmp_{}", var_name);
            let tmp_var = SmtExpr::Var(tmp_name.clone(), "Int".into());
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
            
            wp_eval_expr(inner, &tmp_name, branches, ensures_wp, assigns)
        }
        HirExprKind::Match(cond, arms, _) => {
            let tmp_cond = format!("__match_cond_{}", var_name);
            let cond_var = SmtExpr::Var(tmp_cond.clone(), "Int".into());
            
            let mut final_wp = SmtExpr::BoolConst(true);
            for (pat, arm_expr) in arms {
                let (arm_cond, bindings) = pattern_to_smt(pat, &cond_var);
                let mut arm_post = wp_eval_expr(arm_expr, var_name, post.clone(), ensures_wp, assigns);
                for (b_name, b_val) in bindings {
                    arm_post = arm_post.substitute(&b_name, &b_val);
                }
                final_wp = SmtExpr::And(
                    Box::new(final_wp),
                    Box::new(SmtExpr::Implies(Box::new(arm_cond), Box::new(arm_post)))
                );
            }
            
            wp_eval_expr(cond, &tmp_cond, final_wp, ensures_wp, assigns)
        }
        HirExprKind::If(cond, thn, els, _) => {
            let mut thn_wp = post.clone();
            for s in thn.statements.iter().rev() {
                thn_wp = compute_wp(s, thn_wp, ensures_wp, assigns);
            }
            let mut els_wp = post;
            if let Some(e) = els {
                for s in e.statements.iter().rev() {
                    els_wp = compute_wp(s, els_wp, ensures_wp, assigns);
                }
            }
            
            let tmp_cond = format!("__if_cond_{}", var_name);
            let cond_var = SmtExpr::Var(tmp_cond.clone(), "Int".into());
            
            let ite_wp = SmtExpr::And(
                Box::new(SmtExpr::Implies(Box::new(cond_var.clone()), Box::new(thn_wp))),
                Box::new(SmtExpr::Implies(Box::new(SmtExpr::Not(Box::new(cond_var))), Box::new(els_wp)))
            );
            
            wp_eval_expr(cond, &tmp_cond, ite_wp, ensures_wp, assigns)
        }
        HirExprKind::ResultOk(inner, _) => {
            let tmp_name = format!("__ok_{}", var_name);
            let tmp_var = SmtExpr::Var(tmp_name.clone(), "Int".into());
            let mk_ok = SmtExpr::FuncCall("mk_ok".into(), vec![tmp_var]);
            let post_sub = post.substitute(var_name, &mk_ok);
            wp_eval_expr(inner, &tmp_name, post_sub, ensures_wp, assigns)
        }
        HirExprKind::ResultErr(inner, _) => {
            let tmp_name = format!("__err_{}", var_name);
            let tmp_var = SmtExpr::Var(tmp_name.clone(), "Int".into());
            let mk_err = SmtExpr::FuncCall("mk_err".into(), vec![tmp_var]);
            let post_sub = post.substitute(var_name, &mk_err);
            wp_eval_expr(inner, &tmp_name, post_sub, ensures_wp, assigns)
        }
        HirExprKind::VariantConstructor(_, case, args, _) => {
            if case == "Some" && args.len() == 1 {
                let tmp_name = format!("__some_{}", var_name);
                let tmp_var = SmtExpr::Var(tmp_name.clone(), "Int".into());
                let mk_ok = SmtExpr::FuncCall("mk_ok".into(), vec![tmp_var]);
                let post_sub = post.substitute(var_name, &mk_ok);
                wp_eval_expr(&args[0], &tmp_name, post_sub, ensures_wp, assigns)
            } else if case == "None" {
                let mk_err = SmtExpr::FuncCall("mk_err".into(), vec![SmtExpr::IntConst(0)]);
                post.substitute(var_name, &mk_err)
            } else {
                post.substitute(var_name, &SmtExpr::BoolConst(false))
            }
        }
        HirExprKind::BinaryOp(op, l, r, _) => {
            let tmp_l = format!("{}_l", var_name);
            let tmp_r = format!("{}_r", var_name);
            
            let op_smt = match op {
                BinaryOp::Add => SmtExpr::Add(Box::new(SmtExpr::Var(tmp_l.clone(), "Int".into())), Box::new(SmtExpr::Var(tmp_r.clone(), "Int".into()))),
                BinaryOp::Sub => SmtExpr::Sub(Box::new(SmtExpr::Var(tmp_l.clone(), "Int".into())), Box::new(SmtExpr::Var(tmp_r.clone(), "Int".into()))),
                BinaryOp::Mul => SmtExpr::Mul(Box::new(SmtExpr::Var(tmp_l.clone(), "Int".into())), Box::new(SmtExpr::Var(tmp_r.clone(), "Int".into()))),
                BinaryOp::Div => SmtExpr::Div(Box::new(SmtExpr::Var(tmp_l.clone(), "Int".into())), Box::new(SmtExpr::Var(tmp_r.clone(), "Int".into()))),
                BinaryOp::Eq => SmtExpr::Eq(Box::new(SmtExpr::Var(tmp_l.clone(), "Int".into())), Box::new(SmtExpr::Var(tmp_r.clone(), "Int".into()))),
                BinaryOp::Neq => SmtExpr::Not(Box::new(SmtExpr::Eq(Box::new(SmtExpr::Var(tmp_l.clone(), "Int".into())), Box::new(SmtExpr::Var(tmp_r.clone(), "Int".into()))))),
                BinaryOp::Lt => SmtExpr::Lt(Box::new(SmtExpr::Var(tmp_l.clone(), "Int".into())), Box::new(SmtExpr::Var(tmp_r.clone(), "Int".into()))),
                BinaryOp::Gt => SmtExpr::Gt(Box::new(SmtExpr::Var(tmp_l.clone(), "Int".into())), Box::new(SmtExpr::Var(tmp_r.clone(), "Int".into()))),
                BinaryOp::Le => SmtExpr::Le(Box::new(SmtExpr::Var(tmp_l.clone(), "Int".into())), Box::new(SmtExpr::Var(tmp_r.clone(), "Int".into()))),
                BinaryOp::Ge => SmtExpr::Ge(Box::new(SmtExpr::Var(tmp_l.clone(), "Int".into())), Box::new(SmtExpr::Var(tmp_r.clone(), "Int".into()))),
                BinaryOp::And => SmtExpr::And(Box::new(SmtExpr::Var(tmp_l.clone(), "Int".into())), Box::new(SmtExpr::Var(tmp_r.clone(), "Int".into()))),
                BinaryOp::Or => SmtExpr::Or(Box::new(SmtExpr::Var(tmp_l.clone(), "Int".into())), Box::new(SmtExpr::Var(tmp_r.clone(), "Int".into()))),
                _ => SmtExpr::BoolConst(false),
            };
            
            let p_sub = post.substitute(var_name, &op_smt);
            let wp_r = wp_eval_expr(r, &tmp_r, p_sub, ensures_wp, assigns);
            wp_eval_expr(l, &tmp_l, wp_r, ensures_wp, assigns)
        }
        HirExprKind::Call(func, _, args, ty) => {
            let mut current_post = post;
            let mut arg_vars = Vec::new();
            for i in 0..args.len() {
                arg_vars.push(format!("{}_arg_{}", var_name, i));
            }
            
            let call_smt = if func.as_str() == "valid" || func.as_str() == "valid_read" || func.as_str() == "separated" {
                let smt_args: Vec<SmtExpr> = arg_vars.iter().map(|n| SmtExpr::Var(n.clone(), "Int".into())).collect();
                SmtExpr::FuncCall(func.as_str(), smt_args)
            } else if ty == &HirType::I32 {
                SmtExpr::Var(format!("__call_{}", func.as_str().replace("::", "_")), "Int".into())
            } else {
                SmtExpr::BoolConst(false)
            };
            current_post = current_post.substitute(var_name, &call_smt);
            
            for i in (0..args.len()).rev() {
                current_post = wp_eval_expr(&args[i], &arg_vars[i], current_post, ensures_wp, assigns);
            }
            current_post
        }
        _ => {
            post.substitute(var_name, &hir_to_smt(expr))
        }
    }
}

fn pattern_to_smt(pat: &crate::hir::HirPattern, val: &SmtExpr) -> (SmtExpr, Vec<(String, SmtExpr)>) {
    use crate::hir::{HirExprKind, HirStmtKind, HirPattern};
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
    match &expr.kind {
        HirExprKind::IntLiteral(v, _) => SmtExpr::IntConst(*v),
        HirExprKind::BoolLiteral(v, _) => SmtExpr::BoolConst(*v),
        HirExprKind::VarRef(name, sym_id, ty) => SmtExpr::Var(format!("{}_{}", name.as_str(), sym_id.0), hir_type_to_smt_sort(ty)),
        HirExprKind::EnumVariant(_, _, val, _) => SmtExpr::IntConst(*val as i64),
        HirExprKind::Quantifier(kind, params, body, _) => {
            let bounds: Vec<(String, String)> = params.iter().map(|(n, sym_id, t)| (format!("{}_{}", n, sym_id.0), hir_type_to_smt_sort(t))).collect();
            let body_smt = Box::new(hir_to_smt(body));
            match kind {
                crate::hir::QuantifierKind::Forall => SmtExpr::Forall(bounds, body_smt),
                crate::hir::QuantifierKind::Exists => SmtExpr::Exists(bounds, body_smt),
                crate::hir::QuantifierKind::Choose => {
                    SmtExpr::Var(format!("__choose_{}", params.first().map(|(n, _, _)| n.clone()).unwrap_or_default()), "Int".into())
                }
            }
        }
        HirExprKind::Call(name, _, args, ty) => {
            if name.as_str() == "valid" || name.as_str() == "valid_read" || name.as_str() == "separated" {
                let smt_args: Vec<SmtExpr> = args.iter().map(|a| hir_to_smt(a)).collect();
                SmtExpr::FuncCall(name.as_str(), smt_args)
            } else if ty == &HirType::I32 {
                SmtExpr::Var(format!("__call_{}", name.as_str()), "Int".into()) // Treat as an arbitrary variable
            } else {
                SmtExpr::BoolConst(false)
            }
        }
        HirExprKind::BinaryOp(op, lhs, rhs, _) => {
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
        HirExprKind::ResultOk(inner, _) => SmtExpr::FuncCall("mk_ok".to_string(), vec![hir_to_smt(inner)]),
        HirExprKind::ResultErr(inner, _) => SmtExpr::FuncCall("mk_err".to_string(), vec![hir_to_smt(inner)]),
        HirExprKind::Block(block, _) => {
            if let Some(crate::hir::HirStmtKind::Expr(e)) = block.statements.last().map(|s| &s.kind) {
                hir_to_smt(&e)
            } else if let Some(crate::hir::HirStmtKind::Return(Some(e))) = block.statements.last().map(|s| &s.kind) {
                hir_to_smt(&e)
            } else {
                SmtExpr::BoolConst(false)
            }
        }
        _ => SmtExpr::BoolConst(false), // fallback
    }
}

fn collect_modified_vars_stmt(stmt: &HirStmt, vars: &mut std::collections::HashSet<String>) {
    match &stmt.kind {
        HirStmtKind::Expr(expr) => collect_modified_vars_expr(expr, vars),
        HirStmtKind::While(_, body, _, _, _) | HirStmtKind::For(_, _, _, body, _) | HirStmtKind::GhostBlock(body) => {
            for s in &body.statements {
                collect_modified_vars_stmt(s, vars);
            }
        }
        HirStmtKind::Let(_, _, _, _, init) => collect_modified_vars_expr(init, vars),
        HirStmtKind::Return(Some(e)) => collect_modified_vars_expr(e, vars),
        HirStmtKind::Assert(e) | HirStmtKind::Assume(e) => collect_modified_vars_expr(e, vars),
        _ => {}
    }
}

fn collect_modified_vars_expr(expr: &HirExpr, vars: &mut std::collections::HashSet<String>) {
    match &expr.kind {
        HirExprKind::BinaryOp(BinaryOp::Assign, lhs, rhs, _) => {
            if let HirExprKind::VarRef(name, sym_id, _) = &lhs.kind {
                vars.insert(format!("{}_{}", name.as_str(), sym_id.0));
            }
            collect_modified_vars_expr(rhs, vars);
        }
        HirExprKind::BinaryOp(_, lhs, rhs, _) => {
            collect_modified_vars_expr(lhs, vars);
            collect_modified_vars_expr(rhs, vars);
        }
        HirExprKind::UnaryOp(_, inner, _) | HirExprKind::Ref(inner, _, _) | HirExprKind::Deref(inner, _) |
        HirExprKind::Try(inner, _) | HirExprKind::ResultOk(inner, _) | HirExprKind::ResultErr(inner, _) |
        HirExprKind::FieldAccess(inner, _, _) => {
            collect_modified_vars_expr(inner, vars);
        }
        HirExprKind::Call(_, _, args, _) | HirExprKind::VariantConstructor(_, _, args, _) | HirExprKind::ArrayExpr(args, _) => {
            for arg in args {
                collect_modified_vars_expr(arg, vars);
            }
        }
        HirExprKind::CallIndirect(callee, args, _) => {
            collect_modified_vars_expr(callee, vars);
            for arg in args {
                collect_modified_vars_expr(arg, vars);
            }
        }
        HirExprKind::If(cond, thn, els, _) => {
            collect_modified_vars_expr(cond, vars);
            for s in &thn.statements { collect_modified_vars_stmt(s, vars); }
            if let Some(e) = els {
                for s in &e.statements { collect_modified_vars_stmt(s, vars); }
            }
        }
        HirExprKind::Match(cond, arms, _) => {
            collect_modified_vars_expr(cond, vars);
            for (_, arm) in arms {
                collect_modified_vars_expr(arm, vars);
            }
        }
        HirExprKind::Block(block, _) => {
            for s in &block.statements { collect_modified_vars_stmt(s, vars); }
        }
        HirExprKind::IndexExpr(base, idx, _) => {
            collect_modified_vars_expr(base, vars);
            collect_modified_vars_expr(idx, vars);
        }
        HirExprKind::SliceExpr(base, start, end, _) => {
            collect_modified_vars_expr(base, vars);
            collect_modified_vars_expr(start, vars);
            collect_modified_vars_expr(end, vars);
        }
        HirExprKind::StructExpr(_, _, fields, _) => {
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
    use crate::hir::{HirExprKind, HirStmtKind, HirType, HirExpr, HirStmt, BinaryOp, Path, SymbolId, Span};
    use crate::verification::smt::SmtExpr;

    // ------------------------------------------------------------------
    // compute_wp
    // ------------------------------------------------------------------

    /// WP(assert Q, P) = Q && P
    #[test]
    fn test_wp_assert() {
        // WP(assert x > 0, true) = (x > 0) && true
        let q = HirExprKind::BinaryOp(
            BinaryOp::Gt,
            Box::new(HirExpr::new(HirExprKind::VarRef(Path::from_ident("x".to_string()), SymbolId(0), HirType::I32), Span::default())),
            Box::new(HirExpr::new(HirExprKind::IntLiteral(0, HirType::I32), Span::default())),
            HirType::Bool,
        );
        let post = SmtExpr::BoolConst(true);
        let ensures_wp = SmtExpr::BoolConst(true);
        let wp = compute_wp(&HirStmt::new(HirStmtKind::Assert(HirExpr::new(q, Span::default())), Span::default()), post, &ensures_wp, &[]);
        assert_eq!(wp.to_smtlib2(), "(and (> x_0 0) true)");
    }

    /// WP(assume Q, P) = Q => P
    #[test]
    fn test_wp_assume() {
        // WP(assume x > 0, true) = (x > 0) => true
        let q = HirExprKind::BinaryOp(
            BinaryOp::Gt,
            Box::new(HirExpr::new(HirExprKind::VarRef(Path::from_ident("x".to_string()), SymbolId(0), HirType::I32), Span::default())),
            Box::new(HirExpr::new(HirExprKind::IntLiteral(0, HirType::I32), Span::default())),
            HirType::Bool,
        );
        let post = SmtExpr::BoolConst(true);
        let ensures_wp = SmtExpr::BoolConst(true);
        let wp = compute_wp(&HirStmt::new(HirStmtKind::Assume(HirExpr::new(q, Span::default())), Span::default()), post, &ensures_wp, &[]);
        assert_eq!(wp.to_smtlib2(), "(=> (> x_0 0) true)");
    }

    /// WP(let x = E, P) = P[E/x]
    #[test]
    fn test_wp_let_substitution() {
        // WP(const x = 5, x > 0) = 5 > 0
        let init = HirExprKind::IntLiteral(5, HirType::I32);
        let post = SmtExpr::Gt(
            Box::new(SmtExpr::Var("x_0".into(), "Int".into())),
            Box::new(SmtExpr::IntConst(0)),
        );
        let ensures_wp = SmtExpr::BoolConst(true);
        let wp = compute_wp(&HirStmt::new(HirStmtKind::Let("x".into(), SymbolId(0), true, HirType::I32, HirExpr::new(init, Span::default())), Span::default()), post, &ensures_wp, &[]);
        assert_eq!(wp.to_smtlib2(), "(> 5 0)");
    }

    /// WP(x = E, P) = P[E/x] — exercises the bug fix for assignment via HirStmtKind::Expr.
    #[test]
    fn test_wp_assignment_substitution() {
        // WP(x = 10, x == 10) should give: 10 == 10  (i.e., true)
        let assign = HirExprKind::BinaryOp(
            BinaryOp::Assign,
            Box::new(HirExpr::new(HirExprKind::VarRef(Path::from_ident("x".to_string()), SymbolId(0), HirType::I32), Span::default())),
            Box::new(HirExpr::new(HirExprKind::IntLiteral(10, HirType::I32), Span::default())),
            HirType::I32,
        );
        let post = SmtExpr::Eq(
            Box::new(SmtExpr::Var("x_0".into(), "Int".into())),
            Box::new(SmtExpr::IntConst(10)),
        );
        let ensures_wp = SmtExpr::BoolConst(true);
        let wp = compute_wp(&HirStmt::new(HirStmtKind::Expr(HirExpr::new(assign, Span::default())), Span::default()), post, &ensures_wp, &[]);
        // After substitution x -> 10 in (= x 10): (= 10 10)
        assert_eq!(wp.to_smtlib2(), "(= 10 10)");
    }

    // ------------------------------------------------------------------
    // hir_to_smt
    // ------------------------------------------------------------------

    /// IntLiteral lowers to IntConst.
    #[test]
    fn test_hir_to_smt_int_literal() {
        let expr = HirExprKind::IntLiteral(42, HirType::I32);
        assert_eq!(hir_to_smt(&HirExpr::new(expr, Span::default())).to_smtlib2(), "42");
    }

    /// BoolLiteral lowers to BoolConst.
    #[test]
    fn test_hir_to_smt_bool_literal() {
        assert_eq!(hir_to_smt(&HirExpr::new(HirExprKind::BoolLiteral(true, HirType::Bool), Span::default())).to_smtlib2(), "true");
        assert_eq!(hir_to_smt(&HirExpr::new(HirExprKind::BoolLiteral(false, HirType::Bool), Span::default())).to_smtlib2(), "false");
    }

    /// VarRef lowers to Var with the same name.
    #[test]
    fn test_hir_to_smt_var_ref() {
        let expr = HirExprKind::VarRef(Path::from_ident("my_var".to_string()), SymbolId(0), HirType::I32);
        assert_eq!(hir_to_smt(&HirExpr::new(expr, Span::default())).to_smtlib2(), "my_var_0");
    }

    /// BinaryOp::Add lowers to SmtExpr::Add.
    #[test]
    fn test_hir_to_smt_add() {
        let expr = HirExprKind::BinaryOp(
            BinaryOp::Add,
            Box::new(HirExpr::new(HirExprKind::IntLiteral(1, HirType::I32), Span::default())),
            Box::new(HirExpr::new(HirExprKind::IntLiteral(2, HirType::I32), Span::default())),
            HirType::I32,
        );
        assert_eq!(hir_to_smt(&HirExpr::new(expr, Span::default())).to_smtlib2(), "(+ 1 2)");
    }

    /// BinaryOp::Lt lowers to SmtExpr::Lt.
    #[test]
    fn test_hir_to_smt_comparison() {
        let expr = HirExprKind::BinaryOp(
            BinaryOp::Lt,
            Box::new(HirExpr::new(HirExprKind::VarRef(Path::from_ident("x".to_string()), SymbolId(0), HirType::I32), Span::default())),
            Box::new(HirExpr::new(HirExprKind::IntLiteral(5, HirType::I32), Span::default())),
            HirType::Bool,
        );
        assert_eq!(hir_to_smt(&HirExpr::new(expr, Span::default())).to_smtlib2(), "(< x_0 5)");
    }
}
