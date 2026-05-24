use super::syntax::{SyntaxKind, SyntaxNode, SyntaxToken};

pub trait AstNode {
    fn can_cast(kind: SyntaxKind) -> bool;
    fn cast(node: SyntaxNode) -> Option<Self>
    where
        Self: Sized;
    fn syntax(&self) -> &SyntaxNode;
}

macro_rules! ast_node {
    ($name:ident, $kind:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(SyntaxNode);

        impl AstNode for $name {
            fn can_cast(kind: SyntaxKind) -> bool {
                kind == $kind
            }
            fn cast(node: SyntaxNode) -> Option<Self> {
                if Self::can_cast(node.kind()) {
                    Some(Self(node))
                } else {
                    None
                }
            }
            fn syntax(&self) -> &SyntaxNode {
                &self.0
            }
        }
    };
}

ast_node!(SourceFile, SyntaxKind::SOURCE_FILE);
ast_node!(FuncDecl, SyntaxKind::FUNC_DECL);
ast_node!(ParamList, SyntaxKind::PARAM_LIST);
ast_node!(Param, SyntaxKind::PARAM);
ast_node!(TypeRef, SyntaxKind::TYPE_REF);
ast_node!(BlockExpr, SyntaxKind::BLOCK_EXPR);

// Structs
ast_node!(StructDecl, SyntaxKind::STRUCT_DECL);
ast_node!(FieldDeclList, SyntaxKind::FIELD_DECL_LIST);
ast_node!(FieldDecl, SyntaxKind::FIELD_DECL);

// Statements
ast_node!(ReturnStmt, SyntaxKind::RETURN_STMT);
ast_node!(LetStmt, SyntaxKind::LET_STMT);
ast_node!(ExprStmt, SyntaxKind::EXPR_STMT);

// Expressions
ast_node!(ArrayExpr, SyntaxKind::ARRAY_EXPR);
ast_node!(IndexExpr, SyntaxKind::INDEX_EXPR);
ast_node!(SliceExpr, SyntaxKind::SLICE_EXPR);
ast_node!(ArrayType, SyntaxKind::ARRAY_TYPE);
ast_node!(SliceType, SyntaxKind::SLICE_TYPE);
ast_node!(ResultType, SyntaxKind::RESULT_TYPE);
ast_node!(PointerType, SyntaxKind::POINTER_TYPE);
ast_node!(RefType, SyntaxKind::REF_TYPE);
ast_node!(BinExpr, SyntaxKind::BIN_EXPR);
ast_node!(PrefixExpr, SyntaxKind::PREFIX_EXPR);
ast_node!(IfExpr, SyntaxKind::IF_EXPR);
ast_node!(NameRef, SyntaxKind::NAME_REF);
ast_node!(Condition, SyntaxKind::CONDITION);
ast_node!(Literal, SyntaxKind::LITERAL);
ast_node!(CallExpr, SyntaxKind::CALL_EXPR);
ast_node!(ArgList, SyntaxKind::ARG_LIST);
ast_node!(TryExpr, SyntaxKind::TRY_EXPR);
ast_node!(RefExpr, SyntaxKind::REF_EXPR);
ast_node!(DerefExpr, SyntaxKind::DEREF_EXPR);

ast_node!(StructExpr, SyntaxKind::STRUCT_EXPR);
ast_node!(StructExprFieldList, SyntaxKind::STRUCT_EXPR_FIELD_LIST);
ast_node!(StructExprField, SyntaxKind::STRUCT_EXPR_FIELD);
ast_node!(FieldExpr, SyntaxKind::FIELD_EXPR);

ast_node!(EnumDecl, SyntaxKind::ENUM_DECL);
ast_node!(EnumVariant, SyntaxKind::ENUM_VARIANT);
ast_node!(VariantDecl, SyntaxKind::VARIANT_DECL);
ast_node!(VariantCase, SyntaxKind::VARIANT_CASE);
ast_node!(MatchExpr, SyntaxKind::MATCH_EXPR);
ast_node!(MatchArm, SyntaxKind::MATCH_ARM);
ast_node!(Pattern, SyntaxKind::PATTERN);

