import { test } from 'node:test';
import assert from 'node:assert/strict';
import { fitCamera, MIN_READABLE_ZOOM } from './geometry.ts';

const doc = (width: number, height: number) => ({ width, height }) as any;
const box = { width: 1000, height: 600 };

test('a picture that fits keeps the zoom readable', () => {
  // 1200x700 asks for 0.75 by height, which is the floor, so the floor is what applies.
  const camera = fitCamera(doc(1200, 700), box)!;
  assert.equal(camera.zoom, MIN_READABLE_ZOOM);
});

test('a picture several times the viewport zooms out to fit instead of showing a corner', () => {
  // The action-flow view of a model with three machines is 1600x3450 in a 570-high pane. Held at
  // the readable floor only the top-left corner is on screen, and the successions — which run
  // between boxes further down — are outside the window.
  const tall = fitCamera(doc(1600, 3450), box)!;
  assert.ok(tall.zoom < MIN_READABLE_ZOOM, `expected below the floor, got ${tall.zoom}`);
  assert.ok(
    tall.zoom * 3450 <= box.height || tall.zoom * 1600 <= box.width,
    'the whole picture should fit on at least the axis that allows it',
  );
});

test('the floor still wins when it hides less than two and a half viewports', () => {
  // 1200x1200 in a 1000x600 pane: the floor shows 900x900, which overflows by less than 2.5x.
  const camera = fitCamera(doc(1200, 1200), box)!;
  assert.equal(camera.zoom, MIN_READABLE_ZOOM);
});

test('a picture that fits is centred', () => {
  const camera = fitCamera(doc(400, 200), box)!;
  assert.ok(camera.x > 0 && camera.y > 0, 'a small picture sits in the middle');
});
