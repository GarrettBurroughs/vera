use miette::Diagnostic;
use thiserror::Error;
use std::collections::BTreeMap;
use crate::parser::ast::{self, AstNode};
use crate::hir::{HirProgram, HirFunc, HirType, HirBlock, HirStmt, HirExpr, BinaryOp, UnaryOp};

#[derive(Error, Debug, Diagnostic)]
pub enum SemanticError {
    #[error("Type mismatch: expected {expected:?}, found {found:?}")]
    #[diagnostic(code(vera::type_mismatch))]
    TypeMismatch {
        expected: HirType,
        found: HirType,
    },
    
    #[error("Unknown type: {name}")]
    #[diagnostic(code(vera::unknown_type))]
    UnknownType {
        name: String,
    },

    #[error("Undefined variable: {name}")]
    #[diagnostic(code(vera::undefined_variable))]
    UndefinedVariable {
        name: String,
    },

    #[error("Cannot mutate constant variable: {name}")]
    #[diagnostic(code(vera::immutable_assignment))]
    ImmutableAssignment {
        name: String,
    },

    #[error("Binary operator mismatch: cannot apply {op} to {lhs:?} and {rhs:?}")]
    #[diagnostic(code(vera::bin_op_mismatch))]
    BinOpMismatch {
        op: String,
        lhs: HirType,
        rhs: HirType,
    },
}

#[derive(Clone)]
struct Scope {
    variables: BTreeMap<String, (HirType, bool)>, // type, is_const
}

pub struct LoweringContext {
    pub errors: Vec<SemanticError>,
    scopes: Vec<Scope>,
    current_func_ret_type: HirType,
}

