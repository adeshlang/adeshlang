const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const { test } = require('node:test');

test('Marketplace publisher and default formatter use the registered extension ID', () => {
    const manifest = require('../package.json');
    assert.equal(manifest.publisher, 'adesh-lang-org');
    assert.equal(
        manifest.contributes.configurationDefaults['[adesh]']['editor.defaultFormatter'],
        `${manifest.publisher}.${manifest.name}`,
    );
});

test('Marketplace icon is a PNG within the 1024x1024 upload limit', () => {
    const manifest = require('../package.json');
    const png = fs.readFileSync(path.join(__dirname, '..', manifest.icon));
    assert.equal(png.toString('ascii', 1, 4), 'PNG');
    assert.ok(png.readUInt32BE(16) <= 1024 && png.readUInt32BE(20) <= 1024);
});
test('client settings, discovery, commands, and trusted process tasks', async () => {
    const commands = new Map();
    const notifications = [];
    const tasks = [];
    const values = new Map([['server.enabled', true], ['inlayHints', { showParameterNames: false }]]);
    const folder = { uri: { fsPath: path.resolve('fixture') }, name: 'fixture' };
    const binary = path.join(folder.uri.fsPath, 'target', 'release', process.platform === 'win32' ? 'als.exe' : 'als');
    let options, serverOptions, changeConfig;
    const disposable = { dispose() {} };
    const config = {
        get(key, fallback) { return values.has(key) ? values.get(key) : fallback; },
        async update(key, value) { values.set(key, value); },
    };
    class Position { constructor(line, character) { Object.assign(this, { line, character }); } }
    class Location { constructor(uri, range) { Object.assign(this, { uri, range }); } }
    class ProcessExecution { constructor(command, args, options) { Object.assign(this, { command, args, options }); } }
    class Task { constructor(definition, scope, name, source, execution) { Object.assign(this, { definition, scope, name, source, execution }); } }
    const target = {
        scheme: 'file',
        fsPath: path.join(folder.uri.fsPath, 'file with spaces & symbols.adesh'),
        toString() { return 'file:///fixture/file%20with%20spaces%20%26%20symbols.adesh'; },
    };
    const vscode = {
        workspace: {
            isTrusted: true, workspaceFolders: [folder],
            getConfiguration() { return config; },
            onDidChangeConfiguration(callback) { changeConfig = callback; return disposable; },
            onDidGrantWorkspaceTrust() { return disposable; },
            getWorkspaceFolder() { return folder; },
            createFileSystemWatcher() { return disposable; },
            async openTextDocument() { return { async save() { return true; } }; },
        },
        window: {
            activeTextEditor: { document: { uri: target } },
            createOutputChannel() { return { ...disposable, appendLine() {}, show() {} }; },
            createStatusBarItem() { return { ...disposable, show() {} }; },
            showWarningMessage() {}, showInformationMessage() {},
        },
        commands: {
            registerCommand(name, callback) { commands.set(name, callback); return disposable; },
            async executeCommand() { return []; },
        },
        tasks: { async executeTask(task) { tasks.push(task); } },
        Uri: { parse() { return target; } },
        TaskScope: { Workspace: 1 }, StatusBarAlignment: { Right: 2 },
        ConfigurationTarget: { Global: 1 },
        Position, Location, ProcessExecution, Task,
    };
    class LanguageClient {
        constructor(_id, _name, server, client) { serverOptions = server; options = client; }
        onDidChangeState() { return disposable; }
        async start() {}
        async stop() {}
        isRunning() { return true; }
        async sendNotification(method, params) { notifications.push({ method, params }); }
    }
    const exports = {};
    vm.runInNewContext(fs.readFileSync(path.join(__dirname, '../out/extension.js'), 'utf8'), {
        exports, __dirname: path.join(__dirname, '../out'), process, console, setTimeout, clearTimeout,
        require(name) {
            if (name === 'vscode') return vscode;
            if (name === 'vscode-languageclient/node') return { LanguageClient, TransportKind: { stdio: 0 }, State: {} };
            if (name === 'fs') return { existsSync(candidate) { return candidate === binary; } };
            return require(name);
        },
    });
    exports.activate({ subscriptions: [] });
    await new Promise(resolve => setImmediate(resolve));
    assert.equal(serverOptions.run.command, binary);
    assert.equal(options.initializationOptions.adesh.inlayHints.showParameterNames, false);
    for (const command of ['runFile', 'runTest', 'showReferences', 'showImplementations', 'organizeImports', 'toggleInlayHints']) {
        assert.ok(commands.has('adesh.' + command), command);
    }
    await commands.get('adesh.runTest')(target.toString(), 'one_test');
    assert.deepEqual(Array.from(tasks[0].execution.args), ['test', target.fsPath, '--test-name', 'one_test', '--exact']);
    assert.equal(tasks[0].execution.command, 'adesh');
    vscode.workspace.isTrusted = false;
    await commands.get('adesh.runFile')(target.toString());
    assert.equal(tasks.length, 1, 'untrusted workspace must not execute code');
    await changeConfig({ affectsConfiguration(key) { return key === 'adesh'; } });
    assert.equal(notifications[0].method, 'workspace/didChangeConfiguration');
    assert.equal(notifications[0].params.settings.adesh.inlayHints.showParameterNames, false);
    await exports.deactivate();
});
