use clap::Parser;

mod lexer;
mod parser;
mod hir;
mod backend;
mod verification;
use crate::parser::ast::AstNode;

#[derive(Parser)]
#[command(name = "vera")]
#[command(about = "Vera Language Compiler", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    Build {
        file: String,
        #[arg(long, short)]
        output: Option<String>,
    },
    Run {
        file: String,
    }
}

fn main() -> miette::Result<()> {
    let cli = Cli::parse();
    
    match &cli.command {
        Commands::Build { file, output } => {
            let input = std::fs::read_to_string(file).expect("Failed to read file");
            let parser = parser::Parser::new(&input);
            let (cst, errors) = parser.parse();
            let mut has_errors = false;
            
            if !errors.is_empty() {
                for err in errors { eprintln!("Parse Error: {}", err); }
                has_errors = true;
            }
            
            let source_file = parser::ast::SourceFile::cast(cst).expect("Root is not a SourceFile");
            let mut lower_ctx = hir::lower::LoweringContext::new();
            let hir_program = lower_ctx.lower_program(&source_file);
            
            if !lower_ctx.errors.is_empty() {
                for err in lower_ctx.errors { eprintln!("Semantic Error: {}", err); }
                has_errors = true;
            }
            
            if has_errors {
                std::process::exit(1);
            }
            
            if let Err(e) = verification::verify_program(&hir_program) {
                eprintln!("Verification Error: {:?}", e);
                std::process::exit(1);
            }
            
            let out_bin = output.clone().unwrap_or_else(|| "a.out".to_string());
            backend::compile_to_binary(&hir_program, &out_bin).expect("Failed to compile");
        }
        Commands::Run { file } => {
            let input = std::fs::read_to_string(file).expect("Failed to read file");
            let parser = parser::Parser::new(&input);
            let (cst, errors) = parser.parse();
            let mut has_errors = false;
            
            if !errors.is_empty() {
                for err in errors { eprintln!("Parse Error: {}", err); }
                has_errors = true;
            }
            
            let source_file = parser::ast::SourceFile::cast(cst).expect("Root is not a SourceFile");
            let mut lower_ctx = hir::lower::LoweringContext::new();
            let hir_program = lower_ctx.lower_program(&source_file);
            
            if !lower_ctx.errors.is_empty() {
                for err in lower_ctx.errors { eprintln!("Semantic Error: {}", err); }
                has_errors = true;
            }
            
            if has_errors {
                std::process::exit(1);
            }
            
            if let Err(e) = verification::verify_program(&hir_program) {
                eprintln!("Verification Error: {:?}", e);
                std::process::exit(1);
            }
            
            let out_bin = "./.tmp.vera.out";
            backend::compile_to_binary(&hir_program, out_bin).expect("Failed to compile");
            
            let status = std::process::Command::new(out_bin)
                .status()
                .expect("Failed to execute binary");
                
            // Clean up the temporary binary
            let _ = std::fs::remove_file(out_bin);
                
            std::process::exit(status.code().unwrap_or(1));
        }
    }
    
    Ok(())
}
