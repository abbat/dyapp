// Spectrum international design expansion table; T1 fixes the 70+ boundary at 130%.
window.expandPrototypeText = (text) => {
  if (!text) return text;
  const length = [...text].length;
  const expansion = [
    [10, 3],
    [20, 2],
    [30, 1.8],
    [50, 1.6],
    [69, 1.4],
  ].find(([max]) => length <= max);
  const factor = expansion ? expansion[1] : 1.3;
  return `[${text}${'·'.repeat(Math.max(0, Math.ceil(length * factor) - length - 2))}]`;
};
for (const [length, factor] of [
  [1, 3],
  [10, 3],
  [11, 2],
  [20, 2],
  [21, 1.8],
  [30, 1.8],
  [31, 1.6],
  [50, 1.6],
  [51, 1.4],
  [69, 1.4],
  [70, 1.3],
  [100, 1.3],
]) {
  if ([...window.expandPrototypeText('a'.repeat(length))].length !== Math.ceil(length * factor))
    throw new Error('prototype text expansion calibration failed');
}
