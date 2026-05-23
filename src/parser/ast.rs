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
ast_node!(ReturnStmt, SyntaxKind::RETURN_STMT);

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
    pub fn statements(&self) -> impl Iterator<Item = SyntaxNode> {
        self.syntax().children().filter(|n| {
            matches!(
                n.kind(),
                SyntaxKind::RETURN_STMT
            )
        })
    }
    
    pub fn return_stmt(&self) -> Option<ReturnStmt> {
        self.syntax().children().find_map(ReturnStmt::cast)
    }
}

impl ReturnStmt {
    pub fn expr_token(&self) -> Option<SyntaxToken> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|it| matches!(it.kind(), SyntaxKind::IntLit | SyntaxKind::FloatLit | SyntaxKind::Ident | SyntaxKind::BoolTrue | SyntaxKind::BoolFalse))
    }
}
