use std::error::Error;
use lsp_types::{
    InitializeParams, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, DidOpenTextDocumentParams, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams,
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

    for msg in &connection.receiver {
        info!("Got message: {:?}", msg);
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                info!("Got request: {:?}", req);
                // Dispatch requests
            }
            Message::Response(resp) => {
                info!("Got response: {:?}", resp);
            }
            Message::Notification(not) => {
                info!("Got notification: {:?}", not);
                // Dispatch notifications
                match not.method.as_str() {
                    "textDocument/didOpen" => {
                        let params: DidOpenTextDocumentParams =
                            serde_json::from_value(not.params).unwrap();
                        let uri_str = params.text_document.uri.to_string();
                        let path_str = if uri_str.starts_with("file://") {
                            uri_str[7..].to_string()
                        } else {
                            uri_str
                        };
                        let path = std::path::PathBuf::from(path_str);
                        let _file_id = qctx.load_from_source(&path, params.text_document.text);
                    }
                    "textDocument/didChange" => {
                        let mut params: DidChangeTextDocumentParams =
                            serde_json::from_value(not.params).unwrap();
                        let uri_str = params.text_document.uri.to_string();
                        let path_str = if uri_str.starts_with("file://") {
                            uri_str[7..].to_string()
                        } else {
                            uri_str
                        };
                        let path = std::path::PathBuf::from(path_str);
                        if let Some(change) = params.content_changes.pop() {
                            // Only support full sync, so change.text is the whole file
                            if let Some(&file_id) = qctx.workspace().files.iter().find_map(|(id, f)| {
                                if f.path == path { Some(id) } else { None }
                            }) {
                                qctx.update_file_source(file_id, change.text);
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
