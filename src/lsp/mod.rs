use std::error::Error;
use lsp_types::{
    InitializeParams, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, DidOpenTextDocumentParams, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, Diagnostic, DiagnosticSeverity, Range, Position,
    PublishDiagnosticsParams,
};
use lsp_server::{Connection, Message, Request, RequestId, Response};
use tracing::info;

pub fn run() -> Result<(), Box<dyn Error + Sync + Send>> {
    info!("Starting LSP server");

    let (connection, io_threads) = Connection::stdio();

    let server_capabilities = serde_json::to_value(&ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::FULL),
                ..Default::default()
            },
        )),
        ..Default::default()
    })
    .unwrap();

    let initialization_params = connection.initialize(server_capabilities)?;
    main_loop(connection, initialization_params)?;
    io_threads.join()?;

    info!("Shutting down LSP server");
    Ok(())
}

fn main_loop(
    connection: Connection,
    params: serde_json::Value,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    let _params: InitializeParams = serde_json::from_value(params).unwrap();
    info!("Starting main loop");

    let mut qctx = crate::query::QueryContext::new();

    let mut log_f = std::fs::OpenOptions::new().append(true).create(true).open("/tmp/lsp.log").unwrap();
    use std::io::Write;

    for msg in &connection.receiver {
        writeln!(log_f, "Got message: {:?}", msg).unwrap();
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    writeln!(log_f, "Shutdown handled").unwrap();
                    return Ok(());
                }
                writeln!(log_f, "Got request: {:?}", req).unwrap();
                // Dispatch requests
            }
            Message::Response(resp) => {
                writeln!(log_f, "Got response: {:?}", resp).unwrap();
            }
            Message::Notification(not) => {
                writeln!(log_f, "Got notification: {:?}", not).unwrap();
                // Dispatch notifications
                match not.method.as_str() {
                    "textDocument/didOpen" => {
                        let params: DidOpenTextDocumentParams =
                            serde_json::from_value(not.params).unwrap();
                        let uri = params.text_document.uri;
                        let uri_str = uri.to_string();
                        let path_str = if uri_str.starts_with("file://") {
                            uri_str[7..].to_string()
                        } else {
                            uri_str.clone()
                        };
                        let path = std::path::PathBuf::from(path_str);
                        let _file_id = qctx.load_from_source(&path, params.text_document.text);
                        writeln!(log_f, "loaded source, syncing diag...").unwrap();
                        sync_diagnostics(&connection, &mut qctx, &path, uri);
                        writeln!(log_f, "sync_diagnostics done").unwrap();
                    }
                    "textDocument/didChange" => {
                        let mut params: DidChangeTextDocumentParams =
                            serde_json::from_value(not.params).unwrap();
                        let uri = params.text_document.uri;
                        let uri_str = uri.to_string();
                        let path_str = if uri_str.starts_with("file://") {
                            uri_str[7..].to_string()
                        } else {
                            uri_str.clone()
                        };
                        let path = std::path::PathBuf::from(path_str);
                        if let Some(change) = params.content_changes.pop() {
                            // Only support full sync, so change.text is the whole file
                            if let Some(&file_id) = qctx.workspace().files.iter().find_map(|(id, f)| {
                                if f.path == path { Some(id) } else { None }
                            }) {
                                writeln!(log_f, "updating source...").unwrap();
                                qctx.update_file_source(file_id, change.text);
                                writeln!(log_f, "syncing diag...").unwrap();
                                sync_diagnostics(&connection, &mut qctx, &path, uri);
                                writeln!(log_f, "syncing diag done").unwrap();
                            }
                        }
                    }
                    "textDocument/didClose" => {
                        let _params: DidCloseTextDocumentParams =
                            serde_json::from_value(not.params).unwrap();
                        // Optional: remove file from QueryContext if needed, 
                        // though normally we can just leave it cached.
                    }
                    _ => {}
                }
            }
        }
    }
    writeln!(log_f, "querying verify program done!").unwrap();
    Ok(())
}

