const assert = require('node:assert/strict');
const { EventEmitter } = require('node:events');
const { readFileSync } = require('node:fs');
const http = require('node:http');
const path = require('node:path');
const { test } = require('node:test');
const ts = require('typescript');

function makePanel() {
  const result = { messages: [], disposed: false, webview: { html: '' }, reveal() { this.revealed = true; } };
  let dispose = () => {};
  result.webview.postMessage = async message => { result.messages.push(message); return true; };
  result.webview.onDidReceiveMessage = fn => { result.webview.receive = fn; return { dispose() {} }; };
  result.onDidDispose = fn => { dispose = fn; };
  result.dispose = () => { result.disposed = true; dispose(); };
  return result;
}

function load() {
  const fake = {
    warningChoice: 'Cancel', panels: [], children: [], errors: [], warnings: [], logs: [], grid: [],
    output: { append(text) { fake.logs.push(text); }, appendLine(text) { fake.logs.push(text); }, show() {}, dispose() {} },
    spawnError: null, readyUrl: 'http://127.0.0.1:45678', settings: {},
  };
  const disposable = () => ({ dispose() {} });
  const vscode = {
    ColorThemeKind: { Light: 1, Dark: 2, HighContrastLight: 4 }, ViewColumn: { Beside: 2, One: 1 },
    Uri: { parse(text) { return { toString() { return text; } }; } },
    Position: class { constructor(line, character) { this.line = line; this.character = character; } },
    Range: class { constructor(start, end) { this.start = start; this.end = end; } },
    window: {
      activeColorTheme: { kind: 1 }, visibleTextEditors: [], activeTextEditor: undefined,
      async showWarningMessage(message) { fake.warnings.push(message); return fake.warningChoice; },
      async showErrorMessage(message) { fake.errors.push(message); },
      async showInformationMessage(message) { fake.information = message; },
      async showQuickPick(items) { fake.items = items; return fake.pick?.(items); },
      createOutputChannel() { return fake.output; },
      createWebviewPanel() { const p = makePanel(); fake.panels.push(p); return p; },
      async openTextDocument(uri) { return { uri, isDirty: false }; },
      async showTextDocument(document) { const editor = { document, selection: null, revealRange() {} }; vscode.window.visibleTextEditors = [editor]; return editor; },
      onDidChangeActiveColorTheme: disposable,
      onDidChangeTextEditorSelection: disposable,
    },
    workspace: {
      textDocuments: [],
      getConfiguration(section) { return { get(name, fallback) { return fake.settings[`${section}.${name}`] ?? fallback; } }; },
      async findFiles() { return []; },
      asRelativePath(uri) { return uri.fsPath; },
    },
    env: { async asExternalUri(uri) { return uri; } },
  };
  const childProcess = { spawn() {
    const child = new EventEmitter();
    child.stdout = new EventEmitter(); child.stderr = new EventEmitter();
    child.exitCode = null; child.killed = false;
    child.kill = () => { child.killed = true; return true; };
    fake.children.push(child);
    queueMicrotask(() => {
      if (fake.spawnError) child.emit('error', fake.spawnError);
      else child.stdout.emit('data', Buffer.from(`sysml studio: ${fake.readyUrl}\n`));
    });
    return child;
  } };
  const file = path.join(__dirname, '../src/extension.ts');
  const source = readFileSync(file, 'utf8') + `\nexport const testing = {
    panelHtml, startServer, openStudio, chooseDiagram, showGrid, pathAtPosition, visualizeElement, changeDiagramDepth,
    setSession(value: any) { session = value; }, getSession() { return session; }, setOutput(value: any) { output = value; },
    setClient(value: any) { client = value; clientReady = Promise.resolve(); }, diagramDefaults
  };`;
  const compiled = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 },
  }).outputText;
  const module = { exports: {} };
  new Function('require', 'module', 'exports', compiled)(name => {
    if (name === 'vscode') return vscode;
    if (name === 'child_process') return childProcess;
    if (name === 'fs') return { existsSync() { return true; } };
    if (name === './grid') return { async openGrid(...args) { fake.grid.push(args); } };
    if (name === 'vscode-languageclient/node') return { TransportKind: { stdio: 0 }, LanguageClient: class {} };
    return require(name);
  }, module, module.exports);
  const api = module.exports.testing;
  api.setOutput(fake.output);
  return { api, fake, vscode, context: { extensionPath: 'extension', subscriptions: [] } };
}

