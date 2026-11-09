// Test-only DVF1 consumer. The producer lives in video_frames.rs; this file is injected
// into Linux UI smoke, never loaded by the normal desktop page.
(() => {
  const HEADER = 20;
  const SIZES = new Set(['640x360', '1280x720']);
  const MAX_SAMPLES = 120;
  const AGE_BIN_MS = 5;
  const AGE_BINS = 201; // Last bin includes ages of 1000 ms or more.
  const reject = (state, reason) => {
    state.counts.errors += 1;
    state.lastError = reason;
    return false;
  };
  const observeAge = (state, sentAtUs) => {
    const ageMs = Date.now() - sentAtUs / 1000;
    if (Number.isFinite(ageMs) && ageMs >= 0 && ageMs <= 60000) {
      if (state.ages.length === MAX_SAMPLES) state.ages.shift();
      state.ages.push(ageMs);
      state.ageHistogram[Math.min(AGE_BINS - 1, Math.floor(ageMs / AGE_BIN_MS))] += 1;
      state.ageObservations += 1;
      if (ageMs >= 1000) state.ageOver1000 += 1;
    } else {
      reject(state, 'timestamp');
    }
  };
  const paintFrame = (state) => {
    state.scheduled = false;
    const frame = state.pending;
    state.pending = null;
    if (!frame) return;
    try {
      const { canvas, context } = state;
      if (canvas.width !== frame.width || canvas.height !== frame.height) {
        canvas.width = frame.width;
        canvas.height = frame.height;
      }
      const pixels = new Uint8ClampedArray(frame.bytes, HEADER);
      context.putImageData(new ImageData(pixels, frame.width, frame.height), 0, 0);
      state.counts.painted += 1;
      observeAge(state, frame.sentAtUs);
    } catch (_) {
      reject(state, 'paint');
    }
  };
  const readHeader = (state, bytes) => {
    if (!(bytes instanceof ArrayBuffer)) return reject(state, 'payload');
    if (bytes.byteLength < HEADER) return reject(state, 'header');
    const header = new DataView(bytes);
    if (header.getUint32(0, false) !== 0x44564631) return reject(state, 'magic');
    const width = header.getUint16(4, true);
    const height = header.getUint16(6, true);
    if (!SIZES.has(`${width}x${height}`)) return reject(state, 'dimensions');
    if (bytes.byteLength !== HEADER + width * height * 4) return reject(state, 'length');
    return header;
  };
  const acceptFrame = (state, bytes, requestFrame, paint) => {
    state.counts.delivered += 1;
    const header = readHeader(state, bytes);
    if (!header) return false;
    const width = header.getUint16(4, true);
    const height = header.getUint16(6, true);
    const sequence = header.getUint32(8, true);
    if (sequence <= state.lastSequence) {
      state.counts.dropped += 1;
      return false;
    }
    const sentAtUs = Number(header.getBigUint64(12, true));
    if (!Number.isSafeInteger(sentAtUs)) return reject(state, 'timestamp');
    state.lastSequence = sequence;
    if (state.pending) state.counts.dropped += 1;
    state.pending = { bytes, width, height, sentAtUs };
    if (!state.scheduled) {
      state.scheduled = true;
      requestFrame(paint);
    }
    return true;
  };
  const percentile = (state, p) => {
    if (!state.ageObservations) return null;
    const target = Math.ceil(state.ageObservations * p);
    let seen = 0;
    for (let bin = 0; bin < AGE_BINS; bin += 1) {
      seen += state.ageHistogram[bin];
      if (seen >= target) return Math.min((bin + 1) * AGE_BIN_MS, 1000);
    }
    return null;
  };
  const metrics = (state) => ({
    ...state.counts,
    lastSequence: state.lastSequence,
    lastError: state.lastError,
    ageSamples: state.ages.length,
    ageObservations: state.ageObservations,
    ageOver1000: state.ageOver1000,
    p50AgeMs: percentile(state, 0.5),
    p95AgeMs: percentile(state, 0.95),
  });
  smoke.createVideoFrameRenderer = (canvas, requestFrame = requestAnimationFrame) => {
    const context = canvas.getContext('2d', { alpha: false });
    if (!context) throw new Error('Canvas 2D unavailable');
    const state = {
      canvas,
      context,
      ages: [],
      ageHistogram: new Uint32Array(AGE_BINS),
      ageObservations: 0,
      ageOver1000: 0,
      pending: null,
      scheduled: false,
      lastSequence: -1,
      counts: { produced: 0, delivered: 0, painted: 0, dropped: 0, errors: 0 },
      lastError: '',
    };
    const paint = () => paintFrame(state);
    return {
      accept(bytes) {
        return acceptFrame(state, bytes, requestFrame, paint);
      },
      setProduced(count) {
        if (Number.isInteger(count) && count >= 0 && count <= 1800) state.counts.produced = count;
      },
      metrics() {
        return metrics(state);
      },
    };
  };
})();
