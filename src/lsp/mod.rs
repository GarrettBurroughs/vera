use std::error::Error;
use lsp_types::{
    InitializeParams, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, DidOpenTextDocumentParams, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, Diagnostic, DiagnosticSeverity, Range, Position,
    PublishDiagnosticsParams, InlayHint, InlayHintLabel, InlayHintParams,
    GotoDefinitionParams, Location, GotoDefinitionResponse, OneOf,
    HoverProviderCapability, HoverParams, Hover, HoverContents, MarkedString,
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
            }
        )),
        inlay_hint_provider: Some(OneOf::Left(true)),
        definition_provider: Some(OneOf::Left(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        ..Default::default()
    })
    .unwrap();

    let initialization_params = connection.initialize(server_capabilities)?;
    main_loop(connection, initialization_params)?;
    io_threads.join()?;

    info!("Shutting down LSP server");
    Ok(())
}

struct VerifyRequest {
    files: std::collections::BTreeMap<std::path::PathBuf, String>,
}

struct VerifyResponse {
    diagnostics_by_file: std::collections::BTreeMap<std::path::PathBuf, Vec<Diagnostic>>,
    status: std::collections::BTreeMap<String, Result<(), crate::verification::VerificationError>>,
}

fn verification_worker(
    req_rx: crossbeam_channel::Receiver<VerifyRequest>,
    resp_tx: crossbeam_channel::Sender<VerifyResponse>,
) {
    let mut qctx = crate::query::QueryContext::new();

    while let Ok(mut req) = req_rx.recv() {
        while let Ok(latest_req) = req_rx.try_recv() {
            req = latest_req;
        }

        for (path, source) in req.files {
            qctx.load_from_source(&path, source);
        }

        let mut diagnostics_by_file: std::collections::BTreeMap<std::path::PathBuf, Vec<Diagnostic>> = std::collections::BTreeMap::new();
        let mut status = std::collections::BTreeMap::new();

        let all_results = qctx.query_verify_all();
        
        for (func_name, res) in all_results {
            status.insert(func_name.clone(), res.clone());
            if let Err(err) = res {
                let span = err.span();
                if let Some(file_data) = qctx.workspace().files.get(&span.file_id) {
                    let source = file_data.source.clone();
                    let path = file_data.path.clone();
                    
                    if !span.is_unknown() {
                        let (start_line, start_col) = crate::diagnostics::byte_offset_to_position(&source, span.start);
                        let (end_line, end_col) = crate::diagnostics::byte_offset_to_position(&source, span.end);
                        
                        let diag = Diagnostic {
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
                        };
                        diagnostics_by_file.entry(path).or_default().push(diag);
                    }
                }
            }
        }

        let _ = resp_tx.send(VerifyResponse {
            diagnostics_by_file,
            status,
        });
    }
}