ast_node!(SpecBlock, SyntaxKind::SPEC_BLOCK);
ast_node!(RequiresClause, SyntaxKind::REQUIRES_CLAUSE);
ast_node!(EnsuresClause, SyntaxKind::ENSURES_CLAUSE);
ast_node!(InvariantClause, SyntaxKind::INVARIANT_CLAUSE);
ast_node!(DecreasesClause, SyntaxKind::DECREASES_CLAUSE);
ast_node!(AssignsClause, SyntaxKind::ASSIGNS_CLAUSE);
ast_node!(AssertStmt, SyntaxKind::ASSERT_STMT);
ast_node!(AssumeStmt, SyntaxKind::ASSUME_STMT);
ast_node!(WhileStmt, SyntaxKind::WHILE_STMT);
ast_node!(ForStmt, SyntaxKind::FOR_STMT);
ast_node!(BreakStmt, SyntaxKind::BREAK_STMT);
ast_node!(ContinueStmt, SyntaxKind::CONTINUE_STMT);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Stmt {
    ReturnStmt(ReturnStmt),
    LetStmt(LetStmt),
    ExprStmt(ExprStmt),
    IfExpr(IfExpr),
    WhileStmt(WhileStmt),
    BreakStmt(BreakStmt),
    ContinueStmt(ContinueStmt),
    AssertStmt(AssertStmt),
    AssumeStmt(AssumeStmt),
    ForStmt(ForStmt),
}

impl Stmt {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        match node.kind() {
            SyntaxKind::RETURN_STMT => ReturnStmt::cast(node).map(Stmt::ReturnStmt),
            SyntaxKind::LET_STMT => LetStmt::cast(node).map(Stmt::LetStmt),
            SyntaxKind::EXPR_STMT => ExprStmt::cast(node).map(Stmt::ExprStmt),
            SyntaxKind::IF_EXPR => IfExpr::cast(node).map(Stmt::IfExpr),
            SyntaxKind::WHILE_STMT => WhileStmt::cast(node).map(Stmt::WhileStmt),
            SyntaxKind::BREAK_STMT => BreakStmt::cast(node).map(Stmt::BreakStmt),
            SyntaxKind::CONTINUE_STMT => ContinueStmt::cast(node).map(Stmt::ContinueStmt),
            SyntaxKind::ASSERT_STMT => AssertStmt::cast(node).map(Stmt::AssertStmt),
            SyntaxKind::ASSUME_STMT => AssumeStmt::cast(node).map(Stmt::AssumeStmt),
            SyntaxKind::FOR_STMT => ForStmt::cast(node).map(Stmt::ForStmt),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expr {
    BinExpr(BinExpr),
    PrefixExpr(PrefixExpr),
    CallExpr(CallExpr),
    StructExpr(StructExpr),
    FieldExpr(FieldExpr),
    MatchExpr(MatchExpr),
    ArrayExpr(ArrayExpr),
    IfExpr(IfExpr),
    NameRef(NameRef),
    Literal(Literal),
    IndexExpr(IndexExpr),
    SliceExpr(SliceExpr),
    TryExpr(TryExpr),
    RefExpr(RefExpr),
    DerefExpr(DerefExpr),
}

impl Expr {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        match node.kind() {
            SyntaxKind::BIN_EXPR => BinExpr::cast(node).map(Expr::BinExpr),
            SyntaxKind::PREFIX_EXPR => PrefixExpr::cast(node).map(Expr::PrefixExpr),
            SyntaxKind::IF_EXPR => IfExpr::cast(node).map(Expr::IfExpr),
            SyntaxKind::NAME_REF => NameRef::cast(node).map(Expr::NameRef),
            SyntaxKind::LITERAL => Literal::cast(node).map(Expr::Literal),
            SyntaxKind::CALL_EXPR => CallExpr::cast(node).map(Expr::CallExpr),
            SyntaxKind::STRUCT_EXPR => StructExpr::cast(node).map(Expr::StructExpr),
            SyntaxKind::FIELD_EXPR => FieldExpr::cast(node).map(Expr::FieldExpr),
            SyntaxKind::MATCH_EXPR => MatchExpr::cast(node).map(Expr::MatchExpr),
            SyntaxKind::ARRAY_EXPR => ArrayExpr::cast(node).map(Expr::ArrayExpr),
            SyntaxKind::INDEX_EXPR => IndexExpr::cast(node).map(Expr::IndexExpr),
            SyntaxKind::SLICE_EXPR => SliceExpr::cast(node).map(Expr::SliceExpr),
            SyntaxKind::TRY_EXPR => TryExpr::cast(node).map(Expr::TryExpr),
            SyntaxKind::REF_EXPR => RefExpr::cast(node).map(Expr::RefExpr),
            SyntaxKind::DEREF_EXPR => DerefExpr::cast(node).map(Expr::DerefExpr),
            _ => None,
        }
    }
}

// Accessor methods for AST Nodes

impl SourceFile {
    pub fn functions(&self) -> impl Iterator<Item = FuncDecl> {
        self.syntax().children().filter_map(FuncDecl::cast)
    }
    pub fn structs(&self) -> impl Iterator<Item = StructDecl> {
        self.syntax().children().filter_map(StructDecl::cast)
    }
    pub fn enums(&self) -> impl Iterator<Item = EnumDecl> {
        self.syntax().children().filter_map(EnumDecl::cast)
    }
    pub fn variants(&self) -> impl Iterator<Item = VariantDecl> {
        self.syntax().children().filter_map(VariantDecl::cast)
    }
}

impl FuncDecl {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == SyntaxKind::Ident)
    }
    
    pub fn param_list(&self) -> Option<ParamList> {
        self.syntax().children().find_map(ParamList::cast)
    }
    
    pub fn ret_type(&self) -> Option<TypeRef> {
        self.syntax().children().find_map(TypeRef::cast)
    }
    
    pub fn body(&self) -> Option<BlockExpr> {
        self.syntax().children().find_map(BlockExpr::cast)
    }
    
    pub fn spec(&self) -> Option<SpecBlock> {
        self.syntax().children().find_map(SpecBlock::cast)
    }
}

