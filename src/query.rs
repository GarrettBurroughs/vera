use std::collections::BTreeMap;
use std::path::Path;

use crate::hir::HirProgram;
use crate::hir::borrowck::{BorrowChecker, BorrowError};
use crate::hir::lower::{LoweringContext, SemanticError};
use crate::parser::ast::{AstNode, SourceFile};
use crate::verification::{self, VerificationError};
use crate::workspace::{FileId, Workspace};

type Revision = u64;

struct HirCache {
    program: HirProgram,
    errors: Vec<SemanticError>,
    /// Revision snapshot of every loaded file at the time this was computed.
    deps: BTreeMap<FileId, Revision>,
}

struct BorrowCache {
    errors: Vec<BorrowError>,
    deps: BTreeMap<FileId, Revision>,
}

struct VerifyCache {
    result: Result<(), VerificationError>,
    deps: BTreeMap<FileId, Revision>,
}

/// Incremental query engine for the Vera compiler.
///
/// Wraps the `Workspace` with a memoization layer.  Every compiler stage
/// (parsing, HIR lowering, borrow-checking, per-function verification) is a
/// *query*: its result is cached and reused as long as none of its source-file
/// inputs have changed.  Calling `update_file_source` bumps a file's revision
/// and automatically invalidates all downstream caches.
pub struct QueryContext {
    workspace: Workspace,
    /// Monotonically increasing revision counter per file.  Starts at 1 and
    /// is bumped by `update_file_source`.
    revisions: BTreeMap<FileId, Revision>,

    hir_cache: Option<HirCache>,
    borrow_cache: Option<BorrowCache>,
    /// Per-function verification cache; keyed by fully-qualified function name.
    verify_cache: BTreeMap<String, VerifyCache>,
}

impl Default for QueryContext {
    fn default() -> Self {
        Self::new()
    }
}

impl QueryContext {
    /// Creates a query context in lossless mode (retains trivia; use for LSP).
    pub fn new() -> Self {
        Self {
            workspace: Workspace::new(),
            revisions: BTreeMap::new(),
            hir_cache: None,
            borrow_cache: None,
            verify_cache: BTreeMap::new(),
        }
    }

    /// Creates a query context in strip mode (discards trivia; use for CLI builds).
    pub fn new_strip() -> Self {
        Self {
            workspace: Workspace::new_strip(),
            revisions: BTreeMap::new(),
            hir_cache: None,
            borrow_cache: None,
            verify_cache: BTreeMap::new(),
        }
    }

    // -------------------------------------------------------------------------
    // File management
    // -------------------------------------------------------------------------

    /// Load and parse the entry file and all transitive imports from disk.
    pub fn load_entry_file(&mut self, path: impl AsRef<Path>) -> miette::Result<FileId> {
        let entry_id = self.workspace.load_entry_file(path)?;
        for &id in self.workspace.files.keys() {
            self.revisions.entry(id).or_insert(1);
        }
        Ok(entry_id)
    }

    /// Load a single source file from a string, bypassing disk I/O.
    ///
    /// Sets the loaded file as the workspace entry file.  Intended for LSP
    /// text-sync and unit tests.
    pub fn load_from_source(&mut self, path: &Path, source: String) -> FileId {
        let file_id = self.workspace.load_from_source(path, source);
        self.workspace.entry_file_id = file_id;
        self.revisions.insert(file_id, 1);
        file_id
    }

    /// Replace the source of an already-loaded file (e.g. from an LSP
    /// `textDocument/didChange` notification).
    ///
    /// Re-parses the file in place, bumps its revision, and clears all
    /// downstream caches so the next query will recompute from scratch.
    pub fn update_file_source(&mut self, file_id: FileId, new_source: String) {
        let parser = crate::parser::Parser::new_with_mode(&new_source, self.workspace.parse_mode());
        let (cst, parse_errors) = parser.parse();
        let has_errors = !parse_errors.is_empty();
        let ast = SourceFile::cast(cst).expect("root must be SourceFile");

        if let Some(file_data) = self.workspace.files.get_mut(&file_id) {
            file_data.source = new_source;
            file_data.ast = ast;
            file_data.has_errors = has_errors;
            file_data.parse_errors = parse_errors;
        }

        let rev = self.revisions.entry(file_id).or_insert(1);
        *rev += 1;
        self.invalidate_downstream();
    }

    // -------------------------------------------------------------------------
    // Queries
    // -------------------------------------------------------------------------

    /// Query: build the HIR program from all loaded files (cached).
    ///
    /// Returns `(&HirProgram, &[SemanticError])`.  The HIR is recomputed only
    /// when at least one source file has changed since the last call.
    pub fn query_hir_program(&mut self) -> (&HirProgram, &[SemanticError]) {
        self.ensure_hir_fresh();
        let cache = self.hir_cache.as_ref().unwrap();
        (&cache.program, &cache.errors)
    }

