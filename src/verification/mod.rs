pub mod wp;
pub mod smt;

use crate::hir::{HirProgram, Span};
use miette::Diagnostic;
use thiserror::Error;

#[derive(Error, Debug, Clone, Diagnostic)]
pub enum VerificationError {
    #[error("Z3 Error: {message}")]
    #[diagnostic(code(vera::z3_error))]
    Z3Error { message: String, #[doc(hidden)] span: Span },

    #[error("Verification failed: {message}")]
    #[diagnostic(code(vera::proof_failed))]
    ProofFailed { message: String, #[doc(hidden)] span: Span, counterexample: Option<std::collections::BTreeMap<String, String>> },

    #[error("Vacuous precondition: {message}")]
    #[diagnostic(code(vera::vacuous_precondition))]
    VacuousPrecondition { message: String, #[doc(hidden)] span: Span },
}

impl VerificationError {
    pub fn span(&self) -> Span {
        match self {
            Self::Z3Error { span, .. } => *span,
            Self::ProofFailed { span, .. } => *span,
            Self::VacuousPrecondition { span, .. } => *span,
        }
    }
}

/// Runs the verification pipeline on the HIR program.
/// This will generate weakest preconditions for each function and prove them using Z3.
pub fn verify_program(program: &HirProgram) -> Result<(), VerificationError> {
    for func in &program.functions {
        if func.body.is_some() {
            wp::verify_func(func)?;
        }
    }
    Ok(())
}
