import { byId as domById } from './dom.js';

/** @param {import('./ipc.js').QrMatrix} matrix */
export const drawContactQr = (
  { size, modules },
  canvas = domById('view-safety-qr', HTMLCanvasElement),
) => {
  if (!Number.isInteger(size) || size < 21 || size > 177 || modules?.length !== size * size) {
    throw new Error('invalid contact QR matrix');
  }
  const scale = 6;
  canvas.width = (size + 8) * scale;
  canvas.height = canvas.width;
  const context = canvas.getContext('2d');
  if (!context) throw new Error('Canvas rendering is unavailable');
  context.fillStyle = '#fff';
  context.fillRect(0, 0, canvas.width, canvas.height);
  context.fillStyle = '#000';
  for (let y = 0; y < size; y += 1) {
    for (let x = 0; x < size; x += 1) {
      if (modules[y * size + x]) context.fillRect((x + 4) * scale, (y + 4) * scale, scale, scale);
    }
  }
};

/** @param {CanvasRenderingContext2D} context @param {number} width @param {number} height */
const grayscaleFrame = (context, width, height) => {
  const rgba = context.getImageData(0, 0, width, height).data;
  const pixels = new Array(width * height);
  for (let i = 0; i < pixels.length; i += 1) {
    const at = i * 4;
    const luma = Math.round((rgba[at] * 299 + rgba[at + 1] * 587 + rgba[at + 2] * 114) / 1000);
    pixels[i] = Math.round((luma * rgba[at + 3] + 255 * (255 - rgba[at + 3])) / 255);
  }
  return pixels;
};

/**
 * Gray frame of `source` on white, scaled down to the side the native scanner accepts.
 * @param {CanvasImageSource} source @param {number} sourceWidth @param {number} sourceHeight
 */
export const qrFrame = (source, sourceWidth, sourceHeight) => {
  const scale = Math.min(1, 768 / Math.max(sourceWidth, sourceHeight));
  const width = Math.max(1, Math.round(sourceWidth * scale));
  const height = Math.max(1, Math.round(sourceHeight * scale));
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext('2d', { willReadFrequently: true });
  if (!context) throw new Error('Canvas rendering is unavailable');
  context.fillStyle = '#fff';
  context.fillRect(0, 0, width, height);
  context.drawImage(source, 0, 0, width, height);
  return { width, height, pixels: grayscaleFrame(context, width, height) };
};

/** @param {HTMLVideoElement} preview */
export const safetyCameraFrame = (preview) =>
  !preview.videoWidth || !preview.videoHeight
    ? null
    : qrFrame(preview, preview.videoWidth, preview.videoHeight);
