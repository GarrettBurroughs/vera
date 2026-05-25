use miette::Diagnostic;
use thiserror::Error;
use std::collections::BTreeMap;
use crate::parser::ast::{self, AstNode};
use crate::hir::{Span, HirExprKind, HirStmtKind, HirProgram, HirFunc, HirType, HirBlock, HirStmt, HirExpr, BinaryOp, UnaryOp, HirPattern};

#[derive(Error, Debug, Diagnostic)]
pub enum SemanticError {
    #[error("Type mismatch: expected {expected:?}, found {found:?}")]
    #[diagnostic(code(vera::type_mismatch))]
    TypeMismatch {
        expected: HirType,
        found: HirType,
    },
    
    #[error("Unknown type: {name}")]
    #[diagnostic(code(vera::unknown_type))]
    UnknownType {
        name: String,
    },
    
    #[error("{0}")]
    #[diagnostic(code(vera::custom))]
    Custom(String),

    #[error("Undefined variable: {name}")]
    #[diagnostic(code(vera::undefined_variable))]
    UndefinedVariable {
        name: String,
    },

    #[error("Cannot mutate constant variable: {name}")]
    #[diagnostic(code(vera::immutable_assignment))]
    ImmutableAssignment {
        name: String,
    },

    #[error("Binary operator mismatch: cannot apply {op} to {lhs:?} and {rhs:?}")]
    #[diagnostic(code(vera::bin_op_mismatch))]
    BinOpMismatch {
        op: String,
        lhs: HirType,
        rhs: HirType,
    },
}

#[derive(Clone)]
struct Scope {
    variables: BTreeMap<String, (HirType, bool)>, // type, is_const
}

#[derive(Default)]
#[allow(dead_code)] // `traits` and `impls` fields are scaffolded for the trait system (Phase 3)
pub struct TemplateRegistry {
    pub funcs: BTreeMap<String, ast::FuncDecl>,
    pub structs: BTreeMap<String, ast::StructDecl>,
    pub enums: BTreeMap<String, ast::EnumDecl>,
    pub variants: BTreeMap<String, ast::VariantDecl>,
    pub traits: BTreeMap<String, ast::TraitDecl>,
    pub impls: Vec<ast::ImplDecl>,
}

pub struct LoweringContext {
    pub errors: Vec<SemanticError>,
    scopes: Vec<Scope>,
    pub functions: BTreeMap<String, (Vec<(String, HirType)>, HirType)>, // name -> (params, ret_ty)
    pub structs: BTreeMap<String, Vec<(String, HirType)>>, // name -> fields
    pub enums: BTreeMap<String, Vec<String>>, // name -> variants
    pub variants: BTreeMap<String, Vec<(String, Vec<HirType>)>>, // name -> cases
    pub generic_templates: TemplateRegistry,
    pub type_env: BTreeMap<String, HirType>,
    pub func_worklist: Vec<(String, Vec<HirType>)>,
    pub struct_worklist: Vec<(String, Vec<HirType>)>,
    pub enum_worklist: Vec<(String, Vec<HirType>)>,
    pub variant_worklist: Vec<(String, Vec<HirType>)>,
    pub type_aliases: BTreeMap<String, HirType>,
    current_func_ret_type: HirType,
    in_unsafe_block: bool,
}

impl LoweringContext {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            scopes: Vec::new(),
            functions: BTreeMap::new(),
            structs: BTreeMap::new(),
            enums: BTreeMap::new(),
            variants: BTreeMap::new(),
            generic_templates: TemplateRegistry::default(),
            type_env: BTreeMap::new(),
            func_worklist: Vec::new(),
            struct_worklist: Vec::new(),
            enum_worklist: Vec::new(),
            variant_worklist: Vec::new(),
            type_aliases: BTreeMap::new(),
            current_func_ret_type: HirType::Void,
            in_unsafe_block: false,
        }
    }

    fn enter_scope(&mut self) {
        self.scopes.push(Scope { variables: BTreeMap::new() });
    }

    fn exit_scope(&mut self) {
        self.scopes.pop();
    }

    fn request_monomorphize_struct(&mut self, name: &str, args: Vec<HirType>) -> String {
        let monomorphized_name = if args.is_empty() {
            name.to_string()
        } else {
            format!("{}_{}", name, args.iter().map(ty_to_string).collect::<Vec<_>>().join("_"))
        };
        if self.structs.contains_key(&monomorphized_name) {
            return monomorphized_name;
        }
        if let Some(s) = self.generic_templates.structs.get(name).cloned() {
            // Insert dummy to prevent duplicate requests/infinite recursion
            self.structs.insert(monomorphized_name.clone(), Vec::new());
            
            let mut temp_env = self.type_env.clone();
            if let Some(params) = s.generic_params() {
                for (param, arg) in params.params().zip(args.iter()) {
                    temp_env.insert(param.as_string().unwrap_or_default(), arg.clone());
                }
            }
            
            let prev_env = std::mem::replace(&mut self.type_env, temp_env);
            let mut fields = Vec::new();
            for f in s.fields() {
                if let (Some(f_name), Some(f_ty_ref)) = (f.name(), f.ty()) {
                    let f_ty = self.lower_type(&f_ty_ref);
                    fields.push((f_name.text().to_string(), f_ty));
                }
            }
            self.type_env = prev_env;
            self.structs.insert(monomorphized_name.clone(), fields);
        }
        monomorphized_name
    }

    fn request_monomorphize_enum(&mut self, name: &str, args: Vec<HirType>) -> String {
        let monomorphized_name = if args.is_empty() {
            name.to_string()
        } else {
            format!("{}_{}", name, args.iter().map(ty_to_string).collect::<Vec<_>>().join("_"))
        };
        if self.enums.contains_key(&monomorphized_name) {
            return monomorphized_name;
        }
        if let Some(e) = self.generic_templates.enums.get(name).cloned() {
            let mut variants = Vec::new();
            for v in e.variants() {
                if let Some(v_name) = v.name() {
                    variants.push(v_name.text().to_string());
                }
            }
            self.enums.insert(monomorphized_name.clone(), variants);
        }
        monomorphized_name
    }

    fn request_monomorphize_variant(&mut self, name: &str, args: Vec<HirType>) -> String {
        let monomorphized_name = if args.is_empty() {
            name.to_string()
        } else {
            format!("{}_{}", name, args.iter().map(ty_to_string).collect::<Vec<_>>().join("_"))
        };
        if self.variants.contains_key(&monomorphized_name) {
            return monomorphized_name;
        }
        if let Some(v) = self.generic_templates.variants.get(name).cloned() {
            self.variants.insert(monomorphized_name.clone(), Vec::new());
            let mut temp_env = self.type_env.clone();
            if let Some(params) = v.generic_params() {
                for (param, arg) in params.params().zip(args.iter()) {
                    temp_env.insert(param.as_string().unwrap_or_default(), arg.clone());
                }
            }
            let prev_env = std::mem::replace(&mut self.type_env, temp_env);
            let mut cases = Vec::new();
            for case in v.cases() {
                let case_name = case.name().map(|n| n.text().to_string()).unwrap_or_default();
                let mut payload_tys = Vec::new();
                for ty_ref in case.types() {
                    payload_tys.push(self.lower_type(&ty_ref));
                }
                cases.push((case_name, payload_tys));
            }
            self.type_env = prev_env;
            self.variants.insert(monomorphized_name.clone(), cases);
        }
        monomorphized_name
    }

    fn request_monomorphize_func(&mut self, name: &str, args: Vec<HirType>) -> String {
        let monomorphized_name = if args.is_empty() {
            name.to_string()
        } else {
            format!("{}_{}", name, args.iter().map(ty_to_string).collect::<Vec<_>>().join("_"))
        };
        if self.functions.contains_key(&monomorphized_name) {
            return monomorphized_name;
        }
        if let Some(func) = self.generic_templates.funcs.get(name).cloned() {
            self.func_worklist.push((name.to_string(), args.clone()));
            
            let old_env = self.type_env.clone();
            self.type_env = self.type_aliases.clone();
            if let Some(params) = func.generic_params() {
                for (param, arg) in params.params().zip(args.iter()) {
                    self.type_env.insert(param.as_string().unwrap_or_default(), arg.clone());
                }
            }
            
            let ret_type = match func.ret_type() {
                Some(type_ref) => self.lower_type(&type_ref),
                None => HirType::Void,
            };
            let mut params_list = Vec::new();
            if let Some(param_list) = func.param_list() {
                for param in param_list.params() {
                    if let (Some(p_name), Some(p_ty_ref)) = (param.name(), param.ty()) {
                        let p_ty = self.lower_type(&p_ty_ref);
                        params_list.push((p_name, p_ty));
                    }
                }
            }
            self.functions.insert(monomorphized_name.clone(), (params_list, ret_type));
            
            self.type_env = old_env;
        }
        monomorphized_name
    }

    fn declare_var(&mut self, name: String, ty: HirType, is_const: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.variables.insert(name, (ty, is_const));
        }
    }

    fn lookup_var(&self, name: &str) -> Option<(HirType, bool)> {
        for scope in self.scopes.iter().rev() {
            if let Some(var) = scope.variables.get(name) {
                return Some(var.clone());
            }
        }
        None
    }

    fn types_compatible(&self, expected: &HirType, found: &HirType) -> bool {
        if expected == found {
            return true;
        }
        match (expected, found) {
            (HirType::Ptr(t1, _), HirType::Ref(t2, _)) => t1 == t2,
            _ => false,
        }
    }
}

