use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use crate::parser::ast::{SourceFile, AstNode, ImportDecl};
use crate::parser::syntax::SyntaxNode;
use crate::parser::ParseMode;
use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

pub type FileId = usize;

#[derive(Debug)]
pub struct FileData {
    pub path: PathBuf,
    pub source: String,
    pub ast: SourceFile,
    pub has_errors: bool,
}

#[derive(Debug)]
pub struct Workspace {
    pub files: BTreeMap<FileId, FileData>,
    pub entry_file_id: FileId,
    next_file_id: FileId,
    parse_mode: ParseMode,
}

#[derive(Error, Debug, Diagnostic)]
#[error("File not found: {path}")]
pub struct FileNotFound {
    path: PathBuf,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

impl Workspace {
    /// Creates a lossless workspace that retains all trivia (for LSP and tooling).
    pub fn new() -> Self {
        Self::new_with_mode(ParseMode::Lossless)
    }

    /// Creates a strip-mode workspace that discards trivia for smaller CSTs (for CLI builds).
    pub fn new_strip() -> Self {
        Self::new_with_mode(ParseMode::Strip)
    }

    fn new_with_mode(parse_mode: ParseMode) -> Self {
        Self {
            files: BTreeMap::new(),
            entry_file_id: 0,
            next_file_id: 1,
            parse_mode,
        }
    }

    pub fn load_entry_file(&mut self, path: impl AsRef<Path>) -> miette::Result<FileId> {
        let file_id = self.load_file(path.as_ref())?;
        self.entry_file_id = file_id;
        
        let mut queue = vec![file_id];
        let mut visited = std::collections::BTreeSet::new();
        visited.insert(file_id);
        
        while let Some(current_id) = queue.pop() {
            let file_data = self.files.get(&current_id).unwrap();
            let parent_path = file_data.path.parent().unwrap_or(Path::new("")).to_path_buf();
            
            // Extract imports. We need to collect them to avoid borrowing self.files while mutating
            let mut imports = Vec::new();
            for import_decl in file_data.ast.imports() {
                if let Some(path_node) = import_decl.path() {
                    let mut segments = Vec::new();
                    for segment in path_node.segments() {
                        if let Some(name) = segment.name() {
                            segments.push(name.text().to_string());
                        }
                    }
                    if !segments.is_empty() {
                        imports.push(segments);
                    }
                }
            }
            
            for segments in imports {
                let mut import_path = parent_path.clone();
                for segment in &segments {
                    import_path.push(segment);
                }
                let mut actual_path = import_path.clone();
                actual_path.set_extension("vera");
                if !actual_path.exists() {
                    let mut spec_path = import_path.clone();
                    spec_path.set_extension("spec");
                    if spec_path.exists() {
                        actual_path = spec_path;
                    }
                }
                
                // If not found locally, try checking if it's 'std.xxx' and resolve from a standard lib location.
                // For now, we assume everything is relative to the importing file.
                
                let import_file_id = if let Some(id) = self.find_loaded_file(&actual_path) {
                    id
                } else {
                    let new_id = self.load_file(&actual_path)?;
                    new_id
                };
                
                if !visited.contains(&import_file_id) {
                    visited.insert(import_file_id);
                    queue.push(import_file_id);
                }
            }
        }
        
        Ok(self.entry_file_id)
    }

    fn find_loaded_file(&self, path: &Path) -> Option<FileId> {
        for (&id, data) in &self.files {
            if data.path == path {
                return Some(id);
            }
        }
        None
    }

    /// Returns the parse mode configured for this workspace.
    pub fn parse_mode(&self) -> ParseMode {
        self.parse_mode
    }

    /// Load a file from an in-memory source string, bypassing disk I/O.
    /// Useful for LSP text-sync and unit tests.
    pub fn load_from_source(&mut self, path: &Path, source: String) -> FileId {
        let parser = crate::parser::Parser::new_with_mode(&source, self.parse_mode);
        let (cst, errors) = parser.parse();
        let has_errors = !errors.is_empty();
        let ast = SourceFile::cast(cst).expect("root must be SourceFile");
        let file_id = self.next_file_id;
        self.next_file_id += 1;
        self.files.insert(file_id, FileData {
            path: path.to_path_buf(),
            source,
            ast,
            has_errors,
        });
        file_id
    }

    fn load_file(&mut self, path: &Path) -> miette::Result<FileId> {
        let source = std::fs::read_to_string(path).map_err(|_| FileNotFound { path: path.to_path_buf() })?;

        let parser = crate::parser::Parser::new_with_mode(&source, self.parse_mode);
        let (cst, errors) = parser.parse();
        
        let has_errors = !errors.is_empty();
        if has_errors {
            for err in errors {
                tracing::error!("Parse Error in {}: {}", path.display(), err);
            }
        }
        
        let ast = SourceFile::cast(cst).expect("Root is not a SourceFile");
        
        let file_id = self.next_file_id;
        self.next_file_id += 1;
        
        self.files.insert(file_id, FileData {
            path: path.to_path_buf(),
            source,
            ast,
            has_errors,
        });
        
        Ok(file_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::syntax::SyntaxKind;

    /// Workspace created with `new_strip` must produce CSTs free of trivia tokens.
    #[test]
    fn test_workspace_strip_mode_no_trivia() {
        let mut ws = Workspace::new_strip();
        let id = ws.load_from_source(
            Path::new("test.vera"),
            "// comment\nfunc main(): i32 { return 42; }".to_string(),
        );

        let file = ws.files.get(&id).unwrap();

        fn has_trivia(node: &crate::parser::syntax::SyntaxNode) -> bool {
            for child in node.children_with_tokens() {
                match child {
                    rowan::NodeOrToken::Token(t) => {
                        if matches!(t.kind(), SyntaxKind::Whitespace | SyntaxKind::Comment | SyntaxKind::BlockComment) {
                            return true;
                        }
                    }
                    rowan::NodeOrToken::Node(n) => {
                        if has_trivia(&n) {
                            return true;
                        }
                    }
                }
            }
            false
        }

        let cst = file.ast.syntax();
        assert!(!has_trivia(cst), "Workspace in strip mode must produce trivia-free CSTs");
    }

    /// Workspace created with `new` (lossless) must retain trivia tokens.
    #[test]
    fn test_workspace_lossless_mode_has_trivia() {
        let mut ws = Workspace::new();
        let id = ws.load_from_source(
            Path::new("test.vera"),
            "// comment\nfunc main(): i32 { return 42; }".to_string(),
        );

        let file = ws.files.get(&id).unwrap();

        fn has_trivia(node: &crate::parser::syntax::SyntaxNode) -> bool {
            for child in node.children_with_tokens() {
                match child {
                    rowan::NodeOrToken::Token(t) => {
                        if matches!(t.kind(), SyntaxKind::Whitespace | SyntaxKind::Comment | SyntaxKind::BlockComment) {
                            return true;
                        }
                    }
                    rowan::NodeOrToken::Node(n) => {
                        if has_trivia(&n) {
                            return true;
                        }
                    }
                }
            }
            false
        }

        let cst = file.ast.syntax();
        assert!(has_trivia(cst), "Lossless workspace must retain trivia tokens");
    }
}
