#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirType {
    I32,
    Bool,
    Void,
    Struct(String),
    Ptr(Box<HirType>),
    Ref(Box<HirType>),
    Error,
}

#[derive(Debug, Clone)]
pub struct HirProgram {
    pub structs: std::collections::BTreeMap<String, Vec<(String, HirType)>>,
    pub functions: Vec<HirFunc>,
}

#[derive(Debug, Clone)]
pub struct HirFunc {
    pub name: String,
    pub params: Vec<(String, HirType)>,
    pub ret_type: HirType,
    pub body: HirBlock,
    pub requires: Vec<HirExpr>,
    pub ensures: Vec<HirExpr>,
}

#[derive(Debug, Clone)]
pub struct HirBlock {
    pub statements: Vec<HirStmt>,
}

#[derive(Debug, Clone)]
pub enum HirStmt {
    Let(String, bool, HirType, HirExpr), // name, is_const, type, initializer
    Expr(HirExpr),
    Return(Option<HirExpr>),
    Assert(HirExpr),
    Assume(HirExpr),
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinaryOp {
    Add, Sub, Mul, Div, Rem,
    Eq, Neq, Lt, Gt, Le, Ge,
    And, Or, Assign,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnaryOp {
    Neg, Not,
}

#[derive(Debug, Clone)]
pub enum HirExpr {
    IntLiteral(i64, HirType),
    BoolLiteral(bool, HirType),
    BinaryOp(BinaryOp, Box<HirExpr>, Box<HirExpr>, HirType),
    UnaryOp(UnaryOp, Box<HirExpr>, HirType),
    VarRef(String, HirType),
    Call(String, Vec<HirExpr>, HirType),
    If(Box<HirExpr>, HirBlock, Option<HirBlock>, HirType),
    StructExpr(String, Vec<(String, HirExpr)>, HirType),
    FieldAccess(Box<HirExpr>, String, HirType),
    Error,
}

impl HirExpr {
    pub fn ty(&self) -> HirType {
        match self {
            HirExpr::IntLiteral(_, ty) => ty.clone(),
            HirExpr::BoolLiteral(_, ty) => ty.clone(),
            HirExpr::BinaryOp(_, _, _, ty) => ty.clone(),
            HirExpr::UnaryOp(_, _, ty) => ty.clone(),
            HirExpr::VarRef(_, ty) => ty.clone(),
            HirExpr::Call(_, _, ty) => ty.clone(),
            HirExpr::If(_, _, _, ty) => ty.clone(),
            HirExpr::StructExpr(_, _, ty) => ty.clone(),
            HirExpr::FieldAccess(_, _, ty) => ty.clone(),
            HirExpr::Error => HirType::Error,
        }
    }
}

pub mod lower;