impl ParamList {
    pub fn params(&self) -> impl Iterator<Item = Param> {
        self.syntax().children().filter_map(Param::cast)
    }
}

impl Param {
    pub fn name(&self) -> Option<String> {
        self.syntax().children_with_tokens().find_map(|it| {
            if it.kind() == SyntaxKind::Ident {
                Some(it.into_token()?.text().to_string())
            } else {
                None
            }
        })
    }
    
    pub fn ty(&self) -> Option<TypeRef> {
        self.syntax().children().find_map(TypeRef::cast)
    }
}

impl TypeRef {
    pub fn as_string(&self) -> Option<String> {
        // Find the first token inside TYPE_REF that represents the type name
        self.syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| !matches!(it.kind(), SyntaxKind::Whitespace | SyntaxKind::Comment | SyntaxKind::BlockComment))
            .map(|it| it.text().to_string())
    }
}

impl ArrayType {
    pub fn ty(&self) -> Option<TypeRef> {
        self.syntax().children().find_map(TypeRef::cast)
    }
    pub fn size(&self) -> Option<Literal> {
        self.syntax().children().find_map(Literal::cast)
    }
}

impl SliceType {
    pub fn ty(&self) -> Option<TypeRef> {
        self.syntax().children().find_map(TypeRef::cast)
    }
    pub fn is_mut(&self) -> bool {
        self.syntax().children_with_tokens().any(|t| t.kind() == SyntaxKind::KwMut)
    }
}

impl ResultType {
    pub fn ok_ty(&self) -> Option<TypeRef> {
        self.syntax().children().filter_map(TypeRef::cast).nth(0)
    }
    pub fn err_ty(&self) -> Option<TypeRef> {
        self.syntax().children().filter_map(TypeRef::cast).nth(1)
    }
}

impl RefType {
    pub fn ty(&self) -> Option<TypeRef> {
        self.syntax().children().find_map(TypeRef::cast)
    }
    pub fn is_mut(&self) -> bool {
        self.syntax().children_with_tokens().any(|t| t.kind() == SyntaxKind::KwMut)
    }
}

impl PointerType {
    pub fn ty(&self) -> Option<TypeRef> {
        self.syntax().children().find_map(TypeRef::cast)
    }
    pub fn is_mut(&self) -> bool {
        self.syntax().children_with_tokens().any(|t| t.kind() == SyntaxKind::KwMut)
    }
}

