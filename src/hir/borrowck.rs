use super::{HirProgram, HirFunc, HirStmt, HirExpr, HirType, BinaryOp, UnaryOp};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BorrowError {
    MutableBorrowConflict(String),     // Cannot borrow as mutable because it's already borrowed
    ImmutableBorrowConflict(String),   // Cannot borrow as immutable because it's already borrowed as mutable
    MutatingBorrowed(String),          // Cannot mutate because it is borrowed
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
        match stmt {
            HirStmt::Let(name, _is_const, _ty, initializer) => {
                self.check_expr(initializer, ctx);
                ctx.declare_var(name.clone());
            }
            HirStmt::Expr(expr) => {
                self.check_expr(expr, ctx);
            }
            HirStmt::Return(Some(expr)) => {
                self.check_expr(expr, ctx);
            }
            HirStmt::Return(None) => {}
            HirStmt::Assert(expr) | HirStmt::Assume(expr) => {
                self.check_expr(expr, ctx);
            }
            HirStmt::While(cond, body, invariants) => {
                self.check_expr(cond, ctx);
                for inv in invariants {
                    self.check_expr(inv, ctx);
                }
                self.check_block(body, ctx);
            }
            HirStmt::For(name, iterable, body) => {
                self.check_expr(iterable, ctx);
                ctx.enter_scope();
                ctx.declare_var(name.clone());
                self.check_block(body, ctx); // wait, block handles enter/exit scope itself
                ctx.exit_scope();
            }
            HirStmt::Break | HirStmt::Continue | HirStmt::Error => {}
        }
    }

    fn check_expr(&mut self, expr: &HirExpr, ctx: &mut FuncBorrowCtx) {
        match expr {
            HirExpr::VarRef(name, _) => {
                if let Err(e) = ctx.check_read(name) {
                    self.errors.push(e);
                }
            }
            HirExpr::BinaryOp(BinaryOp::Assign, lhs, rhs, _) => {
                self.check_expr(rhs, ctx);
                if let Some(name) = get_root_var(lhs) {
                    if let Err(e) = ctx.check_write(&name) {
                        self.errors.push(e);
                    }
                }
                // also check the lhs itself for array indexing, field access (reads of indices)
                self.check_expr_lvalue(lhs, ctx);
            }
            HirExpr::BinaryOp(_, lhs, rhs, _) => {
                self.check_expr(lhs, ctx);
                self.check_expr(rhs, ctx);
            }
            HirExpr::Ref(inner, is_mut, _) => {
                if let Some(name) = get_root_var(inner) {
                    if *is_mut {
                        if let Err(e) = ctx.borrow_mut(&name) {
                            self.errors.push(e);
                        }
                    } else {
                        if let Err(e) = ctx.borrow_immut(&name) {
                            self.errors.push(e);
                        }
                    }
                }
                // Also check inner expression for things like array indices
                self.check_expr_lvalue(inner, ctx);
            }
            HirExpr::UnaryOp(_, inner, _) | HirExpr::Deref(inner, _) | HirExpr::FieldAccess(inner, _, _) => {
                self.check_expr(inner, ctx);
            }
            HirExpr::Call(_, args, _) | HirExpr::VariantConstructor(_, _, args, _) | HirExpr::ArrayExpr(args, _) => {
                for arg in args {
                    self.check_expr(arg, ctx);
                }
            }
            HirExpr::If(cond, then_b, else_b, _) => {
                self.check_expr(cond, ctx);
                self.check_block(then_b, ctx);
                if let Some(e) = else_b {
                    self.check_block(e, ctx);
                }
            }
            HirExpr::Match(cond, arms, _) => {
                self.check_expr(cond, ctx);
                for (_, expr) in arms {
                    self.check_expr(expr, ctx);
                }
            }
            HirExpr::StructExpr(_, fields, _) => {
                for (_, expr) in fields {
                    self.check_expr(expr, ctx);
                }
            }
            HirExpr::IndexExpr(base, idx, _) => {
                self.check_expr(base, ctx);
                self.check_expr(idx, ctx);
            }
            HirExpr::SliceExpr(base, start, end, _) => {
                self.check_expr(base, ctx);
                self.check_expr(start, ctx);
                self.check_expr(end, ctx);
            }
            HirExpr::Try(inner, _) | HirExpr::ResultOk(inner, _) | HirExpr::ResultErr(inner, _) => {
                self.check_expr(inner, ctx);
            }
            HirExpr::Block(block, _) => {
                self.check_block(block, ctx);
            }
            HirExpr::IntLiteral(_, _) | HirExpr::BoolLiteral(_, _) | HirExpr::EnumVariant(_, _, _, _) | HirExpr::Error => {}
        }
    }

    fn check_expr_lvalue(&mut self, expr: &HirExpr, ctx: &mut FuncBorrowCtx) {
        match expr {
            HirExpr::IndexExpr(_base, idx, _) => {
                self.check_expr(idx, ctx);
            }
            HirExpr::FieldAccess(base, _, _) | HirExpr::Deref(base, _) => {
                self.check_expr_lvalue(base, ctx);
            }
            _ => {}
        }
    }
}

fn get_root_var(expr: &HirExpr) -> Option<String> {
    match expr {
        HirExpr::VarRef(name, _) => Some(name.clone()),
        HirExpr::FieldAccess(base, _, _) => get_root_var(base),
        HirExpr::IndexExpr(base, _, _) => get_root_var(base),
        HirExpr::Deref(base, _) => None, // Deref breaks the root var path, since it's a pointer indirection
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
            for (borrowed, kind) in &scope.borrows {
                if borrowed == name {
                    if *kind == BorrowKind::Mut {
                        return Err(BorrowError::MutableBorrowConflict(name.to_string()));
                    } else {
                        return Err(BorrowError::MutableBorrowConflict(name.to_string()));
                    }
                }
            }
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.borrows.push((name.to_string(), BorrowKind::Mut));
        }
        Ok(())
    }
}
