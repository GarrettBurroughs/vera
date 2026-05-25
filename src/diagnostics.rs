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

/// Convert a `(line, character)` LSP `Position` to a byte offset.
pub fn position_to_byte_offset(source: &str, line: u32, col: u32) -> Option<u32> {
    let mut current_line = 0;
    let mut current_offset = 0;
    
    for (i, b) in source.bytes().enumerate() {
        if current_line == line {
            if (i - current_offset) as u32 == col {
                return Some(i as u32);
            }
        }
        if b == b'\n' {
            current_line += 1;
            current_offset = i + 1;
        }
    }
    
    if current_line == line && (source.len() - current_offset) as u32 == col {
        return Some(source.len() as u32);
    }
    
    None
}

/// Render a `Diagnostic` to a human-readable string.
///
/// Set `color = true` to emit ANSI escape sequences for terminal output.
/// Call `render_diagnostic_cli` instead for automatic colour detection.
pub fn render_diagnostic(diag: &Diagnostic, file_path: &str, source: &str, color: bool) -> String {
    let c = |code: &str, s: &str| -> String {
        if color { format!("\x1b[{}m{}\x1b[0m", code, s) } else { s.to_string() }
    };

    let severity_str = match diag.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
        Severity::Hint => "hint",
    };
    let sev_color = match diag.severity {
        Severity::Error => "1;31",
        Severity::Warning => "1;33",
        _ => "1;36",
    };

    let mut out = String::new();

    // "error[E001]: message"
    out += &format!("{}{}: {}\n",
        c(sev_color, severity_str),
        c(sev_color, &format!("[{}]", diag.code)),
        c("1", &diag.message),
    );

    if diag.primary_span.is_unknown() {
        return out;
    }

    let (line, col) = byte_offset_to_position(source, diag.primary_span.start);
    let line_no = line + 1;
    let col_no = col + 1;
    let gutter = line_no.to_string().len();

    // " --> file:line:col"
    out += &format!(" {} {}:{}:{}\n", c("36", "-->"), file_path, line_no, col_no);

    let source_line = source.lines().nth(line as usize).unwrap_or("");

    // blank gutter line
    out += &format!("{:gutter$} {}\n", "", c("36", "|"));

    // "N | source_line"
    out += &format!("{} {} {}\n", c("36", &line_no.to_string()), c("36", "|"), source_line);

    // underline: "  |    ^^^^ message"
    let underline_len = if !diag.primary_span.is_unknown()
        && diag.primary_span.end > diag.primary_span.start
        && byte_offset_to_position(source, diag.primary_span.end).0 == line
    {
        let end_col = byte_offset_to_position(source, diag.primary_span.end).1;
        (end_col.saturating_sub(col)).max(1) as usize
    } else {
        1
    };
    let underline = "^".repeat(underline_len);
    let spaces = " ".repeat(col as usize);
    out += &format!("{:gutter$} {} {}{} {}\n",
        "", c("36", "|"), spaces, c(sev_color, &underline), c(sev_color, &diag.message),
    );

    if let Some(ref help) = diag.help {
        out += &format!("{:gutter$} {} help: {}\n", "", c("36", "="), help);
    }

    out
}

/// Render a `Diagnostic` for terminal output, auto-detecting colour support.
///
/// Colour is suppressed when the `NO_COLOR` environment variable is set.
pub fn render_diagnostic_cli(diag: &Diagnostic, file_path: &str, source: &str) -> String {
    render_diagnostic(diag, file_path, source, std::env::var_os("NO_COLOR").is_none())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_diagnostic_basic() {
        // "x" is at byte 30 (line 2, col 12 in 1-indexed)
        let source = "func main(): i32 {\n    return x + 1;\n}";
        let span = DiagnosticSpan::new(0, 30, 31);
        let diag = Diagnostic::error("E001", "undefined variable `x`", span);
        let rendered = render_diagnostic(&diag, "test.vera", source, false);

        assert!(rendered.contains("error[E001]"), "code missing: {rendered}");
        assert!(rendered.contains("undefined variable"), "message missing: {rendered}");
        assert!(rendered.contains("test.vera"), "filename missing: {rendered}");
        assert!(rendered.contains("2:12"), "line:col missing: {rendered}");
        assert!(rendered.contains("return x + 1"), "source line missing: {rendered}");
        assert!(rendered.contains('^'), "caret missing: {rendered}");
    }

    #[test]
    fn test_render_diagnostic_unknown_span() {
        let source = "func main(): i32 { return 0; }";
        let diag = Diagnostic::error("E002", "some error", DiagnosticSpan::default());
        let rendered = render_diagnostic(&diag, "test.vera", source, false);

        assert!(rendered.contains("error[E002]"), "code missing: {rendered}");
        assert!(rendered.contains("some error"), "message missing: {rendered}");
        assert!(!rendered.contains("-->"), "should not show location for unknown span: {rendered}");
    }

    #[test]
    fn test_render_diagnostic_help() {
        let span = DiagnosticSpan::new(0, 0, 4);
        let source = "func main(): i32 { return 0; }";
        let diag = Diagnostic::error("E003", "bad function", span)
            .with_help("add a return statement");
        let rendered = render_diagnostic(&diag, "test.vera", source, false);

        assert!(rendered.contains("help: add a return statement"), "help missing: {rendered}");
    }

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
