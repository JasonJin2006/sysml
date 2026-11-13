import * as path from 'path';
import * as fs from 'fs';
import * as vscode from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

/**
 * Where the server is.
 *
 * Three places, in order: the setting, `PATH`, and a build of this repository beside the
 * extension. The third is what makes a clone work without installing anything — the person ran
 * `cargo build --release` and the extension finds what it produced.
 */
function findServer(context: vscode.ExtensionContext): string | null {
  const configured = vscode.workspace.getConfiguration('sysml').get<string>('server.path', '');
  if (configured) {
    return configured;
  }
  const exe = process.platform === 'win32' ? 'sysml-lsp.exe' : 'sysml-lsp';
  // The extension lives at core/editors/vscode; the binary at core/target/release.
  const beside = path.resolve(context.extensionPath, '..', '..', 'target', 'release', exe);
  return fs.existsSync(beside) ? beside : null;
}

export function activate(context: vscode.ExtensionContext): void {
  const command = findServer(context);
  if (!command) {
    vscode.window.showWarningMessage(
      'sysml-lsp was not found. Set `sysml.server.path`, or build it with ' +
        '`cargo build --release --manifest-path core/Cargo.toml`.',
    );
    return;
  }

  const serverOptions: ServerOptions = {
    run: { command, transport: TransportKind.stdio },
    debug: { command, transport: TransportKind.stdio },
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: 'file', language: 'sysml' }],
    synchronize: {
      // Only the open documents are sent. The server checks what it is given; watching the
      // workspace would make it re-check files nobody is looking at.
      fileEvents: [],
    },
  };

  client = new LanguageClient('sysml', 'SysML v2', serverOptions, clientOptions);
  client.start().catch((error: unknown) => {
    vscode.window.showErrorMessage(
      `SysML language server failed to start: ${error instanceof Error ? error.message : String(error)}`,
    );
  });
  context.subscriptions.push({ dispose: () => void client?.stop() });
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}
