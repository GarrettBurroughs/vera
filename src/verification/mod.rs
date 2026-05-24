pub mod wp;
pub mod smt;

use crate::hir::HirProgram;

#[derive(Debug)]
#[allow(dead_code)] // String payload fields are observed via the Debug impl in error reporting
pub enum VerificationError {
    Z3Error(String),
    ProofFailed(String),
}

/// Runs the verification pipeline on the HIR program.
/// This will generate weakest preconditions for each function and prove them using Z3.
pub fn verify_program(program: &HirProgram) -> Result<(), VerificationError> {
    for func in &program.functions {
        wp::verify_func(func)?;
    }
    Ok(())
}