impl LoweringContext {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            scopes: Vec::new(),
            current_func_ret_type: HirType::Void,
        }
    }

    fn enter_scope(&mut self) {
        self.scopes.push(Scope { variables: BTreeMap::new() });
    }

    fn exit_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare_var(&mut self, name: String, ty: HirType, is_const: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.variables.insert(name, (ty, is_const));
        }
    }

    fn lookup_var(&self, name: &str) -> Option<(HirType, bool)> {
        for scope in self.scopes.iter().rev() {
            if let Some(var) = scope.variables.get(name) {
                return Some(var.clone());
            }
        }
        None
    }

    pub fn lower_program(&mut self, source_file: &ast::SourceFile) -> HirProgram {
        let mut functions = Vec::new();
        for func in source_file.functions() {
            if let Some(f) = self.lower_func(&func) {
                functions.push(f);
            }
        }
        HirProgram { functions }
    }

    fn lower_func(&mut self, func: &ast::FuncDecl) -> Option<HirFunc> {
        let name = func.name()?.text().to_string();
        
        let ret_type = match func.ret_type() {
            Some(type_ref) => self.lower_type(&type_ref),
            None => HirType::Void,
        };
        
        self.enter_scope(); // Function scope
        self.current_func_ret_type = ret_type.clone();

        // TODO: Lower params and insert them into scope

        let body = match func.body() {
            Some(block) => self.lower_block(&block),
            None => HirBlock { statements: Vec::new() },
        };

        self.exit_scope();
        self.current_func_ret_type = HirType::Void;

        Some(HirFunc {
            name,
            ret_type,
            body,
        })
    }

    fn lower_type(&mut self, type_ref: &ast::TypeRef) -> HirType {
        let name = type_ref.as_string().unwrap_or_default();
        match name.as_str() {
            "i32" => HirType::I32,
            "bool" => HirType::Bool,
            "" => HirType::Error,
            _ => {
                self.errors.push(SemanticError::UnknownType { name });
                HirType::Error
            }
        }
    }

    fn lower_block(&mut self, block: &ast::BlockExpr) -> HirBlock {
        self.enter_scope(); // Block scope
        let mut statements = Vec::new();
        
        for stmt in block.statements() {
            statements.push(self.lower_stmt(&stmt));
        }

        self.exit_scope();
        HirBlock { statements }
    }

    fn lower_stmt(&mut self, stmt: &ast::Stmt) -> HirStmt {
        match stmt {
            ast::Stmt::ReturnStmt(ret_stmt) => {
                let expr = ret_stmt.expr().map(|e| self.lower_expr(&e));
                
                let expr_ty = expr.as_ref().map(|e| e.ty()).unwrap_or(HirType::Void);
                let expected = self.current_func_ret_type.clone();
                if expr_ty != HirType::Error && expr_ty != expected && expected != HirType::Error {
                    self.errors.push(SemanticError::TypeMismatch {
                        expected,
                        found: expr_ty,
                    });
                }
                
                HirStmt::Return(expr)
            }
            ast::Stmt::LetStmt(let_stmt) => {
                let name = let_stmt.name().map(|n| n.text().to_string()).unwrap_or_default();
                let is_const = let_stmt.is_const();
                
                let initializer = if let Some(expr) = let_stmt.initializer() {
                    self.lower_expr(&expr)
                } else {
                    HirExpr::Error
                };
                
                let declared_ty = if let Some(ty_ref) = let_stmt.ty() {
                    self.lower_type(&ty_ref)
                } else {
                    initializer.ty()
                };

                if initializer.ty() != HirType::Error && declared_ty != HirType::Error && initializer.ty() != declared_ty {
                    self.errors.push(SemanticError::TypeMismatch {
                        expected: declared_ty.clone(),
                        found: initializer.ty(),
                    });
                }

                self.declare_var(name.clone(), declared_ty.clone(), is_const);

                HirStmt::Let(name, is_const, declared_ty, initializer)
            }
            ast::Stmt::ExprStmt(expr_stmt) => {
                if let Some(expr) = expr_stmt.expr() {
                    HirStmt::Expr(self.lower_expr(&expr))
                } else {
                    HirStmt::Error
                }
            }
            ast::Stmt::IfExpr(if_expr) => {
                HirStmt::Expr(self.lower_if_expr(if_expr))
            }
        }
    }

    fn lower_expr(&mut self, expr: &ast::Expr) -> HirExpr {
        match expr {
            ast::Expr::Literal(lit) => {
                if let Some(tok) = lit.token() {
                    if tok.kind() == crate::parser::syntax::SyntaxKind::IntLit {
                        let val: i64 = tok.text().parse().unwrap_or(0);
                        HirExpr::IntLiteral(val, HirType::I32)
                    } else if tok.kind() == crate::parser::syntax::SyntaxKind::BoolTrue {
                        HirExpr::BoolLiteral(true, HirType::Bool)
                    } else if tok.kind() == crate::parser::syntax::SyntaxKind::BoolFalse {
                        HirExpr::BoolLiteral(false, HirType::Bool)
                    } else {
                        HirExpr::Error
                    }
                } else {
                    HirExpr::Error
                }
            }
            ast::Expr::NameRef(name_ref) => {
                let name = name_ref.ident().map(|n| n.text().to_string()).unwrap_or_default();
                if let Some((ty, _is_const)) = self.lookup_var(&name) {
                    HirExpr::VarRef(name, ty)
                } else {
                    self.errors.push(SemanticError::UndefinedVariable { name: name.clone() });
                    HirExpr::Error
                }
            }
            ast::Expr::BinExpr(bin_expr) => {
                let lhs = bin_expr.lhs().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::Error);
                let rhs = bin_expr.rhs().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::Error);
                let op_tok = bin_expr.op();
                
                if let Some(tok) = op_tok {
                    use crate::parser::syntax::SyntaxKind::*;
                    let (op, expected_ty, ret_ty) = match tok.kind() {
                        Plus => (BinaryOp::Add, HirType::I32, HirType::I32),
                        Minus => (BinaryOp::Sub, HirType::I32, HirType::I32),
                        Star => (BinaryOp::Mul, HirType::I32, HirType::I32),
                        Slash => (BinaryOp::Div, HirType::I32, HirType::I32),
                        Percent => (BinaryOp::Rem, HirType::I32, HirType::I32),
                        EqEq => (BinaryOp::Eq, lhs.ty(), HirType::Bool),
                        BangEq => (BinaryOp::Neq, lhs.ty(), HirType::Bool),
                        Less => (BinaryOp::Lt, HirType::I32, HirType::Bool),
                        Greater => (BinaryOp::Gt, HirType::I32, HirType::Bool),
                        LessEq => (BinaryOp::Le, HirType::I32, HirType::Bool),
                        GreaterEq => (BinaryOp::Ge, HirType::I32, HirType::Bool),
                        AmpAmp => (BinaryOp::And, HirType::Bool, HirType::Bool),
                        PipePipe => (BinaryOp::Or, HirType::Bool, HirType::Bool),
                        Eq => (BinaryOp::Assign, lhs.ty(), lhs.ty()), // Assignment returns the value
                        _ => return HirExpr::Error,
                    };

                    if op == BinaryOp::Assign {
                        // Check if lhs is a VarRef and if it's mutable
                        if let ast::Expr::NameRef(name_ref) = &bin_expr.lhs().unwrap() {
                            let name = name_ref.ident().map(|n| n.text().to_string()).unwrap_or_default();
                            if let Some((_, is_const)) = self.lookup_var(&name) {
                                if is_const {
                                    self.errors.push(SemanticError::ImmutableAssignment { name });
                                }
                            }
                        } else {
                            // Can only assign to variables for now
                            self.errors.push(SemanticError::UndefinedVariable { name: "invalid assignment target".to_string() });
                        }
                    }

                    if lhs.ty() != HirType::Error && rhs.ty() != HirType::Error {
                        if op != BinaryOp::Eq && op != BinaryOp::Neq && op != BinaryOp::Assign {
                            if lhs.ty() != expected_ty || rhs.ty() != expected_ty {
                                self.errors.push(SemanticError::BinOpMismatch {
                                    op: tok.text().to_string(),
                                    lhs: lhs.ty(),
                                    rhs: rhs.ty(),
                                });
                                return HirExpr::Error;
                            }
                        } else {
                            if lhs.ty() != rhs.ty() {
                                self.errors.push(SemanticError::BinOpMismatch {
                                    op: tok.text().to_string(),
                                    lhs: lhs.ty(),
                                    rhs: rhs.ty(),
                                });
                                return HirExpr::Error;
                            }
                        }
                    }

                    HirExpr::BinaryOp(op, Box::new(lhs), Box::new(rhs), ret_ty)
                } else {
                    HirExpr::Error
                }
            }
            ast::Expr::PrefixExpr(prefix_expr) => {
                let inner = prefix_expr.expr().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::Error);
                if let Some(op_tok) = prefix_expr.op() {
                    let op = match op_tok.kind() {
                        crate::parser::syntax::SyntaxKind::Minus => UnaryOp::Neg,
                        crate::parser::syntax::SyntaxKind::Bang => UnaryOp::Not,
                        _ => return HirExpr::Error,
                    };
                    
                    let expected_ty = match op {
                        UnaryOp::Neg => HirType::I32,
                        UnaryOp::Not => HirType::Bool,
                    };
                    
                    if inner.ty() != HirType::Error && inner.ty() != expected_ty {
                        self.errors.push(SemanticError::BinOpMismatch {
                            op: op_tok.text().to_string(), // Reusing BinOpMismatch for unary
                            lhs: inner.ty(),
                            rhs: inner.ty(),
                        });
                        return HirExpr::Error;
                    }
                    HirExpr::UnaryOp(op, Box::new(inner), expected_ty)
                } else {
                    HirExpr::Error
                }
            }
            ast::Expr::IfExpr(if_expr) => {
                self.lower_if_expr(if_expr)
            }
        }
    }

    fn lower_if_expr(&mut self, if_expr: &ast::IfExpr) -> HirExpr {
        let cond = if let Some(c) = if_expr.condition() {
            let c_expr = self.lower_expr(&c);
            if c_expr.ty() != HirType::Error && c_expr.ty() != HirType::Bool {
                self.errors.push(SemanticError::TypeMismatch {
                    expected: HirType::Bool,
                    found: c_expr.ty(),
                });
            }
            c_expr
        } else {
            HirExpr::Error
        };

        // For type checking, if-else should return the same type.
        // If it's a statement, we can assume Void. We will simplify for now and return Void.
        let then_block = if let Some(b) = if_expr.then_block() {
            self.lower_block(&b)
        } else {
            HirBlock { statements: Vec::new() }
        };

        let else_block = if let Some(b) = if_expr.else_branch() {
            if b.kind() == crate::parser::syntax::SyntaxKind::BLOCK_EXPR {
                if let Some(block) = ast::BlockExpr::cast(b) {
                    Some(self.lower_block(&block))
                } else {
                    None
                }
            } else if b.kind() == crate::parser::syntax::SyntaxKind::IF_EXPR {
                if let Some(elif) = ast::IfExpr::cast(b) {
                    let elif_expr = self.lower_if_expr(&elif);
                    Some(HirBlock { statements: vec![HirStmt::Expr(elif_expr)] })
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        HirExpr::If(Box::new(cond), then_block, else_block, HirType::Void)
    }
}