fn sync_diagnostics(connection: &Connection, qctx: &mut crate::query::QueryContext, path: &std::path::Path, uri: lsp_types::Uri) {
    let mut log_f = std::fs::OpenOptions::new().append(true).create(true).open("/tmp/lsp.log").unwrap();
    use std::io::Write;
    
    let file_id = qctx.workspace().files.iter().find_map(|(id, f)| {
        if f.path == path { Some(*id) } else { None }
    });
    
    let file_id = match file_id {
        Some(id) => id,
        None => return,
    };
    
    let mut diagnostics = Vec::new();
    let source = qctx.workspace().files.get(&file_id).unwrap().source.clone();
    
    if let Some(file_data) = qctx.workspace().files.get(&file_id) {
        for (msg, offset) in &file_data.parse_errors {
            let (line, col) = crate::diagnostics::byte_offset_to_position(&source, *offset);
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position { line, character: col },
                    end: Position { line, character: col },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                code_description: None,
                source: Some("vera-parser".to_string()),
                message: msg.clone(),
                related_information: None,
                tags: None,
                data: None,
            });
        }
    }
    
    writeln!(log_f, "querying hir program...").unwrap();
    let (_, semantic_errors) = qctx.query_hir_program();
    writeln!(log_f, "querying hir program done!").unwrap();
    for err in semantic_errors {
        let span = err.span();
        if span.file_id == file_id && !span.is_unknown() {
            let (start_line, start_col) = crate::diagnostics::byte_offset_to_position(&source, span.start);
            let (end_line, end_col) = crate::diagnostics::byte_offset_to_position(&source, span.end);
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position { line: start_line, character: start_col },
                    end: Position { line: end_line, character: end_col },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                code_description: None,
                source: Some("vera-semantic".to_string()),
                message: err.to_string(),
                related_information: None,
                tags: None,
                data: None,
            });
        }
    }
    
    writeln!(log_f, "querying borrow check...").unwrap();
    let borrow_errors = qctx.query_borrow_check();
    writeln!(log_f, "querying borrow check done!").unwrap();
    for err in borrow_errors {
        let span = err.span();
        if span.file_id == file_id && !span.is_unknown() {
            let (start_line, start_col) = crate::diagnostics::byte_offset_to_position(&source, span.start);
            let (end_line, end_col) = crate::diagnostics::byte_offset_to_position(&source, span.end);
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position { line: start_line, character: start_col },
                    end: Position { line: end_line, character: end_col },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                code_description: None,
                source: Some("vera-borrowck".to_string()),
                message: err.to_string(),
                related_information: None,
                tags: None,
                data: None,
            });
        }
    }
    
    writeln!(log_f, "querying verify program...").unwrap();
    if let Err(err) = qctx.query_verify_program() {
        writeln!(log_f, "verify program returned err").unwrap();
        let span = err.span();
        if span.file_id == file_id && !span.is_unknown() {
            let (start_line, start_col) = crate::diagnostics::byte_offset_to_position(&source, span.start);
            let (end_line, end_col) = crate::diagnostics::byte_offset_to_position(&source, span.end);
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position { line: start_line, character: start_col },
                    end: Position { line: end_line, character: end_col },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                code_description: None,
                source: Some("vera-verify".to_string()),
                message: err.to_string(),
                related_information: None,
                tags: None,
                data: None,
            });
        }
    }
    writeln!(log_f, "querying verify program done!").unwrap();
    
    let not = lsp_server::Notification::new(
        "textDocument/publishDiagnostics".to_string(),
        PublishDiagnosticsParams {
            uri,
            diagnostics,
            version: None,
        },
    );
    let mut log_f = std::fs::OpenOptions::new().append(true).create(true).open("/tmp/lsp.log").unwrap();
    writeln!(log_f, "Sending publishDiagnostics: {:?}", not).unwrap();
    let _ = connection.sender.send(Message::Notification(not));
}
