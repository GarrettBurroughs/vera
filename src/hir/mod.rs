#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirType {
    I32,
    Bool,
    Void,
    Struct(String),
    Enum(String),
    Variant(String),
    Array(Box<HirType>, u64),
    Slice(Box<HirType>),
    Ptr(Box<HirType>),
    Ref(Box<HirType>),
    Error,
}

#[derive(Debug, Clone)]
pub struct HirProgram {
    pub structs: std::collections::BTreeMap<String, Vec<(String, HirType)>>,
    pub enums: std::collections::BTreeMap<String, Vec<String>>,
    pub variants: std::collections::BTreeMap<String, Vec<(String, Vec<HirType>)>>,
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
    While(HirExpr, HirBlock, Vec<HirExpr>), // condition, body, invariants
    Break,
    Continue,
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
    EnumVariant(String, String, u64, HirType), // enum_name, variant_name, value, type
    VariantConstructor(String, String, Vec<HirExpr>, HirType), // variant_name, case_name, args, type
    Match(Box<HirExpr>, Vec<(HirPattern, HirExpr)>, HirType),
    ArrayExpr(Vec<HirExpr>, HirType),
    IndexExpr(Box<HirExpr>, Box<HirExpr>, HirType),
    SliceExpr(Box<HirExpr>, Box<HirExpr>, Box<HirExpr>, HirType), // base, start, end, type
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
            HirExpr::EnumVariant(_, _, _, ty) => ty.clone(),
            HirExpr::VariantConstructor(_, _, _, ty) => ty.clone(),
            HirExpr::Match(_, _, ty) => ty.clone(),
            HirExpr::ArrayExpr(_, ty) => ty.clone(),
            HirExpr::IndexExpr(_, _, ty) => ty.clone(),
            HirExpr::SliceExpr(_, _, _, ty) => ty.clone(),
            HirExpr::Error => HirType::Error,
        }
    }
}

pub mod lower;

#[derive(Debug, Clone)]
pub enum HirPattern {
    VariantCase(String, Vec<String>), // CaseName, bindings
    Literal(HirExpr),
    Wildcard,
    Binding(String),
}
