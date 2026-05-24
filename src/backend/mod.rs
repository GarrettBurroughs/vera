use inkwell::context::Context;
use inkwell::targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine};
use inkwell::OptimizationLevel;
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::values::{PointerValue, BasicValueEnum, IntValue, BasicMetadataValueEnum};
use inkwell::types::BasicTypeEnum;
use std::collections::BTreeMap;
use crate::hir::{HirProgram, HirFunc, HirType, HirBlock, HirStmt, HirExpr, BinaryOp, UnaryOp, HirPattern};

struct CodeGen<'ctx> {
    context: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    scopes: Vec<BTreeMap<String, (PointerValue<'ctx>, HirType)>>,
    struct_layouts: BTreeMap<String, Vec<(String, HirType)>>,
    structs: BTreeMap<String, inkwell::types::StructType<'ctx>>,
    variants: BTreeMap<String, Vec<(String, Vec<HirType>)>>,
    loop_stack: Vec<(inkwell::basic_block::BasicBlock<'ctx>, inkwell::basic_block::BasicBlock<'ctx>)>,
}

impl<'ctx> CodeGen<'ctx> {
    fn lower_type(&self, ty: &HirType) -> Result<BasicTypeEnum<'ctx>, String> {
        match ty {
            HirType::I32 => Ok(self.context.i32_type().into()),
            HirType::Bool => Ok(self.context.bool_type().into()),
            HirType::Void => Ok(self.context.i32_type().into()), // LLVM needs a type, so i32 is used for void returns
            HirType::Ptr(_, _) => {
                // Wait, LLVM 17 uses opaque pointers! So context.ptr_type() is just ptr.
                Ok(self.context.ptr_type(inkwell::AddressSpace::default()).into())
            }
            HirType::Ref(_, _) => {
                Ok(self.context.ptr_type(inkwell::AddressSpace::default()).into())
            }
            HirType::Struct(name) => {
                if let Some(struct_ty) = self.structs.get(name) {
                    Ok((*struct_ty).into())
                } else {
                    Err(format!("Struct type not found: {}", name))
                }
            }
            HirType::Enum(_) => Ok(self.context.i32_type().into()),
            HirType::Variant(_) => {
                let i32_ty = self.context.i32_type();
                let payload_ty = self.context.i64_type().array_type(4); // 32 bytes max payload
                Ok(self.context.struct_type(&[i32_ty.into(), payload_ty.into()], false).into())
            }
            HirType::Array(inner, size) => {
                let inner_ty = self.lower_type(inner)?;
                match inner_ty {
                    inkwell::types::BasicTypeEnum::ArrayType(t) => Ok(t.array_type(*size as u32).into()),
                    inkwell::types::BasicTypeEnum::FloatType(t) => Ok(t.array_type(*size as u32).into()),
                    inkwell::types::BasicTypeEnum::IntType(t) => Ok(t.array_type(*size as u32).into()),
                    inkwell::types::BasicTypeEnum::PointerType(t) => Ok(t.array_type(*size as u32).into()),
                    inkwell::types::BasicTypeEnum::StructType(t) => Ok(t.array_type(*size as u32).into()),
                    inkwell::types::BasicTypeEnum::VectorType(t) => Ok(t.array_type(*size as u32).into()),
                    inkwell::types::BasicTypeEnum::ScalableVectorType(_) => Err("ScalableVectorType not supported in array".into()),
                }
            }
            HirType::Slice(_) => {
                let ptr_ty = self.context.ptr_type(inkwell::AddressSpace::default());
                let len_ty = self.context.i64_type();
                Ok(self.context.struct_type(&[ptr_ty.into(), len_ty.into()], false).into())
            }
            HirType::Result(ok_ty, err_ty) => {
                let tag_ty = self.context.i32_type();
                let ok_llvm = self.lower_type(ok_ty)?;
                let err_llvm = self.lower_type(err_ty)?;
                Ok(self.context.struct_type(&[tag_ty.into(), ok_llvm.into(), err_llvm.into()], false).into())
            }
            HirType::Error => Err("Cannot lower error type".into()),
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
            BasicTypeEnum::StructType(s) => s.fn_type(&param_types, false),
            BasicTypeEnum::ArrayType(a) => a.fn_type(&param_types, false),
            BasicTypeEnum::FloatType(f) => f.fn_type(&param_types, false),
            _ => return Err(format!("Unsupported return type: {:?}", ret_type)),
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
        let block = self.builder.get_insert_block().unwrap();
        if block.get_terminator().is_none() {
            let ret_ty = llvm_func.get_type().get_return_type().unwrap();
            let default_val: inkwell::values::BasicValueEnum<'ctx> = match ret_ty {
                BasicTypeEnum::IntType(i) => i.const_zero().into(),
                BasicTypeEnum::PointerType(p) => p.const_null().into(),
                BasicTypeEnum::StructType(s) => s.const_zero().into(),
                BasicTypeEnum::ArrayType(a) => a.const_zero().into(),
                BasicTypeEnum::FloatType(f) => f.const_zero().into(),
                _ => panic!("Unsupported return type for implicit return"),
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
                    let val = self.compile_expr(expr)?;
                    self.builder.build_return(Some(&val)).unwrap();
                } else {
                    let func = self.builder.get_insert_block().unwrap().get_parent().unwrap();
                    let ret_ty = func.get_type().get_return_type().unwrap();
                    let default_val: inkwell::values::BasicValueEnum<'ctx> = match ret_ty {
                        BasicTypeEnum::IntType(i) => i.const_zero().into(),
                        BasicTypeEnum::PointerType(p) => p.const_null().into(),
                        BasicTypeEnum::StructType(s) => s.const_zero().into(),
                        BasicTypeEnum::ArrayType(a) => a.const_zero().into(),
                        BasicTypeEnum::FloatType(f) => f.const_zero().into(),
                        _ => panic!("Unsupported return type for implicit return"),
                    };
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
            HirStmt::For(item_name, iterable, body) => {
                let iter_val = self.compile_expr(iterable)?;
                let iter_ty = iterable.ty();

                let (ptr_val, len_val) = match iter_ty {
                    HirType::Array(_, size) => {
                        let arr_ty_llvm = self.lower_type(&iter_ty)?;
                        let arr_ptr = self.builder.build_alloca(arr_ty_llvm, "arr_tmp").unwrap();
                        self.builder.build_store(arr_ptr, iter_val).unwrap();
                        let ptr = unsafe { self.builder.build_in_bounds_gep(arr_ty_llvm, arr_ptr, &[self.context.i32_type().const_zero(), self.context.i32_type().const_zero()], "start_ptr") }.unwrap();
                        let len = self.context.i64_type().const_int(size, false);
                        (ptr, len)
                    }
                    HirType::Slice(_) => {
                        let slice_val = iter_val.into_struct_value();
                        let ptr = self.builder.build_extract_value(slice_val, 0, "slice_ptr").unwrap().into_pointer_value();
                        let len = self.builder.build_extract_value(slice_val, 1, "slice_len").unwrap().into_int_value();
                        (ptr, len)
                    }
                    _ => return Err("Unsupported iterable type".into())
                };

                let current_func = self.builder.get_insert_block().unwrap().get_parent().unwrap();
                let idx_ptr = self.builder.build_alloca(self.context.i64_type(), "idx").unwrap();
                self.builder.build_store(idx_ptr, self.context.i64_type().const_zero()).unwrap();

                let header_block = self.context.append_basic_block(current_func, "for.cond");
                let body_block = self.context.append_basic_block(current_func, "for.body");
                let merge_block = self.context.append_basic_block(current_func, "for.merge");

                self.builder.build_unconditional_branch(header_block).unwrap();
                self.builder.position_at_end(header_block);

                let current_idx = self.builder.build_load(self.context.i64_type(), idx_ptr, "current_idx").unwrap().into_int_value();
                let cond = self.builder.build_int_compare(inkwell::IntPredicate::ULT, current_idx, len_val, "cond").unwrap();
                self.builder.build_conditional_branch(cond, body_block, merge_block).unwrap();

                self.loop_stack.push((header_block, merge_block));
                self.builder.position_at_end(body_block);

                let inner_ty_hir = match iter_ty {
                    HirType::Array(t, _) => *t,
                    HirType::Slice(t) => *t,
                    _ => unreachable!()
                };
                let inner_ty_llvm = self.lower_type(&inner_ty_hir)?;
                let item_ptr = unsafe { self.builder.build_in_bounds_gep(inner_ty_llvm, ptr_val, &[current_idx], "item_ptr") }.unwrap();
                let item_val = self.builder.build_load(inner_ty_llvm, item_ptr, "item_val").unwrap();

                let item_alloca = self.builder.build_alloca(inner_ty_llvm, item_name).unwrap();
                self.builder.build_store(item_alloca, item_val).unwrap();
                
                self.enter_scope();
                self.scopes.last_mut().unwrap().insert(item_name.clone(), (item_alloca, inner_ty_hir));
                
                self.compile_block(body)?;

                self.exit_scope();

                if self.builder.get_insert_block().unwrap().get_terminator().is_none() {
                    let current_idx_end = self.builder.build_load(self.context.i64_type(), idx_ptr, "current_idx").unwrap().into_int_value();
                    let next_idx = self.builder.build_int_add(current_idx_end, self.context.i64_type().const_int(1, false), "next_idx").unwrap();
                    self.builder.build_store(idx_ptr, next_idx).unwrap();
                    self.builder.build_unconditional_branch(header_block).unwrap();
                }

                self.loop_stack.pop();
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

    fn compile_lvalue(&mut self, expr: &HirExpr) -> Result<PointerValue<'ctx>, String> {
        match expr {
            HirExpr::VarRef(name, _) => {
                if let Some((alloca, _)) = self.lookup_var(name) {
                    Ok(alloca)
                } else {
                    Err(format!("Undefined variable in lvalue: {}", name))
                }
            }
            HirExpr::FieldAccess(base, field_name, _) => {
                let base_ptr = self.compile_lvalue(base)?;
                let base_ty = base.ty();
                let struct_name = if let HirType::Struct(s) = base_ty { s } else { return Err("Field access on non-struct".into()); };
                let struct_type = self.structs.get(&struct_name).unwrap();
                let fields = self.struct_layouts.get(&struct_name).unwrap();
                let field_idx = fields.iter().position(|(n, _)| n == field_name).unwrap() as u32;
                Ok(self.builder.build_struct_gep(*struct_type, base_ptr, field_idx, "field_ptr").unwrap())
            }
            HirExpr::IndexExpr(base, idx, _) => {
                let base_ptr = self.compile_lvalue(base)?;
                let base_ty = base.ty();
                let idx_val = self.compile_expr(idx)?.into_int_value();
                let zero = self.context.i32_type().const_int(0, false);
                match base_ty {
                    HirType::Array(_, _) => {
                        let array_llvm_ty = self.lower_type(&base_ty)?;
                        Ok(unsafe { self.builder.build_gep(array_llvm_ty, base_ptr, &[zero, idx_val], "array_ptr").unwrap() })
                    }
                    HirType::Slice(ref inner_ty) => {
                        let slice_llvm_ty = self.lower_type(&base_ty)?;
                        let ptr_ptr = self.builder.build_struct_gep(slice_llvm_ty, base_ptr, 0, "slice_ptr_ptr").unwrap();
                        let ptr_val = self.builder.build_load(self.context.ptr_type(inkwell::AddressSpace::default()), ptr_ptr, "slice_ptr").unwrap().into_pointer_value();
                        let inner_llvm_ty = self.lower_type(&inner_ty)?;
                        Ok(unsafe { self.builder.build_gep(inner_llvm_ty, ptr_val, &[idx_val], "slice_idx_ptr").unwrap() })
                    }
                    _ => Err("Index on non-array/slice".into()),
                }
            }
            HirExpr::Deref(inner, _) => {
                let inner_val = self.compile_expr(inner)?.into_pointer_value();
                Ok(inner_val)
            }
            _ => Err(format!("Invalid lvalue expression: {:?}", expr)),
        }
    }

    fn compile_expr(&mut self, expr: &HirExpr) -> Result<BasicValueEnum<'ctx>, String> {
        match expr {
            HirExpr::IntLiteral(val, _) => {
                Ok(self.context.i32_type().const_int(*val as u64, false).into())
            }
            HirExpr::BoolLiteral(val, _) => {
                Ok(self.context.bool_type().const_int(if *val { 1 } else { 0 }, false).into())
            }
            HirExpr::EnumVariant(_, _, val, _) => {
                Ok(self.context.i32_type().const_int(*val, false).into())
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
                    let rhs_val = self.compile_expr(rhs)?;
                    let lhs_ptr = self.compile_lvalue(lhs)?;
                    self.builder.build_store(lhs_ptr, rhs_val).unwrap();
                    return Ok(rhs_val);
                }

                let lhs_ty = lhs.ty();
                let lhs_val = self.compile_expr(lhs)?;
                let rhs_val = self.compile_expr(rhs)?;

                let res = match lhs_ty {
                    HirType::I32 | HirType::Bool | HirType::Enum(_) => {
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
            HirExpr::VariantConstructor(variant_name, case_name, args, ty) => {
                let variant_ty = self.lower_type(ty)?;
                let ptr = self.builder.build_alloca(variant_ty, "variant_alloca").unwrap();

                let cases = self.variants.get(variant_name).unwrap();
                let case_idx = cases.iter().position(|(n, _)| n == case_name).unwrap();

                let tag_ptr = self.builder.build_struct_gep(variant_ty, ptr, 0, "tag_ptr").unwrap();
                self.builder.build_store(tag_ptr, self.context.i32_type().const_int(case_idx as u64, false)).unwrap();

                if args.len() > 0 {
                    let mut arg_types = vec![];
                    for arg in args {
                        arg_types.push(self.lower_type(&arg.ty())?.into());
                    }
                    let payload_struct_ty = self.context.struct_type(&arg_types, false);
                    
                    let payload_ptr = self.builder.build_struct_gep(variant_ty, ptr, 1, "payload_ptr").unwrap();
                    
                    let mut payload_val = payload_struct_ty.get_undef();
                    for (i, arg_expr) in args.iter().enumerate() {
                        let arg_val = self.compile_expr(arg_expr)?;
                        payload_val = self.builder.build_insert_value(payload_val, arg_val, i as u32, "insert").unwrap().into_struct_value();
                    }
                    self.builder.build_store(payload_ptr, payload_val).unwrap();
                }

                let val = self.builder.build_load(variant_ty, ptr, "variant_val").unwrap();
                Ok(val)
            }
            HirExpr::Match(target, arms, match_ty) => {
                let target_val = self.compile_expr(target)?;
                let target_ty_hir = target.ty();
                let variant_name = if let HirType::Variant(n) = target_ty_hir { n } else { return Err("Match target must be a variant".into()); };
                let variant_ty_llvm = self.lower_type(&HirType::Variant(variant_name.clone()))?;
                
                let tag_val = self.builder.build_extract_value(target_val.into_struct_value(), 0, "tag").unwrap().into_int_value();
                
                let function = self.builder.get_insert_block().unwrap().get_parent().unwrap();
                let merge_bb = self.context.append_basic_block(function, "match_merge");
                
                let res_llvm_ty = self.lower_type(match_ty)?;
                let res_ptr = self.builder.build_alloca(res_llvm_ty, "match_res").unwrap();
                
                let mut switch_cases = vec![];
                
                let cases = self.variants.get(&variant_name).unwrap().clone();
                let mut case_blocks = vec![];
                
                for (pattern, arm_expr) in arms {
                    if let HirPattern::VariantCase(case_name, bindings) = pattern {
                        let case_idx = cases.iter().position(|(n, _)| n == case_name).unwrap();
                        let case_bb = self.context.append_basic_block(function, &format!("match_case_{}", case_name));
                        switch_cases.push((self.context.i32_type().const_int(case_idx as u64, false), case_bb));
                        case_blocks.push((case_idx, bindings, arm_expr, case_bb));
                    }
                }
                
                let default_bb = self.context.append_basic_block(function, "match_default");
                self.builder.build_switch(tag_val, default_bb, &switch_cases).unwrap();
                
                self.builder.position_at_end(default_bb);
                self.builder.build_unreachable().unwrap();
                
                for (case_idx, bindings, arm_expr, case_bb) in case_blocks {
                    self.builder.position_at_end(case_bb);
                    
                    self.enter_scope();
                    
                    if bindings.len() > 0 {
                        let target_ptr = self.builder.build_alloca(variant_ty_llvm, "match_target_ptr").unwrap();
                        self.builder.build_store(target_ptr, target_val).unwrap();
                        
                        let payload_ptr = self.builder.build_struct_gep(variant_ty_llvm, target_ptr, 1, "payload_ptr").unwrap();
                        
                        let case_field_types = &cases[case_idx].1;
                        let mut arg_types = vec![];
                        for ty in case_field_types {
                            arg_types.push(self.lower_type(ty)?.into());
                        }
                        let payload_struct_ty = self.context.struct_type(&arg_types, false);
                        
                        let payload_val = self.builder.build_load(payload_struct_ty, payload_ptr, "payload_val").unwrap().into_struct_value();
                        
                        for (i, bind_name) in bindings.iter().enumerate() {
                            let field_val = self.builder.build_extract_value(payload_val, i as u32, bind_name).unwrap();
                            let field_ty_llvm = self.lower_type(&case_field_types[i])?;
                            let bind_alloca = self.builder.build_alloca(field_ty_llvm, bind_name).unwrap();
                            self.builder.build_store(bind_alloca, field_val).unwrap();
                            self.declare_var(bind_name.clone(), bind_alloca, case_field_types[i].clone());
                        }
                    }
                    
                    let arm_val = self.compile_expr(arm_expr)?;
                    self.builder.build_store(res_ptr, arm_val).unwrap();
                    
                    self.exit_scope();
                    
                    if self.builder.get_insert_block().unwrap().get_terminator().is_none() {
                        self.builder.build_unconditional_branch(merge_bb).unwrap();
                    }
                }
                
                self.builder.position_at_end(merge_bb);
                let res = self.builder.build_load(res_llvm_ty, res_ptr, "match_res_val").unwrap();
                Ok(res)
            }
            HirExpr::ArrayExpr(elements, ty) => {
                let arr_ty = self.lower_type(ty)?;
                let ptr = self.builder.build_alloca(arr_ty, "array_alloca").unwrap();
                for (i, el) in elements.iter().enumerate() {
                    let el_val = self.compile_expr(el)?;
                    let idx_val = self.context.i32_type().const_int(i as u64, false);
                    let el_ptr = unsafe { self.builder.build_in_bounds_gep(arr_ty, ptr, &[self.context.i32_type().const_zero(), idx_val], "el_ptr") }.unwrap();
                    self.builder.build_store(el_ptr, el_val).unwrap();
                }
                let arr_val = self.builder.build_load(arr_ty, ptr, "arr_val").unwrap();
                Ok(arr_val)
            }
            HirExpr::IndexExpr(base, idx, ty) => {
                let base_val = self.compile_expr(base)?;
                let idx_val = self.compile_expr(idx)?.into_int_value();
                
                let base_ty_hir = base.ty();
                match base_ty_hir {
                    HirType::Array(_, _) => {
                        let arr_ty_llvm = self.lower_type(&base_ty_hir)?;
                        let arr_ptr = self.builder.build_alloca(arr_ty_llvm, "arr_tmp").unwrap();
                        self.builder.build_store(arr_ptr, base_val).unwrap();
                        let el_ptr = unsafe { self.builder.build_in_bounds_gep(arr_ty_llvm, arr_ptr, &[self.context.i32_type().const_zero(), idx_val], "el_ptr") }.unwrap();
                        let el_ty_llvm = self.lower_type(ty)?;
                        let el_val = self.builder.build_load(el_ty_llvm, el_ptr, "el_val").unwrap();
                        Ok(el_val)
                    }
                    HirType::Slice(_) => {
                        let slice_val = base_val.into_struct_value();
                        let ptr_val = self.builder.build_extract_value(slice_val, 0, "slice_ptr").unwrap().into_pointer_value();
                        let el_ty_llvm = self.lower_type(ty)?;
                        let el_ptr = unsafe { self.builder.build_in_bounds_gep(el_ty_llvm, ptr_val, &[idx_val], "el_ptr") }.unwrap();
                        let el_val = self.builder.build_load(el_ty_llvm, el_ptr, "el_val").unwrap();
                        Ok(el_val)
                    }
                    _ => unreachable!()
                }
            }
            HirExpr::SliceExpr(base, start, end, ty) => {
                let base_val = self.compile_expr(base)?;
                let start_val = self.compile_expr(start)?.into_int_value();
                let end_val = self.compile_expr(end)?.into_int_value();
                
                let base_ty_hir = base.ty();
                
                let (ptr_val, len_val) = match base_ty_hir {
                    HirType::Array(_, _) => {
                        let arr_ty_llvm = self.lower_type(&base_ty_hir)?;
                        let arr_ptr = self.builder.build_alloca(arr_ty_llvm, "arr_tmp").unwrap();
                        self.builder.build_store(arr_ptr, base_val).unwrap();
                        
                        let start_ptr = unsafe { self.builder.build_in_bounds_gep(arr_ty_llvm, arr_ptr, &[self.context.i32_type().const_zero(), start_val], "start_ptr") }.unwrap();
                        
                        let len_i32 = self.builder.build_int_sub(end_val, start_val, "len_i32").unwrap();
                        let len_i64 = self.builder.build_int_cast(len_i32, self.context.i64_type(), "len_i64").unwrap();
                        (start_ptr, len_i64)
                    }
                    HirType::Slice(inner) => {
                        let slice_val = base_val.into_struct_value();
                        let orig_ptr = self.builder.build_extract_value(slice_val, 0, "orig_ptr").unwrap().into_pointer_value();
                        
                        let inner_ty_llvm = self.lower_type(&inner)?;
                        let start_ptr = unsafe { self.builder.build_in_bounds_gep(inner_ty_llvm, orig_ptr, &[start_val], "start_ptr") }.unwrap();
                        
                        let len_i32 = self.builder.build_int_sub(end_val, start_val, "len_i32").unwrap();
                        let len_i64 = self.builder.build_int_cast(len_i32, self.context.i64_type(), "len_i64").unwrap();
                        (start_ptr, len_i64)
                    }
                    _ => unreachable!()
                };
                
                let mut slice_struct = self.context.struct_type(&[self.context.ptr_type(inkwell::AddressSpace::default()).into(), self.context.i64_type().into()], false).get_undef();
                slice_struct = self.builder.build_insert_value(slice_struct, ptr_val, 0, "insert_ptr").unwrap().into_struct_value();
                slice_struct = self.builder.build_insert_value(slice_struct, len_val, 1, "insert_len").unwrap().into_struct_value();
                
                Ok(slice_struct.into())
            }
            HirExpr::ResultOk(inner, ty) => {
                let inner_val = self.compile_expr(inner)?;
                let res_ty_llvm = self.lower_type(ty)?.into_struct_type();
                let mut res_struct = res_ty_llvm.get_undef();
                res_struct = self.builder.build_insert_value(res_struct, self.context.i32_type().const_zero(), 0, "tag_ok").unwrap().into_struct_value();
                res_struct = self.builder.build_insert_value(res_struct, inner_val, 1, "ok_val").unwrap().into_struct_value();
                Ok(res_struct.into())
            }
            HirExpr::ResultErr(inner, ty) => {
                let inner_val = self.compile_expr(inner)?;
                let res_ty_llvm = self.lower_type(ty)?.into_struct_type();
                let mut res_struct = res_ty_llvm.get_undef();
                res_struct = self.builder.build_insert_value(res_struct, self.context.i32_type().const_int(1, false), 0, "tag_err").unwrap().into_struct_value();
                res_struct = self.builder.build_insert_value(res_struct, inner_val, 2, "err_val").unwrap().into_struct_value();
                Ok(res_struct.into())
            }
            HirExpr::Try(inner, _ok_ty) => {
                let inner_val = self.compile_expr(inner)?.into_struct_value();
                let tag = self.builder.build_extract_value(inner_val, 0, "tag").unwrap().into_int_value();
                let is_err = self.builder.build_int_compare(inkwell::IntPredicate::EQ, tag, self.context.i32_type().const_int(1, false), "is_err").unwrap();
                
                let current_func = self.builder.get_insert_block().unwrap().get_parent().unwrap();
                let ok_block = self.context.append_basic_block(current_func, "try.ok");
                let err_block = self.context.append_basic_block(current_func, "try.err");
                
                self.builder.build_conditional_branch(is_err, err_block, ok_block).unwrap();
                
                self.builder.position_at_end(err_block);
                self.builder.build_return(Some(&inner_val)).unwrap();
                
                self.builder.position_at_end(ok_block);
                let ok_val = self.builder.build_extract_value(inner_val, 1, "ok_val").unwrap();
                Ok(ok_val)
            }
            HirExpr::Ref(inner, _, _) => {
                let ptr = self.compile_lvalue(inner)?;
                Ok(ptr.into())
            }
            HirExpr::Deref(inner, _) => {
                let inner_val = self.compile_expr(inner)?.into_pointer_value();
                let ty = self.lower_type(&expr.ty())?;
                Ok(self.builder.build_load(ty, inner_val, "deref").unwrap())
            }
            HirExpr::Block(block, _) => {
                let mut last_val = None;
                for stmt in &block.statements {
                    if self.builder.get_insert_block().unwrap().get_terminator().is_some() {
                        break;
                    }
                    if let HirStmt::Expr(e) = stmt {
                        last_val = Some(self.compile_expr(e)?);
                    } else {
                        self.compile_stmt(stmt)?;
                        last_val = None;
                    }
                }
                if let Some(val) = last_val {
                    Ok(val)
                } else {
                    Ok(self.context.i32_type().const_zero().into()) // dummy void
                }
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
        variants: hir.variants.clone(),
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
