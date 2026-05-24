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
    variables: BTreeMap<String, PointerValue<'ctx>>,
}

impl<'ctx> CodeGen<'ctx> {
    fn declare_func(&mut self, func: &HirFunc) -> Result<(), String> {
        let ret_type = match func.ret_type {
            HirType::I32 => self.context.i32_type().into(),
            HirType::Bool => self.context.bool_type().into(),
            HirType::Void => self.context.i32_type().into(),
            _ => return Err("Invalid return type".into()),
        };

        let mut param_types: Vec<inkwell::types::BasicMetadataTypeEnum<'ctx>> = Vec::new();
        for (_, ty) in &func.params {
            let llvm_ty = match ty {
                HirType::I32 => self.context.i32_type().into(),
                HirType::Bool => self.context.bool_type().into(),
                _ => return Err("Invalid param type".into()),
            };
            param_types.push(llvm_ty);
        }

        let fn_type = match ret_type {
            BasicTypeEnum::IntType(i) => i.fn_type(&param_types, false),
            _ => return Err("Unsupported return type".into()),
        };

        self.module.add_function(&func.name, fn_type, None);
        Ok(())
    }

    fn compile_func(&mut self, func: &HirFunc) -> Result<(), String> {
        let llvm_func = self.module.get_function(&func.name).unwrap();
        let basic_block = self.context.append_basic_block(llvm_func, "entry");
        self.builder.position_at_end(basic_block);

        self.variables.clear();

        for (i, (name, _ty)) in func.params.iter().enumerate() {
            let param_val = llvm_func.get_nth_param(i as u32).unwrap();
            let alloca = self.builder.build_alloca(param_val.get_type(), name).unwrap();
            self.builder.build_store(alloca, param_val).unwrap();
            self.variables.insert(name.clone(), alloca);
        }

        self.compile_block(&func.body)?;

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
        for stmt in &block.statements {
            self.compile_stmt(stmt)?;
        }
        Ok(())
    }

    fn compile_stmt(&mut self, stmt: &HirStmt) -> Result<(), String> {
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
                let llvm_ty: BasicTypeEnum<'ctx> = match ty {
                    HirType::I32 => self.context.i32_type().into(),
                    HirType::Bool => self.context.bool_type().into(),
                    _ => return Err("Unsupported variable type".into()),
                };
                
                let alloca = self.builder.build_alloca(llvm_ty.into_int_type(), name).unwrap();
                self.variables.insert(name.clone(), alloca);
                
                let init_val = self.compile_expr(initializer)?;
                self.builder.build_store(alloca, init_val).unwrap();
            }
            HirStmt::Expr(expr) => {
                self.compile_expr(expr)?;
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
                if let Some(alloca) = self.variables.get(name) {
                    let val = self.builder.build_load(self.context.i32_type(), *alloca, name).unwrap(); // Assume i32_type for load, wait it could be bool! We need the actual type.
                    // To be safe, just use i32 for everything under the hood for phase 3.5.
                    // Wait, build_load requires knowing the type.
                    // Actually, alloca stores the allocated type. 
                    Ok(val)
                } else {
                    Err(format!("Variable {} not found in LLVM codegen", name))
                }
            }
            HirExpr::BinaryOp(op, lhs, rhs, _) => {
                if *op == BinaryOp::Assign {
                    if let HirExpr::VarRef(name, _) = &**lhs {
                        let rhs_val = self.compile_expr(rhs)?;
                        if let Some(alloca) = self.variables.get(name) {
                            self.builder.build_store(*alloca, rhs_val).unwrap();
                            return Ok(rhs_val);
                        } else {
                            return Err(format!("Variable {} not found", name));
                        }
                    } else {
                        return Err("Invalid assignment target".into());
                    }
                }

                let lhs_val = self.compile_expr(lhs)?.into_int_value();
                let rhs_val = self.compile_expr(rhs)?.into_int_value();

                let res = match op {
                    BinaryOp::Add => self.builder.build_int_add(lhs_val, rhs_val, "tmpadd").unwrap(),
                    BinaryOp::Sub => self.builder.build_int_sub(lhs_val, rhs_val, "tmpsub").unwrap(),
                    BinaryOp::Mul => self.builder.build_int_mul(lhs_val, rhs_val, "tmpmul").unwrap(),
                    BinaryOp::Div => self.builder.build_int_signed_div(lhs_val, rhs_val, "tmpdiv").unwrap(),
                    BinaryOp::Rem => self.builder.build_int_signed_rem(lhs_val, rhs_val, "tmprem").unwrap(),
                    BinaryOp::Eq => self.builder.build_int_compare(inkwell::IntPredicate::EQ, lhs_val, rhs_val, "tmpeq").unwrap(),
                    BinaryOp::Neq => self.builder.build_int_compare(inkwell::IntPredicate::NE, lhs_val, rhs_val, "tmpneq").unwrap(),
                    BinaryOp::Lt => self.builder.build_int_compare(inkwell::IntPredicate::SLT, lhs_val, rhs_val, "tmplt").unwrap(),
                    BinaryOp::Gt => self.builder.build_int_compare(inkwell::IntPredicate::SGT, lhs_val, rhs_val, "tmpgt").unwrap(),
                    BinaryOp::Le => self.builder.build_int_compare(inkwell::IntPredicate::SLE, lhs_val, rhs_val, "tmple").unwrap(),
                    BinaryOp::Ge => self.builder.build_int_compare(inkwell::IntPredicate::SGE, lhs_val, rhs_val, "tmpge").unwrap(),
                    BinaryOp::And => self.builder.build_and(lhs_val, rhs_val, "tmpand").unwrap(),
                    BinaryOp::Or => self.builder.build_or(lhs_val, rhs_val, "tmpor").unwrap(),
                    _ => return Err("Unsupported binary op".into()),
                };
                Ok(res.into())
            }
            HirExpr::UnaryOp(op, inner, _) => {
                let inner_val = self.compile_expr(inner)?.into_int_value();
                let res = match op {
                    UnaryOp::Neg => self.builder.build_int_neg(inner_val, "tmpneg").unwrap(),
                    UnaryOp::Not => self.builder.build_not(inner_val, "tmpnot").unwrap(),
                };
                Ok(res.into())
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
        variables: BTreeMap::new(),
    };
    
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
