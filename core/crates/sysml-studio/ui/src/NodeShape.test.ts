import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';
import type { CanvasFrame, CanvasNode } from './types.ts';

const built = await build({
  stdin: {
    contents: `
      import { createElement } from 'react';
      import { renderToStaticMarkup } from 'react-dom/server';
      import { NodeShape } from './NodeShape.tsx';
      import { FrameLayer } from './Scene.tsx';
      export { styleTheme } from './style.ts';
      export function renderFrames(frames) {
        return renderToStaticMarkup(createElement('svg', null,
          createElement(FrameLayer, { frames, selection: [], select() {} })));
      }
      export function renderNode(node, options = {}) {
        return renderToStaticMarkup(createElement('svg', null,
          createElement(NodeShape, {
            node, rect: node, selected: false,
            display: { stereotype: true, members: true, ports: true },
            judgement: null, onPointerDown() {}, ...options,
          })));
      }
      export function renderPair(node) {
        const props = { node, rect: node, selected: false,
          display: { stereotype: true, members: true, ports: true },
          judgement: null, onPointerDown() {} };
        return renderToStaticMarkup(createElement('svg', null,
          createElement(NodeShape, props), createElement(NodeShape, props)));
      }
    `,
    resolveDir: fileURLToPath(new URL('.', import.meta.url)),
    loader: 'ts',
  },
  bundle: true,
  platform: 'node',
  format: 'cjs',
  write: false,
});
const compiled = { exports: {} };
new Function('require', 'module', 'exports', built.outputFiles[0].text)(
  createRequire(import.meta.url), compiled, compiled.exports,
);
const { renderNode, renderPair, renderFrames, styleTheme } = compiled.exports as {
  renderNode: (node: CanvasNode, options?: { selected?: boolean; view?: string }) => string;
  renderPair: (node: CanvasNode) => string;
  renderFrames: (frames: CanvasFrame[]) => string;
  styleTheme: (name: 'light' | 'dark') => void;
};

function node(changes: Partial<CanvasNode> = {}): CanvasNode {
  return {
    key: 'P::wheel', index: 0, symbol: 'rectangle', stereotype: 'part', label: 'wheel',
    is_def: false, is_frame: false, unmet: false,
    x: 10, y: 20, w: 96, h: 34, placed: false,
    rows: [], ports: [], parent: null, ...changes,
  };
}

test('a header filling a usage stays inside all four rounded corners', () => {
  const svg = renderNode(node());
  const clip = svg.match(/<clipPath id="([^"]+)"><rect ([^>]+)><\/rect><\/clipPath>/);
  assert.ok(clip, svg);
  assert.match(clip[2], /x="10" y="20" width="96" height="34" rx="6"/);
  assert.ok(svg.includes(`clip-path="url(#${clip[1]})"`), svg);
  assert.ok(svg.indexOf('class="node-band"') < svg.indexOf('class="node-body"'), svg);
  assert.match(svg, /class="node-body" style="fill:none;stroke:[^"]+" pointer-events="none"/);
});

test('definitions retain square corners and every node has its own clip', () => {
  const svg = renderPair(node({ is_def: true }));
  const ids = [...svg.matchAll(/<clipPath id="([^"]+)"/g)].map((match) => match[1]);
  assert.equal(ids.length, 2);
  assert.notEqual(ids[0], ids[1]);
  assert.match(svg, /height="34" rx="0"/);
});

test('fallback node headers use theme ink instead of assumed black', () => {
  styleTheme('dark');
  const svg = renderNode(node({ stereotype: 'attribute' }));
  assert.match(svg, /class="label" style="fill:var\(--ui-text\)"/);
  assert.match(svg, /class="stereo" style="fill:var\(--ui-text\)"/);
  styleTheme('light');
});

