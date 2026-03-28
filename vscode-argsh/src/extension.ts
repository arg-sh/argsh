import * as path from 'path';
import * as vscode from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext) {
  const config = vscode.workspace.getConfiguration('argsh');

  if (!config.get<boolean>('lsp.enabled', true)) {
    return;
  }

  // Find the LSP binary
  let serverPath = config.get<string>('lsp.path', '');
  if (!serverPath) {
    // Try common locations
    const candidates = [
      path.join(context.extensionPath, 'bin', 'argsh-lsp'),
      'argsh-lsp', // PATH lookup
    ];
    serverPath = candidates[0]; // TODO: check existence
  }

  const serverOptions: ServerOptions = {
    run: { command: serverPath, transport: TransportKind.stdio },
    debug: { command: serverPath, transport: TransportKind.stdio },
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: 'file', language: 'shellscript' },
    ],
  };

  client = new LanguageClient(
    'argsh-lsp',
    'argsh Language Server',
    serverOptions,
    clientOptions
  );

  client.start();
  context.subscriptions.push({
    dispose: () => client?.stop(),
  });

  // Register preview command
  const previewCmd = vscode.commands.registerCommand('argsh.showPreview', async () => {
    const editor = vscode.window.activeTextEditor;
    if (!editor || !client) return;

    const uri = editor.document.uri.toString();
    const html = await client.sendRequest('workspace/executeCommand', {
      command: 'argsh.preview',
      arguments: [uri],
    });

    if (typeof html === 'string') {
      const panel = vscode.window.createWebviewPanel(
        'argshPreview',
        'argsh Preview',
        vscode.ViewColumn.Beside,
        { enableScripts: false }
      );
      panel.webview.html = html;
    }
  });
  context.subscriptions.push(previewCmd);

  // Restart server command
  const restartCmd = vscode.commands.registerCommand('argsh.restartServer', async () => {
    if (client) {
      await client.stop();
      await client.start();
      vscode.window.showInformationMessage('argsh Language Server restarted');
    }
  });
  context.subscriptions.push(restartCmd);

  // Show help for current function
  const helpCmd = vscode.commands.registerCommand('argsh.showHelp', async () => {
    const editor = vscode.window.activeTextEditor;
    if (!editor || !client) return;

    // Trigger hover at cursor position to show help
    const position = editor.selection.active;
    const hover = await client.sendRequest('textDocument/hover', {
      textDocument: { uri: editor.document.uri.toString() },
      position: { line: position.line, character: position.character }
    }) as any;

    if (hover && hover.contents) {
      const content = typeof hover.contents === 'string'
        ? hover.contents
        : hover.contents.value || JSON.stringify(hover.contents);
      vscode.window.showInformationMessage(content.substring(0, 500));
    } else {
      vscode.window.showInformationMessage('No argsh info at cursor position');
    }
  });
  context.subscriptions.push(helpCmd);

  // Validate script command
  const validateCmd = vscode.commands.registerCommand('argsh.validateScript', async () => {
    const editor = vscode.window.activeTextEditor;
    if (!editor || !client) return;

    // Force re-analysis by sending didChange
    const uri = editor.document.uri.toString();
    const text = editor.document.getText();
    await client.sendNotification('textDocument/didChange', {
      textDocument: { uri, version: editor.document.version },
      contentChanges: [{ text }]
    });
    vscode.window.showInformationMessage('argsh: Script validation triggered');
  });
  context.subscriptions.push(validateCmd);
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}
