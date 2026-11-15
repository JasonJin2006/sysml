const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const http = require('node:http');
const path = require('node:path');
const ts = require('typescript');
const { test } = require('node:test');

function load(vscode = {}) {
  const file = path.join(__dirname, '../src/grid.ts');
  const compiled = ts.transpileModule(readFileSync(file, 'utf8'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2020 },
  }).outputText;
  const module = { exports: {} };
  new Function('require', 'module', 'exports', compiled)(
    (name) => name === 'vscode' ? vscode : require(name), module, module.exports,
  );
  return module.exports;
}

function fixture(changes = {}) {
  return {
    title: 'Model elements', description: 'Saved model elements.', columns: ['qualified', 'kind', 'value'],
    rows: [['P::beta', 'part', '12'], ['P::alpha', 'attribute', '2'], ['P::gamma', 'part', '']],
    rowKeys: ['P::beta', 'P::alpha', 'P::gamma'],
    csv: 'qualified,kind,value\nP::beta,part,12\nP::alpha,attribute,2\nP::gamma,part,\n',
    ...changes,
  };
}

function pageScript(markup) {
  const scripts = [...markup.matchAll(/<script([^>]*)>([\s\S]*?)<\/script>/g)];
  return scripts.find((match) => !match[1].includes('application/json'))[2];
}

function fakePage(markup) {
  class Element {
    constructor(tag = 'div') { this.tag = tag; this.children = []; this.events = {}; this.attributes = {}; this.value = ''; this.textContent = ''; }
    appendChild(child) { this.children.push(child); }
    replaceChildren(...children) { this.children = [...children]; }
    setAttribute(name, value) { this.attributes[name] = value; }
    addEventListener(name, listener) { this.events[name] = listener; }
  }
  const ids = {};
  const document = {
    getElementById(id) { return ids[id] || (ids[id] = new Element()); },
    createElement(tag) { return new Element(tag); },
  };
  document.getElementById('grid-data').textContent = markup.match(/id="grid-data"[^>]*>([\s\S]*?)<\/script>/)[1];
  let receive;
  const messages = [];
  const window = { addEventListener(name, listener) { receive = listener; } };
  new Function('document', 'window', 'acquireVsCodeApi', pageScript(markup))(
    document, window, () => ({ postMessage(message) { messages.push(message); } }),
  );
  return { ids, messages, receive: (data) => receive({ data }) };
}

test('grid data validation rejects inconsistent rows and source identities', () => {
  const { parseGridData } = load();
  assert.deepEqual(parseGridData(fixture()), fixture());
  for (const value of [null, {}, fixture({ rowKeys: [] }), fixture({ rows: [['only one cell']] }),
    fixture({ rows: [[false, 'a', 'b']] }), fixture({ csv: 17 }), fixture({ columns: [] })]) {
    assert.throws(() => parseGridData(value), /invalid table data/);
  }
});

test('all-cell search and stable single-column sort preserve original row identities', () => {
  const { gridRowOrder } = load();
  const data = fixture();
  assert.deepEqual(gridRowOrder(data, 'ATTRIBUTE'), [1]);
  assert.deepEqual(gridRowOrder(data, '12'), [0]);
  assert.deepEqual(gridRowOrder(data, '', 0), [1, 0, 2]);
  assert.deepEqual(gridRowOrder(data, '', 2), [2, 1, 0]);
  assert.deepEqual(gridRowOrder(data, '', 0, true), [2, 0, 1]);
  assert.deepEqual(data, fixture());
  assert.deepEqual(gridRowOrder(data, 'not found'), []);
});

test('webview HTML has strict CSP and cannot execute model strings', () => {
  const { renderGridHtml } = load();
  const attack = '</script><script>throw new Error("injected")</script><img src=x onerror=alert(1)>';
  const data = fixture({ title: attack, description: attack, rows: [[attack, 'part', '12']], rowKeys: ['P::evil'] });
  const markup = renderGridHtml(data, 'matrix', 'testnonce');
  assert.match(markup, /default-src 'none'/);
  assert.match(markup, /script-src 'nonce-testnonce'/);
  assert.doesNotMatch(markup, /unsafe-inline|<script>throw|<img src=x/);
  const encoded = markup.match(/id="grid-data"[^>]*>([\s\S]*?)<\/script>/)[1];
  assert.deepEqual(JSON.parse(encoded), data);
  assert.doesNotThrow(() => new Function(pageScript(markup)));
  assert.match(markup, /Export all rows as CSV/);
  assert.match(markup, /Read-only/);
  assert.doesNotMatch(markup, /contenteditable|Save Model Changes/);
});

test('the generated script filters, sorts, sends row indices and keeps prior data on failure', () => {
  const page = fakePage(load().renderGridHtml(fixture(), 'elements', 'testnonce'));
  const labels = () => page.ids.rows.children.map((row) => row.children[0].children[0].textContent);
  assert.deepEqual(labels(), ['P::beta', 'P::alpha', 'P::gamma']);
  page.ids.columns.children[0].children[0].events.click();
  assert.deepEqual(labels(), ['P::alpha', 'P::beta', 'P::gamma']);
  page.ids.rows.children[0].children[0].children[0].events.click({ preventDefault() {} });
  assert.deepEqual(page.messages.pop(), { type: 'source', index: 1 });
  page.ids.search.value = 'part';
  page.ids.search.events.input();
  assert.deepEqual(labels(), ['P::beta', 'P::gamma']);
  assert.match(page.ids.count.textContent, /2 of 3 rows/);
  assert.match(page.ids.count.textContent, /includes all 3 rows/);
  page.ids.refresh.events.click();
  assert.equal(page.ids.refresh.disabled, true);
  page.receive({ type: 'error', message: 'Refresh failed. Previous data is unchanged.' });
  assert.deepEqual(labels(), ['P::beta', 'P::gamma']);
  assert.equal(page.ids.refresh.disabled, false);
  assert.equal(page.ids.problem.hidden, false);
  page.ids.export.events.click();
  assert.deepEqual(page.messages.pop(), { type: 'export' });
  page.receive(null);
});

