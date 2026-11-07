// Only validated RGBA crosses this boundary; never give peer encodings to the browser decoder.
window.stickerPixels = (frame, label, limit = 512) => {
  const { width, height, pixels } = frame;
  if (!Number.isInteger(width) || !Number.isInteger(height) || width < 1 || height < 1
    || width > limit || height > limit || pixels.length !== width * height * 4
    || !Array.from(pixels).every((value) => Number.isInteger(value) && value >= 0 && value <= 255)) {
    throw new Error('Invalid sticker pixels');
  }
  const canvas = document.createElement('canvas');
  canvas.width = width; canvas.height = height;
  canvas.setAttribute('role', 'img');
  canvas.setAttribute('aria-label', label);
  canvas.getContext('2d').putImageData(new ImageData(new Uint8ClampedArray(pixels), width, height), 0, 0);
  return canvas;
};
