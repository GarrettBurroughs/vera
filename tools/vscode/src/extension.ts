import * as path from 'path';
import * as fs from 'fs';
import * as vscode from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    Executable
} from 'vscode-languageclient/node';

let client: LanguageClient;

export function activate(context: vscode.ExtensionContext) {
    const workspaceFolders = vscode.workspace.workspaceFolders;
    if (!workspaceFolders) {
        return;
    }
    
    const rootPath = workspaceFolders[0].uri.fsPath;
    
    // Fallbacks for the executable:
    // 1. target/debug/verify
    // 2. target/release/verify
    // 3. 'verify' in PATH
    let command = 'verify';
    const debugPath = path.join(rootPath, 'target', 'debug', 'verify');
    const releasePath = path.join(rootPath, 'target', 'release', 'verify');
    
    if (fs.existsSync(debugPath)) {
        command = debugPath;
    } else if (fs.existsSync(releasePath)) {
        command = releasePath;
    }
    
    const run: Executable = {
        command,
        args: ['lsp'],
        options: { env: { ...process.env, RUST_BACKTRACE: "1" } }
    };
    
    const serverOptions: ServerOptions = {
        run,
        debug: run
    };
    
    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: 'file', language: 'vera' }],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/.clientrc')
        }
    };
    
    client = new LanguageClient(
        'veraLanguageServer',
        'Vera Language Server',
        serverOptions,
        clientOptions
    );
    
    client.start();
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
