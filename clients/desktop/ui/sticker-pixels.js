// Native binary framing is checked before allocating pixels or creating a canvas.
window.stickerBinaryPixels = (value, label) => {
  let bytes;
  if (value instanceof ArrayBuffer) bytes = new Uint8Array(value);
  else if (ArrayBuffer.isView(value))
    bytes = new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  else if (
    Array.isArray(value) &&
    value.length <= 8 + 512 * 512 * 4 &&
    value.every((byte) => Number.isInteger(byte) && byte >= 0 && byte <= 255)
  )
    bytes = new Uint8Array(value);
  else throw new Error('Invalid sticker frame');
  if (bytes.byteLength < 8 || bytes.byteLength > 8 + 512 * 512 * 4)
    throw new Error('Invalid sticker frame');
  const header = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return window.stickerPixels(
    {
      width: header.getUint32(0, true),
      height: header.getUint32(4, true),
      pixels: bytes.subarray(8),
    },
    label,
  );
};
// Only validated RGBA crosses this boundary; never give peer encodings to the browser decoder.
window.stickerPixels = (frame, label, limit = 512) => {
  const { width, height, pixels } = frame;
  if (
    !Number.isInteger(width) ||
    !Number.isInteger(height) ||
    width < 1 ||
    height < 1 ||
    width > limit ||
    height > limit ||
    pixels.length !== width * height * 4 ||
    !Array.from(pixels).every((value) => Number.isInteger(value) && value >= 0 && value <= 255)
  ) {
    throw new Error('Invalid sticker pixels');
  }
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  canvas.setAttribute('role', 'img');
  canvas.setAttribute('aria-label', label);
  canvas
    .getContext('2d')
    .putImageData(new ImageData(new Uint8ClampedArray(pixels), width, height), 0, 0);
  return canvas;
};