function browserScript() {
  const { api } = load();
  const html = api.panelHtml('http://127.0.0.1:7000', 'http://127.0.0.1:7000/#sysml.host=vscode');
  const script = html.match(/<script nonce="[^"]+">([\s\S]*?)<\/script>/)[1];
  new Function(script);
  return script;
}

function runBridge() {
  const callbacks = [], sent = [], observers = [], classNames = new Set(['vscode-light']);
  const frame = { contentWindow: { postMessage(message) { sent.push(message); } },
    addEventListener(_name, fn) { frame.load = fn; } };
  const document = { body: { classList: { contains: name => classNames.has(name) } }, documentElement: {}, getElementById() { return frame; } };
  let palette = '#ffffff';
  const observer = class {
    constructor(fn) { this.callback = fn; observers.push(this); this.targets = []; }
    observe(target, options) { this.targets.push({ target, options }); }
  };
  new Function('window', 'document', 'getComputedStyle', 'MutationObserver', 'acquireVsCodeApi', 'setTimeout', browserScript())(
    { addEventListener(name, fn) { callbacks.push(fn); } }, document,
    target => { assert.equal(target, document.body); return { getPropertyValue(name) { return name === '--vscode-editor-background' ? palette : ''; } }; },
    observer, () => ({ postMessage() {} }), fn => fn(),
  );
  return { sent, frame, observers, document, callbacks, classNames, setPalette(value) { palette = value; } };
}

test('the embedded relay is plain browser JavaScript', () => {
  assert.doesNotThrow(() => new Function(browserScript()));
});

test('iframe load sends the current body palette and forwards queued diagram requests', () => {
  const bridge = runBridge();
  const request = { channel: 'vscode-sysml', message: { type: 'diagram', view: 'stv', only: 'P::States' } };
  bridge.callbacks.forEach(fn => fn({ data: request }));
  assert.equal(bridge.sent.length, 0);
  bridge.setPalette('#eeeeee');
  bridge.frame.load();
  assert.deepEqual(bridge.sent.find(m => m.type === 'diagram'), { channel: 'sysml', type: 'diagram', view: 'stv', only: 'P::States' });
  assert.equal(bridge.sent.filter(m => m.type === 'palette').at(-1).colors['--vscode-editor-background'], '#eeeeee');
  assert.equal(bridge.sent.filter(m => m.type === 'theme').at(-1).appearance, 'light');
});

test('style and named-theme changes are observed even without a light/dark change', () => {
  const bridge = runBridge();
  bridge.frame.load();
  const observer = bridge.observers[0];
  assert.deepEqual(observer.targets.map(t => t.target), [bridge.document.body, bridge.document.documentElement]);
  assert.ok(observer.targets[0].options.attributeFilter.includes('style'));
  assert.ok(observer.targets[0].options.attributeFilter.includes('data-vscode-theme-id'));
  bridge.setPalette('#f5f5f5');
  observer.callback();
  assert.equal(bridge.sent.at(-1).colors['--vscode-editor-background'], '#f5f5f5');
});

test('canceling save leaves the current diagram and model untouched', async () => {
  const { api, fake, vscode, context } = load();
  const previous = { files: ['old.sysml'], panel: makePanel(), server: {}, url: 'http://127.0.0.1:7000' };
  api.setSession(previous);
  let saves = 0;
  vscode.workspace.textDocuments = [{ isDirty: true, uri: { scheme: 'file', fsPath: 'new.sysml' }, async save() { saves++; return true; } }];
  const result = await api.openStudio(context, { scheme: 'file', fsPath: 'new.sysml' });
  assert.equal(result, null);
  assert.equal(saves, 0);
  assert.equal(previous.panel.disposed, false);
  assert.equal(fake.children.length, 0);
});

test('a failed replacement server preserves the previous diagram and logs details', async () => {
  const { api, fake, context } = load();
  const previous = { files: ['old.sysml'], panel: makePanel(), server: {}, url: 'http://127.0.0.1:7000' };
  api.setSession(previous);
  fake.spawnError = new Error('spawn denied');
  assert.equal(await api.openStudio(context, { scheme: 'file', fsPath: 'new.sysml' }), null);
  assert.equal(previous.panel.disposed, false);
  assert.ok(fake.logs.some(m => m.includes('spawn denied')));
  assert.ok(fake.errors[0].includes('current diagram is unchanged'));
});

