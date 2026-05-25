use super::{BinaryOp};
use crate::hir::{HirBlock, HirExpr, HirFunc, HirProgram, HirStmt, HirExprKind, HirStmtKind};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BorrowError {
    MutableBorrowConflict(String),     // Cannot borrow as mutable because it's already borrowed
    ImmutableBorrowConflict(String),   // Cannot borrow as immutable because it's already borrowed as mutable
    MutatingBorrowed(String),          // Cannot mutate because it is borrowed
    #[allow(dead_code)]
    UseMoved(String),                  // Cannot use moved value (optional for now)
}

impl std::fmt::Display for BorrowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BorrowError::MutableBorrowConflict(name) => write!(f, "Cannot borrow `{}` as mutable because it is already borrowed", name),
            BorrowError::ImmutableBorrowConflict(name) => write!(f, "Cannot borrow `{}` as immutable because it is already borrowed as mutable", name),
            BorrowError::MutatingBorrowed(name) => write!(f, "Cannot mutate `{}` because it is currently borrowed", name),
            BorrowError::UseMoved(name) => write!(f, "Cannot use moved value `{}`", name),
        }
    }
}

pub struct BorrowChecker {
    pub errors: Vec<BorrowError>,
}

impl BorrowChecker {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
        }
    }

    pub fn check_program(&mut self, program: &HirProgram) {
        for func in &program.functions {
            self.check_function(func);
        }
    }

    fn check_function(&mut self, func: &HirFunc) {
        // We'll need some state for the function body
        let mut ctx = FuncBorrowCtx::new();
        self.check_block(&func.body, &mut ctx);
    }

    fn check_block(&mut self, block: &super::HirBlock, ctx: &mut FuncBorrowCtx) {
        ctx.enter_scope();
        for stmt in &block.statements {
            self.check_stmt(stmt, ctx);
        }
        ctx.exit_scope();
    }

    fn check_stmt(&mut self, stmt: &HirStmt, ctx: &mut FuncBorrowCtx) {
        match &stmt.kind {
            HirStmtKind::Let(name, _is_const, _ty, initializer) => {
                self.check_expr(initializer, ctx);
                ctx.declare_var(name.clone());
            }
            HirStmtKind::Expr(expr) => {
                self.check_expr(expr, ctx);
            }
            HirStmtKind::Return(Some(expr)) => {
                self.check_expr(expr, ctx);
            }
            HirStmtKind::Return(None) => {}
            HirStmtKind::Assert(expr) | HirStmtKind::Assume(expr) => {
                self.check_expr(expr, ctx);
            }
            HirStmtKind::While(cond, body, invariants, decreases, assigns) => {
                self.check_expr(cond, ctx);
                for inv in invariants {
                    self.check_expr(inv, ctx);
                }
                if let Some(dec) = decreases {
                    self.check_expr(dec, ctx);
                }
                self.check_block(body, ctx);
            }
            HirStmtKind::For(name, iterable, body, assigns) => {
                self.check_expr(iterable, ctx);
                // Inline block checking here so we can declare the iteration variable
                // inside the same scope that the block body uses. Calling check_block
                // would open a *second* scope around the variable, making it invisible.
                ctx.enter_scope();
                ctx.declare_var(name.clone());
                for stmt in &body.statements {
                    self.check_stmt(stmt, ctx);
                }
                ctx.exit_scope();
            }
            HirStmtKind::Break | HirStmtKind::Continue | HirStmtKind::Error => {}
            HirStmtKind::GhostBlock(body) => {
                self.check_block(body, ctx);
            }
        }
    }

    fn check_expr(&mut self, expr: &HirExpr, ctx: &mut FuncBorrowCtx) {
        match &expr.kind {
            HirExprKind::VarRef(name, _) => {
                if let Err(e) = ctx.check_read(name) {
                    self.errors.push(e);
                }
            }
            HirExprKind::BinaryOp(BinaryOp::Assign, lhs, rhs, _) => {
                self.check_expr(rhs, ctx);
                if let Some(name) = get_root_var(lhs)
                    && let Err(e) = ctx.check_write(&name) {
                        self.errors.push(e);
                    }
                // also check the lhs itself for array indexing, field access (reads of indices)
                self.check_expr_lvalue(lhs, ctx);
            }
            HirExprKind::BinaryOp(_, lhs, rhs, _) => {
                self.check_expr(lhs, ctx);
                self.check_expr(rhs, ctx);
            }
            HirExprKind::Ref(inner, is_mut, _) => {
                if let Some(name) = get_root_var(inner) {
                    if *is_mut {
                        if let Err(e) = ctx.borrow_mut(&name) {
                            self.errors.push(e);
                        }
                    } else if let Err(e) = ctx.borrow_immut(&name) {
                        self.errors.push(e);
                    }
                }
                // Also check inner expression for things like array indices
                self.check_expr_lvalue(inner, ctx);
            }
            HirExprKind::UnaryOp(_, inner, _) | HirExprKind::Deref(inner, _) | HirExprKind::FieldAccess(inner, _, _) => {
                self.check_expr(inner, ctx);
            }
            HirExprKind::Call(_, args, _) | HirExprKind::VariantConstructor(_, _, args, _) | HirExprKind::ArrayExpr(args, _) => {
                for arg in args {
                    self.check_expr(arg, ctx);
                }
            }
            HirExprKind::CallIndirect(callee, args, _) => {
                self.check_expr(callee, ctx);
                for arg in args {
                    self.check_expr(arg, ctx);
                }
            }
            HirExprKind::If(cond, then_b, else_b, _) => {
                self.check_expr(cond, ctx);
                self.check_block(then_b, ctx);
                if let Some(e) = else_b {
                    self.check_block(e, ctx);
                }
            }
            HirExprKind::Match(cond, arms, _) => {
                self.check_expr(cond, ctx);
                for (_, expr) in arms {
                    self.check_expr(expr, ctx);
                }
            }
            HirExprKind::StructExpr(_, fields, _) => {
                for (_, expr) in fields {
                    self.check_expr(expr, ctx);
                }
            }
            HirExprKind::IndexExpr(base, idx, _) => {
                self.check_expr(base, ctx);
                self.check_expr(idx, ctx);
            }
            HirExprKind::SliceExpr(base, start, end, _) => {
                self.check_expr(base, ctx);
                self.check_expr(start, ctx);
                self.check_expr(end, ctx);
            }
            HirExprKind::Try(inner, _) | HirExprKind::ResultOk(inner, _) | HirExprKind::ResultErr(inner, _) => {
                self.check_expr(inner, ctx);
            }
            HirExprKind::Block(block, _) => {
                self.check_block(block, ctx);
            }
            HirExprKind::Closure(_, body, _, _) | HirExprKind::Quantifier(_, _, body, _) => {
                self.check_expr(body, ctx);
            }
            HirExprKind::IntLiteral(_, _) | HirExprKind::BoolLiteral(_, _) | HirExprKind::EnumVariant(_, _, _, _) | HirExprKind::Error => {}
        }
    }

    fn check_expr_lvalue(&mut self, expr: &HirExpr, ctx: &mut FuncBorrowCtx) {
        match &expr.kind {
            HirExprKind::IndexExpr(_base, idx, _) => {
                self.check_expr(idx, ctx);
            }
            HirExprKind::FieldAccess(base, _, _) | HirExprKind::Deref(base, _) => {
                self.check_expr_lvalue(base, ctx);
            }
            _ => {}
        }
    }
}

