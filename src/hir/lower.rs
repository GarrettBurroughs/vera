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
    pub functions: BTreeMap<String, (Vec<(String, HirType)>, HirType)>, // name -> (params, ret_ty)
    pub structs: BTreeMap<String, Vec<(String, HirType)>>, // name -> fields
    current_func_ret_type: HirType,
}

impl LoweringContext {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            scopes: Vec::new(),
            functions: BTreeMap::new(),
            structs: BTreeMap::new(),
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
        // Pass 0: Gather structs
        for s in source_file.structs() {
            let name = s.name().map(|n| n.text().to_string()).unwrap_or_default();
            let mut fields = Vec::new();
            for f in s.fields() {
                if let (Some(f_name), Some(f_ty_ref)) = (f.name(), f.ty()) {
                    let f_ty = self.lower_type(&f_ty_ref);
                    fields.push((f_name.text().to_string(), f_ty));
                }
            }
            self.structs.insert(name, fields);
        }

        // Pass 1: Gather signatures
        for func in source_file.functions() {
            let name = func.name().map(|n| n.text().to_string()).unwrap_or_default();
            let ret_type = match func.ret_type() {
                Some(type_ref) => self.lower_type(&type_ref),
                None => HirType::Void,
            };
            let mut params = Vec::new();
            if let Some(param_list) = func.param_list() {
                for param in param_list.params() {
                    if let (Some(p_name), Some(p_ty_ref)) = (param.name(), param.ty()) {
                        let p_ty = self.lower_type(&p_ty_ref);
                        params.push((p_name, p_ty));
                    }
                }
            }
            self.functions.insert(name, (params, ret_type));
        }

