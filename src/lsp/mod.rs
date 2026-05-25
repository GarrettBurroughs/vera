use std::error::Error;
use lsp_types::{
    InitializeParams, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, DidOpenTextDocumentParams, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, Diagnostic, DiagnosticSeverity, Range, Position,
    PublishDiagnosticsParams, InlayHint, InlayHintLabel, InlayHintParams,
    GotoDefinitionParams, Location, GotoDefinitionResponse, OneOf,
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
        inlay_hint_provider: Some(OneOf::Left(true)),
        definition_provider: Some(OneOf::Left(true)),
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

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                
                if req.method == "textDocument/inlayHint" {
                    let params: InlayHintParams = serde_json::from_value(req.params).unwrap();
                    let uri = params.text_document.uri;
                    let uri_str = uri.to_string();
                    let path_str = if uri_str.starts_with("file://") {
                        uri_str[7..].to_string()
                    } else {
                        uri_str.clone()
                    };
                    let path = std::path::PathBuf::from(path_str);
                    
                    let mut hints = Vec::new();
                    
                    if let Some(&file_id) = qctx.workspace().files.iter().find_map(|(id, f)| {
                        if f.path == path { Some(id) } else { None }
                    }) {
                        // Ensure HIR is fresh so we can iterate functions
                        qctx.query_hir_program();
                        
                        let (hir_program, _) = qctx.query_hir_program();
                        let funcs: Vec<_> = hir_program.functions.iter()
                            .filter(|f| f.span.file_id == file_id && f.body.is_some())
                            .map(|f| (f.name.clone(), f.span))
                            .collect();
                            
                        let source = qctx.workspace().files.get(&file_id).unwrap().source.clone();
                            
                        for (func_name, span) in funcs {
                            if !span.is_unknown() {
                                let (line, col) = crate::diagnostics::byte_offset_to_position(&source, span.start);
                                
                                let status_text = match qctx.query_verify_function(&func_name) {
                                    Ok(_) => "✓ verified",
                                    Err(_) => "✗ failed",
                                };
                                
                                hints.push(InlayHint {
                                    position: Position { line, character: col },
                                    label: InlayHintLabel::String(status_text.to_string()),
                                    kind: None,
                                    text_edits: None,
                                    tooltip: None,
                                    padding_left: Some(true),
                                    padding_right: Some(true),
                                    data: None,
                                });
                            }
                        }
                    }
                    
                    let result = serde_json::to_value(hints).unwrap();
                    let resp = Response { id: req.id, result: Some(result), error: None };
                    connection.sender.send(Message::Response(resp)).unwrap();
                } else if req.method == "textDocument/definition" {
                    let params: GotoDefinitionParams = serde_json::from_value(req.params).unwrap();
                    let uri = params.text_document_position_params.text_document.uri;
                    let uri_str = uri.to_string();
                    let path_str = if uri_str.starts_with("file://") {
                        uri_str[7..].to_string()
                    } else {
                        uri_str.clone()
                    };
                    let path = std::path::PathBuf::from(path_str);
                    
                    let mut result = serde_json::Value::Null;
                    
                    if let Some(&file_id) = qctx.workspace().files.iter().find_map(|(id, f)| {
                        if f.path == path { Some(id) } else { None }
                    }) {
                        let source = qctx.workspace().files.get(&file_id).unwrap().source.clone();
                        let pos = params.text_document_position_params.position;
                        if let Some(byte_offset) = crate::diagnostics::position_to_byte_offset(&source, pos.line, pos.character) {
                            if let Some(def_span) = qctx.query_definition(file_id, byte_offset) {
                                if !def_span.is_unknown() {
                                    let def_file_id = def_span.file_id;
                                    if let Some(def_file) = qctx.workspace().files.get(&def_file_id) {
                                        let def_source = def_file.source.clone();
                                        let (start_line, start_col) = crate::diagnostics::byte_offset_to_position(&def_source, def_span.start);
                                        let (end_line, end_col) = crate::diagnostics::byte_offset_to_position(&def_source, def_span.end);
                                        let def_uri = format!("file://{}", def_file.path.display()).parse().unwrap();
                                        
                                        let location = Location {
                                            uri: def_uri,
                                            range: Range {
                                                start: Position { line: start_line, character: start_col },
                                                end: Position { line: end_line, character: end_col },
                                            },
                                        };
                                        result = serde_json::to_value(location).unwrap();
                                    }
                                }
                            }
                        }
                    }
                    
                    let resp = Response { id: req.id, result: Some(result), error: None };
                    connection.sender.send(Message::Response(resp)).unwrap();
                }
            }
            Message::Response(_resp) => {
            }
            Message::Notification(not) => {
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
                        sync_diagnostics(&connection, &mut qctx, &path, uri);
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
                                qctx.update_file_source(file_id, change.text);
                                sync_diagnostics(&connection, &mut qctx, &path, uri);
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
    Ok(())
}

fn sync_diagnostics(connection: &Connection, qctx: &mut crate::query::QueryContext, path: &std::path::Path, uri: lsp_types::Uri) {
    
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
    
    let (_, semantic_errors) = qctx.query_hir_program();
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
    
    let borrow_errors = qctx.query_borrow_check();
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
    
    if let Err(err) = qctx.query_verify_program() {
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
    
    let not = lsp_server::Notification::new(
        "textDocument/publishDiagnostics".to_string(),
        PublishDiagnosticsParams {
            uri,
            diagnostics,
            version: None,
        },
    );
    let _ = connection.sender.send(Message::Notification(not));
}