impl BlockExpr {
    pub fn statements(&self) -> impl Iterator<Item = Stmt> {
        self.syntax().children().filter_map(Stmt::cast)
    }
}

// Expr accessors
impl Literal {
    pub fn text(&self) -> String {
        self.syntax().first_token().map(|t| t.text().to_string()).unwrap_or_default()
    }
}

impl CallExpr {
    pub fn callee(&self) -> Option<Expr> {
        self.syntax().children().find_map(Expr::cast)
    }
    
    pub fn arg_list(&self) -> Option<ArgList> {
        self.syntax().children().find_map(ArgList::cast)
    }
}

impl ArgList {
    pub fn args(&self) -> impl Iterator<Item = Expr> {
        self.syntax().children().filter_map(Expr::cast)
    }
}

impl TryExpr {
    pub fn expr(&self) -> Option<Expr> {
        self.syntax().children().find_map(Expr::cast)
    }
}


impl BinExpr {
    pub fn lhs(&self) -> Option<Expr> {
        self.syntax().children().filter_map(Expr::cast).next()
    }
    pub fn op(&self) -> Option<SyntaxToken> {
        self.syntax().children_with_tokens().filter_map(|it| it.into_token()).find(|it| {
            matches!(it.kind(), 
                SyntaxKind::Plus | SyntaxKind::Minus | SyntaxKind::Star | SyntaxKind::Slash | SyntaxKind::Percent |
                SyntaxKind::EqEq | SyntaxKind::BangEq | SyntaxKind::Less | SyntaxKind::Greater | SyntaxKind::LessEq | SyntaxKind::GreaterEq |
                SyntaxKind::AmpAmp | SyntaxKind::PipePipe | SyntaxKind::Implies | SyntaxKind::Iff | SyntaxKind::Eq
            )
        })
    }
    pub fn rhs(&self) -> Option<Expr> {
        self.syntax().children().filter_map(Expr::cast).nth(1)
    }
}

impl PrefixExpr {
    pub fn op(&self) -> Option<SyntaxToken> {
        self.syntax().children_with_tokens().filter_map(|it| it.into_token()).find(|it| {
            matches!(it.kind(), SyntaxKind::Minus | SyntaxKind::Bang | SyntaxKind::Star | SyntaxKind::Amp)
        })
    }
    pub fn expr(&self) -> Option<Expr> {
        self.syntax().children().find_map(Expr::cast)
    }
}

impl NameRef {
    pub fn ident(&self) -> Option<SyntaxToken> {
        self.syntax().children_with_tokens().filter_map(|it| it.into_token()).find(|it| it.kind() == SyntaxKind::Ident)
    }
}

impl IfExpr {
    pub fn condition(&self) -> Option<Expr> {
        self.syntax().children().find_map(Condition::cast).and_then(|c| c.expr())
    }
    pub fn then_block(&self) -> Option<BlockExpr> {
        self.syntax().children().find_map(BlockExpr::cast)
    }
    pub fn else_branch(&self) -> Option<SyntaxNode> {
        self.syntax().children()
            .filter(|n| n.kind() == SyntaxKind::BLOCK_EXPR || n.kind() == SyntaxKind::IF_EXPR)
            .find(|n| {
                if let Some(tb) = self.then_block() {
                    n != tb.syntax()
                } else {
                    true
                }
            })
    }
}

impl RefExpr {
    pub fn expr(&self) -> Option<Expr> {
        self.syntax().children().find_map(Expr::cast)
    }
    pub fn is_mut(&self) -> bool {
        self.syntax().children_with_tokens().any(|it| it.kind() == SyntaxKind::KwMut)
    }
}

impl DerefExpr {
    pub fn expr(&self) -> Option<Expr> {
        self.syntax().children().find_map(Expr::cast)
    }
}

impl Condition {
    pub fn expr(&self) -> Option<Expr> {
        self.syntax().children().find_map(Expr::cast)
    }
}

impl Literal {
    pub fn token(&self) -> Option<SyntaxToken> {
        self.syntax().children_with_tokens().filter_map(|it| it.into_token()).find(|it| {
            matches!(it.kind(), SyntaxKind::IntLit | SyntaxKind::FloatLit | SyntaxKind::StringLit | SyntaxKind::BoolTrue | SyntaxKind::BoolFalse)
        })
    }
}