        // Pass 2: Lower bodies
        let mut functions = Vec::new();
        for func in source_file.functions() {
            if let Some(f) = self.lower_func(&func) {
                functions.push(f);
            }
        }
        HirProgram { structs: self.structs.clone(), functions }
    }

    fn lower_func(&mut self, func: &ast::FuncDecl) -> Option<HirFunc> {
        let name = func.name()?.text().to_string();
        
        let ret_type = match func.ret_type() {
            Some(type_ref) => self.lower_type(&type_ref),
            None => HirType::Void,
        };

        self.enter_scope(); // Function scope
        self.current_func_ret_type = ret_type.clone();

        let mut params = Vec::new();
        if let Some((sig_params, _)) = self.functions.get(&name) {
            params = sig_params.clone();
            for (p_name, p_ty) in &params {
                self.declare_var(p_name.clone(), p_ty.clone(), false);
            }
        }

        // Spec clauses (requires/ensures) are lowered AFTER entering scope and declaring
        // parameters, because they can reference the function's formal parameters.
        let mut requires = Vec::new();
        let mut ensures = Vec::new();
        if let Some(spec) = func.spec() {
            for req in spec.requires_clauses() {
                if let Some(e) = req.expr() {
                    requires.push(self.lower_expr(&e));
                }
            }
            for ens in spec.ensures_clauses() {
                if let Some(e) = ens.expr() {
                    ensures.push(self.lower_expr(&e));
                }
            }
        }

        let body = match func.body() {
            Some(block) => self.lower_block(&block),
            None => HirBlock { statements: Vec::new() },
        };

        self.exit_scope();
        self.current_func_ret_type = HirType::Void;

        Some(HirFunc {
            name,
            params,
            ret_type,
            body,
            requires,
            ensures,
        })
    }

    fn lower_type(&mut self, type_ref: &ast::TypeRef) -> HirType {
        let name = type_ref.as_string().unwrap_or_default();
        match name.as_str() {
            "i32" => HirType::I32,
            "bool" => HirType::Bool,
            "" => HirType::Error,
            _ => {
                if self.structs.contains_key(&name) {
                    HirType::Struct(name)
                } else {
                    self.errors.push(SemanticError::UnknownType { name });
                    HirType::Error
                }
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
            ast::Stmt::AssertStmt(assert_stmt) => {
                let expr = assert_stmt.expr().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::Error);
                HirStmt::Assert(expr)
            }
            ast::Stmt::AssumeStmt(assume_stmt) => {
                let expr = assume_stmt.expr().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::Error);
                HirStmt::Assume(expr)
            }
            ast::Stmt::WhileStmt(while_stmt) => {
                let cond = while_stmt.condition().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::Error);
                if cond.ty() != HirType::Error && cond.ty() != HirType::Bool {
                    self.errors.push(SemanticError::TypeMismatch {
                        expected: HirType::Bool,
                        found: cond.ty(),
                    });
                }
                
                let mut invariants = Vec::new();
                if let Some(spec) = while_stmt.spec() {
                    for inv in spec.invariant_clauses() {
                        if let Some(expr) = inv.expr() {
                            invariants.push(self.lower_expr(&expr));
                        }
                    }
                }
                
                let body = while_stmt.body().map(|b| self.lower_block(&b)).unwrap_or(HirBlock { statements: Vec::new() });
                HirStmt::While(cond, body, invariants)
            }
            ast::Stmt::BreakStmt(_) => {
                HirStmt::Break
            }
            ast::Stmt::ContinueStmt(_) => {
                HirStmt::Continue
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
            ast::Expr::CallExpr(call_expr) => {
                if let Some(ast::Expr::NameRef(name_ref)) = call_expr.callee() {
                    let name = name_ref.ident().map(|n| n.text().to_string()).unwrap_or_default();
                    let func_info = self.functions.get(&name).cloned();
                    if let Some((sig_params, ret_ty)) = func_info {
                        let mut args = Vec::new();
                        if let Some(arg_list) = call_expr.arg_list() {
                            for arg in arg_list.args() {
                                args.push(self.lower_expr(&arg));
                            }
                        }
                        if args.len() != sig_params.len() {
                            // In a real compiler we'd report an arity error
                            HirExpr::Error
                        } else {
                            HirExpr::Call(name, args, ret_ty.clone())
                        }
                    } else {
                        self.errors.push(SemanticError::UndefinedVariable { name });
                        HirExpr::Error
                    }
                } else {
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
            ast::Expr::StructExpr(struct_expr) => {
                let name = struct_expr.name().and_then(|n| n.ident()).map(|i| i.text().to_string()).unwrap_or_default();
                let mut field_exprs = Vec::new();
                for f in struct_expr.fields() {
                    let f_name = f.name().map(|n| n.text().to_string()).unwrap_or_default();
                    let f_expr = f.expr().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::Error);
                    field_exprs.push((f_name, f_expr));
                }
                
                if let Some(def_fields) = self.structs.get(&name) {
                    // Type check fields
                    for (f_name, f_expr) in &field_exprs {
                        if let Some((_, def_ty)) = def_fields.iter().find(|(n, _)| n == f_name) {
                            if f_expr.ty() != HirType::Error && *def_ty != f_expr.ty() {
                                self.errors.push(SemanticError::TypeMismatch {
                                    expected: def_ty.clone(),
                                    found: f_expr.ty(),
                                });
                            }
                        } else {
                            self.errors.push(SemanticError::UndefinedVariable { name: format!("field {} in struct {}", f_name, name) });
                        }
                    }
                    HirExpr::StructExpr(name.clone(), field_exprs, HirType::Struct(name))
                } else {
                    self.errors.push(SemanticError::UnknownType { name: name.clone() });
                    HirExpr::Error
                }
            }
            ast::Expr::FieldExpr(field_expr) => {
                let base = field_expr.base().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::Error);
                let field_name = field_expr.field().and_then(|n| n.ident()).map(|i| i.text().to_string()).unwrap_or_default();
                
                if let HirType::Struct(s_name) = base.ty() {
                    if let Some(def_fields) = self.structs.get(&s_name) {
                        if let Some((_, def_ty)) = def_fields.iter().find(|(n, _)| n == &field_name) {
                            HirExpr::FieldAccess(Box::new(base), field_name, def_ty.clone())
                        } else {
                            self.errors.push(SemanticError::UndefinedVariable { name: format!("field {} in struct {}", field_name, s_name) });
                            HirExpr::Error
                        }
                    } else {
                        HirExpr::Error
                    }
                } else if base.ty() != HirType::Error {
                    self.errors.push(SemanticError::UndefinedVariable { name: format!("field access on non-struct type {:?}", base.ty()) });
                    HirExpr::Error
                } else {
                    HirExpr::Error
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{Parser, ast::{AstNode, SourceFile}};
    use crate::hir::{HirType, HirStmt, HirExpr, BinaryOp};

    /// Parses `src` and runs the HIR lowering pass.
    /// Returns `(HirProgram, Vec<SemanticError>)`.
    fn parse_and_lower(src: &str) -> (HirProgram, Vec<SemanticError>) {
        let (cst, _parse_errors) = Parser::new(src).parse();
        let source_file = SourceFile::cast(cst).expect("Root is not a SourceFile");
        let mut ctx = LoweringContext::new();
        let program = ctx.lower_program(&source_file);
        (program, ctx.errors)
    }

    /// A minimal `func main(): i32 { return 0; }` lowers to one function
    /// with name "main", return type I32, and no parameters.
    #[test]
    fn test_lower_basic_function() {
        let (prog, errors) = parse_and_lower("func main(): i32 { return 0; }");
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert_eq!(prog.functions.len(), 1);
        let f = &prog.functions[0];
        assert_eq!(f.name, "main");
        assert_eq!(f.ret_type, HirType::I32);
        assert!(f.params.is_empty());
    }

    /// A function with two i32 parameters lowers them in the correct order.
    #[test]
    fn test_lower_function_params() {
        let (prog, errors) = parse_and_lower("func add(a: i32, b: i32): i32 { return a; }");
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        let f = &prog.functions[0];
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.params[0], ("a".to_string(), HirType::I32));
        assert_eq!(f.params[1], ("b".to_string(), HirType::I32));
    }

    /// `const x: i32 = 1;` lowers to `HirStmt::Let("x", is_const=true, I32, IntLiteral(1))`.
    #[test]
    fn test_lower_const_let() {
        let (prog, errors) = parse_and_lower("func f(): i32 { const x: i32 = 1; return x; }");
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        let stmts = &prog.functions[0].body.statements;
        if let HirStmt::Let(name, is_const, ty, init) = &stmts[0] {
            assert_eq!(name, "x");
            assert!(*is_const, "expected const binding");
            assert_eq!(*ty, HirType::I32);
            assert!(matches!(init, HirExpr::IntLiteral(1, _)));
        } else {
            panic!("Expected HirStmt::Let, got {:?}", stmts[0]);
        }
    }

    /// `var y: i32 = 2;` lowers with `is_const = false`.
    #[test]
    fn test_lower_var_let() {
        let (prog, errors) = parse_and_lower("func f(): i32 { var y: i32 = 2; return y; }");
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        if let HirStmt::Let(name, is_const, _, _) = &prog.functions[0].body.statements[0] {
            assert_eq!(name, "y");
            assert!(!is_const, "expected mutable binding");
        } else {
            panic!("Expected HirStmt::Let");
        }
    }

    /// Without an explicit type annotation the type is inferred from the initializer.
    #[test]
    fn test_lower_type_inferred_from_initializer() {
        let (prog, errors) = parse_and_lower("func f(): i32 { const x = 42; return x; }");
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        if let HirStmt::Let(_, _, ty, _) = &prog.functions[0].body.statements[0] {
            assert_eq!(*ty, HirType::I32, "type should be inferred as I32");
        } else {
            panic!("Expected HirStmt::Let");
        }
    }

    /// Returning a bool from an i32 function produces a TypeMismatch error.
    #[test]
    fn test_lower_return_type_mismatch() {
        let (_, errors) = parse_and_lower("func f(): i32 { return true; }");
        assert!(
            errors.iter().any(|e| matches!(e, SemanticError::TypeMismatch { .. })),
            "expected TypeMismatch error, got {:?}", errors
        );
    }

    /// Using an undeclared variable emits an UndefinedVariable error.
    #[test]
    fn test_lower_undefined_variable() {
        let (_, errors) = parse_and_lower("func f(): i32 { return z; }");
        assert!(
            errors.iter().any(|e| matches!(e, SemanticError::UndefinedVariable { .. })),
            "expected UndefinedVariable error, got {:?}", errors
        );
    }

    /// Assigning to a `const` variable emits an ImmutableAssignment error.
    #[test]
    fn test_lower_immutable_assignment() {
        let src = "func f(): i32 { const x: i32 = 1; x = 2; return x; }";
        let (_, errors) = parse_and_lower(src);
        assert!(
            errors.iter().any(|e| matches!(e, SemanticError::ImmutableAssignment { .. })),
            "expected ImmutableAssignment error, got {:?}", errors
        );
    }

    /// Adding an i32 and a bool emits a BinOpMismatch error.
    #[test]
    fn test_lower_binop_type_mismatch() {
        let src = "func f(): i32 { const x: i32 = 1 + true; return x; }";
        let (_, errors) = parse_and_lower(src);
        assert!(
            errors.iter().any(|e| matches!(e, SemanticError::BinOpMismatch { .. })),
            "expected BinOpMismatch error, got {:?}", errors
        );
    }

    /// An if-condition that is not bool emits a TypeMismatch error.
    #[test]
    fn test_lower_if_condition_must_be_bool() {
        let src = "func f(): i32 { if 1 { return 0; } return 1; }";
        let (_, errors) = parse_and_lower(src);
        assert!(
            errors.iter().any(|e| matches!(e, SemanticError::TypeMismatch { .. })),
            "expected TypeMismatch for non-bool if-condition, got {:?}", errors
        );
    }

    /// Multiple functions in one source file are all lowered.
    #[test]
    fn test_lower_multiple_functions() {
        let src = "func a(): i32 { return 1; } func b(): i32 { return 2; }";
        let (prog, errors) = parse_and_lower(src);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert_eq!(prog.functions.len(), 2);
    }

    /// `spec { requires x > 0; ensures true; }` clauses are extracted into
    /// the `requires` and `ensures` fields of `HirFunc`.
    #[test]
    fn test_lower_spec_clauses() {
        let src = r#"
            func f(x: i32): i32
            spec {
                requires x > 0;
                ensures true;
            }
            { return x; }
        "#;
        let (prog, errors) = parse_and_lower(src);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        let f = &prog.functions[0];
        assert_eq!(f.requires.len(), 1, "expected one requires clause");
        assert_eq!(f.ensures.len(), 1, "expected one ensures clause");
    }

    /// Struct declarations are lowered into `prog.structs` with correct field types.
    #[test]
    fn test_lower_struct_decl() {
        let src = "struct Point { x: i32, y: i32 } func f(): i32 { return 0; }";
        let (prog, errors) = parse_and_lower(src);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        let fields = prog.structs.get("Point").expect("Point struct not found");
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0], ("x".to_string(), HirType::I32));
        assert_eq!(fields[1], ("y".to_string(), HirType::I32));
    }

    /// Field access on a struct resolves to the correct field type (I32 for `p.x`).
    #[test]
    fn test_lower_field_access_type() {
        let src = r#"
            struct Point { x: i32, y: i32 }
            func f(): i32 {
                const p: Point = Point { x: 1, y: 2 };
                return p.x;
            }
        "#;
        let (prog, errors) = parse_and_lower(src);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        let stmts = &prog.functions[0].body.statements;
        if let HirStmt::Return(Some(expr)) = stmts.last().unwrap() {
            assert_eq!(expr.ty(), HirType::I32, "field access should have type I32");
        } else {
            panic!("Expected Return statement");
        }
    }

    /// Binary arithmetic lowers to `HirExpr::BinaryOp` with the correct operator and result type.
    #[test]
    fn test_lower_binary_add() {
        let src = "func f(): i32 { return 1 + 2; }";
        let (prog, errors) = parse_and_lower(src);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        if let HirStmt::Return(Some(expr)) = &prog.functions[0].body.statements[0] {
            if let HirExpr::BinaryOp(op, _, _, ty) = expr {
                assert_eq!(*op, BinaryOp::Add);
                assert_eq!(*ty, HirType::I32);
            } else {
                panic!("Expected BinaryOp, got {:?}", expr);
            }
        } else {
            panic!("Expected Return");
        }
    }
}
