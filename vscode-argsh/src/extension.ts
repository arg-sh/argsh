import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from 'vscode-languageclient/node';
import {
  ArgshDebugAdapterDescriptorFactory,
  ArgshDebugConfigurationProvider,
} from './debugProvider';

let client: LanguageClient | undefined;

// --- Command Tree View ---

interface CommandTreeItem {
  name: string;
  kind: number; // vscode.SymbolKind
  detail?: string;
  range: { start: { line: number }; end: { line: number } };
  children?: CommandTreeItem[];
}

/** Key used to identify a breakpoint location tied to a command tree node. */
function breakpointKey(uri: string, line: number): string {
  return `${uri}:${line}`;
}

class ArgshCommandTreeProvider implements vscode.TreeDataProvider<CommandTreeItem> {
  private _onDidChangeTreeData = new vscode.EventEmitter<CommandTreeItem | undefined>();
  readonly onDidChangeTreeData = this._onDidChangeTreeData.event;
  private items: CommandTreeItem[] = [];
  private uri: string = '';
  private activeItem: CommandTreeItem | undefined;
  /** Tracks which command nodes have breakpoints (key = "uri:line"). */
  private breakpoints = new Set<string>();

  refresh(symbols: CommandTreeItem[], uri: string) {
    this.items = symbols;
    this.uri = uri;
    this._onDidChangeTreeData.fire(undefined);
    vscode.commands.executeCommand('setContext', 'argsh.hasCommands', symbols.length > 0);
  }

  highlightFunction(line: number): CommandTreeItem | undefined {
    this.activeItem = this.findByLine(this.items, line);
    this._onDidChangeTreeData.fire(undefined);
    return this.activeItem;
  }

  private findByLine(items: CommandTreeItem[], line: number): CommandTreeItem | undefined {
    for (const item of items) {
      if (line >= item.range.start.line && line <= item.range.end.line) {
        // Only return Function items, not Property/Enum children
        if (item.kind === vscode.SymbolKind.Function) {
          // Check if a child function is a better match
          if (item.children) {
            const child = this.findByLine(item.children, line);
            if (child) return child;
          }
          return item;
        }
      }
    }
    return undefined;
  }

  getTreeItem(element: CommandTreeItem): vscode.TreeItem {
    const isFunction = element.kind === vscode.SymbolKind.Function;
    const hasChildren = element.children && element.children.length > 0;
    const hasBp = isFunction && this.hasBreakpoint(element);
    const collapsible = hasChildren
      ? vscode.TreeItemCollapsibleState.Expanded
      : isFunction
        ? vscode.TreeItemCollapsibleState.None
        : vscode.TreeItemCollapsibleState.None;

    const item = new vscode.TreeItem(element.name, collapsible);

    if (isFunction) {
      item.iconPath = hasBp
        ? new vscode.ThemeIcon('debug-breakpoint', new vscode.ThemeColor('debugIcon.breakpointForeground'))
        : hasChildren
          ? new vscode.ThemeIcon('git-merge')
          : new vscode.ThemeIcon('terminal');
      item.description = element.detail || '';
      item.command = {
        command: 'argsh.goToSymbol',
        title: 'Go to',
        arguments: [element],
      };
      item.contextValue = hasBp ? 'argshCommand:breakpoint' : 'argshCommand';
      item.tooltip = hasBp
        ? `${element.name} (breakpoint set on line ${element.range.start.line + 1})`
        : element.name;
    } else if (element.kind === vscode.SymbolKind.Property) {
      item.iconPath = new vscode.ThemeIcon('symbol-field');
      item.description = element.detail || '';
    } else if (element.kind === vscode.SymbolKind.Enum) {
      item.iconPath = new vscode.ThemeIcon('symbol-enum');
      item.description = element.detail || '';
    }

    // Highlight active function (overrides breakpoint icon when active)
    if (this.activeItem && element.name === this.activeItem.name
        && element.range.start.line === this.activeItem.range.start.line) {
      if (hasBp) {
        // Active + breakpoint: use breakpoint-filled with green tint
        item.iconPath = new vscode.ThemeIcon(
          'debug-breakpoint',
          new vscode.ThemeColor('charts.green')
        );
      } else {
        item.iconPath = new vscode.ThemeIcon(
          hasChildren ? 'git-merge' : 'terminal',
          new vscode.ThemeColor('charts.green')
        );
      }
    }

    return item;
  }

