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

// Statements
ast_node!(ReturnStmt, SyntaxKind::RETURN_STMT);
ast_node!(LetStmt, SyntaxKind::LET_STMT);
ast_node!(ExprStmt, SyntaxKind::EXPR_STMT);

// Expressions
ast_node!(BinExpr, SyntaxKind::BIN_EXPR);
ast_node!(PrefixExpr, SyntaxKind::PREFIX_EXPR);
ast_node!(IfExpr, SyntaxKind::IF_EXPR);
ast_node!(NameRef, SyntaxKind::NAME_REF);
ast_node!(Condition, SyntaxKind::CONDITION);
ast_node!(Literal, SyntaxKind::LITERAL);
ast_node!(CallExpr, SyntaxKind::CALL_EXPR);
ast_node!(ArgList, SyntaxKind::ARG_LIST);

ast_node!(SpecBlock, SyntaxKind::SPEC_BLOCK);
ast_node!(RequiresClause, SyntaxKind::REQUIRES_CLAUSE);
ast_node!(EnsuresClause, SyntaxKind::ENSURES_CLAUSE);
ast_node!(AssertStmt, SyntaxKind::ASSERT_STMT);
ast_node!(AssumeStmt, SyntaxKind::ASSUME_STMT);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Stmt {
    ReturnStmt(ReturnStmt),
    LetStmt(LetStmt),
    ExprStmt(ExprStmt),
    IfExpr(IfExpr),
    AssertStmt(AssertStmt),
    AssumeStmt(AssumeStmt),
}

impl Stmt {
    pub fn cast(node: SyntaxNode) -> Option<Self> {
        match node.kind() {
            SyntaxKind::RETURN_STMT => ReturnStmt::cast(node).map(Stmt::ReturnStmt),
            SyntaxKind::LET_STMT => LetStmt::cast(node).map(Stmt::LetStmt),
            SyntaxKind::EXPR_STMT => ExprStmt::cast(node).map(Stmt::ExprStmt),
            SyntaxKind::IF_EXPR => IfExpr::cast(node).map(Stmt::IfExpr),
            SyntaxKind::ASSERT_STMT => AssertStmt::cast(node).map(Stmt::AssertStmt),
            SyntaxKind::ASSUME_STMT => AssumeStmt::cast(node).map(Stmt::AssumeStmt),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expr {
    BinExpr(BinExpr),
    PrefixExpr(PrefixExpr),
    IfExpr(IfExpr),
    NameRef(NameRef),
    Literal(Literal),
    CallExpr(CallExpr),
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
            _ => None,
        }
    }
}

// Accessor methods for AST Nodes

impl SourceFile {
    pub fn functions(&self) -> impl Iterator<Item = FuncDecl> {
        self.syntax().children().filter_map(FuncDecl::cast)
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

impl SpecBlock {
    pub fn requires_clauses(&self) -> impl Iterator<Item = RequiresClause> {
        self.syntax().children().filter_map(RequiresClause::cast)
    }
    
    pub fn ensures_clauses(&self) -> impl Iterator<Item = EnsuresClause> {
        self.syntax().children().filter_map(EnsuresClause::cast)
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