pub fn ty_to_string(ty: &HirType) -> String {
    match ty {
        HirType::I32 => "I32".to_string(),
        HirType::Bool => "Bool".to_string(),
        HirType::Void => "Void".to_string(),
        HirType::Struct(name) => name.clone(),
        HirType::Enum(name) => name.clone(),
        HirType::Variant(name) => name.clone(),
        HirType::Array(t, size) => format!("Array_{}_{}", ty_to_string(t), size),
        HirType::Slice(t) => format!("Slice_{}", ty_to_string(t)),
        HirType::Ptr(t, mutability) => format!("Ptr_{}_{}", if *mutability { "mut" } else { "const" }, ty_to_string(t)),
        HirType::Ref(t, mutability) => format!("Ref_{}_{}", if *mutability { "mut" } else { "const" }, ty_to_string(t)),
        HirType::Func(params, ret) => {
            let mut s = "Func".to_string();
            for p in params {
                s.push('_');
                s.push_str(&ty_to_string(p));
            }
            s.push_str("_ret_");
            s.push_str(&ty_to_string(ret));
            s
        }
        HirType::Refinement(base, _) => format!("Refinement_{}", ty_to_string(base)),
        HirType::Result(ok, err) => format!("Result_{}_{}", ty_to_string(ok), ty_to_string(err)),
        HirType::Error => "Error".to_string(),
    }
}

impl LoweringContext {
    pub fn lower_program(&mut self, source_file: &ast::SourceFile) -> HirProgram {
        self.generic_templates = TemplateRegistry::default();
        self.structs.clear();
        self.enums.clear();
        self.variants.clear();
        self.functions.clear();
        self.func_worklist.clear();
        self.struct_worklist.clear();
        self.enum_worklist.clear();
        self.variant_worklist.clear();
        self.type_aliases.clear();

        for a in source_file.type_aliases() {
            let name = a.name().map(|n| n.text().to_string()).unwrap_or_default();
            if let Some(ty_ref) = a.ty() {
                let lowered = self.lower_type(&ty_ref);
                self.type_env.insert(name.clone(), lowered.clone());
                self.type_aliases.insert(name, lowered);
            }
        }

        // Pass 0: Gather templates
        for s in source_file.structs() {
            let name = s.name().map(|n| n.text().to_string()).unwrap_or_default();
            self.generic_templates.structs.insert(name.clone(), s.clone());
            if s.generic_params().is_none() {
                self.request_monomorphize_struct(&name, Vec::new());
            }
        }

        for e in source_file.enums() {
            let name = e.name().map(|n| n.text().to_string()).unwrap_or_default();
            self.generic_templates.enums.insert(name.clone(), e.clone());
            if e.generic_params().is_none() {
                self.request_monomorphize_enum(&name, Vec::new());
            }
        }

        for v in source_file.variants() {
            let name = v.name().map(|n| n.text().to_string()).unwrap_or_default();
            self.generic_templates.variants.insert(name.clone(), v.clone());
            if v.generic_params().is_none() {
                self.request_monomorphize_variant(&name, Vec::new());
            }
        }

        for func in source_file.functions() {
            let name = func.name().map(|n| n.text().to_string()).unwrap_or_default();
            self.generic_templates.funcs.insert(name.clone(), func.clone());
            if func.generic_params().is_none() {
                self.request_monomorphize_func(&name, Vec::new());
            }
        }

        // TODO: Traits and Impls

        // Process worklists until empty
        let mut functions = Vec::new();

        loop {
            if let Some((name, args)) = self.struct_worklist.pop() {
                if let Some(s) = self.generic_templates.structs.get(&name).cloned() {
                    self.type_env.clear();
                    if let Some(params) = s.generic_params() {
                        for (param, arg) in params.params().zip(args.iter()) {
                            self.type_env.insert(param.as_string().unwrap_or_default(), arg.clone());
                        }
                    }
                    
                    let mut fields = Vec::new();
                    for f in s.fields() {
                        if let (Some(f_name), Some(f_ty_ref)) = (f.name(), f.ty()) {
                            let f_ty = self.lower_type(&f_ty_ref);
                            fields.push((f_name.text().to_string(), f_ty));
                        }
                    }
                    let mono_name = if args.is_empty() { name } else { format!("{}_{}", name, args.iter().map(ty_to_string).collect::<Vec<_>>().join("_")) };
                    self.structs.insert(mono_name, fields);
                }
                continue;
            }

            if let Some((name, args)) = self.enum_worklist.pop() {
                if let Some(e) = self.generic_templates.enums.get(&name).cloned() {
                    let mut variants = Vec::new();
                    for v in e.variants() {
                        if let Some(v_name) = v.name() {
                            variants.push(v_name.text().to_string());
                        }
                    }
                    let mono_name = if args.is_empty() { name } else { format!("{}_{}", name, args.iter().map(ty_to_string).collect::<Vec<_>>().join("_")) };
                    self.enums.insert(mono_name, variants);
                }
                continue;
            }

            if let Some((name, args)) = self.variant_worklist.pop() {
                if let Some(v) = self.generic_templates.variants.get(&name).cloned() {
                    self.type_env.clear();
                    if let Some(params) = v.generic_params() {
                        for (param, arg) in params.params().zip(args.iter()) {
                            self.type_env.insert(param.as_string().unwrap_or_default(), arg.clone());
                        }
                    }
                    
                    let mut cases = Vec::new();
                    for case in v.cases() {
                        let case_name = case.name().map(|n| n.text().to_string()).unwrap_or_default();
                        let mut payload_tys = Vec::new();
                        for ty_ref in case.types() {
                            payload_tys.push(self.lower_type(&ty_ref));
                        }
                        cases.push((case_name, payload_tys));
                    }
                    let mono_name = if args.is_empty() { name } else { format!("{}_{}", name, args.iter().map(ty_to_string).collect::<Vec<_>>().join("_")) };
                    self.variants.insert(mono_name, cases);
                }
                continue;
            }

            if let Some((name, args)) = self.func_worklist.pop() {
                if let Some(func) = self.generic_templates.funcs.get(&name).cloned() {
                    self.type_env = self.type_aliases.clone();
                    if let Some(params) = func.generic_params() {
                        for (param, arg) in params.params().zip(args.iter()) {
                            self.type_env.insert(param.as_string().unwrap_or_default(), arg.clone());
                        }
                    }
                    
                    let mono_name = if args.is_empty() { name.clone() } else { format!("{}_{}", name, args.iter().map(ty_to_string).collect::<Vec<_>>().join("_")) };
                    
                    // The signature was already computed and stored by request_monomorphize_func
                    if let Some((params_list, ret_type)) = self.functions.get(&mono_name).cloned() {
                        // Now lower the body
                        if let Some(f) = self.lower_func_mono(&func, &mono_name, params_list, ret_type) {
                            functions.push(f);
                        }
                    }
                }
                continue;
            }

            break;
        }

        HirProgram {
            type_aliases: self.type_aliases.clone(),
            structs: self.structs.clone(),
            enums: self.enums.clone(),
            variants: self.variants.clone(),
            functions,
        }
    }

    fn lower_func_mono(&mut self, func: &ast::FuncDecl, mono_name: &str, params: Vec<(String, HirType)>, ret_type: HirType) -> Option<HirFunc> {
        self.enter_scope(); // Function scope
        self.current_func_ret_type = ret_type.clone();

        for (p_name, p_ty) in &params {
            self.declare_var(p_name.clone(), p_ty.clone(), false);
        }

        // Spec clauses (requires/ensures) are lowered AFTER entering scope and declaring
        // parameters, because they can reference the function's formal parameters.
        let mut requires = Vec::new();
        let mut ensures = Vec::new();
        let mut assigns = Vec::new();
        if let Some(spec) = func.spec() {
            let prev_unsafe = self.in_unsafe_block;
            self.in_unsafe_block = true;
            for req in spec.requires_clauses() {
                if let Some(e) = req.expr() {
                    requires.push(self.lower_expr(&e));
                }
            }
            
            for ass in spec.assigns_clauses() {
                for expr in ass.exprs() {
                    assigns.push(self.lower_expr(&expr));
                }
            }
            
            self.enter_scope();
            if ret_type != HirType::Void {
                self.declare_var("result".to_string(), ret_type.clone(), true);
            }
            for ens in spec.ensures_clauses() {
                if let Some(e) = ens.expr() {
                    ensures.push(self.lower_expr(&e));
                }
            }
            self.exit_scope();
            self.in_unsafe_block = prev_unsafe;
        }

        let body = match func.body() {
            Some(block) => self.lower_block(&block),
            None => HirBlock { statements: Vec::new() },
        };

        self.exit_scope();
        self.current_func_ret_type = HirType::Void;

        Some(HirFunc {
            name: mono_name.to_string(),
            params,
            ret_type,
            body,
            requires,
            ensures,
            assigns,
        })
    }

    fn lower_type(&mut self, type_ref: &ast::TypeRef) -> HirType {
        let base_ty = self.lower_type_base(type_ref);
        if let Some(ref_ty) = type_ref.refinement()
            && let Some(cond) = ref_ty.condition() {
                self.enter_scope();
                self.declare_var("self".to_string(), base_ty.clone(), true);
                let lowered_cond = self.lower_expr(&cond);
                self.exit_scope();
                return HirType::Refinement(Box::new(base_ty), Box::new(lowered_cond));
            }
        base_ty
    }

