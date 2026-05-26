use std::collections::BTreeMap;
use crate::parser::ast;
use crate::workspace::{Workspace, FileId};
use crate::hir::Path;
use crate::hir::lower::SemanticError;

#[derive(Debug, Default)]
pub struct ModuleScope {
    pub module_name: String, // globally unique module name, e.g. "math" or "std::math"
    pub funcs: BTreeMap<String, (bool, crate::hir::Span)>, // name -> (is_pub, span)
    pub structs: BTreeMap<String, (bool, crate::hir::Span)>,
    pub enums: BTreeMap<String, (bool, crate::hir::Span)>,
    pub variants: BTreeMap<String, (bool, crate::hir::Span)>,
    pub type_aliases: BTreeMap<String, (bool, crate::hir::Span)>,
    
    // aliases
    pub module_aliases: BTreeMap<String, String>, // alias -> global module name
    pub item_aliases: BTreeMap<String, String>, // alias -> global item path
}

pub enum ResolutionError {
    EmptyPath,
    UnknownModule(String),
    PrivateItemAccess { name: String, target_module: String },
}

pub struct NameResolver {
    pub scopes: BTreeMap<FileId, ModuleScope>,
    pub file_to_module: BTreeMap<FileId, String>,
    pub module_to_file: BTreeMap<String, FileId>,
}

impl NameResolver {
    pub fn build(workspace: &Workspace, errors: &mut Vec<SemanticError>) -> Self {
        let mut resolver = Self {
            scopes: BTreeMap::new(),
            file_to_module: BTreeMap::new(),
            module_to_file: BTreeMap::new(),
        };

        // Pass 1: Assign module names to each file
        let entry_dir = workspace.files.get(&workspace.entry_file_id)
            .and_then(|f| f.path.parent())
            .unwrap_or(std::path::Path::new(""));

        for (&file_id, file_data) in &workspace.files {
            if file_id == workspace.entry_file_id {
                let file_stem = file_data.path.file_stem().unwrap().to_string_lossy().into_owned();
                resolver.file_to_module.insert(file_id, file_stem.clone());
                resolver.module_to_file.insert(file_stem, file_id);
            } else {
                if let Ok(rel_path) = file_data.path.strip_prefix(entry_dir) {
                    let mut segments = Vec::new();
                    for comp in rel_path.components() {
                        if let std::path::Component::Normal(os_str) = comp {
                            segments.push(os_str.to_string_lossy().into_owned());
                        }
                    }
                    if let Some(last) = segments.last_mut() {
                        *last = file_data.path.file_stem().unwrap().to_string_lossy().into_owned();
                    }
                    let module_name = segments.join("::");
                    resolver.file_to_module.insert(file_id, module_name.clone());
                    resolver.module_to_file.insert(module_name, file_id);
                } else {
                    let file_stem = file_data.path.file_stem().unwrap().to_string_lossy().into_owned();
                    resolver.file_to_module.insert(file_id, file_stem.clone());
                    resolver.module_to_file.insert(file_stem, file_id);
                }
            }
        }

        // Pass 2: Build scopes
        for (&file_id, file_data) in &workspace.files {
            let module_name = resolver.file_to_module[&file_id].clone();
            let mut scope = ModuleScope {
                module_name: module_name.clone(),
                ..Default::default()
            };

            for s in file_data.ast.structs() {
                if let Some(name) = s.name() {
                    scope.structs.insert(name.text().to_string(), (s.is_pub(), crate::hir::Span::new(file_id, name.text_range().start().into(), name.text_range().end().into())));
                }
            }
            for e in file_data.ast.enums() {
                if let Some(name) = e.name() {
                    scope.enums.insert(name.text().to_string(), (e.is_pub(), crate::hir::Span::new(file_id, name.text_range().start().into(), name.text_range().end().into())));
                }
            }
            for v in file_data.ast.variants() {
                if let Some(name) = v.name() {
                    scope.variants.insert(name.text().to_string(), (v.is_pub(), crate::hir::Span::new(file_id, name.text_range().start().into(), name.text_range().end().into())));
                }
            }
            for f in file_data.ast.functions() {
                if let Some(name) = f.name() {
                    scope.funcs.insert(name.text().to_string(), (f.is_pub(), crate::hir::Span::new(file_id, name.text_range().start().into(), name.text_range().end().into())));
                }
            }
            for a in file_data.ast.type_aliases() {
                if let Some(name) = a.name() {
                    scope.type_aliases.insert(name.text().to_string(), (a.is_pub(), crate::hir::Span::new(file_id, name.text_range().start().into(), name.text_range().end().into())));
                }
            }
            
            // Imports
            for import in file_data.ast.imports() {
                if let Some(path) = import.path() {
                    let mut segments = Vec::new();
                    for seg in path.segments() {
                        if let Some(name) = seg.name() {
                            segments.push(name.text().to_string());
                        }
                    }
                    
                    // The last segment is either a module, or we have an import list
                    if segments.is_empty() {
                        continue;
                    }
                    
                    // For now, map the import path to the module name assuming single depth or simple structure
                    // In a real system we'd resolve this via the workspace file tree.
                    // Assuming local file for now: segments = ["math"], module_name = "math".
                    // If it is `import std.collections.{List}`, `std.collections` -> `std::collections`.
                    let target_module_name = segments.join("::"); 
                    let module_alias = if let Some(alias) = import.alias() {
                        alias.name().map(|n| n.text().to_string()).unwrap_or_else(|| segments.last().unwrap().clone())
                    } else {
                        segments.last().unwrap().clone()
                    };

                    if let Some(import_list) = import.import_list() {
                        for item in import_list.items() {
                            let item_name = item.text().to_string();
                            let global_path = format!("{}::{}", target_module_name, item_name);
                            scope.item_aliases.insert(item_name, global_path);
                        }
                    } else {
                        scope.module_aliases.insert(module_alias, target_module_name);
                    }
                }
            }

            resolver.scopes.insert(file_id, scope);
        }

        resolver
    }

