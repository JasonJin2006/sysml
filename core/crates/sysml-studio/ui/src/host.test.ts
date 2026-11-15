import { test } from 'node:test';
import assert from 'node:assert/strict';
import { diagramRequest, hostAppearance, hostKind, installHostBridge, onHost, reportDiagramState, type DiagramRequest } from './host.ts';

// The fragment is the only channel the editor has for saying "this page is a canvas, not the whole
// studio", because the server serves one `index.html` for every request and a fragment never
// reaches it. So the parsing rule is what decides how much of the interface a panel draws.

test('a page with no fragment is the whole studio', () => {
  assert.equal(hostKind(''), 'standalone');
  assert.equal(hostKind('#'), 'standalone');
});

test('the editor asks for the picture alone by naming itself', () => {
  assert.equal(hostKind('#sysml.host=vscode'), 'vscode');
});

test('another host value is not the editor', () => {
  assert.equal(hostKind('#sysml.host=jetbrains'), 'standalone');
});

test('the review routes are still standalone', () => {
  // `#design` and `#ide` are the design system's own pages, not an editor's canvas.
  assert.equal(hostKind('#design'), 'standalone');
  assert.equal(hostKind('#ide-light'), 'standalone');
});

test('the key survives other parameters beside it', () => {
  assert.equal(hostKind('#a=1&sysml.host=vscode&b=2'), 'vscode');
});

// The appearance rides in the fragment for the same reason `sysml.host` does: it has to be known
// before the first paint. A message arrives after the document has been drawn, so a light window
// showed a dark canvas for a frame and then corrected itself — which reads as a flash, not a theme.

test('the host can name the appearance in the fragment', () => {
  assert.equal(hostAppearance('#sysml.host=vscode&sysml.appearance=light'), 'light');
  assert.equal(hostAppearance('#sysml.host=vscode&sysml.appearance=dark'), 'dark');
});

test('no appearance named means the page keeps its own', () => {
  assert.equal(hostAppearance(''), null);
  assert.equal(hostAppearance('#sysml.host=vscode'), null);
});

test('a value that is not an appearance is ignored', () => {
  // A high-contrast theme is dark or light before it reaches here; a third word is a mistake, and
  // guessing at it would put the page in a palette the host did not ask for.
  assert.equal(hostAppearance('#sysml.appearance=high-contrast'), null);
  assert.equal(hostAppearance('#sysml.appearance='), null);
});

test('diagram messages distinguish a declared view from an element focus', () => {
  assert.deepEqual(diagramRequest({ view: 'stv', only: 'P::states' }), { view: 'stv', only: 'P::states' });
  assert.deepEqual(diagramRequest({ view: 'gv', focus: 'P::car' }), { view: 'gv', only: null, focus: 'P::car' });
  for (const value of [null, {}, { view: 'geometry' }, { view: 'gv', only: 1 }, { view: 'gv', focus: [] }]) {
    assert.equal(diagramRequest(value), null);
  }
});

test('the embedded page reports its successful diagram state to the host', () => {
  const previous = Object.getOwnPropertyDescriptor(globalThis, 'window');
  const sent: unknown[] = [];
  Object.defineProperty(globalThis, 'window', { configurable: true, value: {
    location: { hash: '#sysml.host=vscode' },
    parent: { postMessage(message: unknown) { sent.push(message); } },
  } });
  try {
    reportDiagramState({ view: 'gv', only: null, scope: 'P::Rover', depth: 2, layout: 'tree' });
    assert.deepEqual(sent, [{ channel: 'sysml', type: 'diagramState', view: 'gv', only: null,
      scope: 'P::Rover', depth: 2, layout: 'tree' }]);
  } finally {
    if (previous) Object.defineProperty(globalThis, 'window', previous);
    else Reflect.deleteProperty(globalThis, 'window');
  }
});

test('an early host diagram request is queued until the application listens', () => {
  const previous = Object.getOwnPropertyDescriptor(globalThis, 'window');
  const parent = { sent: [] as unknown[] };
  let receive: (event: unknown) => void = () => {};
  Object.defineProperty(globalThis, 'window', { configurable: true, value: {
    location: { hash: '#sysml.host=vscode' }, parent,
    addEventListener(_name: string, handler: (event: unknown) => void) { receive = handler; },
    parentTarget: parent,
    postMessage(message: unknown) { parent.sent.push(message); },
  } });
  const requests: DiagramRequest[] = [];
  let stop = () => {};
  try {
    installHostBridge();
    const data = { channel: 'sysml', type: 'diagram', view: 'stv', only: 'P::states' };
    receive({ source: parent, data });
    stop = onHost({ diagram: (request) => requests.push(request) });
    assert.deepEqual(requests, [{ view: 'stv', only: 'P::states' }]);
    receive({ source: {}, data: { ...data, view: 'pv' } });
    receive({ source: parent, data: { ...data, view: 'unsupported' } });
    assert.equal(requests.length, 1);
    receive({ source: parent, data: { channel: 'sysml', type: 'diagram', view: 'iv' } });
    assert.deepEqual(requests[1], { view: 'iv', only: null });
  } finally {
    stop();
    if (previous) Object.defineProperty(globalThis, 'window', previous);
    else Reflect.deleteProperty(globalThis, 'window');
  }
});
