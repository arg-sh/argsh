import * as path from 'path';
import * as vscode from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

// --- Command Tree View ---

interface CommandTreeItem {
  name: string;
  kind: number; // vscode.SymbolKind
  detail?: string;
  range: { start: { line: number }; end: { line: number } };
  children?: CommandTreeItem[];
}

class ArgshCommandTreeProvider implements vscode.TreeDataProvider<CommandTreeItem> {
  private _onDidChangeTreeData = new vscode.EventEmitter<CommandTreeItem | undefined>();
  readonly onDidChangeTreeData = this._onDidChangeTreeData.event;
  private items: CommandTreeItem[] = [];
  private uri: string = '';
  private activeItem: CommandTreeItem | undefined;

  refresh(symbols: CommandTreeItem[], uri: string) {
    this.items = symbols;
    this.uri = uri;
    this._onDidChangeTreeData.fire(undefined);
    vscode.commands.executeCommand('setContext', 'argsh.hasCommands', symbols.length > 0);
  }

  highlightFunction(line: number) {
    this.activeItem = this.findByLine(this.items, line);
    this._onDidChangeTreeData.fire(undefined);
  }

  private findByLine(items: CommandTreeItem[], line: number): CommandTreeItem | undefined {
    for (const item of items) {
      if (line >= item.range.start.line && line <= item.range.end.line) {
        // Check children first for more specific match
        if (item.children) {
          const child = this.findByLine(item.children, line);
          if (child) return child;
        }
        return item;
      }
    }
    return undefined;
  }

  getTreeItem(element: CommandTreeItem): vscode.TreeItem {
    const isFunction = element.kind === 12; // SymbolKind.Function
    const hasChildren = element.children && element.children.length > 0;
    const collapsible = hasChildren
      ? vscode.TreeItemCollapsibleState.Expanded
      : isFunction
        ? vscode.TreeItemCollapsibleState.None
        : vscode.TreeItemCollapsibleState.None;

    const item = new vscode.TreeItem(element.name, collapsible);

    if (isFunction) {
      item.iconPath = hasChildren
        ? new vscode.ThemeIcon('git-merge')
        : new vscode.ThemeIcon('terminal');
      item.description = element.detail || '';
      item.command = {
        command: 'argsh.goToSymbol',
        title: 'Go to',
        arguments: [element],
      };
    } else if (element.kind === 7) { // Property (args entry)
      item.iconPath = new vscode.ThemeIcon('symbol-field');
      item.description = element.detail || '';
    } else if (element.kind === 10) { // Enum (usage entry)
      item.iconPath = new vscode.ThemeIcon('symbol-enum');
      item.description = element.detail || '';
    }

    // Highlight active function
    if (this.activeItem && element.name === this.activeItem.name) {
      item.iconPath = new vscode.ThemeIcon(
        hasChildren ? 'git-merge' : 'terminal',
        new vscode.ThemeColor('charts.green')
      );
    }

    return item;
  }

  getChildren(element?: CommandTreeItem): CommandTreeItem[] {
    if (!element) return this.items;
    return element.children || [];
  }
}