  getUri(): string { return this.uri; }

  hasBreakpoint(item: CommandTreeItem): boolean {
    return this.breakpoints.has(breakpointKey(this.uri, item.range.start.line));
  }

  /**
   * Toggle a breakpoint on the first line of the given command function.
   * Returns true if a breakpoint was added, false if removed.
   */
  toggleBreakpoint(item: CommandTreeItem): boolean {
    if (item.kind !== vscode.SymbolKind.Function || !this.uri) return false;

    const uri = vscode.Uri.parse(this.uri);
    const key = breakpointKey(this.uri, item.range.start.line);
    const location = new vscode.Location(uri, new vscode.Position(item.range.start.line, 0));

    if (this.breakpoints.has(key)) {
      // Remove the matching breakpoint
      const existing = vscode.debug.breakpoints.filter(
        (bp): bp is vscode.SourceBreakpoint =>
          bp instanceof vscode.SourceBreakpoint
          && bp.location.uri.toString() === this.uri
          && bp.location.range.start.line === item.range.start.line
      );
      if (existing.length > 0) {
        vscode.debug.removeBreakpoints(existing);
      }
      this.breakpoints.delete(key);
      this._onDidChangeTreeData.fire(undefined);
      return false;
    } else {
      const bp = new vscode.SourceBreakpoint(location, true);
      vscode.debug.addBreakpoints([bp]);
      this.breakpoints.add(key);
      this._onDidChangeTreeData.fire(undefined);
      return true;
    }
  }

  /**
   * Synchronise internal breakpoint set with the actual VSCode breakpoints.
   * Called when breakpoints change externally (e.g. gutter click).
   */
  syncBreakpoints(): void {
    const prev = new Set(this.breakpoints);
    this.breakpoints.clear();
    const allFunctions = this.collectFunctions(this.items);
    for (const fn of allFunctions) {
      const line = fn.range.start.line;
      const hasMatch = vscode.debug.breakpoints.some(
        (bp): bp is vscode.SourceBreakpoint =>
          bp instanceof vscode.SourceBreakpoint
          && bp.location.uri.toString() === this.uri
          && bp.location.range.start.line === line
      );
      if (hasMatch) {
        this.breakpoints.add(breakpointKey(this.uri, line));
      }
    }
    // Only refresh tree if something actually changed
    if (prev.size !== this.breakpoints.size || [...prev].some(k => !this.breakpoints.has(k))) {
      this._onDidChangeTreeData.fire(undefined);
    }
  }

  private collectFunctions(items: CommandTreeItem[]): CommandTreeItem[] {
    const result: CommandTreeItem[] = [];
    for (const item of items) {
      if (item.kind === vscode.SymbolKind.Function) {
        result.push(item);
      }
      if (item.children) {
        result.push(...this.collectFunctions(item.children));
      }
    }
    return result;
  }

  getChildren(element?: CommandTreeItem): CommandTreeItem[] {
    if (!element) return this.items;
    return element.children || [];
  }

  getParent(element: CommandTreeItem): CommandTreeItem | undefined {
    return this.findParent(this.items, element);
  }

  private findParent(items: CommandTreeItem[], target: CommandTreeItem): CommandTreeItem | undefined {
    for (const item of items) {
      if (item.children) {
        for (const child of item.children) {
          if (child.name === target.name && child.range.start.line === target.range.start.line) {
            return item;
          }
        }
        const found = this.findParent(item.children, target);
        if (found) return found;
      }
    }
    return undefined;
  }
}

// --- Extension activation ---

