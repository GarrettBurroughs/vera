use super::syntax::{SyntaxNode, SyntaxKind};

#[derive(Debug)]
pub struct FuncDecl {
    pub name: String,
    pub ret_type: String,
    pub body_ret_val: i64,
}

/// A temporary AST extractor to bridge the gap between Phase 2 CST generation 
/// and Phase 3 HIR/Backend generation.
pub fn extract_func_decl(root: &SyntaxNode) -> Option<FuncDecl> {
    let func_node = root.children().find(|n| n.kind() == SyntaxKind::FUNC_DECL)?;
    
    let name = func_node.children_with_tokens().find_map(|it| {
        if it.kind() == SyntaxKind::Ident { Some(it.into_token()?.text().to_string()) } else { None }
    })?;
    
    let ret_type = "i32".to_string();
    
    let block = func_node.children().find(|n| n.kind() == SyntaxKind::BLOCK_EXPR)?;
    let ret_stmt = block.children().find(|n| n.kind() == SyntaxKind::RETURN_STMT)?;
    let int_lit = ret_stmt.children_with_tokens().find_map(|it| {
        if it.kind() == SyntaxKind::IntLit { Some(it.into_token()?.text().to_string()) } else { None }
    })?;
    
    let body_ret_val = int_lit.parse().unwrap_or(0);
    
    Some(FuncDecl { name, ret_type, body_ret_val })
}