    fn lower_type_base(&mut self, type_ref: &ast::TypeRef) -> HirType {
        if let Some(arr) = type_ref.syntax().children().find_map(ast::ArrayType::cast) {
            let inner_ty = arr.ty().map(|t| self.lower_type(&t)).unwrap_or(HirType::Error);
            let size = arr.size().and_then(|lit| {
                lit.syntax().text().to_string().trim().parse::<u64>().ok()
            }).unwrap_or(0);
            return HirType::Array(Box::new(inner_ty), size);
        }
        
        if let Some(slc) = type_ref.syntax().children().find_map(ast::SliceType::cast) {
            let inner_ty = slc.ty().map(|t| self.lower_type(&t)).unwrap_or(HirType::Error);
            return HirType::Slice(Box::new(inner_ty));
        }

        if let Some(res) = type_ref.syntax().children().find_map(ast::ResultType::cast) {
            let ok_ty = res.ok_ty().map(|t| self.lower_type(&t)).unwrap_or(HirType::Error);
            let err_ty = res.err_ty().map(|t| self.lower_type(&t)).unwrap_or(HirType::Error);
            return HirType::Result(Box::new(ok_ty), Box::new(err_ty));
        }

        if let Some(r) = type_ref.syntax().children().find_map(ast::RefType::cast) {
            let is_mut = r.syntax().children_with_tokens().any(|it| it.kind() == crate::parser::syntax::SyntaxKind::KwMut);
            let inner_ty = r.ty().map(|t| self.lower_type(&t)).unwrap_or(HirType::Error);
            return HirType::Ref(Box::new(inner_ty), is_mut);
        }

        if let Some(p) = type_ref.syntax().children().find_map(ast::PointerType::cast) {
            let is_mut = p.syntax().children_with_tokens().any(|it| it.kind() == crate::parser::syntax::SyntaxKind::KwMut);
            let inner_ty = p.ty().map(|t| self.lower_type(&t)).unwrap_or(HirType::Error);
            return HirType::Ptr(Box::new(inner_ty), is_mut);
        }

        if let Some(f) = type_ref.syntax().children().find_map(ast::FuncType::cast) {
            let types = f.types();
            let has_arrow = f.syntax().children_with_tokens().any(|it| it.kind() == crate::parser::syntax::SyntaxKind::Arrow);
            
            let mut param_tys = Vec::new();
            let ret_ty = if has_arrow && !types.is_empty() {
                for t in types.iter().take(types.len() - 1) {
                    param_tys.push(self.lower_type(t));
                }
                self.lower_type(types.last().unwrap())
            } else {
                for t in types {
                    param_tys.push(self.lower_type(&t));
                }
                HirType::Void
            };
            return HirType::Func(param_tys, Box::new(ret_ty));
        }

        let name = type_ref.as_string().unwrap_or_default();
        if let Some(ty) = self.type_env.get(&name) {
            return ty.clone();
        }

        match name.as_str() {
            "i32" => HirType::I32,
            "bool" => HirType::Bool,
            "" => HirType::Error,
            _ => {
                if let Some(generic_args) = type_ref.generic_args() {
                    let mut args = Vec::new();
                    for arg in generic_args.args() {
                        args.push(self.lower_type(&arg));
                    }
                    if self.generic_templates.structs.contains_key(&name) {
                        let mono_name = self.request_monomorphize_struct(&name, args);
                        return HirType::Struct(mono_name);
                    } else if self.generic_templates.enums.contains_key(&name) {
                        let mono_name = self.request_monomorphize_enum(&name, args);
                        return HirType::Enum(mono_name);
                    }
                }

                if self.structs.contains_key(&name) || self.generic_templates.structs.contains_key(&name) {
                    if self.generic_templates.structs.contains_key(&name) && type_ref.generic_args().is_none() {
                        // Needs generic arguments! We should probably throw an error, but let's just request with empty args if they have defaults, or we just pass empty args and fail later if it expects some.
                        // Actually, if it's a generic struct, it must have arguments unless we infer them. 
                        // But wait! If we are inside the template itself, `Point` refers to `Point<T>`.
                        // Let's just request monomorphization with empty args for now.
                        let mono_name = self.request_monomorphize_struct(&name, Vec::new());
                        HirType::Struct(mono_name)
                    } else {
                        HirType::Struct(name)
                    }
                } else if self.enums.contains_key(&name) || self.generic_templates.enums.contains_key(&name) {
                    if self.generic_templates.enums.contains_key(&name) && type_ref.generic_args().is_none() {
                        let mono_name = self.request_monomorphize_enum(&name, Vec::new());
                        HirType::Enum(mono_name)
                    } else {
                        HirType::Enum(name)
                    }
                } else if self.variants.contains_key(&name) {
                    HirType::Variant(name)
                } else {
                    self.errors.push(SemanticError::UnknownType { name: name.clone() });
                    HirType::Error
                }
            }
        }
    }

    fn lower_pattern(&mut self, pat: &ast::Pattern) -> HirPattern {
        let name = pat.name().map(|n| n.text().to_string()).unwrap_or_default();
        if name == "_" {
            HirPattern::Wildcard
        } else {
            let has_paren = pat.syntax().children().any(|c| c.kind() == crate::parser::syntax::SyntaxKind::PATTERN);
            if has_paren {
                let mut bindings = Vec::new();
                for child in pat.syntax().children().filter_map(ast::Pattern::cast) {
                    if let Some(c_name) = child.name() {
                        bindings.push(c_name.text().to_string());
                    }
                }
                HirPattern::VariantCase(name, bindings)
            } else {
                let mut is_case = false;
                for cases in self.variants.values() {
                    if cases.iter().any(|(n, _)| n == &name) {
                        is_case = true;
                        break;
                    }
                }
                if is_case {
                    HirPattern::VariantCase(name, Vec::new())
                } else {
                    HirPattern::Binding(name)
                }
            }
        }
    }