fn main_loop(
    connection: Connection,
    params: serde_json::Value,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    let _params: InitializeParams = serde_json::from_value(params).unwrap();
    info!("Starting main loop");

    let mut qctx = crate::query::QueryContext::new();

    let (req_tx, req_rx) = crossbeam_channel::unbounded::<VerifyRequest>();
    let (resp_tx, resp_rx) = crossbeam_channel::unbounded::<VerifyResponse>();

    std::thread::spawn(move || {
        verification_worker(req_rx, resp_tx);
    });

    let mut latest_fast_diagnostics: std::collections::BTreeMap<std::path::PathBuf, Vec<Diagnostic>> = std::collections::BTreeMap::new();
    let mut verification_status: std::collections::BTreeMap<String, Result<(), crate::verification::VerificationError>> = std::collections::BTreeMap::new();

    loop {
        crossbeam_channel::select! {
            recv(connection.receiver) -> msg => {
                let msg = match msg {
                    Ok(msg) => msg,
                    Err(_) => break,
                };
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
                                qctx.query_hir_program();
                                
                                let (hir_program, _) = qctx.query_hir_program();
                                let funcs: Vec<_> = hir_program.functions.iter()
                                    .filter(|f| f.span.file_id == file_id && f.body.is_some())
                                    .map(|f| (f.name.clone(), f.span))
                                    .collect();
                                    
                                let source = qctx.workspace().files.get(&file_id).unwrap().source.clone();
                                    
                                for (func_name, span) in funcs {
                                    if !span.is_unknown() {
                                        if let Some(res) = verification_status.get(&func_name) {
                                            let (line, col) = crate::diagnostics::byte_offset_to_position(&source, span.start);
                                            let status_text = match res {
                                                Ok(_) => "✓ verified".to_string(),
                                                Err(crate::verification::VerificationError::ProofFailed { counterexample: Some(model), .. }) => {
                                                    let vars: Vec<_> = model.iter().map(|(k, v)| format!("{} = {}", k, v)).collect();
                                                    format!("✗ failed [Counterexample: {}]", vars.join(", "))
                                                },
                                                Err(_) => "✗ failed".to_string(),
                                            };
                                            hints.push(InlayHint {
                                                position: Position { line, character: col },
                                                label: InlayHintLabel::String(status_text),
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
                        } else if req.method == "textDocument/hover" {
                            let params: HoverParams = serde_json::from_value(req.params).unwrap();
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
                                    if let Some((span, ty)) = qctx.query_hover(file_id, byte_offset) {
                                        let (start_line, start_col) = crate::diagnostics::byte_offset_to_position(&source, span.start);
                                        let (end_line, end_col) = crate::diagnostics::byte_offset_to_position(&source, span.end);
                                        
                                        let hover = Hover {
                                            contents: HoverContents::Scalar(MarkedString::LanguageString(lsp_types::LanguageString {
                                                language: "vera".to_string(),
                                                value: format!("{}", ty),
                                            })),
                                            range: Some(Range {
                                                start: Position { line: start_line, character: start_col },
                                                end: Position { line: end_line, character: end_col },
                                            }),
                                        };
                                        result = serde_json::to_value(hover).unwrap();
                                    }
                                }
                            }
                            
                            let resp = Response { id: req.id, result: Some(result), error: None };
                            connection.sender.send(Message::Response(resp)).unwrap();
                        }
                    }
                    Message::Response(_resp) => {}
                    Message::Notification(not) => {
                        let mut did_change = false;
                        let mut trigger_path = None;
                        let mut trigger_uri = None;
                        
                        match not.method.as_str() {
                            "textDocument/didOpen" => {
                                let params: DidOpenTextDocumentParams = serde_json::from_value(not.params).unwrap();
                                let uri = params.text_document.uri;
                                let uri_str = uri.to_string();
                                let path_str = if uri_str.starts_with("file://") { uri_str[7..].to_string() } else { uri_str.clone() };
                                let path = std::path::PathBuf::from(path_str);
                                qctx.load_from_source(&path, params.text_document.text);
                                trigger_path = Some(path);
                                trigger_uri = Some(uri);
                                did_change = true;
                            }
                            "textDocument/didChange" => {
                                let mut params: DidChangeTextDocumentParams = serde_json::from_value(not.params).unwrap();
                                let uri = params.text_document.uri;
                                let uri_str = uri.to_string();
                                let path_str = if uri_str.starts_with("file://") { uri_str[7..].to_string() } else { uri_str.clone() };
                                let path = std::path::PathBuf::from(path_str);
                                if let Some(change) = params.content_changes.pop() {
                                    if let Some(&file_id) = qctx.workspace().files.iter().find_map(|(id, f)| {
                                        if f.path == path { Some(id) } else { None }
                                    }) {
                                        qctx.update_file_source(file_id, change.text);
                                        trigger_path = Some(path);
                                        trigger_uri = Some(uri);
                                        did_change = true;
                                    }
                                }
                            }
                            "textDocument/didClose" => {}
                            _ => {}
                        }
                        
                        if did_change {
                            if let (Some(path), Some(uri)) = (trigger_path, trigger_uri) {
                                let fast_diags = sync_diagnostics(&mut qctx, &path);
                                latest_fast_diagnostics.insert(path.clone(), fast_diags.clone());
                                
                                let not = lsp_server::Notification::new(
                                    "textDocument/publishDiagnostics".to_string(),
                                    PublishDiagnosticsParams {
                                        uri,
                                        diagnostics: fast_diags,
                                        version: None,
                                    },
                                );
                                let _ = connection.sender.send(Message::Notification(not));
                                
                                let mut files_map = std::collections::BTreeMap::new();
                                for (_, file_data) in qctx.workspace().files.iter() {
                                    files_map.insert(file_data.path.clone(), file_data.source.clone());
                                }
                                let _ = req_tx.send(VerifyRequest { files: files_map });
                            }
                        }
                    }
                }
            }
            recv(resp_rx) -> resp => {
                let resp = match resp {
                    Ok(r) => r,
                    Err(_) => break,
                };
                
                verification_status = resp.status;
                
                for (path, fast_diags) in &latest_fast_diagnostics {
                    let mut all_diags = fast_diags.clone();
                    if let Some(verify_diags) = resp.diagnostics_by_file.get(path) {
                        all_diags.extend(verify_diags.iter().cloned());
                    }
                    
                    let uri_str = format!("file://{}", path.display());
                    let uri: lsp_types::Uri = uri_str.parse().unwrap();
                    let not = lsp_server::Notification::new(
                        "textDocument/publishDiagnostics".to_string(),
                        PublishDiagnosticsParams {
                            uri,
                            diagnostics: all_diags,
                            version: None,
                        },
                    );
                    let _ = connection.sender.send(Message::Notification(not));
                }
            }
        }
    }
    
    Ok(())
}

fn sync_diagnostics(qctx: &mut crate::query::QueryContext, path: &std::path::Path) -> Vec<Diagnostic> {
    
    let file_id = qctx.workspace().files.iter().find_map(|(id, f)| {
        if f.path == path { Some(*id) } else { None }
    });
    
    let file_id = match file_id {
        Some(id) => id,
        None => return Vec::new(),
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
    
    diagnostics
}
