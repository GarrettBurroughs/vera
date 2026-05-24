use inkwell::context::Context;
use inkwell::targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine};
use inkwell::OptimizationLevel;
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::values::{PointerValue, BasicValueEnum, IntValue, BasicMetadataValueEnum};
use inkwell::types::BasicTypeEnum;
use std::collections::BTreeMap;
use crate::hir::{HirProgram, HirFunc, HirType, HirBlock, HirStmt, HirExpr, BinaryOp, UnaryOp};

struct CodeGen<'ctx> {
    context: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    scopes: Vec<BTreeMap<String, (PointerValue<'ctx>, HirType)>>,
    struct_layouts: BTreeMap<String, Vec<(String, HirType)>>,
    structs: BTreeMap<String, inkwell::types::StructType<'ctx>>,
    loop_stack: Vec<(inkwell::basic_block::BasicBlock<'ctx>, inkwell::basic_block::BasicBlock<'ctx>)>,
}

impl<'ctx> CodeGen<'ctx> {
    fn lower_type(&self, ty: &HirType) -> Result<BasicTypeEnum<'ctx>, String> {
        match ty {
            HirType::I32 => Ok(self.context.i32_type().into()),
            HirType::Bool => Ok(self.context.bool_type().into()),
            HirType::Void => Ok(self.context.i32_type().into()), // LLVM needs a type, so i32 is used for void returns
            HirType::Ptr(inner) => {
                let inner_ty = self.lower_type(inner)?;
                // Wait, LLVM 17 uses opaque pointers! So context.ptr_type() is just ptr.
                Ok(self.context.ptr_type(inkwell::AddressSpace::default()).into())
            }
            HirType::Ref(_) => {
                Ok(self.context.ptr_type(inkwell::AddressSpace::default()).into())
            }
            HirType::Struct(name) => {
                if let Some(struct_ty) = self.structs.get(name) {
                    Ok((*struct_ty).into())
                } else {
                    Err(format!("Struct type not found: {}", name))
                }
            }
            _ => Err(format!("Unsupported LLVM type translation for {:?}", ty)),
        }
    }

    fn enter_scope(&mut self) {
        self.scopes.push(BTreeMap::new());
    }

    fn exit_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare_var(&mut self, name: String, ptr: PointerValue<'ctx>, ty: HirType) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, (ptr, ty));
        }
    }

    fn lookup_var(&self, name: &str) -> Option<(PointerValue<'ctx>, HirType)> {
        for scope in self.scopes.iter().rev() {
            if let Some(var) = scope.get(name) {
                return Some(var.clone());
            }
        }
        None
    }

    fn declare_func(&mut self, func: &HirFunc) -> Result<(), String> {
        let ret_type = self.lower_type(&func.ret_type)?;

        let mut param_types: Vec<inkwell::types::BasicMetadataTypeEnum<'ctx>> = Vec::new();
        for (_, ty) in &func.params {
            let llvm_ty = self.lower_type(ty)?;
            param_types.push(llvm_ty.into());
        }

        let fn_type = match ret_type {
            BasicTypeEnum::IntType(i) => i.fn_type(&param_types, false),
            BasicTypeEnum::PointerType(p) => p.fn_type(&param_types, false),
            _ => return Err("Unsupported return type".into()),
        };

        self.module.add_function(&func.name, fn_type, None);
        Ok(())
    }

    fn compile_func(&mut self, func: &HirFunc) -> Result<(), String> {
        let llvm_func = self.module.get_function(&func.name).unwrap();
        let basic_block = self.context.append_basic_block(llvm_func, "entry");
        self.builder.position_at_end(basic_block);

        self.scopes.clear();
        self.enter_scope(); // Function scope

        for (i, (name, ty)) in func.params.iter().enumerate() {
            let param_val = llvm_func.get_nth_param(i as u32).unwrap();
            let alloca = self.builder.build_alloca(param_val.get_type(), name).unwrap();
            self.builder.build_store(alloca, param_val).unwrap();
            self.declare_var(name.clone(), alloca, ty.clone());
        }

        self.compile_block(&func.body)?;
        
        self.exit_scope();

        // If block doesn't end with return, inject a default return to satisfy LLVM.
        // We really should check if the last instruction was a terminator.
        let block = self.builder.get_insert_block().unwrap();
        if block.get_terminator().is_none() {
            let default_val = match func.ret_type {
                HirType::I32 => self.context.i32_type().const_zero(),
                HirType::Bool => self.context.i32_type().const_zero(), // IntType inside
                _ => self.context.i32_type().const_zero(),
            };
            self.builder.build_return(Some(&default_val)).unwrap();
        }

        if !llvm_func.verify(true) {
            return Err("Function verification failed".to_string());
        }
        
        Ok(())
    }

    fn compile_block(&mut self, block: &HirBlock) -> Result<(), String> {
        self.enter_scope();
        for stmt in &block.statements {
            self.compile_stmt(stmt)?;
        }
        self.exit_scope();
        Ok(())
    }

    fn compile_stmt(&mut self, stmt: &HirStmt) -> Result<(), String> {
        if self.builder.get_insert_block().unwrap().get_terminator().is_some() {
            return Ok(()); // Block already terminated, skip dead code
        }

        match stmt {
            HirStmt::Return(expr_opt) => {
                if let Some(expr) = expr_opt {
                    let val = self.compile_expr(expr)?.into_int_value();
                    self.builder.build_return(Some(&val)).unwrap();
                } else {
                    // return default i32 0 for void functions in LLVM for now
                    let default_val = self.context.i32_type().const_zero();
                    self.builder.build_return(Some(&default_val)).unwrap();
                }
            }
            HirStmt::Let(name, _is_const, ty, initializer) => {
                let llvm_ty = self.lower_type(ty)?;
                let alloca = self.builder.build_alloca(llvm_ty, name).unwrap();
                self.declare_var(name.clone(), alloca, ty.clone());
                
                let init_val = self.compile_expr(initializer)?;
                self.builder.build_store(alloca, init_val).unwrap();
            }
            HirStmt::Expr(expr) => {
                self.compile_expr(expr)?;
            }
            HirStmt::While(cond, body, _invariants) => {
                let parent_func = self.builder.get_insert_block().unwrap().get_parent().unwrap();
                
                let header_block = self.context.append_basic_block(parent_func, "while.cond");
                let body_block = self.context.append_basic_block(parent_func, "while.body");
                let merge_block = self.context.append_basic_block(parent_func, "while.end");
                
                self.builder.build_unconditional_branch(header_block).unwrap();
                
                // 1. Compile condition in header block
                self.builder.position_at_end(header_block);
                let cond_val = self.compile_expr(cond)?.into_int_value();
                self.builder.build_conditional_branch(cond_val, body_block, merge_block).unwrap();
                
                // 2. Compile body block
                self.loop_stack.push((header_block, merge_block));
                self.builder.position_at_end(body_block);
                self.compile_block(body)?;
                
                // If the body doesn't end with a terminator, branch back to loop condition header
                if self.builder.get_insert_block().unwrap().get_terminator().is_none() {
                    self.builder.build_unconditional_branch(header_block).unwrap();
                }
                self.loop_stack.pop();
                
                // 3. Continue compiling after loop in merge block
                self.builder.position_at_end(merge_block);
            }
            HirStmt::Break => {
                if let Some((_, merge_block)) = self.loop_stack.last() {
                    self.builder.build_unconditional_branch(*merge_block).unwrap();
                } else {
                    return Err("break statement outside of loop".into());
                }
            }
            HirStmt::Continue => {
                if let Some((header_block, _)) = self.loop_stack.last() {
                    self.builder.build_unconditional_branch(*header_block).unwrap();
                } else {
                    return Err("continue statement outside of loop".into());
                }
            }
            HirStmt::Assert(_) | HirStmt::Assume(_) => {
                // Verification statements are erased during code generation.
            }
            HirStmt::Error => {}
        }
        Ok(())
    }

    fn compile_expr(&mut self, expr: &HirExpr) -> Result<BasicValueEnum<'ctx>, String> {
        match expr {
            HirExpr::IntLiteral(val, _) => {
                Ok(self.context.i32_type().const_int(*val as u64, false).into())
            }
            HirExpr::BoolLiteral(val, _) => {
                Ok(self.context.bool_type().const_int(if *val { 1 } else { 0 }, false).into())
            }
            HirExpr::VarRef(name, _) => {
                if let Some((alloca, ty)) = self.lookup_var(name) {
                    let llvm_ty = self.lower_type(&ty)?;
                    let val = self.builder.build_load(llvm_ty, alloca, name).unwrap();
                    Ok(val)
                } else {
                    Err(format!("Variable {} not found in LLVM codegen", name))
                }
            }
            HirExpr::BinaryOp(op, lhs, rhs, ty) => {
                if *op == BinaryOp::Assign {
                    if let HirExpr::VarRef(name, _) = &**lhs {
                        let rhs_val = self.compile_expr(rhs)?;
                        if let Some((alloca, _)) = self.lookup_var(name) {
                            self.builder.build_store(alloca, rhs_val).unwrap();
                            return Ok(rhs_val);
                        } else {
                            return Err(format!("Variable {} not found", name));
                        }
                    } else {
                        return Err("Invalid assignment target".into());
                    }
                }

                let lhs_ty = lhs.ty();
                let lhs_val = self.compile_expr(lhs)?;
                let rhs_val = self.compile_expr(rhs)?;

                let res = match lhs_ty {
                    HirType::I32 | HirType::Bool => {
                        let lhs_int = lhs_val.into_int_value();
                        let rhs_int = rhs_val.into_int_value();
                        match op {
                            BinaryOp::Add => self.builder.build_int_add(lhs_int, rhs_int, "tmpadd").unwrap().into(),
                            BinaryOp::Sub => self.builder.build_int_sub(lhs_int, rhs_int, "tmpsub").unwrap().into(),
                            BinaryOp::Mul => self.builder.build_int_mul(lhs_int, rhs_int, "tmpmul").unwrap().into(),
                            BinaryOp::Div => self.builder.build_int_signed_div(lhs_int, rhs_int, "tmpdiv").unwrap().into(),
                            BinaryOp::Rem => self.builder.build_int_signed_rem(lhs_int, rhs_int, "tmprem").unwrap().into(),
                            BinaryOp::Eq => self.builder.build_int_compare(inkwell::IntPredicate::EQ, lhs_int, rhs_int, "tmpeq").unwrap().into(),
                            BinaryOp::Neq => self.builder.build_int_compare(inkwell::IntPredicate::NE, lhs_int, rhs_int, "tmpneq").unwrap().into(),
                            BinaryOp::Lt => self.builder.build_int_compare(inkwell::IntPredicate::SLT, lhs_int, rhs_int, "tmplt").unwrap().into(),
                            BinaryOp::Gt => self.builder.build_int_compare(inkwell::IntPredicate::SGT, lhs_int, rhs_int, "tmpgt").unwrap().into(),
                            BinaryOp::Le => self.builder.build_int_compare(inkwell::IntPredicate::SLE, lhs_int, rhs_int, "tmple").unwrap().into(),
                            BinaryOp::Ge => self.builder.build_int_compare(inkwell::IntPredicate::SGE, lhs_int, rhs_int, "tmpge").unwrap().into(),
                            BinaryOp::And => self.builder.build_and(lhs_int, rhs_int, "tmpand").unwrap().into(),
                            BinaryOp::Or => self.builder.build_or(lhs_int, rhs_int, "tmpor").unwrap().into(),
                            _ => return Err("Unsupported binary op on int/bool".into()),
                        }
                    }
                    _ => return Err(format!("Unsupported type for binary op: {:?}", lhs_ty)),
                };
                Ok(res)
            }
            HirExpr::UnaryOp(op, inner, ty) => {
                let inner_ty = inner.ty();
                let inner_val = self.compile_expr(inner)?;
                
                let res = match inner_ty {
                    HirType::I32 | HirType::Bool => {
                        let inner_int = inner_val.into_int_value();
                        match op {
                            UnaryOp::Neg => self.builder.build_int_neg(inner_int, "tmpneg").unwrap().into(),
                            UnaryOp::Not => self.builder.build_not(inner_int, "tmpnot").unwrap().into(),
                        }
                    }
                    _ => return Err(format!("Unsupported type for unary op: {:?}", inner_ty)),
                };
                Ok(res)
            }
            HirExpr::If(cond, then_block, else_block_opt, _) => {
                let cond_val = self.compile_expr(cond)?.into_int_value();
                
                let function = self.builder.get_insert_block().unwrap().get_parent().unwrap();
                let then_bb = self.context.append_basic_block(function, "then");
                let else_bb = self.context.append_basic_block(function, "else");
                let merge_bb = self.context.append_basic_block(function, "ifcont");

                self.builder.build_conditional_branch(cond_val, then_bb, else_bb).unwrap();

                // Then block
                self.builder.position_at_end(then_bb);
                self.compile_block(then_block)?;
                if self.builder.get_insert_block().unwrap().get_terminator().is_none() {
                    self.builder.build_unconditional_branch(merge_bb).unwrap();
                }

                // Else block
                self.builder.position_at_end(else_bb);
                if let Some(else_block) = else_block_opt {
                    self.compile_block(else_block)?;
                }
                if self.builder.get_insert_block().unwrap().get_terminator().is_none() {
                    self.builder.build_unconditional_branch(merge_bb).unwrap();
                }

                self.builder.position_at_end(merge_bb);
                
                // Return dummy value since we're using Void return for `if` statements right now.
                Ok(self.context.i32_type().const_zero().into())
            }
            HirExpr::Call(name, args, _) => {
                let llvm_func = self.module.get_function(name).ok_or(format!("Function {} not found", name))?;
                let mut llvm_args: Vec<BasicMetadataValueEnum<'ctx>> = Vec::new();
                for arg in args {
                    llvm_args.push(self.compile_expr(arg)?.into());
                }
                let call_site = self.builder.build_call(llvm_func, &llvm_args, "tmpcall").unwrap();
                match call_site.try_as_basic_value() {
                    inkwell::values::ValueKind::Basic(val) => Ok(val),
                    inkwell::values::ValueKind::Instruction(_) => Err("Call returned void".into()),
                }
            }
            HirExpr::StructExpr(name, fields, _) => {
                let struct_type = *self.structs.get(name).unwrap();
                let alloca = self.builder.build_alloca(struct_type, "struct_init").unwrap();
                
                for (f_name, f_expr) in fields {
                    let layout = self.struct_layouts.get(name).unwrap();
                    let field_idx = layout.iter().position(|(n, _)| n == f_name).unwrap();
                    
                    let field_val = self.compile_expr(f_expr)?;
                    let field_ptr = self.builder.build_struct_gep(struct_type, alloca, field_idx as u32, "field_ptr").unwrap();
                    self.builder.build_store(field_ptr, field_val).unwrap();
                }
                
                let val = self.builder.build_load(struct_type, alloca, "struct_val").unwrap();
                Ok(val)
            }
            HirExpr::FieldAccess(base, field_name, _) => {
                let base_ty = base.ty();
                let struct_name = if let HirType::Struct(s) = base_ty { s } else { unreachable!() };
                
                let base_val = self.compile_expr(base)?;
                let layout = self.struct_layouts.get(&struct_name).unwrap();
                let field_idx = layout.iter().position(|(n, _)| n == field_name).unwrap();
                
                let res = self.builder.build_extract_value(base_val.into_struct_value(), field_idx as u32, "extract").unwrap();
                Ok(res)
            }
            HirExpr::Error => Err("Cannot compile HirExpr::Error".into()),
        }
    }
}