impl LetStmt {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax().children_with_tokens().filter_map(|it| it.into_token()).find(|it| it.kind() == SyntaxKind::Ident)
    }
    pub fn ty(&self) -> Option<TypeRef> {
        self.syntax().children().find_map(TypeRef::cast)
    }
    pub fn initializer(&self) -> Option<Expr> {
        self.syntax().children().find_map(Expr::cast)
    }
    pub fn is_const(&self) -> bool {
        self.syntax().children_with_tokens().any(|it| it.kind() == SyntaxKind::KwConst)
    }
}

impl ExprStmt {
    pub fn expr(&self) -> Option<Expr> {
        self.syntax().children().find_map(Expr::cast)
    }
}

impl ReturnStmt {
    pub fn expr(&self) -> Option<Expr> {
        self.syntax().children().find_map(Expr::cast)
    }
}

// Struct implementations

impl StructDecl {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax().children_with_tokens().filter_map(|it| it.into_token()).find(|it| it.kind() == SyntaxKind::Ident)
    }
    pub fn fields(&self) -> impl Iterator<Item = FieldDecl> {
        self.syntax().children().find_map(FieldDeclList::cast)
            .into_iter()
            .flat_map(|list| list.syntax().children().filter_map(FieldDecl::cast))
            .collect::<Vec<_>>()
            .into_iter()
    }
}

impl FieldDecl {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax().children_with_tokens().filter_map(|it| it.into_token()).find(|it| it.kind() == SyntaxKind::Ident)
    }
    pub fn ty(&self) -> Option<TypeRef> {
        self.syntax().children().find_map(TypeRef::cast)
    }
}

impl StructExpr {
    pub fn name(&self) -> Option<NameRef> {
        self.syntax().children().find_map(NameRef::cast)
    }
    pub fn fields(&self) -> impl Iterator<Item = StructExprField> {
        self.syntax().children().find_map(StructExprFieldList::cast)
            .into_iter()
            .flat_map(|list| list.syntax().children().filter_map(StructExprField::cast))
            .collect::<Vec<_>>()
            .into_iter()
    }
}

impl StructExprField {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax().children_with_tokens().filter_map(|it| it.into_token()).find(|it| it.kind() == SyntaxKind::Ident)
    }
    pub fn expr(&self) -> Option<Expr> {
        self.syntax().children().find_map(Expr::cast)
    }
}

impl FieldExpr {
    pub fn base(&self) -> Option<Expr> {
        self.syntax().children().find_map(Expr::cast)
    }
    pub fn field(&self) -> Option<NameRef> {
        self.syntax().children().filter_map(NameRef::cast).last()
    }
}

impl ArrayExpr {
    pub fn elements(&self) -> impl Iterator<Item = Expr> {
        self.syntax().children().filter_map(Expr::cast)
    }
}

impl IndexExpr {
    pub fn base(&self) -> Option<Expr> {
        self.syntax().children().filter_map(Expr::cast).next()
    }
    pub fn index(&self) -> Option<Expr> {
        self.syntax().children().filter_map(Expr::cast).nth(1)
    }
}

impl SliceExpr {
    pub fn base(&self) -> Option<Expr> {
        self.syntax().children().filter_map(Expr::cast).next()
    }
    pub fn start(&self) -> Option<Expr> {
        self.syntax().children().filter_map(Expr::cast).nth(1)
    }
    pub fn end(&self) -> Option<Expr> {
        self.syntax().children().filter_map(Expr::cast).nth(2)
    }
}


impl SpecBlock {
    pub fn requires_clauses(&self) -> impl Iterator<Item = RequiresClause> {
        self.syntax().children().filter_map(RequiresClause::cast)
    }
    
    pub fn ensures_clauses(&self) -> impl Iterator<Item = EnsuresClause> {
        self.syntax().children().filter_map(EnsuresClause::cast)
    }

    pub fn invariant_clauses(&self) -> impl Iterator<Item = InvariantClause> {
        self.syntax().children().filter_map(InvariantClause::cast)
    }

