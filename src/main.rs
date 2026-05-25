use clap::{Parser, Subcommand};
use tracing::{info, warn, error};
use tracing_subscriber::EnvFilter;

mod lexer;
mod parser;
mod hir;
mod backend;
mod verification;
mod workspace;
mod query;
pub mod diagnostics;
use crate::parser::ast::AstNode;

/// Write `content` to `--emit-out <file>` if specified, otherwise to stdout.
fn emit_output(emit_out: Option<&str>, content: &str) {
    if let Some(path) = emit_out {
        std::fs::write(path, content).unwrap_or_else(|e| {
            eprintln!("Failed to write emit output to {path}: {e}");
        });
    } else {
        println!("{content}");
    }
}

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

    let emit = cli.emit.as_deref();
    let emit_out = cli.emit_out.as_deref();

    // --emit=tokens: lex the entry file in lossless mode and dump the token stream.
    // This runs before the main pipeline so trivia tokens are included.
    if emit == Some("tokens") {
        use logos::Logos;
        use crate::lexer::Token;
        let source = std::fs::read_to_string(file).map_err(|e| {
            error!("Failed to read {file}: {e}");
        })?;
        let mut out = String::new();
        for res in Token::lexer(&source) {
            match res {
                Ok(tok) => out.push_str(&format!("{tok:?}\n")),
                Err(_) => out.push_str("ErrorToken\n"),
            }
        }
        emit_output(emit_out, &out);
        return Err(());
    }

    // CLI builds use strip mode to discard CST trivia (whitespace/comments) for
    // lower memory usage. Lossless mode is reserved for the LSP server.
    // Exception: --emit=cst uses lossless so trivia is visible in the output.
    let mut qctx = if emit == Some("cst") {
        query::QueryContext::new()
    } else {
        query::QueryContext::new_strip()
    };
    let entry_file_id = qctx.load_entry_file(file).map_err(|e| {
        error!("Workspace error: {:?}", e);
    })?;

    if emit == Some("cst") {
        let entry_data = qctx.workspace().files.get(&entry_file_id).unwrap();
        emit_output(emit_out, &format!("{:#?}", entry_data.ast));
        return Err(());
    }

    if qctx.has_parse_errors() {
        return Err(());
    }

    // HIR lowering
    {
        let (_, errors) = qctx.query_hir_program();
        if !errors.is_empty() {
            for err in errors {
                error!("Semantic Error: {err}");
            }
            return Err(());
        }
    }

    if emit == Some("hir") {
        let (prog, _) = qctx.query_hir_program();
        emit_output(emit_out, &format!("{prog:#?}"));
        return Err(());
    }

    // Borrow checking
    {
        let errors = qctx.query_borrow_check();
        if !errors.is_empty() {
            for err in errors {
                error!("Borrow Error: {err}");
            }
            return Err(());
        }
    }

    if cli.verify {
        info!("Running verification pipeline");
        if let Err(e) = qctx.query_verify_program() {
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

    if emit == Some("llvm") {
        unsafe { std::env::set_var("PRINT_IR", "1") };
    }

    let emit_obj = emit == Some("obj");
    let out_path = if emit_obj {
        output_bin.unwrap_or_else(|| "a.o".to_string())
    } else {
        output_bin.unwrap_or_else(|| "a.out".to_string())
    };

    let opts = backend::CompileOptions {
        target: cli.target.as_deref(),
        emit_obj_only: emit_obj,
    };
    let (hir_program, _) = qctx.query_hir_program();
    backend::compile_with_options(hir_program, &out_path, &opts).expect("Failed to compile");
    info!("Compilation finished: {}", out_path);

    Ok(out_path)
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
