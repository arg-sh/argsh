import * as path from 'path';
import * as vscode from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

/** Safe command registration — ignores "already exists" errors from extension host restarts. */
function registerCmd(
  context: vscode.ExtensionContext,
  id: string,
  handler: (...args: any[]) => any,
) {
  try {
    context.subscriptions.push(vscode.commands.registerCommand(id, handler));
  } catch {
    // Command already registered
  }
}

export function activate(context: vscode.ExtensionContext) {
  const config = vscode.workspace.getConfiguration('argsh');

  if (!config.get<boolean>('lsp.enabled', true)) {
    return;
  }

  // Find the LSP binary
  let serverPath = config.get<string>('lsp.path', '');
  if (!serverPath) {
    const candidates = [
      path.join(context.extensionPath, 'bin', 'argsh-lsp'),
      'argsh-lsp',
    ];
    serverPath = candidates[0];
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

  client.start().catch((err: Error) => {
    vscode.window.showErrorMessage(`argsh LSP failed to start: ${err.message}`);
  });
  context.subscriptions.push({ dispose: () => client?.stop() });

  // --- Commands ---

  registerCmd(context, 'argsh.showPreview', async () => {
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

  registerCmd(context, 'argsh.restartServer', async () => {
    if (client) {
      await client.restart();
      vscode.window.showInformationMessage('argsh Language Server restarted');
    }
  });

  registerCmd(context, 'argsh.showHelp', async () => {
    const editor = vscode.window.activeTextEditor;
    if (!editor || !client) return;

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

  registerCmd(context, 'argsh.validateScript', async () => {
    const editor = vscode.window.activeTextEditor;
    if (!editor || !client) return;

    const uri = editor.document.uri.toString();
    const text = editor.document.getText();
    await client.sendNotification('textDocument/didChange', {
      textDocument: { uri, version: editor.document.version },
      contentChanges: [{ text }]
    });
    vscode.window.showInformationMessage('argsh: Script validation triggered');
  });

  // Export commands
  const makeExportHandler = (cmdId: string, title: string, lang: string) => {
    return async () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor || !client) return;

      const uri = editor.document.uri.toString();
      const result = await client.sendRequest('workspace/executeCommand', {
        command: cmdId,
        arguments: [uri],
      });

      if (typeof result === 'string' && result.length > 0) {
        const doc = await vscode.workspace.openTextDocument({
          content: result,
          language: lang,
        });
        await vscode.window.showTextDocument(doc, vscode.ViewColumn.Beside);
      } else {
        vscode.window.showInformationMessage(`argsh: No ${title} data available`);
      }
    };
  };

  registerCmd(context, 'argsh.exportMcpJson', makeExportHandler('argsh.exportMcpJson', 'MCP JSON', 'json'));
  registerCmd(context, 'argsh.exportYaml', makeExportHandler('argsh.exportYaml', 'YAML', 'yaml'));
  registerCmd(context, 'argsh.exportJson', makeExportHandler('argsh.exportJson', 'JSON', 'json'));
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}
