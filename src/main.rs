use clap::Parser;

mod lexer;
mod parser;
mod hir;
mod backend;

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
            let ast = parser::parse(&input).expect("Failed to parse vertical slice snippet");
            
            let out_bin = output.clone().unwrap_or_else(|| "a.out".to_string());
            backend::compile_to_binary(&ast, &out_bin).expect("Failed to compile");
        }
        Commands::Run { file } => {
            let input = std::fs::read_to_string(file).expect("Failed to read file");
            let ast = parser::parse(&input).expect("Failed to parse vertical slice snippet");
            
            let out_bin = "./.tmp.vera.out";
            backend::compile_to_binary(&ast, out_bin).expect("Failed to compile");
            
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