export function activate(context: vscode.ExtensionContext) {
  const config = vscode.workspace.getConfiguration('argsh');

  // Issue #8: Register the debug adapter BEFORE checking lsp.enabled so that
  // debugging works even when the LSP is disabled. The debug adapter is a
  // separate binary (argsh-dap) and does not depend on the language server.
  context.subscriptions.push(
    vscode.debug.registerDebugAdapterDescriptorFactory(
      'argsh',
      new ArgshDebugAdapterDescriptorFactory(context)
    )
  );
  context.subscriptions.push(
    vscode.debug.registerDebugConfigurationProvider(
      'argsh',
      new ArgshDebugConfigurationProvider()
    )
  );

  // Force-detect extensionless argsh scripts as shellscript.
  // VSCode may misdetect them as "ini" or "plaintext" without a .sh extension.
  const setArgshLanguage = (doc: vscode.TextDocument) => {
    if (doc.languageId === 'shellscript') return;
    const firstLine = doc.lineAt(0).text;
    if (/^#!.*\bargsh\b/.test(firstLine)) {
      vscode.languages.setTextDocumentLanguage(doc, 'shellscript').then(
        undefined,
        () => {} // ignore errors (document may have been closed)
      );
    }
  };
  // Check all currently open documents
  vscode.workspace.textDocuments.forEach(setArgshLanguage);
  // Check newly opened documents
  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument(setArgshLanguage)
  );

  if (!config.get<boolean>('lsp.enabled', true)) {
    return;
  }

  // Find the LSP binary
  let serverPath = config.get<string>('lsp.path', '');
  if (!serverPath) {
    const bundled = path.join(context.extensionPath, 'bin', 'argsh-lsp');
    if (fs.existsSync(bundled)) {
      serverPath = bundled;
    } else {
      serverPath = 'argsh-lsp'; // Fall back to PATH
    }
  }

  const serverOptions: ServerOptions = {
    run: { command: serverPath, transport: TransportKind.stdio },
    debug: { command: serverPath, transport: TransportKind.stdio },
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: 'file', language: 'shellscript' },
    ],
    initializationOptions: {
      resolveDepth: config.get<number>('resolveDepth', 2),
      codeLensEnabled: config.get<boolean>('codeLens.enabled', true),
    },
  };

  client = new LanguageClient(
    'argsh-lsp',
    'argsh Language Server',
    serverOptions,
    clientOptions
  );

  client.start();

  // --- Command Tree ---

  let treeProvider: ArgshCommandTreeProvider | undefined;

  if (config.get<boolean>('commandTree.enabled', true)) {
    treeProvider = new ArgshCommandTreeProvider();
    const treeView = vscode.window.createTreeView('argsh.commandTree', {
      treeDataProvider: treeProvider,
      showCollapseAll: true,
    });
    context.subscriptions.push(treeView);

    // Toggle breakpoint command for command tree items
    const toggleBpCmd = vscode.commands.registerCommand(
      'argsh.commandTree.toggleBreakpoint',
      (item: CommandTreeItem) => {
        if (!item) return;
        const added = treeProvider!.toggleBreakpoint(item);
        const verb = added ? 'set' : 'removed';
        vscode.window.setStatusBarMessage(
          `argsh: Breakpoint ${verb} on ${item.name} (line ${item.range.start.line + 1})`,
          3000
        );
      }
    );
    context.subscriptions.push(toggleBpCmd);

    // Sync tree breakpoint indicators when breakpoints change externally
    context.subscriptions.push(
      vscode.debug.onDidChangeBreakpoints(() => {
        treeProvider!.syncBreakpoints();
      })
    );

    // Update tree when active editor changes or document is saved
    const updateTree = async () => {
      const editor = vscode.window.activeTextEditor;
      if (!editor || !client || editor.document.languageId !== 'shellscript') {
        treeProvider!.refresh([], '');
        return;
      }

      try {
        const symbols = await client.sendRequest('textDocument/documentSymbol', {
          textDocument: { uri: editor.document.uri.toString() },
        }) as CommandTreeItem[] | null;

        if (symbols && Array.isArray(symbols)) {
          treeProvider!.refresh(symbols, editor.document.uri.toString());
        } else {
          treeProvider!.refresh([], '');
        }
      } catch {
        // LSP not ready yet
      }
    };

    // Highlight current function when cursor moves
    const updateHighlight = () => {
      const editor = vscode.window.activeTextEditor;
      if (editor && editor.document.languageId === 'shellscript') {
        const activeItem = treeProvider!.highlightFunction(editor.selection.active.line);
        if (activeItem) {
          treeView.reveal(activeItem, { select: false, focus: false, expand: true });
        }
      }
    };

    // Initial update after a short delay (LSP needs time to start)
    setTimeout(() => updateTree(), 1500);

    context.subscriptions.push(
      vscode.window.onDidChangeActiveTextEditor(() => { updateTree(); }),
      vscode.workspace.onDidSaveTextDocument(() => { updateTree(); }),
      vscode.window.onDidChangeTextEditorSelection(() => { updateHighlight(); }),
    );
  }

  // Format on save
  context.subscriptions.push(
    vscode.workspace.onWillSaveTextDocument((e) => {
      const cfg = vscode.workspace.getConfiguration('argsh');
      if (!cfg.get<boolean>('formatOnSave', true)) return;
      if (e.document.languageId !== 'shellscript') return;
      if (!client) return;

      // Skip if VSCode's built-in formatOnSave is enabled (the LSP formatting
      // provider will be invoked automatically by VSCode).
      const editorConfig = vscode.workspace.getConfiguration('editor', e.document.uri);
      if (editorConfig.get<boolean>('formatOnSave', false)) return;

      e.waitUntil(
        client.sendRequest('textDocument/formatting', {
          textDocument: { uri: e.document.uri.toString() },
          options: { tabSize: 2, insertSpaces: true },
        }).then((edits: any) => {
          if (!edits || !Array.isArray(edits) || edits.length === 0) return [];
          return edits.map((edit: any) => new vscode.TextEdit(
            new vscode.Range(
              new vscode.Position(edit.range.start.line, edit.range.start.character),
              new vscode.Position(edit.range.end.line, edit.range.end.character),
            ),
            edit.newText,
          ));
        }).catch(() => [])
      );
    }),
  );

  // Go to symbol command (used by tree item clicks)
  const goToSymbolCmd = vscode.commands.registerCommand('argsh.goToSymbol', async (item: CommandTreeItem) => {
    if (!item.range) return;

    // Prefer the URI tracked by the tree provider; fall back to active editor
    const storedUri = treeProvider?.getUri();
    let editor: vscode.TextEditor | undefined;
    if (storedUri) {
      const doc = await vscode.workspace.openTextDocument(vscode.Uri.parse(storedUri));
      editor = await vscode.window.showTextDocument(doc);
    } else {
      editor = vscode.window.activeTextEditor;
    }
    if (!editor) return;

    const pos = new vscode.Position(item.range.start.line, 0);
    editor.selection = new vscode.Selection(pos, pos);
    editor.revealRange(new vscode.Range(pos, pos), vscode.TextEditorRevealType.InCenter);
  });
  context.subscriptions.push(goToSymbolCmd);

  // --- Commands ---

  let previewPanel: vscode.WebviewPanel | undefined;

  const previewCmd = vscode.commands.registerCommand('argsh.showPreview', async () => {
    const editor = vscode.window.activeTextEditor;
    if (!editor || !client) return;

    const uri = editor.document.uri.toString();
    const html = await client.sendRequest('workspace/executeCommand', {
      command: 'argsh.preview',
      arguments: [uri],
    });

    if (typeof html === 'string') {
      if (previewPanel) {
        previewPanel.webview.html = html;
        previewPanel.reveal(vscode.ViewColumn.Beside);
      } else {
        previewPanel = vscode.window.createWebviewPanel(
          'argshPreview',
          'argsh Preview',
          vscode.ViewColumn.Beside,
          { enableScripts: false }
        );
        previewPanel.webview.html = html;
        previewPanel.onDidDispose(() => { previewPanel = undefined; });
      }
    }
  });
  context.subscriptions.push(previewCmd);

  // Auto-update preview on save
  context.subscriptions.push(
    vscode.workspace.onDidSaveTextDocument(async (doc) => {
      if (previewPanel && client && doc.languageId === 'shellscript') {
        const html = await client.sendRequest('workspace/executeCommand', {
          command: 'argsh.preview',
          arguments: [doc.uri.toString()],
        });
        if (typeof html === 'string') {
          previewPanel.webview.html = html;
        }
      }
    })
  );

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
