use inkwell::context::Context;
use inkwell::targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine};
use inkwell::OptimizationLevel;
use crate::hir::{HirProgram, HirStmt, HirExpr};

pub fn compile_to_binary(hir: &HirProgram, output_path: &str) -> Result<(), String> {
    let context = Context::create();
    let module = context.create_module("main_module");
    let builder = context.create_builder();
    
    let i32_type = context.i32_type();
    
    for func in &hir.functions {
        let fn_type = i32_type.fn_type(&[], false); // Assume all return i32 for now
        let function = module.add_function(&func.name, fn_type, None);
        let basic_block = context.append_basic_block(function, "entry");
        
        builder.position_at_end(basic_block);
        
        // Find return statement
        let mut ret_val = 0;
        for stmt in &func.body.statements {
            if let HirStmt::Return(Some(HirExpr::IntLiteral(val, _))) = stmt {
                ret_val = *val;
                break;
            }
        }
        
        let llvm_ret = i32_type.const_int(ret_val as u64, false);
        builder.build_return(Some(&llvm_ret)).unwrap();
        
        if !function.verify(true) {
            return Err("Function verification failed".to_string());
        }
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
    target_machine.write_to_file(&module, FileType::Object, &obj_path).map_err(|e| e.to_string())?;
    
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