    pub fn resolve_path(&self, current_file: FileId, segments: &[String]) -> Result<String, ResolutionError> {
        let scope = &self.scopes[&current_file];
        
        if segments.is_empty() {
            return Err(ResolutionError::EmptyPath);
        }

        if segments.len() == 1 {
            let name = &segments[0];
            // Check item aliases (e.g. from `import std.math.{add}`)
            if let Some(global_path) = scope.item_aliases.get(name) {
                // TODO: check visibility on the target
                return Ok(global_path.clone());
            }
            // Check local scope
            if scope.funcs.contains_key(name) || scope.structs.contains_key(name) || scope.enums.contains_key(name) || scope.variants.contains_key(name) || scope.type_aliases.contains_key(name) {
                return Ok(format!("{}::{}", scope.module_name, name));
            }
            // Check if it's a primitive or generic parameter (handled by caller)
            return Ok(name.clone());
        } else {
            // Path like `math::add`
            let mod_name = &segments[0];
            let item_name = &segments[1];
            
            if let Some(target_mod_name) = scope.module_aliases.get(mod_name) {
                // Check visibility
                if let Some(&target_file_id) = self.module_to_file.get(target_mod_name) {
                    if let Some(target_scope) = self.scopes.get(&target_file_id) {
                        let is_pub = target_scope.funcs.get(item_name).map(|(is_pub, _)| *is_pub)
                            .or_else(|| target_scope.structs.get(item_name).map(|(is_pub, _)| *is_pub))
                            .or_else(|| target_scope.enums.get(item_name).map(|(is_pub, _)| *is_pub))
                            .or_else(|| target_scope.type_aliases.get(item_name).map(|(is_pub, _)| *is_pub))
                            .unwrap_or(false);
                            
                        if !is_pub && current_file != target_file_id {
                            return Err(ResolutionError::PrivateItemAccess {
                                name: item_name.clone(),
                                target_module: target_mod_name.clone(),
                            });
                        }
                    }
                }
                
                // Return global path
                return Ok(format!("{}::{}", target_mod_name, item_name));
            }
            
            return Err(ResolutionError::UnknownModule(mod_name.clone()));
        }
    }

    pub fn get_span(&self, global_path: &str) -> crate::hir::Span {
        let parts: Vec<&str> = global_path.split("::").collect();
        if parts.len() == 2 {
            let mod_name = parts[0];
            let item_name = parts[1];
            if let Some(&file_id) = self.module_to_file.get(mod_name) {
                if let Some(scope) = self.scopes.get(&file_id) {
                    if let Some((_, span)) = scope.funcs.get(item_name) { return *span; }
                    if let Some((_, span)) = scope.structs.get(item_name) { return *span; }
                    if let Some((_, span)) = scope.enums.get(item_name) { return *span; }
                    if let Some((_, span)) = scope.variants.get(item_name) { return *span; }
                    if let Some((_, span)) = scope.type_aliases.get(item_name) { return *span; }
                }
            }
        }
        crate::hir::Span::unknown()
    }
}