test('rows, actors and ports use theme ink and port labels stay outside clipping', () => {
  const svg = renderNode(node({ h: 96, rows: ['mass : Real'], ports: [{ name: 'power', side: 'right', direction: null }] }));
  assert.match(svg, /class="row" style="fill:var\(--ui-text2\)"/);
  assert.match(svg, /class="port-name" style="fill:var\(--ui-text2\)"/);
  assert.ok(svg.indexOf('class="port-name"') > svg.indexOf('class="node-body"'));
  const actor = renderNode(node({ symbol: 'actor', stereotype: 'actor', h: 130 }));
  assert.match(actor, /stroke="var\(--ui-text\)"/);
  assert.match(actor, /class="label" style="fill:var\(--ui-text\)"/);
  assert.ok(!actor.includes('--node-border'));
});

test('selected outlines keep the selection colour above the header', () => {
  const svg = renderNode(node(), { selected: true });
  assert.match(svg, /class="node-body" style="fill:none;stroke:var\(--ui-accent\)"/);
});

test('sequence participants leave the message area transparent and cannot be resized', () => {
  const svg = renderNode(node({ w: 130, h: 500 }), { view: 'sv' });
  assert.match(svg, /class="node sequence-participant"/);
  assert.match(svg, /width="130" height="500" fill="transparent" pointer-events="none"/);
  assert.match(svg, /x1="75" x2="75" y1="66" y2="520"/);
  assert.match(svg, /stroke-dasharray="5 4" pointer-events="none"/);
  assert.match(svg, /width="130" height="46" class="node-body"/);
  assert.equal((svg.match(/class="node-body"/g) ?? []).length, 1);
  assert.ok(!svg.includes('node-resize'), svg);
  assert.ok(!svg.includes('node-band'), svg);
});

test('initial nodes are solid circles with no declaration box or label', () => {
  const svg = renderNode(node({ symbol: 'initial', stereotype: 'pseudo state', label: 'first', w: 20, h: 20 }));
  assert.match(svg, /class="node symbol-initial"/);
  assert.match(svg, /<circle cx="20" cy="30" r="10" fill="var\(--ui-text\)"/);
  assert.ok(!svg.includes('<text'), svg);
  assert.ok(!svg.includes('node-band') && !svg.includes('node-resize') && !svg.includes('node-body'), svg);
});

test('done nodes are double circles without a textual declaration or resize handle', () => {
  const svg = renderNode(node({ symbol: 'done', stereotype: 'pseudo state', label: 'done', w: 20, h: 20 }));
  assert.match(svg, /class="node symbol-done"/);
  assert.match(svg, /<circle cx="20" cy="30" r="10" fill="none" stroke="var\(--ui-text\)" stroke-width="1"/);
  assert.match(svg, /<circle cx="20" cy="30" r="6" fill="var\(--ui-text\)"/);
  assert.equal((svg.match(/<circle /g) ?? []).length, 2);
  assert.ok(!svg.includes('<text') && !svg.includes('node-band') && !svg.includes('node-resize'), svg);
  const selected = renderNode(node({ symbol: 'done', w: 20, h: 20 }), { selected: true });
  assert.match(selected, /fill="none" stroke="var\(--ui-accent\)" stroke-width="2"/);
  assert.match(selected, /r="6" fill="var\(--ui-accent\)"/);
});

for (const symbol of ['decision', 'merge'] as const) {
  test(`${symbol} nodes use a hollow diamond instead of a generic control-node box`, () => {
    const svg = renderNode(node({ symbol, stereotype: 'control node', label: 'route', w: 28, h: 28 }));
    assert.match(svg, /d="M24 20 L38 34 L24 48 L10 34 Z" fill="var\(--ui-canvas\)"/);
    assert.match(svg, /y="60"[^>]*class="row"[^>]*>route<\/text>/);
    assert.ok(!svg.includes('node-band') && !svg.includes('node-resize'), svg);
  });
}

for (const symbol of ['fork', 'join'] as const) {
  test(`${symbol} nodes use a solid synchronization bar`, () => {
    const svg = renderNode(node({ symbol, stereotype: 'control node', label: 'parallel', w: 60, h: 8 }));
    assert.match(svg, /width="60" height="8" fill="var\(--ui-text\)" stroke="var\(--ui-text\)"/);
    assert.ok(!svg.includes('node-band') && !svg.includes('node-resize'), svg);
  });
}

