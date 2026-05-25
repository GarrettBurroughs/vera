#[derive(Debug, Clone)]
pub enum HirType {
    I32,
    Bool,
    Void,
    Struct(String),
    Enum(String),
    Variant(String),
    Array(Box<HirType>, u64),
    Slice(Box<HirType>),
    Result(Box<HirType>, Box<HirType>),
    Ptr(Box<HirType>, bool), // (type, is_mut)
    Ref(Box<HirType>, bool), // (type, is_mut)
    Func(Vec<HirType>, Box<HirType>), // (param_types, ret_type)
    Refinement(Box<HirType>, Box<HirExpr>), // (base_type, condition)
    Error,
}

impl PartialEq for HirType {
    fn eq(&self, other: &Self) -> bool {
        let mut a = self;
        let mut b = other;
        while let HirType::Refinement(base, _) = a { a = base; }
        while let HirType::Refinement(base, _) = b { b = base; }
        
        match (a, b) {
            (HirType::I32, HirType::I32) => true,
            (HirType::Bool, HirType::Bool) => true,
            (HirType::Void, HirType::Void) => true,
            (HirType::Struct(s1), HirType::Struct(s2)) => s1 == s2,
            (HirType::Enum(e1), HirType::Enum(e2)) => e1 == e2,
            (HirType::Variant(v1), HirType::Variant(v2)) => v1 == v2,
            (HirType::Array(t1, s1), HirType::Array(t2, s2)) => t1 == t2 && s1 == s2,
            (HirType::Slice(t1), HirType::Slice(t2)) => t1 == t2,
            (HirType::Result(o1, e1), HirType::Result(o2, e2)) => o1 == o2 && e1 == e2,
            (HirType::Ptr(t1, m1), HirType::Ptr(t2, m2)) => t1 == t2 && m1 == m2,
            (HirType::Ref(t1, m1), HirType::Ref(t2, m2)) => t1 == t2 && m1 == m2,
            (HirType::Func(p1, r1), HirType::Func(p2, r2)) => p1 == p2 && r1 == r2,
            (HirType::Error, HirType::Error) => true,
            _ => false,
        }
    }
}
impl Eq for HirType {}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)] // type_aliases and enums fields used in codegen via pattern matching
pub struct HirProgram {
    pub type_aliases: std::collections::BTreeMap<String, HirType>,
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
    pub assigns: Vec<HirExpr>,
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
    While(HirExpr, HirBlock, Vec<HirExpr>, Option<HirExpr>, Vec<HirExpr>), // condition, body, invariants, decreases, assigns
    For(String, HirExpr, HirBlock, Vec<HirExpr>), // item_name, iterable, body, assigns
    Break,
    Continue,
    GhostBlock(HirBlock),
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinaryOp {
    Add, Sub, Mul, Div, Rem,
    Eq, Neq, Lt, Gt, Le, Ge,
    And, Or, Implies, Iff, Assign,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnaryOp {
    Neg, Not,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuantifierKind {
    Forall,
    Exists,
    Choose,
}

#[derive(Debug, Clone)]
pub enum HirExpr {
    IntLiteral(i64, HirType),
    BoolLiteral(bool, HirType),
    BinaryOp(BinaryOp, Box<HirExpr>, Box<HirExpr>, HirType),
    UnaryOp(UnaryOp, Box<HirExpr>, HirType),
    VarRef(String, HirType),
    Call(String, Vec<HirExpr>, HirType),
    CallIndirect(Box<HirExpr>, Vec<HirExpr>, HirType), // callee, args, type
    If(Box<HirExpr>, HirBlock, Option<HirBlock>, HirType),
    StructExpr(String, Vec<(String, HirExpr)>, HirType),
    FieldAccess(Box<HirExpr>, String, HirType),
    #[allow(dead_code)] // enum_name and variant_name used during LLVM codegen discriminant resolution
    EnumVariant(String, String, u64, HirType), // enum_name, variant_name, value, type
    VariantConstructor(String, String, Vec<HirExpr>, HirType), // variant_name, case_name, args, type
    Match(Box<HirExpr>, Vec<(HirPattern, HirExpr)>, HirType),
    ArrayExpr(Vec<HirExpr>, HirType),
    IndexExpr(Box<HirExpr>, Box<HirExpr>, HirType),
    SliceExpr(Box<HirExpr>, Box<HirExpr>, Box<HirExpr>, HirType), // base, start, end, type
    Try(Box<HirExpr>, HirType), // inner expr, ok type
    ResultOk(Box<HirExpr>, HirType), // inner expr, result type
    ResultErr(Box<HirExpr>, HirType), // inner expr, result type
    Ref(Box<HirExpr>, bool, HirType), // inner expr, is_mut, result type
    Deref(Box<HirExpr>, HirType), // inner expr, result type
    Block(HirBlock, HirType),
    Closure(Vec<String>, Box<HirExpr>, Vec<String>, HirType), // params, body, captures, type
    Quantifier(QuantifierKind, Vec<(String, HirType)>, Box<HirExpr>, HirType), // kind, params, body, type
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
            HirExpr::CallIndirect(_, _, ty) => ty.clone(),
            HirExpr::If(_, _, _, ty) => ty.clone(),
            HirExpr::StructExpr(_, _, ty) => ty.clone(),
            HirExpr::FieldAccess(_, _, ty) => ty.clone(),
            HirExpr::EnumVariant(_, _, _, ty) => ty.clone(),
            HirExpr::VariantConstructor(_, _, _, ty) => ty.clone(),
            HirExpr::Match(_, _, ty) => ty.clone(),
            HirExpr::ArrayExpr(_, ty) => ty.clone(),
            HirExpr::IndexExpr(_, _, ty) => ty.clone(),
            HirExpr::SliceExpr(_, _, _, ty) => ty.clone(),
            HirExpr::Try(_, ty) => ty.clone(),
            HirExpr::ResultOk(_, ty) => ty.clone(),
            HirExpr::ResultErr(_, ty) => ty.clone(),
            HirExpr::Ref(_, _, ty) => ty.clone(),
            HirExpr::Deref(_, ty) => ty.clone(),
            HirExpr::Block(_, ty) => ty.clone(),
            HirExpr::Closure(_, _, _, ty) => ty.clone(),
            HirExpr::Quantifier(_, _, _, ty) => ty.clone(),
            HirExpr::Error => HirType::Error,
        }
    }

    pub fn is_lvalue(&self) -> bool {
        matches!(self, 
            HirExpr::VarRef(..) | 
            HirExpr::FieldAccess(..) | 
            HirExpr::IndexExpr(..) | 
            HirExpr::Deref(..)
        )
    }
}

pub mod lower;
pub mod borrowck;

#[derive(Debug, Clone)]
pub enum HirPattern {
    VariantCase(String, Vec<String>), // CaseName, bindings
    Binding(String),
    #[allow(dead_code)] // Scaffolded for future literal pattern matching
    Literal(HirExpr),
    Wildcard,
}