fn get_root_var(expr: &HirExpr) -> Option<String> {
    match &expr.kind {
        HirExprKind::VarRef(name, _) => Some(name.clone()),
        HirExprKind::FieldAccess(base, _, _) => get_root_var(base),
        HirExprKind::IndexExpr(base, _, _) => get_root_var(base),
        HirExprKind::Deref(_base, _) => None, // Deref breaks the root var path, since it's a pointer indirection
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BorrowKind {
    Immut,
    Mut,
}

struct Scope {
    variables: HashSet<String>,
    borrows: Vec<(String, BorrowKind)>,
}

struct FuncBorrowCtx {
    scopes: Vec<Scope>,
}

impl FuncBorrowCtx {
    fn new() -> Self {
        Self {
            scopes: vec![Scope { variables: HashSet::new(), borrows: Vec::new() }],
        }
    }
    
    fn enter_scope(&mut self) {
        self.scopes.push(Scope { variables: HashSet::new(), borrows: Vec::new() });
    }
    
    fn exit_scope(&mut self) {
        self.scopes.pop();
    }
    
    fn declare_var(&mut self, name: String) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.variables.insert(name);
        }
    }

    fn check_read(&self, name: &str) -> Result<(), BorrowError> {
        for scope in &self.scopes {
            for (borrowed, kind) in &scope.borrows {
                if borrowed == name && *kind == BorrowKind::Mut {
                    return Err(BorrowError::MutatingBorrowed(name.to_string()));
                }
            }
        }
        Ok(())
    }

    fn check_write(&self, name: &str) -> Result<(), BorrowError> {
        for scope in &self.scopes {
            for (borrowed, _kind) in &scope.borrows {
                if borrowed == name {
                    return Err(BorrowError::MutatingBorrowed(name.to_string()));
                }
            }
        }
        Ok(())
    }

    fn borrow_immut(&mut self, name: &str) -> Result<(), BorrowError> {
        // Can't borrow immut if already borrowed mutably
        for scope in &self.scopes {
            for (borrowed, kind) in &scope.borrows {
                if borrowed == name && *kind == BorrowKind::Mut {
                    return Err(BorrowError::ImmutableBorrowConflict(name.to_string()));
                }
            }
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.borrows.push((name.to_string(), BorrowKind::Immut));
        }
        Ok(())
    }

    fn borrow_mut(&mut self, name: &str) -> Result<(), BorrowError> {
        // Can't borrow mut if already borrowed at all
        for scope in &self.scopes {
            for (borrowed, _kind) in &scope.borrows {
                if borrowed == name {
                    return Err(BorrowError::MutableBorrowConflict(name.to_string()));
                }
            }
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.borrows.push((name.to_string(), BorrowKind::Mut));
        }
        Ok(())
    }
}