function hostMock() {
  const state = { writes: [], opened: [], notifications: [], posted: [], disposals: [], logs: [], reveals: 0 };
  class Position { constructor(line, character) { this.line = line; this.character = character; } }
  class Range { constructor(start, end) { this.start = start; this.end = end; } }
  const panel = {
    title: '', webview: {
      html: '', onDidReceiveMessage(handler) { state.receive = handler; return { dispose() {} }; },
      async postMessage(message) { state.posted.push(message); return true; },
    },
    onDidDispose(handler) { state.disposals.push(handler); return { dispose() {} }; },
    reveal() { state.reveals += 1; },
  };
  const vscode = {
    Position, Range, ViewColumn: { Beside: 2 }, TextEditorRevealType: { InCenterIfOutsideViewport: 1 },
    Uri: { file(fsPath) { return { fsPath }; }, joinPath(folder, name) { return { fsPath: `${folder.fsPath}/${name}` }; } },
    window: {
      createOutputChannel() { return { appendLine(line) { state.logs.push(line); }, show() {}, dispose() {} }; },
      createWebviewPanel(type, title) { state.type = type; panel.title = title; return panel; },
      async showErrorMessage(message) { state.notifications.push(message); },
      async showInformationMessage(message) { state.notifications.push(message); return 'Open'; },
      async showSaveDialog() { return state.cancel ? undefined : { fsPath: 'C:/output.csv' }; },
      async showTextDocument(document, options) { state.opened.push({ document, options }); return { revealRange() {} }; },
    },
    workspace: {
      workspaceFolders: [{ uri: { fsPath: 'C:/model' } }],
      fs: { async writeFile(uri, bytes) { state.writes.push({ uri, text: Buffer.from(bytes).toString('utf8') }); } },
      async openTextDocument(uri) { return { uri }; },
    },
  };
  return { vscode, state, panel, context: { subscriptions: [] } };
}

async function serve(handler) {
  const server = http.createServer(handler);
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  return { base: `http://127.0.0.1:${server.address().port}`, close: () => new Promise((resolve) => server.close(resolve)) };
}

test('host exports backend CSV unchanged, treats cancel quietly and opens trusted source by row index', async () => {
  const calls = [];
  let broken = false;
  const endpoint = await serve((req, res) => {
    calls.push(req.url);
    if (broken) { res.writeHead(500); res.end('Model service unavailable'); return; }
    res.setHeader('Content-Type', 'application/json');
    res.end(JSON.stringify(req.url.startsWith('/api/source')
      ? { ok: true, path: 'C:/model/m.sysml', line: 7 } : fixture()));
  });
  const { vscode, context, state, panel } = hostMock();
  const { openGrid } = load(vscode);
  try {
    await openGrid(context, endpoint.base, 'elements');
    assert.equal(state.type, 'sysml.grid');
    assert.match(panel.webview.html, /Model elements/);
    await state.receive({ type: 'export' });
    assert.equal(state.writes[0].text, fixture().csv);
    assert.equal(state.opened[0].document.uri.fsPath, 'C:/output.csv');
    state.cancel = true;
    await state.receive({ type: 'export' });
    assert.equal(state.writes.length, 1);
    await state.receive({ type: 'source', index: 1 });
    assert.equal(state.opened[1].document.uri.fsPath, 'C:/model/m.sysml');
    assert.equal(state.opened[1].options.selection.start.line, 6);
    assert.equal(state.opened[1].options.preserveFocus, true);
    assert.ok(calls.some((url) => url.includes('key=P%3A%3Aalpha')));
    const before = calls.length;
    await state.receive({ type: 'source', index: -1 });
    await state.receive({ type: 'source', index: 55 });
    assert.equal(calls.length, before);
    broken = true;
    await state.receive({ type: 'refresh' });
    assert.equal(state.posted.at(-1).type, 'error');
    assert.match(state.posted.at(-1).message, /previous data is unchanged/);
    assert.match(panel.webview.html, /Model elements/);
    await openGrid(context, endpoint.base, 'elements');
    assert.equal(state.reveals, 1);
  } finally {
    state.disposals.forEach((dispose) => dispose());
    await endpoint.close();
  }
});

test('an initial backend failure leaves no empty grid panel and offers actionable output', async () => {
  const endpoint = await serve((req, res) => { res.writeHead(500); res.end('Cannot parse model'); });
  const { vscode, context, state } = hostMock();
  try {
    await load(vscode).openGrid(context, endpoint.base, 'matrix');
    assert.equal(state.type, undefined);
    assert.match(state.notifications[0], /Cannot open the requirement satisfaction matrix/);
    assert.match(state.notifications[0], /Open SysML Grids Output/);
  } finally {
    await endpoint.close();
  }
});
