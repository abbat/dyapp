// Focused DVF1 parser/Canvas checks; no Tauri or WebKit build required.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');

const source = fs.readFileSync(path.join(__dirname, '../clients/desktop/video-frame-renderer.js'), 'utf8');
const makeRenderer = vm.runInNewContext(`${source}\nglobalThis.createVideoFrameRenderer`, {
  ArrayBuffer, DataView, Uint8ClampedArray, Set, Number, Date,
  ImageData: class {
    constructor(data, width, height) {
      this.data = data;
      this.width = width;
      this.height = height;
    }
  },
});
const callbacks = [];
const painted = [];
let contexts = 0;
const canvas = {
  width: 0, height: 0,
  getContext(kind) {
    assert.equal(kind, '2d');
    contexts += 1;
    return { putImageData(image) { painted.push(image); } };
  },
};
const renderer = makeRenderer(canvas, (callback) => callbacks.push(callback));
const flush = () => {
  assert.equal(callbacks.length, 1);
  callbacks.shift()();
};
const frame = (width, height, sequence, length = 20 + width * height * 4) => {
  const bytes = new ArrayBuffer(length);
  const header = new DataView(bytes);
  header.setUint32(0, 0x44564631, false);
  header.setUint16(4, width, true);
  header.setUint16(6, height, true);
  header.setUint32(8, sequence, true);
  header.setBigUint64(12, BigInt(Date.now()) * 1000n, true);
  if (length >= 24) new Uint8Array(bytes, 20, 4).set([32, 96, 160, 255]);
  return bytes;
};

// Only the newest pending frame is painted; changing size reuses the same Canvas.
assert.equal(renderer.accept(frame(640, 360, 0)), true);
assert.equal(renderer.accept(frame(1280, 720, 1)), true);
flush();
assert.equal(contexts, 1);
assert.deepEqual([canvas.width, canvas.height], [1280, 720]);
assert.deepEqual([painted[0].width, painted[0].height], [1280, 720]);
assert.deepEqual([...painted[0].data.slice(0, 4)], [32, 96, 160, 255]);
assert.equal(renderer.accept(frame(640, 360, 2)), true);
flush();
assert.deepEqual([canvas.width, canvas.height], [640, 360]);
assert.equal(renderer.metrics().dropped, 1);
assert.equal(renderer.metrics().painted, 2);
assert.equal(renderer.metrics().lastSequence, 2);

// Reject non-binary, short, forged, unexpected-size, truncated and oversized frames.
assert.equal(renderer.accept(new Uint8Array(24)), false);
assert.equal(renderer.accept(new ArrayBuffer(4)), false);
const forged = frame(640, 360, 3);
new DataView(forged).setUint32(0, 0, false);
assert.equal(renderer.accept(forged), false);
assert.equal(renderer.accept(frame(800, 600, 3)), false);
assert.equal(renderer.accept(frame(640, 360, 3, 24)), false);
assert.equal(renderer.accept(frame(640, 360, 3, 20 + 640 * 360 * 4 + 4)), false);
const unsafeTime = frame(640, 360, 3);
new DataView(unsafeTime).setBigUint64(12, 0xffffffffffffffffn, true);
assert.equal(renderer.accept(unsafeTime), false);
assert.equal(renderer.accept(frame(640, 360, 2)), false);
assert.equal(renderer.metrics().errors, 7);
assert.equal(renderer.metrics().dropped, 2);
assert.equal(renderer.metrics().painted, 2);

// Age samples retain only the latest 120 observations and counters remain scalar.
const reused = frame(640, 360, 3);
for (let sequence = 3; sequence < 128; sequence += 1) {
  const header = new DataView(reused);
  header.setUint32(8, sequence, true);
  header.setBigUint64(12, BigInt(Date.now() - (sequence < 13 ? 900 : 0)) * 1000n, true);
  assert.equal(renderer.accept(reused), true);
  flush();
}
renderer.setProduced(1800);
renderer.setProduced(1801);
const metrics = renderer.metrics();
assert.equal(metrics.produced, 1800);
assert.equal(metrics.delivered, 136);
assert.equal(metrics.painted, 127);
assert.equal(metrics.ageSamples, 120);
assert.equal(metrics.ageObservations, 127);
assert.ok(metrics.ageOver1000 >= 0 && metrics.ageOver1000 <= 10);
assert.ok(metrics.p50AgeMs >= 0 && metrics.p95AgeMs >= 900 && metrics.p95AgeMs <= 1000);
console.log('OK video frame renderer');
