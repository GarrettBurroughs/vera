/// The unified diagnostic representation for the Vera compiler.
///
/// Every compiler stage (parsing, semantic analysis, borrow checking, verification)
/// emits `Diagnostic` values rather than printing directly.  This intermediary
/// representation is agnostic to the rendering target:
///
/// * **CLI** — rendered as coloured, source-annotated terminal output via `miette`.
/// * **LSP** — serialised into the JSON payload expected by the LSP protocol
///   (`textDocument/publishDiagnostics`).
///
/// See `design/lsp_and_parsing.md` for the rationale behind this design.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    /// Short machine-readable code, e.g. `"E001"` or `"W042"`.
    pub code: String,
    /// High-level human-readable description of the problem.
    pub message: String,
    /// The primary source location the error points at.
    pub primary_span: DiagnosticSpan,
    /// Additional contextual spans, each annotated with an explanatory label.
    pub secondary_spans: Vec<(DiagnosticSpan, String)>,
    /// Optional actionable suggestion shown to the user (e.g. "did you mean …?").
    pub help: Option<String>,
}

/// Source-location attached to a `Diagnostic`.
///
/// Byte offsets are relative to the start of the file identified by `file_id`.
/// The LSP server converts these to `(line, character)` pairs using the file's
/// source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiagnosticSpan {
    pub file_id: usize,
    /// Inclusive start byte offset.
    pub start: u32,
    /// Exclusive end byte offset.
    pub end: u32,
}

impl DiagnosticSpan {
    pub fn new(file_id: usize, start: u32, end: u32) -> Self {
        Self { file_id, start, end }
    }

    /// Returns a zero-length span at `offset` — useful for EOF errors.
    pub fn at(file_id: usize, offset: u32) -> Self {
        Self { file_id, start: offset, end: offset }
    }

    /// Returns true when no real location is available (e.g. compiler-internal errors).
    pub fn is_unknown(&self) -> bool {
        self.start == 0 && self.end == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

impl Diagnostic {
    pub fn error(code: impl Into<String>, message: impl Into<String>, span: DiagnosticSpan) -> Self {
        Self {
            severity: Severity::Error,
            code: code.into(),
            message: message.into(),
            primary_span: span,
            secondary_spans: Vec::new(),
            help: None,
        }
    }

    pub fn warning(code: impl Into<String>, message: impl Into<String>, span: DiagnosticSpan) -> Self {
        Self {
            severity: Severity::Warning,
            code: code.into(),
            message: message.into(),
            primary_span: span,
            secondary_spans: Vec::new(),
            help: None,
        }
    }

    /// Attach an additional contextual span with a label.
    pub fn with_secondary(mut self, span: DiagnosticSpan, label: impl Into<String>) -> Self {
        self.secondary_spans.push((span, label.into()));
        self
    }

    /// Attach an actionable help message.
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

/// Convert a `DiagnosticSpan` byte range into a `(line, character)` LSP `Position`.
///
/// `source` is the full text of the file.  Returns `(0, 0)` for unknown spans.
pub fn byte_offset_to_position(source: &str, offset: u32) -> (u32, u32) {
    let offset = offset as usize;
    if offset > source.len() {
        return (0, 0);
    }
    let before = &source[..offset];
    let line = before.bytes().filter(|&b| b == b'\n').count() as u32;
    let col = before.rfind('\n').map_or(before.len(), |p| before.len() - p - 1) as u32;
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_byte_offset_to_position_first_line() {
        let src = "func main(): i32 { return 42; }";
        assert_eq!(byte_offset_to_position(src, 0), (0, 0));
        assert_eq!(byte_offset_to_position(src, 4), (0, 4)); // after "func"
    }

    #[test]
    fn test_byte_offset_to_position_second_line() {
        let src = "line one\nline two\nline three";
        assert_eq!(byte_offset_to_position(src, 9), (1, 0));  // start of "line two"
        assert_eq!(byte_offset_to_position(src, 13), (1, 4)); // "two"
    }

    #[test]
    fn test_diagnostic_builder() {
        let span = DiagnosticSpan::new(0, 10, 20);
        let diag = Diagnostic::error("E001", "undefined variable `x`", span)
            .with_help("declare it with `var x: i32 = ...`");
        assert_eq!(diag.severity, Severity::Error);
        assert_eq!(diag.code, "E001");
        assert!(diag.help.is_some());
        assert!(diag.secondary_spans.is_empty());
    }

    #[test]
    fn test_diagnostic_span_is_unknown() {
        assert!(DiagnosticSpan::default().is_unknown());
        assert!(!DiagnosticSpan::new(0, 0, 5).is_unknown());
    }
}
