use miette::Diagnostic;
use thiserror::Error;
use crate::parser::ast::{self, AstNode};
use crate::hir::{HirProgram, HirFunc, HirType, HirBlock, HirStmt, HirExpr};

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
}

pub struct LoweringContext {
    pub errors: Vec<SemanticError>,
}

impl LoweringContext {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
        }
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
        
        let body = match func.body() {
            Some(block) => self.lower_block(&block, &ret_type),
            None => HirBlock { statements: Vec::new() },
        };

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

    fn lower_block(&mut self, block: &ast::BlockExpr, expected_ret_type: &HirType) -> HirBlock {
        let mut statements = Vec::new();
        
        if let Some(ret_stmt) = block.return_stmt() {
            statements.push(self.lower_return_stmt(&ret_stmt, expected_ret_type));
        }

        HirBlock { statements }
    }

    fn lower_return_stmt(&mut self, ret_stmt: &ast::ReturnStmt, expected_ret_type: &HirType) -> HirStmt {
        if let Some(expr_token) = ret_stmt.expr_token() {
            let expr = if expr_token.kind() == crate::parser::syntax::SyntaxKind::IntLit {
                let val: i64 = expr_token.text().parse().unwrap_or(0);
                HirExpr::IntLiteral(val, HirType::I32)
            } else if expr_token.kind() == crate::parser::syntax::SyntaxKind::BoolTrue {
                HirExpr::BoolLiteral(true, HirType::Bool)
            } else if expr_token.kind() == crate::parser::syntax::SyntaxKind::BoolFalse {
                HirExpr::BoolLiteral(false, HirType::Bool)
            } else {
                HirExpr::Error
            };
            
            if expr.ty() != HirType::Error && expr.ty() != *expected_ret_type && *expected_ret_type != HirType::Error {
                self.errors.push(SemanticError::TypeMismatch {
                    expected: expected_ret_type.clone(),
                    found: expr.ty(),
                });
            }
            
            HirStmt::Return(Some(expr))
        } else {
            if *expected_ret_type != HirType::Void {
                self.errors.push(SemanticError::TypeMismatch {
                    expected: expected_ret_type.clone(),
                    found: HirType::Void,
                });
            }
            HirStmt::Return(None)
        }
    }
}