    /// Query: borrow-check the program (cached).
    ///
    /// Rebuilds the HIR first if needed.
    pub fn query_borrow_check(&mut self) -> &[BorrowError] {
        if !self
            .borrow_cache
            .as_ref()
            .map(|c| self.deps_are_fresh(&c.deps))
            .unwrap_or(false)
        {
            self.recompute_borrow_check();
        }
        &self.borrow_cache.as_ref().unwrap().errors
    }

    /// Query: verify a single named function (cached per function name).
    ///
    /// Returns a reference to the cached `Result`.  The result is recomputed
    /// only when the program has changed since the last call for this function.
    pub fn query_verify_function(&mut self, func_name: &str) -> &Result<(), VerificationError> {
        if !self
            .verify_cache
            .get(func_name)
            .map(|c| self.deps_are_fresh(&c.deps))
            .unwrap_or(false)
        {
            self.recompute_verify_function(func_name);
        }
        &self.verify_cache.get(func_name).unwrap().result
    }

    /// Query: verify every function in the program.
    ///
    /// Uses the per-function verification cache so only stale functions are
    /// re-verified.  Returns the first error encountered, or `Ok(())`.
    pub fn query_verify_program(&mut self) -> Result<(), VerificationError> {
        self.ensure_hir_fresh();
        let func_names: Vec<String> = self
            .hir_cache
            .as_ref()
            .unwrap()
            .program
            .functions
            .iter()
            .filter(|f| f.body.is_some())
            .map(|f| f.name.clone())
            .collect();

        for func_name in func_names {
            self.query_verify_function(&func_name).clone()?;
        }
        Ok(())
    }

    // -------------------------------------------------------------------------
    // Introspection (mostly for tests and LSP diagnostics)
    // -------------------------------------------------------------------------

    /// Returns `true` if a valid HIR result is currently in the cache.
    pub fn hir_is_cached(&self) -> bool {
        self.hir_cache
            .as_ref()
            .map(|c| self.deps_are_fresh(&c.deps))
            .unwrap_or(false)
    }

    /// Returns `true` if a valid verification result for `func_name` is cached.
    pub fn verify_is_cached(&self, func_name: &str) -> bool {
        self.verify_cache
            .get(func_name)
            .map(|c| self.deps_are_fresh(&c.deps))
            .unwrap_or(false)
    }