    fn declare_pattern_bindings(&mut self, pat: &HirPattern, val_ty: &HirType) {
        match pat {
            HirPattern::Binding(name) => {
                self.declare_var(name.clone(), val_ty.clone(), true);
            }
            HirPattern::VariantCase(case_name, bindings) => {
                if let HirType::Variant(v_name) = val_ty {
                    let payload_tys_opt = if let Some(cases) = self.variants.get(v_name) {
                        if let Some((_, payload_tys)) = cases.iter().find(|(n, _)| n == case_name) {
                            Some(payload_tys.clone())
                        } else { None }
                    } else { None };
                    
                    if let Some(payload_tys) = payload_tys_opt {
                        for (b_name, b_ty) in bindings.iter().zip(payload_tys.iter()) {
                            self.declare_var(b_name.clone(), b_ty.clone(), true);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn lower_block(&mut self, block: &ast::BlockExpr) -> HirBlock {
        self.enter_scope(); // Block scope
        let mut statements = Vec::new();
        
        for stmt in block.statements() {
            statements.push(self.lower_stmt(&stmt));
        }

        self.exit_scope();
        HirBlock { statements }
    }

    fn lower_stmt(&mut self, stmt: &ast::Stmt) -> HirStmt {
        match stmt {
            ast::Stmt::ReturnStmt(ret_stmt) => {
                let expr = ret_stmt.expr().map(|e| self.lower_expr(&e));
                
                let expr_ty = expr.as_ref().map(|e| e.ty()).unwrap_or(HirType::Void);
                let expected = self.current_func_ret_type.clone();
                if expr_ty != HirType::Error && expr_ty != expected && expected != HirType::Error {
                    self.errors.push(SemanticError::TypeMismatch {
                        expected,
                        found: expr_ty,
                    });
                }
                
                HirStmt::new(HirStmtKind::Return(expr), Span::default())
            }
            ast::Stmt::LetStmt(let_stmt) => {
                let name = let_stmt.name().map(|n| n.text().to_string()).unwrap_or_default();
                let is_const = let_stmt.is_const();
                
                let initializer = if let Some(expr) = let_stmt.initializer() {
                    self.lower_expr(&expr)
                } else {
                    HirExpr::new(HirExprKind::Error, Span::default())
                };
                
                let declared_ty = if let Some(ty_ref) = let_stmt.ty() {
                    self.lower_type(&ty_ref)
                } else {
                    initializer.ty()
                };

                if initializer.ty() != HirType::Error && declared_ty != HirType::Error && !self.types_compatible(&declared_ty, &initializer.ty()) {
                    self.errors.push(SemanticError::TypeMismatch {
                        expected: declared_ty.clone(),
                        found: initializer.ty(),
                    });
                }

                self.declare_var(name.clone(), declared_ty.clone(), is_const);

                HirStmt::new(HirStmtKind::Let(name, is_const, declared_ty, initializer), Span::default())
            }
            ast::Stmt::ExprStmt(expr_stmt) => {
                if let Some(expr) = expr_stmt.expr() {
                    HirStmt::new(HirStmtKind::Expr(self.lower_expr(&expr)), Span::default())
                } else {
                    HirStmt::new(HirStmtKind::Error, Span::default())
                }
            }
            ast::Stmt::IfExpr(if_expr) => {
                HirStmt::new(HirStmtKind::Expr(self.lower_if_expr(&if_expr)), Span::default())
            }
            ast::Stmt::AssertStmt(assert_stmt) => {
                let expr = assert_stmt.expr().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::new(HirExprKind::Error, Span::default()));
                HirStmt::new(HirStmtKind::Assert(expr), Span::default())
            }
            ast::Stmt::AssumeStmt(assume_stmt) => {
                let expr = assume_stmt.expr().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::new(HirExprKind::Error, Span::default()));
                HirStmt::new(HirStmtKind::Assume(expr), Span::default())
            }
            ast::Stmt::GhostBlock(ghost_block) => {
                let block = ghost_block.block().map(|b| self.lower_block(&b)).unwrap_or_else(|| HirBlock { statements: Vec::new() });
                HirStmt::new(HirStmtKind::GhostBlock(block), Span::default())
            }
            ast::Stmt::WhileStmt(while_stmt) => {
                let cond = while_stmt.condition().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::new(HirExprKind::Error, Span::default()));
                if cond.ty() != HirType::Error && cond.ty() != HirType::Bool {
                    self.errors.push(SemanticError::TypeMismatch {
                        expected: HirType::Bool,
                        found: cond.ty(),
                    });
                }
                
                let mut invariants = Vec::new();
                let mut decreases = None;
                let mut assigns = Vec::new();
                if let Some(spec) = while_stmt.spec() {
                    let prev_unsafe = self.in_unsafe_block;
                    self.in_unsafe_block = true;
                    for inv in spec.invariant_clauses() {
                        if let Some(expr) = inv.expr() {
                            invariants.push(self.lower_expr(&expr));
                        }
                    }
                    decreases = spec.decreases_clauses()
                        .next()
                        .and_then(|d| d.expr())
                        .map(|e| self.lower_expr(&e));
                    for ass in spec.assigns_clauses() {
                        for expr in ass.exprs() {
                            assigns.push(self.lower_expr(&expr));
                        }
                    }
                    self.in_unsafe_block = prev_unsafe;
                }
                
                let body = while_stmt.body().map(|b| self.lower_block(&b)).unwrap_or(HirBlock { statements: Vec::new() });
                HirStmt::new(HirStmtKind::While(cond, body, invariants, decreases, assigns), Span::default())
            }
            ast::Stmt::BreakStmt(_) => {
                HirStmt::new(HirStmtKind::Break, Span::default())
            }
            ast::Stmt::ContinueStmt(_) => {
                HirStmt::new(HirStmtKind::Continue, Span::default())
            }
            ast::Stmt::ForStmt(for_stmt) => {
                let item_name = for_stmt.item().map(|t| t.text().to_string()).unwrap_or_default();
                let iterable = for_stmt.iterable().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::new(HirExprKind::Error, Span::default()));
                
                let inner_ty = match iterable.ty() {
                    HirType::Array(t, _) => *t,
                    HirType::Slice(t) => *t,
                    HirType::Error => HirType::Error,
                    _ => {
                        self.errors.push(SemanticError::Custom(format!("Type {:?} is not iterable", iterable.ty())));
                        HirType::Error
                    }
                };
                
                self.enter_scope();
                self.declare_var(item_name.clone(), inner_ty, false); // iteration variable is not const
                let body = for_stmt.body().map(|b| self.lower_block(&b)).unwrap_or(HirBlock { statements: Vec::new() });
                self.exit_scope();
                
                let mut assigns = Vec::new();
                // If we want to support spec blocks in For loops, we would parse them here.
                // For now we leave it empty.
                
                HirStmt::new(HirStmtKind::For(item_name, iterable, body, assigns), Span::default())
            }
        }
    }

    fn lower_expr(&mut self, expr: &ast::Expr) -> HirExpr {
        match expr {
            ast::Expr::Literal(lit) => {
                if let Some(tok) = lit.token() {
                    if tok.kind() == crate::parser::syntax::SyntaxKind::IntLit {
                        let val: i64 = tok.text().parse().unwrap_or(0);
                        HirExpr::new(HirExprKind::IntLiteral(val, HirType::I32), Span::default())
                    } else if tok.kind() == crate::parser::syntax::SyntaxKind::BoolTrue {
                        HirExpr::new(HirExprKind::BoolLiteral(true, HirType::Bool), Span::default())
                    } else if tok.kind() == crate::parser::syntax::SyntaxKind::BoolFalse {
                        HirExpr::new(HirExprKind::BoolLiteral(false, HirType::Bool), Span::default())
                    } else {
                        HirExpr::new(HirExprKind::Error, Span::default())
                    }
                } else {
                    HirExpr::new(HirExprKind::Error, Span::default())
                }
            }
            ast::Expr::NameRef(name_ref) => {
                let name = name_ref.ident().map(|n| n.text().to_string()).unwrap_or_default();
                if let Some((ty, _is_const)) = self.lookup_var(&name) {
                    HirExpr::new(HirExprKind::VarRef(name, ty), Span::default())
                } else {
                    self.errors.push(SemanticError::UndefinedVariable { name: name.clone() });
                    HirExpr::new(HirExprKind::Error, Span::default())
                }
            }
            ast::Expr::CallExpr(call_expr) => {
                let mut is_variant_constructor = false;
                let mut variant_name_opt = None;
                let mut case_name_opt = None;
                
                if let Some(ast::Expr::FieldExpr(field_expr)) = call_expr.callee()
                    && let Some(ast::Expr::NameRef(name_ref)) = field_expr.base() {
                        let name = name_ref.ident().map(|n| n.text().to_string()).unwrap_or_default();
                        if self.variants.contains_key(&name) {
                            let field_name = field_expr.field().and_then(|n| n.ident()).map(|i| i.text().to_string()).unwrap_or_default();
                            is_variant_constructor = true;
                            variant_name_opt = Some(name);
                            case_name_opt = Some(field_name);
                        }
                    }
                
                if is_variant_constructor {
                    let variant_name = variant_name_opt.unwrap();
                    let case_name = case_name_opt.unwrap();
                    let payload_tys_opt = {
                        let cases = self.variants.get(&variant_name).unwrap();
                        cases.iter().find(|(n, _)| n == &case_name).map(|(_, tys)| tys.clone())
                    };
                    
                    if let Some(payload_tys) = payload_tys_opt {
                        let mut args = Vec::new();
                        if let Some(arg_list) = call_expr.arg_list() {
                            for arg in arg_list.args() {
                                args.push(self.lower_expr(&arg));
                            }
                        }
                        
                        if args.len() != payload_tys.len() {
                            self.errors.push(SemanticError::UndefinedVariable { name: format!("arity mismatch for variant case {}.{}", variant_name, case_name) });
                            HirExpr::new(HirExprKind::Error, Span::default())
                        } else {
                            for (arg, expected_ty) in args.iter().zip(payload_tys.iter()) {
                                if arg.ty() != HirType::Error && arg.ty() != *expected_ty {
                                    self.errors.push(SemanticError::TypeMismatch {
                                        expected: expected_ty.clone(),
                                        found: arg.ty(),
                                    });
                                }
                            }
                            HirExpr::new(HirExprKind::VariantConstructor(variant_name.clone(), case_name, args, HirType::Variant(variant_name)), Span::default())
                        }
                    } else {
                        self.errors.push(SemanticError::UndefinedVariable { name: format!("variant case {} in variant {}", case_name, variant_name) });
                        HirExpr::new(HirExprKind::Error, Span::default())
                    }
                } else if let Some(callee_ast) = call_expr.callee() {
                    let mut is_direct = false;
                    let mut direct_name = String::new();
                    if let ast::Expr::NameRef(name_ref) = &callee_ast {
                        let name = name_ref.ident().map(|n| n.text().to_string()).unwrap_or_default();
                        if name == "Ok" || name == "Err" || name == "valid" || name == "valid_read" || name == "separated" || self.functions.contains_key(&name) {
                            is_direct = true;
                            direct_name = name.clone();
                        } else if self.generic_templates.funcs.contains_key(&name) {
                            self.errors.push(SemanticError::UndefinedVariable { name: format!("generic function {} requires explicit generic arguments (turbofish)", name) });
                        }
                    } else if let ast::Expr::GenericInstExpr(gen_inst) = &callee_ast
                        && let Some(ast::Expr::NameRef(name_ref)) = gen_inst.expr() {
                            let name = name_ref.ident().map(|n| n.text().to_string()).unwrap_or_default();
                            if self.generic_templates.funcs.contains_key(&name) {
                                if let Some(generic_args) = gen_inst.generic_args() {
                                    let mut args = Vec::new();
                                    for arg in generic_args.args() {
                                        args.push(self.lower_type(&arg));
                                    }
                                    let mono_name = self.request_monomorphize_func(&name, args);
                                    is_direct = true;
                                    direct_name = mono_name;
                                }
                            } else {
                                self.errors.push(SemanticError::UndefinedVariable { name: format!("undefined generic function {}", name) });
                            }
                        }
                    
                    let mut args = Vec::new();
                    if let Some(arg_list) = call_expr.arg_list() {
                        for arg in arg_list.args() {
                            args.push(self.lower_expr(&arg));
                        }
                    }
                    
                    if is_direct {
                        if direct_name == "Ok" {
                            let arg = args.into_iter().next().unwrap_or(HirExpr::new(HirExprKind::Error, Span::default()));
                            HirExpr::new(HirExprKind::ResultOk(Box::new(arg), self.current_func_ret_type.clone()), Span::default())
                        } else if direct_name == "Err" {
                            let arg = args.into_iter().next().unwrap_or(HirExpr::new(HirExprKind::Error, Span::default()));
                            HirExpr::new(HirExprKind::ResultErr(Box::new(arg), self.current_func_ret_type.clone()), Span::default())
                        } else if direct_name == "valid" || direct_name == "valid_read" || direct_name == "separated" {
                            HirExpr::new(HirExprKind::Call(direct_name, args, HirType::Bool), Span::default())
                        } else {
                            let func_info = self.functions.get(&direct_name).cloned().unwrap();
                            if args.len() != func_info.0.len() {
                                self.errors.push(SemanticError::Custom(format!("arity mismatch for {}", direct_name)));
                                HirExpr::new(HirExprKind::Error, Span::default())
                            } else {
                                HirExpr::new(HirExprKind::Call(direct_name, args, func_info.1.clone()), Span::default())
                            }
                        }
                    } else {
                        let callee = self.lower_expr(&callee_ast);
                        if let HirType::Func(param_tys, ret_ty) = callee.ty() {
                            if args.len() != param_tys.len() {
                                self.errors.push(SemanticError::Custom("arity mismatch for indirect call".to_string()));
                                HirExpr::new(HirExprKind::Error, Span::default())
                            } else {
                                HirExpr::new(HirExprKind::CallIndirect(Box::new(callee), args, *ret_ty), Span::default())
                            }
                        } else if callee.ty() != HirType::Error {
                            self.errors.push(SemanticError::Custom("expected function type".to_string()));
                            HirExpr::new(HirExprKind::Error, Span::default())
                        } else {
                            HirExpr::new(HirExprKind::Error, Span::default())
                        }
                    }
                } else {
                    HirExpr::new(HirExprKind::Error, Span::default())
                }
            }
            ast::Expr::BinExpr(bin_expr) => {
                let lhs = bin_expr.lhs().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::new(HirExprKind::Error, Span::default()));
                let rhs = bin_expr.rhs().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::new(HirExprKind::Error, Span::default()));
                let op_tok = bin_expr.op();
                
                if let Some(tok) = op_tok {
                    use crate::parser::syntax::SyntaxKind::*;
                    let (op, expected_ty, ret_ty): (BinaryOp, HirType, HirType) = match tok.kind() {
                        Plus => (BinaryOp::Add, HirType::I32, HirType::I32),
                        Minus => (BinaryOp::Sub, HirType::I32, HirType::I32),
                        Star => (BinaryOp::Mul, HirType::I32, HirType::I32),
                        Slash => (BinaryOp::Div, HirType::I32, HirType::I32),
                        Percent => (BinaryOp::Rem, HirType::I32, HirType::I32),
                        EqEq => (BinaryOp::Eq, lhs.ty(), HirType::Bool),
                        BangEq => (BinaryOp::Neq, lhs.ty(), HirType::Bool),
                        Less => (BinaryOp::Lt, HirType::I32, HirType::Bool),
                        Greater => (BinaryOp::Gt, HirType::I32, HirType::Bool),
                        LessEq => (BinaryOp::Le, HirType::I32, HirType::Bool),
                        GreaterEq => (BinaryOp::Ge, HirType::I32, HirType::Bool),
                        AmpAmp => (BinaryOp::And, HirType::Bool, HirType::Bool),
                        PipePipe => (BinaryOp::Or, HirType::Bool, HirType::Bool),
                        Implies => (BinaryOp::Implies, HirType::Bool, HirType::Bool),
                        Iff => (BinaryOp::Iff, HirType::Bool, HirType::Bool),
                        Eq => (BinaryOp::Assign, lhs.ty(), lhs.ty()), // Assignment returns the value
                        _ => return HirExpr::new(HirExprKind::Error, Span::default()),
                    };

                    if op == BinaryOp::Assign {
                        if !lhs.is_lvalue() {
                            self.errors.push(SemanticError::Custom("invalid assignment target: not an lvalue".to_string()));
                        } else if let HirExprKind::VarRef(name, _) = &lhs.kind
                            && let Some((_, is_const)) = self.lookup_var(name)
                                && is_const {
                                    self.errors.push(SemanticError::ImmutableAssignment { name: name.clone() });
                                }
                    }

                    if lhs.ty() != HirType::Error && rhs.ty() != HirType::Error {
                        if op != BinaryOp::Eq && op != BinaryOp::Neq && op != BinaryOp::Assign {
                            if lhs.ty() != expected_ty || rhs.ty() != expected_ty {
                                self.errors.push(SemanticError::BinOpMismatch {
                                    op: tok.text().to_string(),
                                    lhs: lhs.ty(),
                                    rhs: rhs.ty(),
                                });
                                return HirExpr::new(HirExprKind::Error, Span::default());
                            }
                        } else if op == BinaryOp::Assign {
                            if !self.types_compatible(&lhs.ty(), &rhs.ty()) {
                                self.errors.push(SemanticError::BinOpMismatch {
                                    op: tok.text().to_string(),
                                    lhs: lhs.ty(),
                                    rhs: rhs.ty(),
                                });
                                return HirExpr::new(HirExprKind::Error, Span::default());
                            }
                        } else if lhs.ty() != rhs.ty() {
                            self.errors.push(SemanticError::BinOpMismatch {
                                op: tok.text().to_string(),
                                lhs: lhs.ty(),
                                rhs: rhs.ty(),
                            });
                            return HirExpr::new(HirExprKind::Error, Span::default());
                        }
                    }

                    HirExpr::new(HirExprKind::BinaryOp(op, Box::new(lhs), Box::new(rhs), ret_ty), Span::default())
                } else {
                    HirExpr::new(HirExprKind::Error, Span::default())
                }
            }
            ast::Expr::PrefixExpr(prefix_expr) => {
                let inner = prefix_expr.expr().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::new(HirExprKind::Error, Span::default()));
                if let Some(op_tok) = prefix_expr.op() {
                    let op = match op_tok.kind() {
                        crate::parser::syntax::SyntaxKind::Minus => UnaryOp::Neg,
                        crate::parser::syntax::SyntaxKind::Bang => UnaryOp::Not,
                        _ => return HirExpr::new(HirExprKind::Error, Span::default()),
                    };
                    
                    let expected_ty = match op {
                        UnaryOp::Neg => HirType::I32,
                        UnaryOp::Not => HirType::Bool,
                    };
                    
                    if inner.ty() != HirType::Error && inner.ty() != expected_ty {
                        self.errors.push(SemanticError::BinOpMismatch {
                            op: op_tok.text().to_string(), // Reusing BinOpMismatch for unary
                            lhs: inner.ty(),
                            rhs: inner.ty(),
                        });
                        return HirExpr::new(HirExprKind::Error, Span::default());
                    }
                    HirExpr::new(HirExprKind::UnaryOp(op, Box::new(inner), expected_ty), Span::default())
                } else {
                    HirExpr::new(HirExprKind::Error, Span::default())
                }
            }
            ast::Expr::IfExpr(if_expr) => {
                self.lower_if_expr(&if_expr)
            }
            ast::Expr::StructExpr(struct_expr) => {
                let name = struct_expr.name().and_then(|n| n.ident()).map(|i| i.text().to_string()).unwrap_or_default();
                let mut field_exprs = Vec::new();
                for f in struct_expr.fields() {
                    let f_name = f.name().map(|n| n.text().to_string()).unwrap_or_default();
                    let f_expr = f.expr().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::new(HirExprKind::Error, Span::default()));
                    field_exprs.push((f_name, f_expr));
                }
                
                let mut mono_name = name.clone();
                let mut inferred_args = Vec::new();
                if self.generic_templates.structs.contains_key(&name) {
                    let template = self.generic_templates.structs.get(&name).unwrap().clone();
                    if let Some(params) = template.generic_params() {
                        for param in params.params() {
                            let p_name = param.as_string().unwrap_or_default();
                            let mut inferred_ty = HirType::Error;
                            
                            for f_decl in template.fields() {
                                if let (Some(f_decl_name), Some(f_decl_ty)) = (f_decl.name(), f_decl.ty()) {
                                    if f_decl_ty.as_string().unwrap_or_default() == p_name {
                                        let f_name_str = f_decl_name.text().to_string();
                                        if let Some((_, e)) = field_exprs.iter().find(|(n, _)| n == &f_name_str) {
                                            inferred_ty = e.ty();
                                            break;
                                        }
                                    }
                                }
                            }
                            inferred_args.push(inferred_ty);
                        }
                        mono_name = self.request_monomorphize_struct(&name, inferred_args.clone());
                    }
                }
                
                let mut is_unknown = false;
                let def_fields = if let Some(fields) = self.structs.get(&mono_name) {
                    fields.clone()
                } else {
                    is_unknown = true;
                    Vec::new()
                };
                
                if !is_unknown {
                    // Type check fields
                    for (f_name, f_expr) in &field_exprs {
                        if let Some((_, def_ty)) = def_fields.iter().find(|(n, _)| n == f_name) {
                            if f_expr.ty() != HirType::Error && *def_ty != f_expr.ty() {
                                self.errors.push(SemanticError::TypeMismatch {
                                    expected: def_ty.clone(),
                                    found: f_expr.ty(),
                                });
                            }
                        } else {
                            self.errors.push(SemanticError::UndefinedVariable { name: format!("field {} in struct {}", f_name, mono_name) });
                        }
                    }
                    HirExpr::new(HirExprKind::StructExpr(mono_name.clone(), field_exprs, HirType::Struct(mono_name)), Span::default())
                } else {
                    self.errors.push(SemanticError::UnknownType { name: mono_name.clone() });
                    HirExpr::new(HirExprKind::Error, Span::default())
                }
            }
            ast::Expr::FieldExpr(field_expr) => {
                let mut is_enum_variant = false;
                let mut enum_name_opt = None;
                let mut is_variant_case = false;
                let mut variant_name_opt = None;
                
                if let Some(ast::Expr::NameRef(name_ref)) = field_expr.base() {
                    let name = name_ref.ident().map(|n| n.text().to_string()).unwrap_or_default();
                    if self.enums.contains_key(&name) {
                        is_enum_variant = true;
                        enum_name_opt = Some(name);
                    } else if self.variants.contains_key(&name) {
                        is_variant_case = true;
                        variant_name_opt = Some(name);
                    }
                }
                
                let field_name = field_expr.field().and_then(|n| n.ident()).map(|i| i.text().to_string()).unwrap_or_default();
                
                if is_enum_variant {
                    let enum_name = enum_name_opt.unwrap();
                    let variants = self.enums.get(&enum_name).unwrap();
                    if let Some(idx) = variants.iter().position(|v| v == &field_name) {
                        HirExpr::new(HirExprKind::EnumVariant(enum_name.clone(), field_name, idx as u64, HirType::Enum(enum_name)), Span::default())
                    } else {
                        self.errors.push(SemanticError::UndefinedVariable { name: format!("variant {} in enum {}", field_name, enum_name) });
                        HirExpr::new(HirExprKind::Error, Span::default())
                    }
                } else if is_variant_case {
                    let variant_name = variant_name_opt.unwrap();
                    let cases = self.variants.get(&variant_name).unwrap();
                    if let Some((case_name, payload_tys)) = cases.iter().find(|(n, _)| n == &field_name) {
                        if payload_tys.is_empty() {
                            HirExpr::new(HirExprKind::VariantConstructor(variant_name.clone(), case_name.clone(), Vec::new(), HirType::Variant(variant_name)), Span::default())
                        } else {
                            self.errors.push(SemanticError::UndefinedVariable { name: format!("variant case {} in variant {} requires parameters", field_name, variant_name) });
                            HirExpr::new(HirExprKind::Error, Span::default())
                        }
                    } else {
                        self.errors.push(SemanticError::UndefinedVariable { name: format!("variant case {} in variant {}", field_name, variant_name) });
                        HirExpr::new(HirExprKind::Error, Span::default())
                    }
                } else {
                    let base = field_expr.base().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::new(HirExprKind::Error, Span::default()));
                    if let HirType::Struct(s_name) = base.ty() {
                        if let Some(def_fields) = self.structs.get(&s_name) {
                            if let Some((_, def_ty)) = def_fields.iter().find(|(n, _)| n == &field_name) {
                                HirExpr::new(HirExprKind::FieldAccess(Box::new(base), field_name, def_ty.clone()), Span::default())
                            } else {
                                self.errors.push(SemanticError::UndefinedVariable { name: format!("field {} in struct {}", field_name, s_name) });
                                HirExpr::new(HirExprKind::Error, Span::default())
                            }
                        } else {
                            HirExpr::new(HirExprKind::Error, Span::default())
                        }
                    } else if base.ty() != HirType::Error {
                        self.errors.push(SemanticError::UndefinedVariable { name: format!("field access on non-struct type {:?}", base.ty()) });
                        HirExpr::new(HirExprKind::Error, Span::default())
                    } else {
                        HirExpr::new(HirExprKind::Error, Span::default())
                    }
                }
            }
            ast::Expr::MatchExpr(match_expr) => {
                let expr = match_expr.expr().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::new(HirExprKind::Error, Span::default()));
                let mut arms = Vec::new();
                let mut ret_ty = HirType::Error;
                
                for arm in match_expr.arms() {
                    self.enter_scope();
                    let pat = if let Some(p) = arm.pattern() {
                        self.lower_pattern(&p)
                    } else {
                        HirPattern::Wildcard
                    };
                    self.declare_pattern_bindings(&pat, &expr.ty());
                    
                    let arm_expr = if let Some(e) = arm.val() {
                        self.lower_expr(&e)
                    } else {
                        HirExpr::new(HirExprKind::Error, Span::default())
                    };
                    self.exit_scope();
                    
                    if arm_expr.ty() != HirType::Error {
                        if ret_ty == HirType::Error {
                            ret_ty = arm_expr.ty();
                        } else if ret_ty != arm_expr.ty() {
                            self.errors.push(SemanticError::TypeMismatch {
                                expected: ret_ty.clone(),
                                found: arm_expr.ty(),
                            });
                        }
                    }
                    arms.push((pat, arm_expr));
                }
                HirExpr::new(HirExprKind::Match(Box::new(expr), arms, ret_ty), Span::default())
            }
            ast::Expr::ArrayExpr(arr) => {
                let elements: Vec<HirExpr> = arr.elements().map(|e| self.lower_expr(&e)).collect();
                let mut ty = HirType::Error;
                if !elements.is_empty() {
                    ty = elements[0].ty();
                    for el in &elements {
                        if el.ty() != HirType::Error && el.ty() != ty {
                            self.errors.push(SemanticError::TypeMismatch {
                                expected: ty.clone(),
                                found: el.ty(),
                            });
                        }
                    }
                }
                HirExpr::new(HirExprKind::ArrayExpr(elements.clone(), HirType::Array(Box::new(ty), elements.len() as u64)), Span::default())
            }
            ast::Expr::IndexExpr(idx) => {
                let base = idx.base().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::new(HirExprKind::Error, Span::default()));
                let index = idx.index().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::new(HirExprKind::Error, Span::default()));
                
                if index.ty() != HirType::Error && index.ty() != HirType::I32 {
                    // Assuming indices are i32 for now
                    self.errors.push(SemanticError::TypeMismatch {
                        expected: HirType::I32,
                        found: index.ty(),
                    });
                }
                
                let ret_ty = match base.ty() {
                    HirType::Array(inner, _) => *inner,
                    HirType::Slice(inner) => *inner,
                    HirType::Error => HirType::Error,
                    other => {
                        self.errors.push(SemanticError::Custom(format!("Cannot index into type {:?}", other)));
                        HirType::Error
                    }
                };
                
                HirExpr::new(HirExprKind::IndexExpr(Box::new(base), Box::new(index), ret_ty), Span::default())
            }
            ast::Expr::SliceExpr(slc) => {
                let base = slc.base().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::new(HirExprKind::Error, Span::default()));
                let start = slc.start().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::new(HirExprKind::Error, Span::default()));
                let end = slc.end().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::new(HirExprKind::Error, Span::default()));
                
                if start.ty() != HirType::Error && start.ty() != HirType::I32 {
                    self.errors.push(SemanticError::TypeMismatch { expected: HirType::I32, found: start.ty() });
                }
                if end.ty() != HirType::Error && end.ty() != HirType::I32 {
                    self.errors.push(SemanticError::TypeMismatch { expected: HirType::I32, found: end.ty() });
                }
                
                let ret_ty = match base.ty() {
                    HirType::Array(inner, _) => HirType::Slice(inner),
                    HirType::Slice(inner) => HirType::Slice(inner),
                    HirType::Error => HirType::Error,
                    other => {
                        self.errors.push(SemanticError::Custom(format!("Cannot slice type {:?}", other)));
                        HirType::Error
                    }
                };
                
                HirExpr::new(HirExprKind::SliceExpr(Box::new(base), Box::new(start), Box::new(end), ret_ty), Span::default())
            }
            ast::Expr::TryExpr(try_expr) => {
                let inner = try_expr.expr().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::new(HirExprKind::Error, Span::default()));
                let ty = inner.ty();
                let ok_ty = if let HirType::Result(ok, _) = &ty {
                    *(ok.clone())
                } else {
                    if ty != HirType::Error {
                        self.errors.push(SemanticError::Custom(format!("Cannot use ? operator on non-Result type {:?}", ty)));
                    }
                    HirType::Error
                };
                HirExpr::new(HirExprKind::Try(Box::new(inner), ok_ty), Span::default())
            }
            ast::Expr::RefExpr(ref_expr) => {
                let inner = ref_expr.expr().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::new(HirExprKind::Error, Span::default()));
                if !inner.is_lvalue() && inner.ty() != HirType::Error {
                    self.errors.push(SemanticError::Custom("Cannot take address of non-lvalue expression".to_string()));
                    return HirExpr::new(HirExprKind::Error, Span::default());
                }
                let is_mut = ref_expr.is_mut();
                let ty = HirType::Ref(Box::new(inner.ty()), is_mut);
                HirExpr::new(HirExprKind::Ref(Box::new(inner), is_mut, ty), Span::default())
            }
            ast::Expr::DerefExpr(deref_expr) => {
                let inner = deref_expr.expr().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::new(HirExprKind::Error, Span::default()));
                let ty = match inner.ty() {
                    HirType::Ref(t, _) => *t,
                    HirType::Ptr(t, _) => {
                        if !self.in_unsafe_block {
                            self.errors.push(SemanticError::Custom("Dereference of raw pointer requires unsafe block".to_string()));
                        }
                        *t
                    },
                    HirType::Error => HirType::Error,
                    other => {
                        self.errors.push(SemanticError::Custom(format!("Cannot dereference non-pointer type {:?}", other)));
                        HirType::Error
                    }
                };
                HirExpr::new(HirExprKind::Deref(Box::new(inner), ty), Span::default())
            }
            ast::Expr::UnsafeBlock(unsafe_block) => {
                let prev = self.in_unsafe_block;
                self.in_unsafe_block = true;
                let block = unsafe_block.block().map(|b| self.lower_block(&b)).unwrap_or_else(|| HirBlock { statements: vec![] });
                let ty = if let Some(HirStmtKind::Expr(e)) = block.statements.last().map(|s| &s.kind) {
                    e.ty()
                } else {
                    HirType::Void
                };
                self.in_unsafe_block = prev;
                HirExpr::new(HirExprKind::Block(block, ty), Span::default())
            }
            ast::Expr::ClosureExpr(closure) => {
                let mut params = Vec::new();
                let mut param_tys = Vec::new();
                for p in closure.params() {
                    let name = p.name().unwrap_or_default();
                    let ty = p.ty().map(|t| self.lower_type(&t)).unwrap_or(HirType::Error); // Closures require type annotations for now
                    params.push(name);
                    param_tys.push(ty);
                }
                
                self.enter_scope();
                for (name, ty) in params.iter().zip(param_tys.iter()) {
                    self.declare_var(name.clone(), ty.clone(), false);
                }
                
                let body = closure.expr().map(|e| self.lower_expr(&e)).unwrap_or(HirExpr::new(HirExprKind::Error, Span::default()));
                let ret_ty = body.ty();
                
                self.exit_scope();
                
                let mut captures = std::collections::HashSet::new();
                let mut bound = std::collections::HashSet::new();
                for p in &params {
                    bound.insert(p.clone());
                }
                get_captures(&body, &mut bound, &mut captures);
                let captured_vars: Vec<String> = captures.into_iter().collect();
                
                let closure_ty = HirType::Func(param_tys, Box::new(ret_ty));
                HirExpr::new(HirExprKind::Closure(params, Box::new(body), captured_vars, closure_ty), Span::default())
            }
            ast::Expr::GenericInstExpr(_expr) => {
                // TODO: Implement generic instantiation monomorphization
                HirExpr::new(HirExprKind::Error, Span::default())
            }
            ast::Expr::QuantifierExpr(quant) => {
                let kind_token = quant.quantifier_token().map(|t| t.kind()).unwrap_or(crate::parser::syntax::SyntaxKind::ERROR_NODE);
                let kind = match kind_token {
                    crate::parser::syntax::SyntaxKind::KwForall => crate::hir::QuantifierKind::Forall,
                    crate::parser::syntax::SyntaxKind::KwExists => crate::hir::QuantifierKind::Exists,
                    crate::parser::syntax::SyntaxKind::KwChoose => crate::hir::QuantifierKind::Choose,
                    _ => crate::hir::QuantifierKind::Forall,
                };
                
                self.enter_scope();
                let mut params = Vec::new();
                for param in quant.params() {
                    let name = param.name().unwrap_or_default();
                    let ty = param.ty().map(|t| self.lower_type(&t)).unwrap_or(HirType::Error);
                    self.declare_var(name.clone(), ty.clone(), true);
                    params.push((name, ty));
                }
                
                let body = if let Some(b) = quant.body() {
                    let lowered_block = self.lower_block(&b);
                    let ret_ty = match lowered_block.statements.last() {
                        Some(stmt) => match &stmt.kind { crate::hir::HirStmtKind::Expr(e) => e.ty(), crate::hir::HirStmtKind::Return(Some(e)) => e.ty(), _ => HirType::Void },
                        
                        _ => HirType::Void,
                    };
                    HirExpr::new(HirExprKind::Block(lowered_block, ret_ty), Span::default())
                } else if let Some(e) = quant.expr() {
                    self.lower_expr(&e)
                } else {
                    HirExpr::new(HirExprKind::Error, Span::default())
                };
                self.exit_scope();
                
                let ret_ty = match kind {
                    crate::hir::QuantifierKind::Forall | crate::hir::QuantifierKind::Exists => HirType::Bool,
                    crate::hir::QuantifierKind::Choose => {
                        if params.len() == 1 {
                            params[0].1.clone()
                        } else {
                            self.errors.push(SemanticError::Custom("choose quantifier must have exactly one parameter".to_string()));
                            HirType::Error
                        }
                    }
                };
                
                HirExpr::new(HirExprKind::Quantifier(kind, params, Box::new(body), ret_ty), Span::default())
            }
        }
    }

    fn lower_if_expr(&mut self, if_expr: &ast::IfExpr) -> HirExpr {
        let cond = if let Some(c) = if_expr.condition() {
            let c_expr = self.lower_expr(&c);
            if c_expr.ty() != HirType::Error && c_expr.ty() != HirType::Bool {
                self.errors.push(SemanticError::TypeMismatch {
                    expected: HirType::Bool,
                    found: c_expr.ty(),
                });
            }
            c_expr
        } else {
            HirExpr::new(HirExprKind::Error, Span::default())
        };

        // For type checking, if-else should return the same type.
        // If it's a statement, we can assume Void. We will simplify for now and return Void.
        let then_block = if let Some(b) = if_expr.then_block() {
            self.lower_block(&b)
        } else {
            HirBlock { statements: Vec::new() }
        };

        let else_block = if let Some(b) = if_expr.else_branch() {
            if b.kind() == crate::parser::syntax::SyntaxKind::BLOCK_EXPR {
                ast::BlockExpr::cast(b).map(|block| self.lower_block(&block))
            } else if b.kind() == crate::parser::syntax::SyntaxKind::IF_EXPR {
                if let Some(elif) = ast::IfExpr::cast(b) {
                    let elif_expr = self.lower_if_expr(&elif);
                    Some(HirBlock { statements: vec![HirStmt::new(HirStmtKind::Expr(elif_expr), Span::default())] })
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        HirExpr::new(HirExprKind::If(Box::new(cond), then_block, else_block, HirType::Void), Span::default())
    }
}

fn get_captures(expr: &HirExpr, bound: &mut std::collections::HashSet<String>, captures: &mut std::collections::HashSet<String>) {
    match &expr.kind {
        HirExprKind::VarRef(name, _) => {
            if !bound.contains(name) {
                captures.insert(name.clone());
            }
        }
        HirExprKind::BinaryOp(_, lhs, rhs, _) => {
            get_captures(lhs, bound, captures);
            get_captures(rhs, bound, captures);
        }
        HirExprKind::UnaryOp(_, inner, _) | HirExprKind::Ref(inner, _, _) | HirExprKind::Deref(inner, _) | HirExprKind::Try(inner, _)
        | HirExprKind::ResultOk(inner, _) | HirExprKind::ResultErr(inner, _) | HirExprKind::FieldAccess(inner, _, _) => {
            get_captures(inner, bound, captures);
        }
        HirExprKind::Call(_, args, _) | HirExprKind::VariantConstructor(_, _, args, _) | HirExprKind::ArrayExpr(args, _) => {
            for arg in args {
                get_captures(arg, bound, captures);
            }
        }
        HirExprKind::CallIndirect(callee, args, _) => {
            get_captures(callee, bound, captures);
            for arg in args {
                get_captures(arg, bound, captures);
            }
        }
        HirExprKind::If(cond, then_b, else_b, _) => {
            get_captures(cond, bound, captures);
            get_captures_block(then_b, bound, captures);
            if let Some(b) = else_b {
                get_captures_block(b, bound, captures);
            }
        }
        HirExprKind::StructExpr(_, fields, _) => {
            for (_, e) in fields {
                get_captures(e, bound, captures);
            }
        }
        HirExprKind::IndexExpr(base, idx, _) => {
            get_captures(base, bound, captures);
            get_captures(idx, bound, captures);
        }
        HirExprKind::SliceExpr(base, start, end, _) => {
            get_captures(base, bound, captures);
            get_captures(start, bound, captures);
            get_captures(end, bound, captures);
        }
        HirExprKind::Match(cond, arms, _) => {
            get_captures(cond, bound, captures);
            for (pat, e) in arms {
                let mut new_bound = bound.clone();
                // add pattern bindings
                match pat {
                    HirPattern::VariantCase(_, bindings) => {
                        for b in bindings {
                            new_bound.insert(b.clone());
                        }
                    }
                    HirPattern::Binding(b) => {
                        new_bound.insert(b.clone());
                    }
                    HirPattern::Literal(_) | HirPattern::Wildcard => {}
                }
                get_captures(e, &mut new_bound, captures);
            }
        }
        HirExprKind::Block(block, _) => {
            get_captures_block(block, bound, captures);
        }
        HirExprKind::Closure(params, body, _, _) => {
            let mut new_bound = bound.clone();
            for p in params {
                new_bound.insert(p.clone());
            }
            get_captures(body, &mut new_bound, captures);
        }
        HirExprKind::Quantifier(_, params, body, _) => {
            let mut new_bound = bound.clone();
            for (p, _) in params {
                new_bound.insert(p.clone());
            }
            get_captures(body, &mut new_bound, captures);
        }
        HirExprKind::IntLiteral(_, _) | HirExprKind::BoolLiteral(_, _) | HirExprKind::EnumVariant(_, _, _, _) | HirExprKind::Error => {}
    }
}

fn get_captures_block(block: &HirBlock, bound: &mut std::collections::HashSet<String>, captures: &mut std::collections::HashSet<String>) {
    let mut new_bound = bound.clone();
    for stmt in &block.statements {
        match &stmt.kind {
            HirStmtKind::Let(name, _, _, init) => {
                get_captures(init, &mut new_bound, captures);
                new_bound.insert(name.clone());
            }
            HirStmtKind::Expr(e) | HirStmtKind::Assert(e) | HirStmtKind::Assume(e) => get_captures(e, &mut new_bound, captures),
            HirStmtKind::Return(Some(e)) => get_captures(e, &mut new_bound, captures),
            HirStmtKind::Return(None) | HirStmtKind::Break | HirStmtKind::Continue | HirStmtKind::Error => {}
            HirStmtKind::GhostBlock(ghost_body) => get_captures_block(ghost_body, &mut new_bound, captures),
            HirStmtKind::While(cond, body, invs, decreases, _) => {
                get_captures(cond, &mut new_bound, captures);
                for inv in invs {
                    get_captures(inv, &mut new_bound, captures);
                }
                if let Some(dec) = decreases {
                    get_captures(dec, &mut new_bound, captures);
                }
                get_captures_block(body, &mut new_bound, captures);
            }
            HirStmtKind::For(name, iter, body, _) => {
                get_captures(iter, &mut new_bound, captures);
                let mut b2 = new_bound.clone();
                b2.insert(name.clone());
                get_captures_block(body, &mut b2, captures);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{Parser, ast::{AstNode, SourceFile}};
    use crate::hir::{Span, HirExprKind, HirStmtKind, HirType, HirStmt, HirExpr, BinaryOp};

    /// Parses `src` and runs the HIR lowering pass.
    /// Returns `(HirProgram, Vec<SemanticError>)`.
    fn parse_and_lower(src: &str) -> (HirProgram, Vec<SemanticError>) {
        let (cst, _parse_errors) = Parser::new(src).parse();
        let source_file = SourceFile::cast(cst).expect("Root is not a SourceFile");
        let mut ctx = LoweringContext::new();
        let program = ctx.lower_program(&source_file);
        (program, ctx.errors)
    }

    /// A minimal `func main(): i32 { return 0; }` lowers to one function
    /// with name "main", return type I32, and no parameters.
    #[test]
    fn test_lower_basic_function() {
        let (prog, errors) = parse_and_lower("func main(): i32 { return 0; }");
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert_eq!(prog.functions.len(), 1);
        let f = &prog.functions[0];
        assert_eq!(f.name, "main");
        assert_eq!(f.ret_type, HirType::I32);
        assert!(f.params.is_empty());
    }

    /// A function with two i32 parameters lowers them in the correct order.
    #[test]
    fn test_lower_function_params() {
        let (prog, errors) = parse_and_lower("func add(a: i32, b: i32): i32 { return a; }");
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        let f = &prog.functions[0];
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.params[0], ("a".to_string(), HirType::I32));
        assert_eq!(f.params[1], ("b".to_string(), HirType::I32));
    }

    /// `const x: i32 = 1;` lowers to `HirStmt::new(HirStmtKind::Let("x", is_const=true, I32, IntLiteral(1)), Span::default())`.
    #[test]
    fn test_lower_const_let() {
        let (prog, errors) = parse_and_lower("func f(): i32 { const x: i32 = 1; return x; }");
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        let stmts = &prog.functions[0].body.statements;
        if let HirStmtKind::Let(name, is_const, ty, init) = &stmts[0].kind {
            assert_eq!(name, "x");
            assert!(*is_const, "expected const binding");
            assert_eq!(*ty, HirType::I32);
            assert!(matches!(&init.kind, HirExprKind::IntLiteral(1, _)));
        } else {
            panic!("Expected HirStmtKind::Let, got {:?}", stmts[0]);
        }
    }

    /// `var y: i32 = 2;` lowers with `is_const = false`.
    #[test]
    fn test_lower_var_let() {
        let (prog, errors) = parse_and_lower("func f(): i32 { var y: i32 = 2; return y; }");
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        if let HirStmtKind::Let(name, is_const, _, _) = &prog.functions[0].body.statements[0].kind {
            assert_eq!(name, "y");
            assert!(!is_const, "expected mutable binding");
        } else {
            panic!("Expected HirStmtKind::Let");
        }
    }

    /// Without an explicit type annotation the type is inferred from the initializer.
    #[test]
    fn test_lower_type_inferred_from_initializer() {
        let (prog, errors) = parse_and_lower("func f(): i32 { const x = 42; return x; }");
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        if let HirStmtKind::Let(_, _, ty, _) = &prog.functions[0].body.statements[0].kind {
            assert_eq!(*ty, HirType::I32, "type should be inferred as I32");
        } else {
            panic!("Expected HirStmtKind::Let");
        }
    }

    /// Returning a bool from an i32 function produces a TypeMismatch error.
    #[test]
    fn test_lower_return_type_mismatch() {
        let (_, errors) = parse_and_lower("func f(): i32 { return true; }");
        assert!(
            errors.iter().any(|e| matches!(e, SemanticError::TypeMismatch { .. })),
            "expected TypeMismatch error, got {:?}", errors
        );
    }

    /// Using an undeclared variable emits an UndefinedVariable error.
    #[test]
    fn test_lower_undefined_variable() {
        let (_, errors) = parse_and_lower("func f(): i32 { return z; }");
        assert!(
            errors.iter().any(|e| matches!(e, SemanticError::UndefinedVariable { .. })),
            "expected UndefinedVariable error, got {:?}", errors
        );
    }

    /// Assigning to a `const` variable emits an ImmutableAssignment error.
    #[test]
    fn test_lower_immutable_assignment() {
        let src = "func f(): i32 { const x: i32 = 1; x = 2; return x; }";
        let (_, errors) = parse_and_lower(src);
        assert!(
            errors.iter().any(|e| matches!(e, SemanticError::ImmutableAssignment { .. })),
            "expected ImmutableAssignment error, got {:?}", errors
        );
    }

    /// Adding an i32 and a bool emits a BinOpMismatch error.
    #[test]
    fn test_lower_binop_type_mismatch() {
        let src = "func f(): i32 { const x: i32 = 1 + true; return x; }";
        let (_, errors) = parse_and_lower(src);
        assert!(
            errors.iter().any(|e| matches!(e, SemanticError::BinOpMismatch { .. })),
            "expected BinOpMismatch error, got {:?}", errors
        );
    }

    /// An if-condition that is not bool emits a TypeMismatch error.
    #[test]
    fn test_lower_if_condition_must_be_bool() {
        let src = "func f(): i32 { if 1 { return 0; } return 1; }";
        let (_, errors) = parse_and_lower(src);
        assert!(
            errors.iter().any(|e| matches!(e, SemanticError::TypeMismatch { .. })),
            "expected TypeMismatch for non-bool if-condition, got {:?}", errors
        );
    }

    /// Multiple functions in one source file are all lowered.
    #[test]
    fn test_lower_multiple_functions() {
        let src = "func a(): i32 { return 1; } func b(): i32 { return 2; }";
        let (prog, errors) = parse_and_lower(src);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert_eq!(prog.functions.len(), 2);
    }

    /// `spec { requires x > 0; ensures true; }` clauses are extracted into
    /// the `requires` and `ensures` fields of `HirFunc`.
    #[test]
    fn test_lower_spec_clauses() {
        let src = r#"
            func f(x: i32): i32
            spec {
                requires x > 0;
                ensures true;
            }
            { return x; }
        "#;
        let (prog, errors) = parse_and_lower(src);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        let f = &prog.functions[0];
        assert_eq!(f.requires.len(), 1, "expected one requires clause");
        assert_eq!(f.ensures.len(), 1, "expected one ensures clause");
    }

    /// Struct declarations are lowered into `prog.structs` with correct field types.
    #[test]
    fn test_lower_struct_decl() {
        let src = "struct Point { x: i32, y: i32 } func f(): i32 { return 0; }";
        let (prog, errors) = parse_and_lower(src);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        let fields = prog.structs.get("Point").expect("Point struct not found");
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0], ("x".to_string(), HirType::I32));
        assert_eq!(fields[1], ("y".to_string(), HirType::I32));
    }

    /// Field access on a struct resolves to the correct field type (I32 for `p.x`).
    #[test]
    fn test_lower_field_access_type() {
        let src = r#"
            struct Point { x: i32, y: i32 }
            func f(): i32 {
                const p: Point = Point { x: 1, y: 2 };
                return p.x;
            }
        "#;
        let (prog, errors) = parse_and_lower(src);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        let stmts = &prog.functions[0].body.statements;
        if let HirStmtKind::Return(Some(expr)) = &stmts.last().unwrap().kind {
            assert_eq!(expr.ty(), HirType::I32, "field access should have type I32");
        } else {
            panic!("Expected Return statement");
        }
    }

    /// Binary arithmetic lowers to `HirExprKind::BinaryOp` with the correct operator and result type.
    #[test]
    fn test_lower_binary_add() {
        let src = "func f(): i32 { return 1 + 2; }";
        let (prog, errors) = parse_and_lower(src);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        if let HirStmtKind::Return(Some(expr)) = &prog.functions[0].body.statements[0].kind {
            if let HirExprKind::BinaryOp(op, _, _, ty) = &expr.kind {
                assert_eq!(*op, BinaryOp::Add);
                assert_eq!(*ty, HirType::I32);
            } else {
                panic!("Expected BinaryOp, got {:?}", expr);
            }
        } else {
            panic!("Expected Return");
        }
    }
}