// --- Extension activation ---

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

  client.start();
  context.subscriptions.push({ dispose: () => client?.stop() });

  // --- Command Tree ---

  const treeProvider = new ArgshCommandTreeProvider();
  const treeView = vscode.window.createTreeView('argsh.commandTree', {
    treeDataProvider: treeProvider,
    showCollapseAll: true,
  });
  context.subscriptions.push(treeView);

  // Update tree when active editor changes or document is saved
  const updateTree = async () => {
    const editor = vscode.window.activeTextEditor;
    if (!editor || !client || editor.document.languageId !== 'shellscript') {
      treeProvider.refresh([], '');
      return;
    }

    try {
      const symbols = await client.sendRequest('textDocument/documentSymbol', {
        textDocument: { uri: editor.document.uri.toString() },
      }) as CommandTreeItem[] | null;

      if (symbols && Array.isArray(symbols)) {
        treeProvider.refresh(symbols, editor.document.uri.toString());
      } else {
        treeProvider.refresh([], '');
      }
    } catch {
      // LSP not ready yet
    }
  };

  // Highlight current function when cursor moves
  const updateHighlight = () => {
    const editor = vscode.window.activeTextEditor;
    if (editor && editor.document.languageId === 'shellscript') {
      treeProvider.highlightFunction(editor.selection.active.line);
    }
  };

  // Initial update after a short delay (LSP needs time to start)
  setTimeout(() => updateTree(), 1500);

  context.subscriptions.push(
    vscode.window.onDidChangeActiveTextEditor(() => { updateTree(); }),
    vscode.workspace.onDidSaveTextDocument(() => { updateTree(); }),
    vscode.window.onDidChangeTextEditorSelection(() => { updateHighlight(); }),
  );

  // Go to symbol command (used by tree item clicks)
  const goToSymbolCmd = vscode.commands.registerCommand('argsh.goToSymbol', (item: CommandTreeItem) => {
    const editor = vscode.window.activeTextEditor;
    if (!editor || !item.range) return;

    const pos = new vscode.Position(item.range.start.line, 0);
    editor.selection = new vscode.Selection(pos, pos);
    editor.revealRange(new vscode.Range(pos, pos), vscode.TextEditorRevealType.InCenter);
  });
  context.subscriptions.push(goToSymbolCmd);

  // --- Commands ---

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

  const restartCmd = vscode.commands.registerCommand('argsh.restartServer', async () => {
    if (client) {
      await client.stop();
      await client.start();
      vscode.window.showInformationMessage('argsh Language Server restarted');
    }
  });
  context.subscriptions.push(restartCmd);

  const helpCmd = vscode.commands.registerCommand('argsh.showHelp', async () => {
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
  context.subscriptions.push(helpCmd);

  const validateCmd = vscode.commands.registerCommand('argsh.validateScript', async () => {
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
  context.subscriptions.push(validateCmd);

  const exportMcpCmd = vscode.commands.registerCommand('argsh.exportMcpJson', async () => {
    const editor = vscode.window.activeTextEditor;
    if (!editor || !client) return;
    const result = await client.sendRequest('workspace/executeCommand', {
      command: 'argsh.exportMcpJson',
      arguments: [editor.document.uri.toString()],
    });
    if (typeof result === 'string' && result.length > 0) {
      const doc = await vscode.workspace.openTextDocument({ content: result, language: 'json' });
      await vscode.window.showTextDocument(doc, vscode.ViewColumn.Beside);
    } else {
      vscode.window.showInformationMessage('argsh: No MCP JSON data available');
    }
  });
  context.subscriptions.push(exportMcpCmd);

  const exportYamlCmd = vscode.commands.registerCommand('argsh.exportYaml', async () => {
    const editor = vscode.window.activeTextEditor;
    if (!editor || !client) return;
    const result = await client.sendRequest('workspace/executeCommand', {
      command: 'argsh.exportYaml',
      arguments: [editor.document.uri.toString()],
    });
    if (typeof result === 'string' && result.length > 0) {
      const doc = await vscode.workspace.openTextDocument({ content: result, language: 'yaml' });
      await vscode.window.showTextDocument(doc, vscode.ViewColumn.Beside);
    } else {
      vscode.window.showInformationMessage('argsh: No YAML data available');
    }
  });
  context.subscriptions.push(exportYamlCmd);

  const formatCmd = vscode.commands.registerCommand('argsh.formatArrays', async () => {
    const editor = vscode.window.activeTextEditor;
    if (!editor) return;

    // Trigger the built-in format document which will use our LSP formatter
    await vscode.commands.executeCommand('editor.action.formatDocument');
  });
  context.subscriptions.push(formatCmd);

  const exportJsonCmd = vscode.commands.registerCommand('argsh.exportJson', async () => {
    const editor = vscode.window.activeTextEditor;
    if (!editor || !client) return;
    const result = await client.sendRequest('workspace/executeCommand', {
      command: 'argsh.exportJson',
      arguments: [editor.document.uri.toString()],
    });
    if (typeof result === 'string' && result.length > 0) {
      const doc = await vscode.workspace.openTextDocument({ content: result, language: 'json' });
      await vscode.window.showTextDocument(doc, vscode.ViewColumn.Beside);
    } else {
      vscode.window.showInformationMessage('argsh: No JSON data available');
    }
  });
  context.subscriptions.push(exportJsonCmd);
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}
