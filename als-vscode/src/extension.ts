/**
 * Adesh Language Support — VS Code Extension
 *
 * Production-grade language client that connects to the Adesh Language Server (ALS).
 * Features:
 * - Automatic server discovery (config path → workspace build → PATH)
 * - Status bar indicator showing server state
 * - Inline diagnostics (errors, warnings, hints) like TypeScript/JavaScript
 * - Type-aware auto-completion (IntelliSense) with recommendations on every keystroke
 * - Hover info, go-to-definition, find references, rename, formatting
 * - Semantic syntax highlighting, inlay hints, signature help
 */

import * as path from 'path';
import * as fs from 'fs';
import * as vscode from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind,
    State,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;
let serverStarting = false;
let statusBarItem: vscode.StatusBarItem | undefined;
let outputChannel: vscode.OutputChannel | undefined;

/**
 * Create or update the status bar item.
 */
function createStatusBar(): vscode.StatusBarItem {
    if (!statusBarItem) {
        statusBarItem = vscode.window.createStatusBarItem(
            vscode.StatusBarAlignment.Right,
            50
        );
        statusBarItem.command = 'adesh.showOutput';
        statusBarItem.show();
    }
    return statusBarItem;
}

/**
 * Update the status bar to reflect the current server state.
 */
function updateStatusBar(state: 'starting' | 'running' | 'stopped' | 'error', detail?: string) {
    const bar = createStatusBar();
    switch (state) {
        case 'starting':
            bar.text = '$(loading~spin) Adesh: Starting...';
            bar.tooltip = 'Adesh Language Server is starting';
            bar.backgroundColor = undefined;
            break;
        case 'running':
            bar.text = '$(check) Adesh';
            bar.tooltip = 'Adesh Language Server is running';
            bar.backgroundColor = undefined;
            break;
        case 'stopped':
            bar.text = '$(circle-outline) Adesh';
            bar.tooltip = 'Adesh Language Server is stopped';
            bar.backgroundColor = undefined;
            break;
        case 'error':
            bar.text = '$(error) Adesh';
            bar.tooltip = detail || 'Adesh Language Server error';
            bar.backgroundColor = new vscode.ThemeColor('statusBarItem.errorBackground');
            break;
    }
}

/**
 * Search for the ALS binary in common locations.
 * Order of preference:
 * 1. User-configured `adesh.serverPath`
 * 2. Workspace `als/target/release/als`
 * 3. Workspace `als/target/debug/als`
 * 4. `als` in PATH (system PATH lookup)
 */
function findServerExecutable(): { command: string; args?: string[] } | undefined {
    const config = vscode.workspace.getConfiguration('adesh');
    const configuredPath = config.get<string>('serverPath', '');

    // 1. User-configured path
    if (configuredPath && configuredPath.trim().length > 0) {
        if (fs.existsSync(configuredPath)) {
            return { command: configuredPath };
        }
        // Even if the file doesn't exist yet, respect the user's setting
        // (they might be running on a different platform or have it in a non-standard location)
        return { command: configuredPath };
    }

    // 2 & 3. Look in workspace folders
    const workspaceFolders = vscode.workspace.workspaceFolders;
    if (workspaceFolders) {
        for (const folder of workspaceFolders) {
            const folderPath = folder.uri.fsPath;
            const releasePath = path.join(folderPath, 'als', 'target', 'release', 'als');
            const debugPath = path.join(folderPath, 'als', 'target', 'debug', 'als');

            // On Windows, check for .exe extension
            const releaseExe = process.platform === 'win32' ? releasePath + '.exe' : releasePath;
            const debugExe = process.platform === 'win32' ? debugPath + '.exe' : debugPath;

            if (fs.existsSync(releaseExe)) {
                return { command: releaseExe };
            }
            if (fs.existsSync(releasePath)) {
                return { command: releasePath };
            }
            if (fs.existsSync(debugExe)) {
                return { command: debugExe };
            }
            if (fs.existsSync(debugPath)) {
                return { command: debugPath };
            }
        }
    }

    // Also check if the extension is installed alongside the repo (common dev setup)
    // Walk up from the extension directory to find als/target
    const extensionDir = path.join(__dirname, '..', '..');
    const extReleasePath = path.join(extensionDir, 'als', 'target', 'release', 'als');
    const extDebugPath = path.join(extensionDir, 'als', 'target', 'debug', 'als');
    const extReleaseExe = process.platform === 'win32' ? extReleasePath + '.exe' : extReleasePath;
    const extDebugExe = process.platform === 'win32' ? extDebugPath + '.exe' : extDebugPath;

    if (fs.existsSync(extReleaseExe)) {
        return { command: extReleaseExe };
    }
    if (fs.existsSync(extReleasePath)) {
        return { command: extReleasePath };
    }
    if (fs.existsSync(extDebugExe)) {
        return { command: extDebugExe };
    }
    if (fs.existsSync(extDebugPath)) {
        return { command: extDebugPath };
    }

    // 4. Fall back to PATH lookup
    return { command: 'als' };
}

