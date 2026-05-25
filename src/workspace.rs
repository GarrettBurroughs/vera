use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use crate::parser::ast::{SourceFile, AstNode, ImportDecl};
use crate::parser::syntax::SyntaxNode;
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

#[derive(Debug, Default)]
pub struct Workspace {
    pub files: BTreeMap<FileId, FileData>,
    pub entry_file_id: FileId,
    next_file_id: FileId,
}

#[derive(Error, Debug, Diagnostic)]
#[error("File not found: {path}")]
pub struct FileNotFound {
    path: PathBuf,
}

impl Workspace {
    pub fn new() -> Self {
        Self {
            files: BTreeMap::new(),
            entry_file_id: 0,
            next_file_id: 1,
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

    /// Load a file from an in-memory source string, bypassing disk I/O.
    /// Useful for LSP text-sync and unit tests.
    pub fn load_from_source(&mut self, path: &Path, source: String) -> FileId {
        let parser = crate::parser::Parser::new(&source);
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
        
        let parser = crate::parser::Parser::new(&source);
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
