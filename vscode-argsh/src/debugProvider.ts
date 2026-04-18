import * as vscode from 'vscode';
import * as path from 'path';
import * as fs from 'fs';

/**
 * Resolves the argsh-dap binary path — same strategy as the LSP binary:
 * 1. Check argsh.dap.path setting
 * 2. Check bundled bin/argsh-dap
 * 3. Fall back to PATH
 */
function findDapBinary(context: vscode.ExtensionContext): string {
  const config = vscode.workspace.getConfiguration('argsh');
  const customPath = config.get<string>('dap.path', '');
  if (customPath) {
    return customPath;
  }

  const bundled = path.join(context.extensionPath, 'bin', 'argsh-dap');
  if (fs.existsSync(bundled)) {
    return bundled;
  }

  return 'argsh-dap'; // Fall back to PATH
}

/**
 * Factory that tells VSCode how to start the debug adapter.
 * We use DebugAdapterExecutable — VSCode spawns the binary and connects
 * via stdin/stdout (same as LSP transport).
 */
export class ArgshDebugAdapterDescriptorFactory
  implements vscode.DebugAdapterDescriptorFactory
{
  constructor(private context: vscode.ExtensionContext) {}

  createDebugAdapterDescriptor(
    _session: vscode.DebugSession,
    _executable: vscode.DebugAdapterExecutable | undefined
  ): vscode.ProviderResult<vscode.DebugAdapterDescriptor> {
    const dapPath = findDapBinary(this.context);
    return new vscode.DebugAdapterExecutable(dapPath);
  }
}

/**
 * Provides default debug configurations and resolves incomplete ones.
 */
export class ArgshDebugConfigurationProvider
  implements vscode.DebugConfigurationProvider
{
  /**
   * Called when the user opens a launch.json and there are no configs yet.
   * Returns a default "Debug Current File" config.
   */
  provideDebugConfigurations(
    _folder: vscode.WorkspaceFolder | undefined
  ): vscode.ProviderResult<vscode.DebugConfiguration[]> {
    return [
      {
        type: 'argsh',
        request: 'launch',
        name: 'Debug Current File',
        program: '${file}',
        args: [],
        stopOnEntry: true,
      },
    ];
  }

  /**
   * Called before a debug session starts. Fills in missing fields.
   */
  resolveDebugConfiguration(
    _folder: vscode.WorkspaceFolder | undefined,
    config: vscode.DebugConfiguration,
    _token?: vscode.CancellationToken
  ): vscode.ProviderResult<vscode.DebugConfiguration> {
    // If launched without a launch.json (e.g. F5 with no config), fill defaults
    if (!config.type && !config.request && !config.name) {
      const editor = vscode.window.activeTextEditor;
      if (editor && editor.document.languageId === 'shellscript') {
        config.type = 'argsh';
        config.request = 'launch';
        config.name = 'Debug Current File';
        config.program = editor.document.fileName;
        config.stopOnEntry = true;
      }
    }

    if (!config.program) {
      return vscode.window
        .showInformationMessage('Cannot find a script to debug')
        .then((_) => undefined);
    }

    // Ensure defaults
    config.type = config.type || 'argsh';
    config.request = config.request || 'launch';
    config.args = config.args || [];
    config.cwd = config.cwd || '${workspaceFolder}';

    return config;
  }
}