export function activate(context: vscode.ExtensionContext) {
    outputChannel = vscode.window.createOutputChannel('Adesh Language Server');
    context.subscriptions.push(outputChannel);

    outputChannel.appendLine('[Adesh] Extension activating...');

    // Create status bar
    context.subscriptions.push(createStatusBar());
    updateStatusBar('stopped');

    // Register commands
    context.subscriptions.push(
        vscode.commands.registerCommand('adesh.restartServer', async () => {
            await stopServer();
            await startServer(context);
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('adesh.stopServer', async () => {
            await stopServer();
            vscode.window.showInformationMessage('Adesh Language Server stopped');
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('adesh.formatDocument', async () => {
            const editor = vscode.window.activeTextEditor;
            if (editor && (editor.document.languageId === 'adesh' || editor.document.languageId === 'adl')) {
                await vscode.commands.executeCommand('editor.action.formatDocument');
            }
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('adesh.showOutput', () => {
            outputChannel?.show();
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('adesh.showBorrowInfo', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor || editor.document.languageId !== 'adesh') {
                vscode.window.showWarningMessage('Open an Adesh file first.');
                return;
            }
            // Trigger hover at the current position
            await vscode.commands.executeCommand('editor.action.showHover');
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('adesh.showOwnershipGraph', async () => {
            vscode.window.showInformationMessage(
                'Ownership graph visualization requires the ALS with CFG analysis. Use "Adesh: Run Borrow Checker" for diagnostics.'
            );
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('adesh.runBorrowCheck', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor || editor.document.languageId !== 'adesh') {
                vscode.window.showWarningMessage('Open an Adesh file first.');
                return;
            }
            // Save the document to trigger diagnostics refresh
            await editor.document.save();
            vscode.window.showInformationMessage('Borrow checker: diagnostics refreshed.');
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('adesh.toggleCompletion', () => {
            const config = vscode.workspace.getConfiguration('adesh');
            const current = config.get<boolean>('completion.enabled', true);
            config.update('completion.enabled', !current, vscode.ConfigurationTarget.Global);
            vscode.window.showInformationMessage(
                `Adesh auto-completion ${!current ? 'enabled' : 'disabled'}`
            );
        })
    );

    context.subscriptions.push(
        vscode.commands.registerCommand('adesh.toggleDiagnostics', () => {
            const config = vscode.workspace.getConfiguration('adesh');
            const current = config.get<boolean>('diagnostics.enabled', true);
            config.update('diagnostics.enabled', !current, vscode.ConfigurationTarget.Global);
            vscode.window.showInformationMessage(
                `Adesh diagnostics ${!current ? 'enabled' : 'disabled'}`
            );
        })
    );

    // ADL: Validate manifest command
    context.subscriptions.push(
        vscode.commands.registerCommand('adesh.validateAdl', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor || editor.document.languageId !== 'adl') {
                vscode.window.showWarningMessage('Open an ADL file first.');
                return;
            }
            await editor.document.save();
            vscode.window.showInformationMessage('ADL manifest validation triggered. Check the Problems panel for diagnostics.');
        })
    );

    // ADL: Create lockfile template command
    context.subscriptions.push(
        vscode.commands.registerCommand('adesh.createLockfile', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) {
                vscode.window.showWarningMessage('Open a project directory first.');
                return;
            }
            const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
            if (!workspaceFolder) {
                vscode.window.showWarningMessage('Open a workspace folder first.');
                return;
            }
            const lockfilePath = vscode.Uri.joinPath(workspaceFolder.uri, 'adesh.lock.adl');
            const lockfileTemplate = `lock {
  version = "1"
  generated-at = "${new Date().toISOString().replace(/\.\d{3}Z$/, 'Z')}"
  compiler-version = "0.3.0"
  adl-version = "0.3.0"
  package-id = "${workspaceFolder.name}"
  dependencies = []
  signature = "0000000000000000"
}
`;
            await vscode.workspace.fs.writeFile(lockfilePath, Buffer.from(lockfileTemplate, 'utf-8'));
            await vscode.commands.executeCommand('vscode.open', lockfilePath);
            vscode.window.showInformationMessage('Created adesh.lock.adl template.');
        })
    );

    // ADL: Add dependency command
    context.subscriptions.push(
        vscode.commands.registerCommand('adesh.addDependency', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor || editor.document.languageId !== 'adl') {
                vscode.window.showWarningMessage('Open an adesh.adl file first.');
                return;
            }
            const packageName = await vscode.window.showInputBox({
                prompt: 'Package name',
                placeHolder: 'e.g. HTTP',
            });
            if (!packageName) return;
            const version = await vscode.window.showInputBox({
                prompt: 'Version requirement',
                placeHolder: 'e.g. ^1.0.0',
                value: '^1.0.0',
            });
            if (!version) return;
            const text = editor.document.getText();
            const lines = text.split('\n');
            // Find [dependencies] section or add one
            let depLineIndex = lines.findIndex(l => l.trim() === '[dependencies]');
            if (depLineIndex === -1) {
                // Add [dependencies] section at the end
                lines.push('');
                lines.push('[dependencies]');
                depLineIndex = lines.length - 1;
            }
            // Find the end of the dependencies section
            let insertIndex = depLineIndex + 1;
            while (insertIndex < lines.length && !lines[insertIndex].trim().startsWith('[')) {
                insertIndex++;
            }
            const depLine = `${packageName} = "${version}"`;
            lines.splice(insertIndex, 0, depLine);
            const newText = lines.join('\n');
            await editor.edit(edit => {
                edit.replace(new vscode.Range(0, 0, editor.document.lineCount, 0), newText);
            });
            vscode.window.showInformationMessage(`Added dependency: ${packageName} = "${version}"`);
        })
    );

    // Start server if enabled
    const config = vscode.workspace.getConfiguration('adesh');
    const serverEnabled = config.get<boolean>('server.enabled', true);

    if (serverEnabled) {
        startServer(context);
    } else {
        outputChannel.appendLine('[Adesh] Language Server is disabled in settings');
        updateStatusBar('stopped', 'Server disabled in settings');
    }

    // Watch for configuration changes that affect the server
    context.subscriptions.push(
        vscode.workspace.onDidChangeConfiguration(e => {
            if (e.affectsConfiguration('adesh.serverPath') || e.affectsConfiguration('adesh.server.enabled')) {
                const newConfig = vscode.workspace.getConfiguration('adesh');
                const newEnabled = newConfig.get<boolean>('server.enabled', true);
                if (newEnabled && !client) {
                    startServer(context);
                } else if (!newEnabled && client) {
                    stopServer();
                }
            }
        })
    );
}

async function startServer(context: vscode.ExtensionContext): Promise<void> {
    if (serverStarting || client) {
        return;
    }

    serverStarting = true;
    updateStatusBar('starting');

    try {
        const config = vscode.workspace.getConfiguration('adesh');
        const timeout = config.get<number>('server.timeout', 30000);
        const traceLevel = config.get<string>('trace.server', 'off');

        const serverInfo = findServerExecutable();
        if (!serverInfo) {
            throw new Error('Could not find ALS executable. Set "adesh.serverPath" to the binary path.');
        }

        outputChannel?.appendLine(`[Adesh] Starting language server: ${serverInfo.command}`);

        // Server options - run the ALS executable over stdio
        const serverOptions: ServerOptions = {
            run: {
                command: serverInfo.command,
                transport: TransportKind.stdio,
            },
            debug: {
                command: serverInfo.command,
                transport: TransportKind.stdio,
                options: {
                    env: { ...process.env, RUST_LOG: 'debug' },
                },
            },
        };

        // Client options
        const clientOptions: LanguageClientOptions = {
            documentSelector: [
                { scheme: 'file', language: 'adesh' },
                { scheme: 'file', language: 'adl' },
            ],
            synchronize: {
                fileEvents: [
                    vscode.workspace.createFileSystemWatcher('**/*.adesh'),
                    vscode.workspace.createFileSystemWatcher('**/*.adl'),
                ],
            },
            outputChannel: outputChannel!,
            traceOutputChannel: outputChannel!,
            initializationFailedHandler: (error) => {
                outputChannel!.appendLine(`[Adesh] Server initialization failed: ${error}`);
                updateStatusBar('error', 'Initialization failed');
                vscode.window.showErrorMessage(
                    'Adesh Language Server failed to initialize. ' +
                    'Check the output channel for details. ' +
                    'Set "adesh.serverPath" to the ALS binary or build it with `cargo build --release` in the als/ directory.'
                );
                return false;
            },
            middleware: {
                // Ensure diagnostics are properly handled
                handleDiagnostics: (uri, diagnostics, next) => {
                    const diagConfig = vscode.workspace.getConfiguration('adesh');
                    if (!diagConfig.get<boolean>('diagnostics.enabled', true)) {
                        return; // Skip diagnostics if disabled
                    }
                    next(uri, diagnostics);
                },
            },
        };

        // Create the language client
        client = new LanguageClient(
            'adesh',
            'Adesh Language Server',
            serverOptions,
            clientOptions
        );

        // Track state changes
        context.subscriptions.push(
            client.onDidChangeState(e => {
                if (e.newState === State.Running) {
                    updateStatusBar('running');
                    outputChannel!.appendLine('[Adesh] Server is running');
                } else if (e.newState === State.Starting) {
                    updateStatusBar('starting');
                } else if (e.newState === State.Stopped) {
                    updateStatusBar('stopped');
                    outputChannel!.appendLine('[Adesh] Server stopped');
                }
            })
        );

        // Start with timeout
        const startPromise = client.start();
        let timeoutId: NodeJS.Timeout | undefined;
        const timeoutPromise = new Promise<void>((_, reject) => {
            timeoutId = setTimeout(
                () => reject(new Error('Server startup timeout')),
                timeout
            );
        });

        try {
            await Promise.race([startPromise, timeoutPromise]);
            updateStatusBar('running');
            outputChannel?.appendLine('[Adesh] Language Server started successfully');
            context.subscriptions.push(client);
        } finally {
            if (timeoutId) {
                clearTimeout(timeoutId);
            }
        }
    } catch (error: any) {
        const message = error?.message || String(error);
        outputChannel?.appendLine(`[Adesh] Failed to start language server: ${message}`);
        updateStatusBar('error', message);

        // Show a helpful warning with action buttons
        const action = await vscode.window.showWarningMessage(
            `Adesh Language Server unavailable: ${message}`,
            'Build Server',
            'Set Server Path',
            'Disable Server'
        );

        if (action === 'Build Server') {
            // Open a terminal to build the server
            const terminal = vscode.window.createTerminal('Build ALS');
            terminal.show();
            const workspacePath = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath || '.';
            if (process.platform === 'win32') {
                terminal.sendText(`cd "${workspacePath}\\als" && cargo build --release`);
            } else {
                terminal.sendText(`cd "${workspacePath}/als" && cargo build --release`);
            }
            vscode.window.showInformationMessage(
                'After the build completes, run "Adesh: Restart Language Server" or reload the window.'
            );
        } else if (action === 'Set Server Path') {
            await vscode.commands.executeCommand('workbench.action.openSettings', 'adesh.serverPath');
        } else if (action === 'Disable Server') {
            const config = vscode.workspace.getConfiguration('adesh');
            await config.update('server.enabled', false, vscode.ConfigurationTarget.Global);
        }

        // Clean up client on failure
        if (client) {
            try {
                await client.stop();
            } catch {
                // Ignore errors when stopping failed client
            }
        }
        client = undefined;
    } finally {
        serverStarting = false;
    }
}

async function stopServer(): Promise<void> {
    if (client) {
        try {
            await client.stop();
            outputChannel?.appendLine('[Adesh] Language Server stopped');
        } catch (error) {
            outputChannel?.appendLine(`[Adesh] Error stopping server: ${error}`);
        }
        client = undefined;
    }
    updateStatusBar('stopped');
}

export function deactivate(): Thenable<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}