test('save and successful replacement happen before disposing the old panel', async () => {
  const { api, fake, vscode, context } = load();
  const previous = { files: ['old.sysml'], panel: makePanel(), server: {}, url: 'http://127.0.0.1:7000' };
  api.setSession(previous);
  fake.warningChoice = 'Save and Open';
  let saves = 0;
  vscode.workspace.textDocuments = [{ isDirty: true, uri: { scheme: 'file', fsPath: 'new.sysml' }, async save() { assert.equal(previous.panel.disposed, false); saves++; return true; } }];
  const running = await api.openStudio(context, { scheme: 'file', fsPath: 'new.sysml' });
  assert.equal(saves, 1);
  assert.equal(running.url, fake.readyUrl);
  assert.equal(previous.panel.disposed, true);
  assert.ok(running.panel.webview.html.includes('sysml.appearance=light'));
  fake.children[0].stdout.emit('data', Buffer.from('after readiness\n'));
  fake.children[0].stderr.emit('data', Buffer.from('stderr detail\n'));
  fake.children[0].emit('exit', 2, null);
  assert.ok(fake.logs.some(m => m.includes('after readiness')));
  assert.ok(fake.logs.some(m => m.includes('stderr detail')));
});

test('diagram picker offers built-ins and model views then sends the shared diagram schema', async t => {
  const server = http.createServer((request, response) => {
    assert.equal(request.url, '/api/views');
    response.setHeader('Content-Type', 'application/json');
    response.end(JSON.stringify([{ key: 'P::Saved', name: 'Saved', projection: 'stv', target: 'StateTransitionView' }]));
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => server.close());
  const { api, fake, context } = load();
  const running = { files: ['model.sysml'], panel: makePanel(), server: {}, url: `http://127.0.0.1:${server.address().port}` };
  api.setSession(running);
  fake.pick = items => items.find(i => i.only === 'P::Saved');
  await api.chooseDiagram(context);
  assert.equal(fake.items.filter(i => i.only === null).length, 7);
  assert.deepEqual(running.panel.messages.at(-1), { channel: 'vscode-sysml', message: {
    type: 'diagram', view: 'stv', only: 'P::Saved', scope: null, depth: null, layout: 'nested',
  } });
});

test('grid commands use the local server URL after the saved-model guard', async () => {
  const { api, fake, vscode, context } = load();
  const uri = { scheme: 'file', fsPath: 'model.sysml' };
  vscode.window.activeTextEditor = { document: { languageId: 'sysml', uri } };
  await api.showGrid(context, 'matrix');
  assert.equal(fake.grid.length, 1);
  assert.equal(fake.grid[0][0], context);
  assert.equal(fake.grid[0][1], fake.readyUrl);
  assert.equal(fake.grid[0][2], 'matrix');
});

test('element cursor selection resolves the deepest qualified symbol path', () => {
  const { api } = load();
  const range = (start, end) => ({ start: { line: start, character: 0 }, end: { line: end, character: 20 } });
  const symbols = [{ name: 'RoverSystem', range: range(0, 40), children: [
    { name: 'Rover', range: range(1, 15), children: [
      { name: 'chassis', range: range(3, 7), children: [
        { name: 'front', range: range(4, 4), children: [] },
      ] },
    ] },
  ] }];
  assert.equal(api.pathAtPosition(symbols, 4, 5), 'RoverSystem::Rover::chassis::front');
  assert.equal(api.pathAtPosition(symbols, 18, 5), 'RoverSystem');
  assert.equal(api.pathAtPosition([], 0, 0), null);
});

test('visualizeElement resolves the cursor and sends the chosen scoped layout', async () => {
  const { api, fake, vscode, context } = load();
  const uri = { scheme: 'file', fsPath: 'rover.sysml', toString() { return 'file:///rover.sysml'; } };
  const range = (start, end) => ({ start: { line: start, character: 0 }, end: { line: end, character: 30 } });
  vscode.window.activeTextEditor = { document: { languageId: 'sysml', uri }, selection: { active: { line: 10, character: 8 } } };
  const symbols = [{ name: 'RoverSystem', range: range(0, 30), children: [
    { name: 'Rover', range: range(8, 15), children: [] },
  ] }];
  api.setClient({ sendRequest: async (method) => { assert.equal(method, 'textDocument/documentSymbol'); return symbols; } });
  fake.warningChoice = 'Save and Open';
  fake.pick = items => items.find(item => item.value === 'tree');
  await api.visualizeElement(context);
  const panel = fake.panels.at(-1);
  assert.deepEqual(panel.messages.at(-1), { channel: 'vscode-sysml', message: {
    type: 'diagram', view: 'gv', only: null, scope: 'RoverSystem::Rover', depth: 3, layout: 'tree',
  } });
});

test('element visualization and depth commands require a real scoped diagram', async () => {
  const { api, fake, vscode, context } = load();
  await api.changeDiagramDepth();
  assert.match(fake.information, /Visualize Element/);
  vscode.window.activeTextEditor = { document: { languageId: 'plaintext', uri: { scheme: 'file' } } };
  await api.visualizeElement(context);
  assert.match(fake.information, /SysML/);
});

test('depth chooser retains scope, view and layout and can select All', async () => {
  const { api, fake } = load();
  const running = { files: ['rover.sysml'], panel: makePanel(), server: {}, url: 'http://127.0.0.1:7000',
    diagram: { view: 'gv', only: null, scope: 'RoverSystem::Rover', depth: 2, layout: 'tree' } };
  api.setSession(running);
  fake.pick = items => items.find(item => item.depth === null);
  await api.changeDiagramDepth();
  assert.deepEqual(running.panel.messages.at(-1), { channel: 'vscode-sysml', message: {
    type: 'diagram', view: 'gv', only: null, scope: 'RoverSystem::Rover', depth: null, layout: 'tree',
  } });
});

test('a scope-only host update is retained in session state for depth commands', async () => {
  const { api, fake } = load();
  const running = await api.openStudio({ extensionPath: 'extension', subscriptions: [] }, { scheme: 'file', fsPath: 'model.sysml' });
  running.panel.webview.receive({ type: 'diagramState', view: 'gv', only: null,
    scope: 'P::Car', depth: 2, layout: 'tree' });
  fake.pick = items => items.find(item => item.depth === 4);
  await api.changeDiagramDepth();
  assert.deepEqual(running.panel.messages.at(-1), { channel: 'vscode-sysml', message: {
    type: 'diagram', view: 'gv', only: null, scope: 'P::Car', depth: 4, layout: 'tree',
  } });
});

test('diagram defaults use configured values and fall back from invalid settings', () => {
  const { api, fake } = load();
  assert.deepEqual(api.diagramDefaults(), { depth: 3, layout: 'nested' });
  fake.settings['sysml.diagram.defaultDepth'] = 5;
  fake.settings['sysml.diagram.defaultElementRendering'] = 'tree';
  assert.deepEqual(api.diagramDefaults(), { depth: 5, layout: 'tree' });
  fake.settings['sysml.diagram.defaultDepth'] = 0;
  fake.settings['sysml.diagram.defaultElementRendering'] = 'sideways';
  assert.deepEqual(api.diagramDefaults(), { depth: 3, layout: 'nested' });
});

test('element visualization uses workspace diagram defaults and highlights the configured layout', async () => {
  const { api, fake, vscode, context } = load();
  const uri = { scheme: 'file', fsPath: 'rover.sysml', toString() { return 'file:///rover.sysml'; } };
  const range = (start, end) => ({ start: { line: start, character: 0 }, end: { line: end, character: 30 } });
  vscode.window.activeTextEditor = { document: { languageId: 'sysml', uri }, selection: { active: { line: 10, character: 8 } } };
  api.setClient({ sendRequest: async () => [{ name: 'Rover', range: range(8, 15), children: [] }] });
  fake.settings['sysml.diagram.defaultDepth'] = 5;
  fake.settings['sysml.diagram.defaultElementRendering'] = 'tree';
  fake.warningChoice = 'Save and Open';
  fake.pick = items => items.find(item => item.value === 'tree');
  await api.visualizeElement(context);
  assert.equal(fake.items.find(item => item.value === 'tree').picked, true);
  assert.deepEqual(fake.panels.at(-1).messages.at(-1).message, {
    type: 'diagram', view: 'gv', only: null, scope: 'Rover', depth: 5, layout: 'tree',
  });
});

test('native command contributions retain the old open command ID', () => {
  const manifest = JSON.parse(readFileSync(path.join(__dirname, '../package.json'), 'utf8'));
  const commands = new Map(manifest.contributes.commands.map(command => [command.command, command]));
  assert.equal(commands.get('sysml.openStudio').title, 'Open Diagram');
  for (const id of ['sysml.chooseDiagram', 'sysml.openTable', 'sysml.openMatrix', 'sysml.visualizeElement', 'sysml.changeDiagramDepth']) assert.ok(commands.has(id));
  assert.ok(manifest.contributes.menus['editor/context'].some(item => item.command === 'sysml.chooseDiagram'));
  assert.ok(manifest.contributes.menus['editor/context'].some(item => item.command === 'sysml.visualizeElement'));
  assert.ok(manifest.contributes.keybindings.some(item => item.command === 'sysml.visualizeElement' && item.key === 'ctrl+alt+e'));
});
