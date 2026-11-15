import { test } from 'node:test';
import assert from 'node:assert/strict';
import { api, fetchDocument, fetchSource } from './api.ts';
import { setLang } from './i18n.ts';

test('a plain-text HTTP failure keeps a bounded reason and logs the complete body', async (context) => {
  const body = `Cannot load model: ${'missing file '.repeat(40)}`;
  context.mock.method(globalThis, 'fetch', async () => new Response(body, { status: 500 }));
  const warning = context.mock.method(console, 'warn', () => {});
  await assert.rejects(fetchDocument('gv', 'extended'), (error: Error) => {
    assert.match(error.message, /^Cannot update the diagram: Cannot load model:/);
    assert.ok(error.message.length < 300, error.message);
    assert.match(error.message, /The previous diagram is unchanged\.$/);
    return true;
  });
  assert.equal(warning.mock.calls[0].arguments[1].body, body);
});

test('a JSON HTTP failure uses its error field without serializing the response', async (context) => {
  context.mock.method(globalThis, 'fetch', async () => new Response(
    JSON.stringify({ error: 'The requested view does not exist.' }), { status: 400 },
  ));
  context.mock.method(console, 'warn', () => {});
  await assert.rejects(fetchDocument('gv', 'extended', 'P::missing'), {
    message: 'Cannot update the diagram: The requested view does not exist. The previous diagram is unchanged.',
  });
});

test('an HTML error page is kept in the log, not displayed in the notification', async (context) => {
  context.mock.method(globalThis, 'fetch', async () => new Response('<html><body>internal stack</body></html>', { status: 502 }));
  const warning = context.mock.method(console, 'warn', () => {});
  await assert.rejects(api('/api/document'), { message: 'The model service returned HTTP 502.' });
  assert.match(warning.mock.calls[0].arguments[1].body, /internal stack/);
});

test('a network failure explains how to reconnect without showing the browser exception', async (context) => {
  context.mock.method(globalThis, 'fetch', async () => { throw new TypeError('Failed to fetch'); });
  context.mock.method(console, 'warn', () => {});
  await assert.rejects(api('/api/fingerprint'), {
    message: 'Cannot reach the model service. Reopen the diagram and try again.',
  });
  await assert.rejects(fetchDocument('gv', 'extended'), {
    message: 'Cannot update the diagram: Cannot reach the model service. Reopen the diagram and try again. The previous diagram is unchanged.',
  });
});

test('an aborted document request retains the original AbortError and is not logged', async (context) => {
  const aborted = new DOMException('The operation was aborted', 'AbortError');
  context.mock.method(globalThis, 'fetch', async () => { throw aborted; });
  const warning = context.mock.method(console, 'warn', () => {});
  await assert.rejects(fetchDocument('gv', 'extended'), (error) => error === aborted);
  assert.equal(warning.mock.callCount(), 0);
});

test('a successful document remains intact and the signal reaches fetch', async (context) => {
  const value = { view: 'stv', nodes: [{ key: 'P::idle' }], frames: [], edges: [] };
  const controller = new AbortController();
  const fetch = context.mock.method(globalThis, 'fetch', async () => new Response(JSON.stringify(value)));
  assert.deepEqual(await fetchDocument('stv', 'extended', 'P::states', undefined, controller.signal), value);
  assert.match(String(fetch.mock.calls[0].arguments[0]), /only=P%3A%3Astates/);
  assert.equal(fetch.mock.calls[0].arguments[1]?.signal, controller.signal);
});

test('targeted diagrams encode scope, depth and layout independently', async (context) => {
  const fetch = context.mock.method(globalThis, 'fetch', async () => new Response(JSON.stringify({ view: 'gv' })));
  await fetchDocument('gv', 'extended', null, undefined, undefined, 'P::vehicle', 3, 'tree');
  const url = String(fetch.mock.calls[0].arguments[0]);
  assert.match(url, /scope=P%3A%3Avehicle/);
  assert.match(url, /depth=3/);
  assert.match(url, /layout=tree/);
});

test('node folding encodes qualified names and preserves explicit expansion', async (context) => {
  const fetch = context.mock.method(globalThis, 'fetch', async () => new Response('{}'));
  const folded = { 'P::vehicle::chassis': true, 'P::vehicle::power': false };
  await fetchDocument('gv', 'extended', null, undefined, undefined, 'P::vehicle', 2, 'tree', folded);
  const url = new URL(String(fetch.mock.calls[0].arguments[0]), 'http://localhost');
  assert.deepEqual(JSON.parse(url.searchParams.get('folded')!), folded);
  assert.equal(url.searchParams.get('depth'), '2');
  assert.equal(url.searchParams.get('scope'), 'P::vehicle');
});

test('source lookup failures return a readable reason rather than rejecting or losing it', async (context) => {
  context.mock.method(globalThis, 'fetch', async () => new Response('Source file is unavailable.', { status: 404 }));
  context.mock.method(console, 'warn', () => {});
  const result = await fetchSource('P::car');
  assert.equal(result.ok, false);
  assert.equal(result.error, 'Source file is unavailable.');
  assert.equal(result.text, '');
});

test('invalid successful JSON has a recovery message, not a parsing exception', async (context) => {
  context.mock.method(globalThis, 'fetch', async () => new Response('not JSON', { status: 200 }));
  context.mock.method(console, 'warn', () => {});
  await assert.rejects(api('/api/next'), {
    message: 'The model service returned an invalid response. Reopen the diagram and try again.',
  });
});

test('diagram request recovery messages are translated', async (context) => {
  context.mock.method(globalThis, 'fetch', async () => new Response('<html>failure</html>', { status: 500 }));
  context.mock.method(console, 'warn', () => {});
  setLang('zh');
  try {
    await assert.rejects(fetchDocument('gv', 'extended'), {
      message: '无法更新图表：模型服务返回了 HTTP 500。原图保持不变。',
    });
  } finally {
    setLang('en');
  }
});
