
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SymbolId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Path {
    pub segments: Vec<String>,
}

impl Path {
    pub fn from_ident(ident: String) -> Self {
        Self { segments: vec![ident] }
    }
    
    pub fn as_str(&self) -> String {
        self.segments.join("::")
    }
}

impl std::fmt::Display for Path {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

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
    pub definition_map: std::collections::BTreeMap<crate::workspace::FileId, Vec<(Span, Span)>>,
}

impl HirProgram {
    pub fn new() -> Self {
        Self {
            type_aliases: std::collections::BTreeMap::new(),
            structs: std::collections::BTreeMap::new(),
            enums: std::collections::BTreeMap::new(),
            variants: std::collections::BTreeMap::new(),
            functions: Vec::new(),
            definition_map: std::collections::BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HirFunc {
    pub name: String,
    pub span: Span,
    pub params: Vec<(String, SymbolId, HirType)>,
    pub ret_type: HirType,
    pub ret_sym_id: Option<SymbolId>,
    pub body: Option<HirBlock>,
    pub requires: Vec<HirExpr>,
    pub ensures: Vec<HirExpr>,
    pub assigns: Vec<HirExpr>,
}

/// A source location within a specific file.
///
/// `file_id` matches `workspace::FileId`. Byte offsets (`start`, `end`) are
/// relative to the start of that file's source text. `Span::default()` is a
/// sentinel meaning "unknown location" — callers should use `span_of` / the
/// `LoweringContext::node_span` helper to fill in real positions.
///
/// In lossless parse mode (LSP), rowan's `text_range()` gives exact offsets.
/// In strip mode (CLI builds), rowan omits trivia so offsets differ from the
/// original source; use lossless mode whenever spans are needed for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub file_id: usize,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(file_id: usize, start: u32, end: u32) -> Self {
        Self { file_id, start, end }
    }

    pub fn unknown() -> Self {
        Self { file_id: 0, start: 0, end: 0 }
    }

    /// Returns true when no real location is available (the zero sentinel).
    pub fn is_unknown(self) -> bool {
        self.start == 0 && self.end == 0
    }
}

#[derive(Debug, Clone)]
pub struct HirBlock {
    pub statements: Vec<HirStmt>,
}

#[derive(Debug, Clone)]
pub struct HirStmt {
    pub kind: HirStmtKind,
    pub span: Span,
}

impl HirStmt {
    pub fn new(kind: HirStmtKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[derive(Debug, Clone)]
pub enum HirStmtKind {
    Let(String, SymbolId, bool, HirType, HirExpr), // name, is_const, type, initializer
    Expr(HirExpr),
    Return(Option<HirExpr>),
    Assert(HirExpr),
    Assume(HirExpr),
    While(HirExpr, HirBlock, Vec<HirExpr>, Option<HirExpr>, Vec<HirExpr>), // condition, body, invariants, decreases, assigns
    For(String, SymbolId, HirExpr, HirBlock, Vec<HirExpr>), // item_name, iterable, body, assigns
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
pub struct HirExpr {
    pub kind: HirExprKind,
    pub ty: HirType,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum HirExprKind {
    IntLiteral(i64, HirType),
    BoolLiteral(bool, HirType),
    BinaryOp(BinaryOp, Box<HirExpr>, Box<HirExpr>, HirType),
    UnaryOp(UnaryOp, Box<HirExpr>, HirType),
    VarRef(Path, SymbolId, HirType),
    Call(Path, SymbolId, Vec<HirExpr>, HirType),
    CallIndirect(Box<HirExpr>, Vec<HirExpr>, HirType), // callee, args, type
    If(Box<HirExpr>, HirBlock, Option<HirBlock>, HirType),
    StructExpr(Path, SymbolId, Vec<(String, HirExpr)>, HirType),
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
    Quantifier(QuantifierKind, Vec<(String, SymbolId, HirType)>, Box<HirExpr>, HirType), // kind, params, body, type
    Error,
}

impl HirExpr {
    pub fn new(kind: HirExprKind, span: Span) -> Self {
        let ty = match &kind {
            HirExprKind::IntLiteral(_, ty) => ty.clone(),
            HirExprKind::BoolLiteral(_, ty) => ty.clone(),
            HirExprKind::BinaryOp(_, _, _, ty) => ty.clone(),
            HirExprKind::UnaryOp(_, _, ty) => ty.clone(),
            HirExprKind::VarRef(_, _, ty) => ty.clone(),
            HirExprKind::Call(_, _, _, ty) => ty.clone(),
            HirExprKind::CallIndirect(_, _, ty) => ty.clone(),
            HirExprKind::If(_, _, _, ty) => ty.clone(),
            HirExprKind::StructExpr(_, _, _, ty) => ty.clone(),
            HirExprKind::FieldAccess(_, _, ty) => ty.clone(),
            HirExprKind::EnumVariant(_, _, _, ty) => ty.clone(),
            HirExprKind::VariantConstructor(_, _, _, ty) => ty.clone(),
            HirExprKind::Match(_, _, ty) => ty.clone(),
            HirExprKind::ArrayExpr(_, ty) => ty.clone(),
            HirExprKind::IndexExpr(_, _, ty) => ty.clone(),
            HirExprKind::SliceExpr(_, _, _, ty) => ty.clone(),
            HirExprKind::Try(_, ty) => ty.clone(),
            HirExprKind::ResultOk(_, ty) => ty.clone(),
            HirExprKind::ResultErr(_, ty) => ty.clone(),
            HirExprKind::Ref(_, _, ty) => ty.clone(),
            HirExprKind::Deref(_, ty) => ty.clone(),
            HirExprKind::Block(_, ty) => ty.clone(),
            HirExprKind::Closure(_, _, _, ty) => ty.clone(),
            HirExprKind::Quantifier(_, _, _, ty) => ty.clone(),
            HirExprKind::Error => HirType::Error,
        };
        Self { kind, ty, span }
    }

    pub fn ty(&self) -> HirType {
        self.ty.clone()
    }

    pub fn is_lvalue(&self) -> bool {
        matches!(self.kind, 
            HirExprKind::VarRef(..) | 
            HirExprKind::FieldAccess(..) | 
            HirExprKind::IndexExpr(..) | 
            HirExprKind::Deref(..)
        )
    }
}

pub mod lower;
pub mod borrowck;
pub mod name_resolution;

#[derive(Debug, Clone)]
pub enum HirPattern {
    VariantCase(String, Vec<String>), // CaseName, bindings
    Binding(String),
    #[allow(dead_code)] // Scaffolded for future literal pattern matching
    Literal(HirExpr),
    Wildcard,
}
