// Linux debug smoke: actual binary Tauri Channel, Canvas 2D pixels, and bounded counters.
const videoFrames = async (size, count = 4) => {
  const canvas = document.createElement('canvas');
  canvas.setAttribute('role', 'img');
  canvas.setAttribute('aria-label', 'Synthetic video test frame');
  document.body.append(canvas);
  const renderer = globalThis.createVideoFrameRenderer(canvas);
  let received = 0;
  let channelError = '';
  let resolveFrames;
  const framesArrived = new Promise((resolve) => { resolveFrames = resolve; });
  const channel = new window.__TAURI__.core.Channel();
  channel.onmessage = (bytes) => {
    if (!renderer.accept(bytes)) channelError = renderer.metrics().lastError || 'stale';
    received += 1;
    if (received === count) resolveFrames();
  };
  const timeout = setTimeout(() => resolveFrames(), count === 4 ? 15000 : 90000);
  let produced;
  try {
    produced = await invokeTest('ui_test_video_frames', { size, count, onFrame: channel });
    await framesArrived;
  } finally {
    clearTimeout(timeout);
  }
  renderer.setProduced(produced);
  await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
  const metrics = renderer.metrics();
  const [width, height] = size === '360p' ? [640, 360] : [1280, 720];
  const pixel = [...canvas.getContext('2d').getImageData(0, 0, 1, 1).data];
  if (channelError || metrics.produced !== count || metrics.delivered !== count
    || metrics.painted < 1 || metrics.painted + metrics.dropped !== count
    || metrics.errors || metrics.lastSequence !== count - 1 || metrics.ageSamples > 120
    || canvas.width !== width || canvas.height !== height
    || pixel.join() !== '32,96,160,255') {
    throw new Error(`video ${size}: ${channelError} ${JSON.stringify(metrics)} ${pixel}`);
  }
  const bad = new ArrayBuffer(20 + width * height * 4 + 4);
  const header = new DataView(bad);
  header.setUint32(0, 0x44564631, false);
  header.setUint16(4, width, true);
  header.setUint16(6, height, true);
  if (renderer.accept(bad) || renderer.accept(new ArrayBuffer(4))
    || renderer.metrics().errors !== 2) throw new Error('malformed video frame accepted');
  canvas.remove();
  return metrics;
};
// A separate real-host benchmark step keeps the four-frame CI smoke quick.
globalThis.videoBenchmark = async (size) => {
  if (!['idle', '360p', '720p'].includes(size)) throw new Error('invalid video benchmark size');
  const metrics = size === 'idle'
    ? await new Promise((resolve) => setTimeout(() => resolve({ produced: 0, delivered: 0,
      painted: 0, dropped: 0, errors: 0 }), 60000))
    : await videoFrames(size, 1800);
  return invokeTest('ui_test_result', {
    passed: true, peerId: '', reason: `video:${size}:${JSON.stringify(metrics)}`,
  });
};