    /// Read-only access to the underlying workspace.
    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }

    pub fn entry_file_id(&self) -> FileId {
        self.workspace.entry_file_id
    }

    /// Returns `true` if any loaded file had parse errors.
    pub fn has_parse_errors(&self) -> bool {
        self.workspace.files.values().any(|f| f.has_errors)
    }

    // -------------------------------------------------------------------------
    // Private helpers
    // -------------------------------------------------------------------------

    fn current_deps(&self) -> BTreeMap<FileId, Revision> {
        self.revisions.clone()
    }

    fn deps_are_fresh(&self, deps: &BTreeMap<FileId, Revision>) -> bool {
        deps.len() == self.revisions.len()
            && deps
                .iter()
                .all(|(id, rev)| self.revisions.get(id) == Some(rev))
    }

    fn invalidate_downstream(&mut self) {
        self.hir_cache = None;
        self.borrow_cache = None;
        self.verify_cache.clear();
    }

    fn ensure_hir_fresh(&mut self) {
        if !self
            .hir_cache
            .as_ref()
            .map(|c| self.deps_are_fresh(&c.deps))
            .unwrap_or(false)
        {
            let deps = self.current_deps();
            let mut ctx = LoweringContext::new();
            let program = ctx.lower_program(&self.workspace);
            self.hir_cache = Some(HirCache {
                program,
                errors: ctx.errors,
                deps,
            });
        }
    }

    fn recompute_borrow_check(&mut self) {
        let deps = self.current_deps();
        self.ensure_hir_fresh();
        // Clone the HIR to release the immutable borrow on self before the
        // mutable borrow needed to write borrow_cache.
        let hir_clone = self.hir_cache.as_ref().unwrap().program.clone();
        let mut checker = BorrowChecker::new();
        checker.check_program(&hir_clone);
        self.borrow_cache = Some(BorrowCache {
            errors: checker.errors,
            deps,
        });
    }

    fn recompute_verify_function(&mut self, func_name: &str) {
        let deps = self.current_deps();
        self.ensure_hir_fresh();
        // Clone just the target function so we can release the borrow before
        // mutating verify_cache.
        let func_clone = self
            .hir_cache
            .as_ref()
            .unwrap()
            .program
            .functions
            .iter()
            .find(|f| f.name == func_name)
            .cloned();

        let result = match func_clone {
            Some(func) if func.body.is_some() => verification::wp::verify_func(&func),
            _ => Ok(()),
        };

        self.verify_cache
            .insert(func_name.to_string(), VerifyCache { result, deps });
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates a `QueryContext` loaded with a single in-memory source string.
    /// The virtual file path is `test.vera`, so functions are named `test::<fn>`.
    fn ctx_from_source(source: &str) -> (QueryContext, FileId) {
        let mut ctx = QueryContext::new();
        let fid = ctx.load_from_source(Path::new("test.vera"), source.to_string());
        (ctx, fid)
    }

    // -------------------------------------------------------------------------
    // HIR cache behaviour
    // -------------------------------------------------------------------------

    #[test]
    fn test_hir_cache_empty_before_first_query() {
        let (ctx, _) = ctx_from_source("func main(): i32 { return 0; }");
        assert!(!ctx.hir_is_cached(), "cache should be empty before any query");
    }

    #[test]
    fn test_hir_cache_populated_after_query() {
        let (mut ctx, _) = ctx_from_source("func main(): i32 { return 0; }");
        ctx.query_hir_program();
        assert!(ctx.hir_is_cached(), "cache should be set after querying HIR");
    }

    #[test]
    fn test_hir_query_returns_correct_function_count() {
        let (mut ctx, _) = ctx_from_source("func f(): i32 { return 1; } func g(): i32 { return 2; }");
        let (prog, errors) = ctx.query_hir_program();
        assert!(errors.is_empty(), "unexpected semantic errors: {errors:?}");
        assert_eq!(prog.functions.len(), 2);
    }

    #[test]
    fn test_hir_cache_hit_returns_same_result() {
        let (mut ctx, _) = ctx_from_source("func main(): i32 { return 0; }");
        let count1 = ctx.query_hir_program().0.functions.len();
        let count2 = ctx.query_hir_program().0.functions.len();
        assert_eq!(count1, count2);
    }

    #[test]
    fn test_hir_cache_invalidated_on_source_update() {
        let (mut ctx, fid) = ctx_from_source("func main(): i32 { return 0; }");
        ctx.query_hir_program();
        assert!(ctx.hir_is_cached());

        ctx.update_file_source(fid, "func main(): i32 { return 42; }".to_string());
        assert!(!ctx.hir_is_cached(), "HIR cache must be cleared when source changes");
    }

    #[test]
    fn test_hir_recomputed_after_invalidation() {
        let (mut ctx, fid) = ctx_from_source("func main(): i32 { return 0; }");
        ctx.query_hir_program();

        // Add a second function by updating the source.
        ctx.update_file_source(
            fid,
            "func main(): i32 { return 0; } func extra(): i32 { return 1; }".to_string(),
        );
        let (prog, _) = ctx.query_hir_program();
        assert_eq!(prog.functions.len(), 2, "HIR should reflect the updated source");
    }

    // -------------------------------------------------------------------------
    // Borrow-check cache behaviour
    // -------------------------------------------------------------------------

    #[test]
    fn test_borrow_check_cache_invalidated_on_update() {
        let (mut ctx, fid) = ctx_from_source("func main(): i32 { return 0; }");
        ctx.query_borrow_check();
        assert!(ctx.borrow_cache.is_some());

        ctx.update_file_source(fid, "func main(): i32 { return 1; }".to_string());
        assert!(ctx.borrow_cache.is_none(), "borrow cache must be cleared on source change");
    }

    // -------------------------------------------------------------------------
    // Verification cache behaviour
    // -------------------------------------------------------------------------

    #[test]
    fn test_verify_cache_empty_before_query() {
        let (ctx, _) = ctx_from_source("func main(): i32 { return 0; }");
        assert!(!ctx.verify_is_cached("test::main"));
    }

    #[test]
    fn test_verify_cache_populated_after_query() {
        let (mut ctx, _) = ctx_from_source("func main(): i32 { return 0; }");
        ctx.query_verify_function("test::main");
        assert!(ctx.verify_is_cached("test::main"), "verify cache should be set after query");
    }

    #[test]
    fn test_verify_unknown_function_returns_ok() {
        let (mut ctx, _) = ctx_from_source("func main(): i32 { return 0; }");
        let result = ctx.query_verify_function("test::nonexistent").clone();
        assert!(result.is_ok(), "verifying a nonexistent function should return Ok");
    }

    #[test]
    fn test_verify_cache_invalidated_on_source_update() {
        let (mut ctx, fid) = ctx_from_source("func main(): i32 { return 0; }");
        ctx.query_verify_function("test::main");
        assert!(ctx.verify_is_cached("test::main"));

        ctx.update_file_source(fid, "func main(): i32 { return 1; }".to_string());
        assert!(
            !ctx.verify_is_cached("test::main"),
            "verify cache must be cleared when source changes"
        );
    }

    #[test]
    fn test_verify_program_verifies_all_functions() {
        let src = "func f(): i32 { return 1; } func g(): i32 { return 2; }";
        let (mut ctx, _) = ctx_from_source(src);
        let result = ctx.query_verify_program();
        assert!(result.is_ok());
        assert!(ctx.verify_is_cached("test::f"));
        assert!(ctx.verify_is_cached("test::g"));
    }
}
