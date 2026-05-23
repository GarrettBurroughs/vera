#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirType {
    I32,
    Bool,
    Void,
    Error,
}

#[derive(Debug, Clone)]
pub struct HirProgram {
    pub functions: Vec<HirFunc>,
}

#[derive(Debug, Clone)]
pub struct HirFunc {
    pub name: String,
    pub ret_type: HirType,
    pub body: HirBlock,
}

#[derive(Debug, Clone)]
pub struct HirBlock {
    pub statements: Vec<HirStmt>,
}

#[derive(Debug, Clone)]
pub enum HirStmt {
    Return(Option<HirExpr>),
    Error,
}

#[derive(Debug, Clone)]
pub enum HirExpr {
    IntLiteral(i64, HirType),
    BoolLiteral(bool, HirType),
    Error,
}

impl HirExpr {
    pub fn ty(&self) -> HirType {
        match self {
            HirExpr::IntLiteral(_, ty) => ty.clone(),
            HirExpr::BoolLiteral(_, ty) => ty.clone(),
            HirExpr::Error => HirType::Error,
        }
    }
}

pub mod lower;
