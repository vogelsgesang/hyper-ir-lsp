import type {
  ExtensionContext
} from 'vscode';

import {
  type Executable,
  LanguageClient,
  type LanguageClientOptions,
  type ServerOptions
} from 'vscode-languageclient/node';

let client: LanguageClient | null = null;

export async function activate (context: ExtensionContext) {
  const command = process.env.SERVER_PATH ?? context.asAbsolutePath('hyper-ir-lsp');
  const run: Executable = {
    command,
    options: {
      env: {
        ...process.env,
        // eslint-disable-next-line @typescript-eslint/naming-convention
        RUST_LOG: 'debug',
        // eslint-disable-next-line @typescript-eslint/naming-convention
        RUST_BACKTRACE: '1'
      }
    }
  };
  const serverOptions: ServerOptions = {
    run,
    debug: run
  };
  // If the extension is launched in debug mode then the debug server options are used
  // Otherwise the run options are used
  // Options to control the language client
  const clientOptions: LanguageClientOptions = {
    // Register the server for plain text documents
    documentSelector: [{ language: 'hir' }]
  };

  // Create the language client and start the client.
  client = new LanguageClient('hyper-ir-lsp', 'Hyper IR language server', serverOptions, clientOptions);
  void client.start();
}

export function deactivate (): Thenable<void> | undefined {
  if (client === null) {
    return undefined;
  }
  return client.stop();
}