test('selected behavior symbols use the selection accent', () => {
  const svg = renderNode(node({ symbol: 'initial', w: 20, h: 20 }), { selected: true });
  assert.match(svg, /fill="var\(--ui-accent\)" stroke="var\(--ui-accent\)" stroke-width="2"/);
});

test('actor torsos meet the bottom of the circular head', () => {
  const svg = renderNode(node({ symbol: 'actor', stereotype: 'actor', h: 130 }));
  assert.match(svg, /cy="34" r="7"/);
  assert.match(svg, /d="M58,41 L58,86/);
});

test('stakeholders retain their declared rectangular symbol rather than becoming people', () => {
  const svg = renderNode(node({ symbol: 'rectangle', stereotype: 'stakeholder', label: 'operator' }));
  assert.match(svg, /class="node-body"/);
  assert.match(svg, /height="34" rx="6"/);
  assert.ok(svg.includes('«stakeholder»') && svg.includes('operator'), svg);
  assert.ok(!svg.includes('<circle'), svg);
});

test('state frames preserve definition and usage outlines and keep subaction rows below the title', () => {
  const frames: CanvasFrame[] = [
    { key: 'Machine', stereotype: 'state def', label: 'Machine', x: 0, y: 0, w: 400, h: 300, title: 52, rows: ['entry initialize'] },
    { key: 'Machine::active', stereotype: 'state', label: 'active', x: 20, y: 60, w: 300, h: 200, title: 52, rows: ['do monitor'] },
  ];
  const svg = renderFrames(frames);
  assert.match(svg, /x="0" y="0" width="400" height="300" rx="0" class="frame-body"/);
  assert.match(svg, /x="20" y="60" width="300" height="200" rx="6" class="frame-body"/);
  assert.match(svg, /x="8" y="16" class="frame-title">«state def» Machine<\/text>/);
  assert.match(svg, /y1="52" y2="52" class="frame-rule"/);
  assert.match(svg, /x="8" y="34"[^>]*>state actions<\/text>/);
  assert.match(svg, /x="8" y="48"[^>]*>entry initialize<\/text>/);
  assert.match(svg, /x="28" y="108"[^>]*>do monitor<\/text>/);
  assert.equal((svg.match(/>state actions<\/text>/g) ?? []).length, 2);
  assert.ok(svg.indexOf('data-key="Machine"') < svg.indexOf('data-key="Machine::active"'));
});

test('state action compartments reserve a heading before entry, do and exit rows', () => {
  const svg = renderNode(node({ stereotype: 'state', h: 96, rows: ['entry initialize', 'do monitor', 'exit release'] }));
  assert.match(svg, /x="58" y="65"[^>]*>state actions<\/text>/);
  assert.match(svg, /x="18" y="80"[^>]*>entry initialize<\/text>/);
  assert.match(svg, /x="18" y="96"[^>]*>do monitor<\/text>/);
  assert.match(svg, /x="18" y="112"[^>]*>exit release<\/text>/);
  assert.equal((svg.match(/>state actions<\/text>/g) ?? []).length, 1);
});

test('ordinary compartments and empty state frames do not gain state action headings', () => {
  const part = renderNode(node({ h: 64, rows: ['mass : Real'] }));
  assert.ok(!part.includes('state actions'), part);
  const state = renderNode(node({ stereotype: 'state' }));
  assert.ok(!state.includes('state actions'), state);
  const frames = renderFrames([
    { key: 'Machine', stereotype: 'state def', label: 'Machine', x: 0, y: 0, w: 200, h: 100, title: 22, rows: [] },
    { key: 'Package', stereotype: 'package', label: 'Package', x: 0, y: 120, w: 200, h: 100, title: 22, rows: [] },
  ]);
  assert.ok(!frames.includes('state actions'), frames);
});
