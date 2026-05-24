use clap::{Parser, Subcommand};
use tracing::{info, warn, error};
use tracing_subscriber::EnvFilter;

mod lexer;
mod parser;
mod hir;
mod backend;
mod verification;
use crate::parser::ast::AstNode;

#[derive(Parser, Debug)]
#[command(name = "vera")]
#[command(about = "Vera Language Compiler", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(long, default_value = "warn")]
    log_level: String,

    #[arg(long)]
    log_file: Option<String>,

    #[arg(short = 'O', long)]
    opt_level: Option<String>,

    #[arg(long)]
    verify: bool,

    #[arg(long)]
    solver: Option<String>,

    #[arg(long)]
    target: Option<String>,

    #[arg(long)]
    emit: Option<String>,

    #[arg(long)]
    emit_out: Option<String>,

    #[arg(long)]
    pretty: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Build {
        file: String,
        #[arg(long, short)]
        output: Option<String>,
    },
    Run {
        file: String,
    },
    Check {
        file: String,
    },
    Lsp,
}

fn setup_logging(cli: &Cli) {
    let filter = EnvFilter::new(format!("verify={}", cli.log_level));
    
    if let Some(ref file_path) = cli.log_file {
        let file = std::fs::File::create(file_path).expect("Failed to create log file");
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(file)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .init();
    }
}

fn run_compiler_pipeline(file: &str, cli: &Cli, is_check: bool, output_bin: Option<String>) -> Result<String, ()> {
    info!("Starting compilation for {}", file);
    let input = std::fs::read_to_string(file).expect("Failed to read file");
    
    let parser = parser::Parser::new(&input);
    let (cst, errors) = parser.parse();
    
    if let Some(emit_target) = &cli.emit
        && emit_target == "cst" {
            println!("{:#?}", cst);
            return Err(());
        }
    
    let mut has_errors = false;
    if !errors.is_empty() {
        for err in errors { error!("Parse Error: {}", err); }
        has_errors = true;
    }
    
    let source_file = parser::ast::SourceFile::cast(cst).expect("Root is not a SourceFile");
    let mut lower_ctx = hir::lower::LoweringContext::new();
    let hir_program = lower_ctx.lower_program(&source_file);
    
    if let Some(emit_target) = &cli.emit
        && emit_target == "hir" {
            println!("{:#?}", hir_program);
            return Err(());
        }
    
    if !lower_ctx.errors.is_empty() {
        for err in lower_ctx.errors { error!("Semantic Error: {}", err); }
        has_errors = true;
    }
    
    let mut borrow_ck = hir::borrowck::BorrowChecker::new();
    borrow_ck.check_program(&hir_program);
    if !borrow_ck.errors.is_empty() {
        for err in borrow_ck.errors { error!("Borrow Error: {}", err); }
        has_errors = true;
    }
    
    if has_errors {
        return Err(());
    }
    
    if cli.verify {
        info!("Running verification pipeline");
        if let Err(e) = verification::verify_program(&hir_program) {
            error!("Verification Error: {:?}", e);
            return Err(());
        }
    } else {
        warn!("Verification skipped (use --verify to enable)");
    }
    
    if is_check {
        info!("Check complete. Halting before LLVM codegen.");
        return Ok("".to_string());
    }
    
    if let Some(emit_target) = &cli.emit
        && emit_target == "llvm" {
            unsafe { std::env::set_var("PRINT_IR", "1") };
        }
    
    let out_bin = output_bin.unwrap_or_else(|| "a.out".to_string());
    backend::compile_to_binary(&hir_program, &out_bin).expect("Failed to compile");
    info!("Compilation finished: {}", out_bin);
    
    Ok(out_bin)
}

fn main() -> miette::Result<()> {
    let cli = Cli::parse();
    setup_logging(&cli);
    
    match &cli.command {
        Commands::Build { file, output } => {
            if run_compiler_pipeline(file, &cli, false, output.clone()).is_err() {
                std::process::exit(1);
            }
        }
        Commands::Run { file } => {
            let out_bin = format!("./.tmp.vera.{}.out", std::process::id());
            if run_compiler_pipeline(file, &cli, false, Some(out_bin.clone())).is_err() {
                std::process::exit(1);
            }
            
            let status = std::process::Command::new(&out_bin)
                .status()
                .expect("Failed to execute binary");
                
            let _ = std::fs::remove_file(&out_bin);
            std::process::exit(status.code().unwrap_or(1));
        }
        Commands::Check { file } => {
            if run_compiler_pipeline(file, &cli, true, None).is_err() {
                std::process::exit(1);
            }
        }
        Commands::Lsp => {
            info!("LSP mode requested but not yet implemented");
        }
    }
    
    Ok(())
}