    pub fn decreases_clauses(&self) -> impl Iterator<Item = DecreasesClause> {
        self.syntax().children().filter_map(DecreasesClause::cast)
    }

    pub fn assigns_clauses(&self) -> impl Iterator<Item = AssignsClause> {
        self.syntax().children().filter_map(AssignsClause::cast)
    }
}

impl RequiresClause {
    pub fn expr(&self) -> Option<Expr> {
        self.syntax().children().find_map(Expr::cast)
    }
}

impl EnsuresClause {
    pub fn expr(&self) -> Option<Expr> {
        self.syntax().children().find_map(Expr::cast)
    }
}

impl InvariantClause {
    pub fn expr(&self) -> Option<Expr> {
        self.syntax().children().find_map(Expr::cast)
    }
}

impl DecreasesClause {
    pub fn expr(&self) -> Option<Expr> {
        self.syntax().children().find_map(Expr::cast)
    }
}

impl AssignsClause {
    pub fn expr(&self) -> Option<Expr> {
        self.syntax().children().find_map(Expr::cast)
    }
}

impl AssertStmt {
    pub fn expr(&self) -> Option<Expr> {
        self.syntax().children().find_map(Expr::cast)
    }
}

impl AssumeStmt {
    pub fn expr(&self) -> Option<Expr> {
        self.syntax().children().find_map(Expr::cast)
    }
}

impl WhileStmt {
    pub fn condition(&self) -> Option<Expr> {
        self.syntax().children().find_map(Expr::cast)
    }
    
    pub fn body(&self) -> Option<BlockExpr> {
        self.syntax().children().find_map(BlockExpr::cast)
    }
    
    pub fn spec(&self) -> Option<SpecBlock> {
        self.syntax().children().find_map(SpecBlock::cast)
    }
}

impl ForStmt {
    pub fn item(&self) -> Option<SyntaxToken> {
        self.syntax().children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| it.kind() == SyntaxKind::Ident)
    }
    
    pub fn iterable(&self) -> Option<Expr> {
        self.syntax().children().find_map(Expr::cast)
    }
    
    pub fn body(&self) -> Option<BlockExpr> {
        self.syntax().children().find_map(BlockExpr::cast)
    }
}

impl BreakStmt {}
impl ContinueStmt {}

impl EnumDecl {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax().children_with_tokens().filter_map(|it| it.into_token()).find(|it| it.kind() == SyntaxKind::Ident)
    }
    pub fn variants(&self) -> impl Iterator<Item = EnumVariant> {
        self.syntax().children().filter_map(EnumVariant::cast)
    }
}

impl EnumVariant {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax().children_with_tokens().filter_map(|it| it.into_token()).find(|it| it.kind() == SyntaxKind::Ident)
    }
}

impl VariantDecl {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax().children_with_tokens().filter_map(|it| it.into_token()).find(|it| it.kind() == SyntaxKind::Ident)
    }
    pub fn cases(&self) -> impl Iterator<Item = VariantCase> {
        self.syntax().children().filter_map(VariantCase::cast)
    }
}

impl VariantCase {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax().children_with_tokens().filter_map(|it| it.into_token()).find(|it| it.kind() == SyntaxKind::Ident)
    }
    pub fn types(&self) -> impl Iterator<Item = TypeRef> {
        self.syntax().children().filter_map(TypeRef::cast)
    }
}

impl MatchExpr {
    pub fn expr(&self) -> Option<Expr> {
        self.syntax().children().find_map(Expr::cast)
    }
    pub fn arms(&self) -> impl Iterator<Item = MatchArm> {
        self.syntax().children().filter_map(MatchArm::cast)
    }
}

impl MatchArm {
    pub fn pattern(&self) -> Option<Pattern> {
        self.syntax().children().find_map(Pattern::cast)
    }
    pub fn val(&self) -> Option<Expr> {
        self.syntax().children().find_map(Expr::cast)
    }
    pub fn body(&self) -> Option<BlockExpr> {
        self.syntax().children().find_map(BlockExpr::cast)
    }
}

impl Pattern {
    pub fn name(&self) -> Option<SyntaxToken> {
        self.syntax().children_with_tokens().filter_map(|it| it.into_token()).find(|it| it.kind() == SyntaxKind::Ident)
    }
}
