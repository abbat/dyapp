// Test-only DVF1 consumer. The producer lives in video_frames.rs; this file is injected
// into Linux UI smoke, never loaded by the normal desktop page.
globalThis.createVideoFrameRenderer = (canvas, requestFrame = requestAnimationFrame) => {
  const context = canvas.getContext('2d', { alpha: false });
  if (!context) throw new Error('Canvas 2D unavailable');
  const HEADER = 20;
  const SIZES = new Set(['640x360', '1280x720']);
  const MAX_SAMPLES = 120;
  const AGE_BIN_MS = 5;
  const AGE_BINS = 201; // Last bin includes ages of 1000 ms or more.
  const ages = [];
  const ageHistogram = new Uint32Array(AGE_BINS);
  let ageObservations = 0;
  let ageOver1000 = 0;
  let pending = null;
  let scheduled = false;
  let lastSequence = -1;
  const counts = { produced: 0, delivered: 0, painted: 0, dropped: 0, errors: 0 };
  let lastError = '';

  const reject = (reason) => {
    counts.errors += 1;
    lastError = reason;
    return false;
  };
  const paint = () => {
    scheduled = false;
    const frame = pending;
    pending = null;
    if (!frame) return;
    try {
      if (canvas.width !== frame.width || canvas.height !== frame.height) {
        canvas.width = frame.width;
        canvas.height = frame.height;
      }
      const pixels = new Uint8ClampedArray(frame.bytes, HEADER);
      context.putImageData(new ImageData(pixels, frame.width, frame.height), 0, 0);
      counts.painted += 1;
      const ageMs = Date.now() - frame.sentAtUs / 1000;
      if (Number.isFinite(ageMs) && ageMs >= 0 && ageMs <= 60000) {
        if (ages.length === MAX_SAMPLES) ages.shift();
        ages.push(ageMs);
        ageHistogram[Math.min(AGE_BINS - 1, Math.floor(ageMs / AGE_BIN_MS))] += 1;
        ageObservations += 1;
        if (ageMs >= 1000) ageOver1000 += 1;
      } else {
        reject('timestamp');
      }
    } catch (_) {
      reject('paint');
    }
  };
  return {
    accept(bytes) {
      counts.delivered += 1;
      if (!(bytes instanceof ArrayBuffer)) return reject('payload');
      if (bytes.byteLength < HEADER) return reject('header');
      const header = new DataView(bytes);
      if (header.getUint32(0, false) !== 0x44564631) return reject('magic');
      const width = header.getUint16(4, true);
      const height = header.getUint16(6, true);
      if (!SIZES.has(`${width}x${height}`)) return reject('dimensions');
      if (bytes.byteLength !== HEADER + width * height * 4) return reject('length');
      const sequence = header.getUint32(8, true);
      if (sequence <= lastSequence) {
        counts.dropped += 1;
        return false;
      }
      const sentAtUs = Number(header.getBigUint64(12, true));
      if (!Number.isSafeInteger(sentAtUs)) return reject('timestamp');
      lastSequence = sequence;
      if (pending) counts.dropped += 1;
      pending = { bytes, width, height, sentAtUs };
      if (!scheduled) {
        scheduled = true;
        requestFrame(paint);
      }
      return true;
    },
    setProduced(count) {
      if (Number.isInteger(count) && count >= 0 && count <= 1800) counts.produced = count;
    },
    metrics() {
      const percentile = (p) => {
        if (!ageObservations) return null;
        const target = Math.ceil(ageObservations * p);
        let seen = 0;
        for (let bin = 0; bin < AGE_BINS; bin += 1) {
          seen += ageHistogram[bin];
          if (seen >= target) return Math.min((bin + 1) * AGE_BIN_MS, 1000);
        }
        return null;
      };
      return {
        ...counts, lastSequence, lastError, ageSamples: ages.length,
        ageObservations, ageOver1000,
        p50AgeMs: percentile(0.5), p95AgeMs: percentile(0.95),
      };
    },
  };
};