pub fn compile_to_binary(hir: &HirProgram, output_path: &str) -> Result<(), String> {
    let context = Context::create();
    let module = context.create_module("main_module");
    let builder = context.create_builder();
    
    let mut codegen = CodeGen {
        context: &context,
        module,
        builder,
        scopes: Vec::new(),
        struct_layouts: hir.structs.clone(),
        structs: BTreeMap::new(),
        loop_stack: Vec::new(),
    };
    
    // Predeclare structs
    for (name, _) in &hir.structs {
        let struct_type = codegen.context.opaque_struct_type(name);
        codegen.structs.insert(name.clone(), struct_type);
    }

    // Define structs
    for (name, fields) in &hir.structs {
        let mut field_types = Vec::new();
        for (_, ty) in fields {
            field_types.push(codegen.lower_type(ty).unwrap());
        }
        let struct_type = codegen.structs.get(name).unwrap();
        struct_type.set_body(&field_types, false);
    }
    
    for func in &hir.functions {
        codegen.declare_func(func)?;
    }
    
    for func in &hir.functions {
        codegen.compile_func(func)?;
    }

    if std::env::var("PRINT_IR").is_ok() {
        println!("{}", codegen.module.print_to_string().to_string());
    }
    
    Target::initialize_all(&InitializationConfig::default());
    let target_triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&target_triple).map_err(|e| e.to_string())?;
    
    let target_machine = target.create_target_machine(
        &target_triple,
        "generic",
        "",
        OptimizationLevel::None,
        RelocMode::Default,
        CodeModel::Default,
    ).ok_or("Failed to create target machine")?;
    
    let obj_path = std::path::Path::new(output_path).with_extension("o");
    target_machine.write_to_file(&codegen.module, FileType::Object, &obj_path).map_err(|e| e.to_string())?;
    
    let status = std::process::Command::new("cc")
        .arg(&obj_path)
        .arg("-o")
        .arg(output_path)
        .status()
        .map_err(|e| format!("Failed to run cc: {}", e))?;
        
    if !status.success() {
        return Err("Linking failed".to_string());
    }
    
    Ok(())
}
