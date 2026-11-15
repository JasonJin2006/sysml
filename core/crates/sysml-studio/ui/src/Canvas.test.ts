import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { build } from 'esbuild';

const built = await build({
  stdin: {
    contents: `
      import { createElement } from 'react';
      import { renderToStaticMarkup } from 'react-dom/server';
      import { Canvas } from './Canvas.tsx';
      export function renderCanvas(view, arrow = 'composite', shared = false) {
        const node = (key, index, x) => ({ key, index, x, y: 60, w: 96, h: 34,
          symbol: 'rectangle', stereotype: 'part', label: key, is_def: false, is_frame: false,
          unmet: false, rows: [], ports: [], parent: null, placed: false });
        const doc = { view, nodes: [node('source', 0, 20), node('target', 1, 220)],
          edges: [{ index: 0, from: 0, to: 1, from_key: 'source', to_key: 'target',
            arrow, label: null, points: [[116,77],[220,77]], hops: [] }],
          frames: [{ key: 'frame', stereotype: 'package', label: 'Frame', x: 0, y: 0, w: 360, h: 180, title: 22, rows: [] }] };
        if (shared) doc.edges.push({ ...doc.edges[0], index: 1 });
        return renderToStaticMarkup(createElement(Canvas, {
          doc, view, camera: { x: 0, y: 0, zoom: 1 }, setCamera() {},
          selection: [], select() {}, display: { stereotype: true, members: true, ports: true },
          marks: null, overrides: {}, setOverrides() {}, onPlaced() {}, onDragChange() {},
          viewportRef: { current: null },
        }));
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
const { renderCanvas } = compiled.exports as { renderCanvas: (view: string, arrow?: string, shared?: boolean) => string };

test('shared composite anchors draw one diamond but keep every clickable relationship', () => {
  const svg = renderCanvas('gv', 'composite', true);
  assert.equal((svg.match(/fill="var\(--ui-edge\)" stroke="var\(--ui-edge\)"/g) ?? []).length, 1);
  assert.equal((svg.match(/class="edge hit"/g) ?? []).length, 2);
});

test('opaque behavior frames paint behind transition edges', () => {
  const svg = renderCanvas('stv');
  const frame = svg.indexOf('class="frame-body"');
  const edge = svg.indexOf('class="edge composite"');
  assert.ok(frame >= 0 && edge > frame, svg);
});

for (const view of ['iv', 'sv']) {
  test(`${view} edges remain visible over containing node bodies`, () => {
    const svg = renderCanvas(view);
    const node = svg.lastIndexOf('class="node-body"');
    const edge = svg.indexOf('class="edge composite"');
    assert.ok(node >= 0 && edge > node, svg);
  });
}

for (const arrow of ['satisfy', 'annotate', 'transition']) {
  test(`${arrow} relationships use open arrowheads, not filled triangles`, () => {
    const svg = renderCanvas('gv', arrow);
    const head = svg.match(/<path d="([^"]+)" stroke="var\(--ui-edge\)" stroke-width="1\.1" fill="none"/);
    assert.ok(head, svg);
    assert.ok(!head[1].includes('Z'), head[1]);
    assert.ok(!svg.includes('fill="var(--ui-edge)"'), svg);
  });
}
