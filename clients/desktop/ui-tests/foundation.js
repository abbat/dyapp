(() => {
  const checkCatalogue = async (localization, catalogue) => {
    const before = [localization.locale, localization.pseudoLocale, localization.strings];
    const fetch = window.fetch;
    let rejected = false;
    try {
      window.fetch = async () => new Response(JSON.stringify(catalogue));
      await localization.loadLocale(async () => [false, false]);
    } catch (error) {
      rejected = error instanceof Error && error.message.startsWith('Invalid translation');
    } finally {
      window.fetch = fetch;
    }
    if (
      !rejected ||
      localization.locale !== before[0] ||
      localization.pseudoLocale !== before[1] ||
      localization.strings !== before[2]
    )
      throw new Error('Invalid catalogue accepted or changed locale state');
  };
  const checkCanvas = (pixels) => {
    const getContext = HTMLCanvasElement.prototype.getContext;
    let rejected = false;
    try {
      HTMLCanvasElement.prototype.getContext = () => null;
      pixels.stickerPixels({ width: 1, height: 1, pixels: [0, 0, 0, 255] }, 'fixture');
    } catch (error) {
      rejected = error instanceof Error && error.message === 'Sticker canvas unavailable';
    } finally {
      HTMLCanvasElement.prototype.getContext = getContext;
    }
    if (!rejected) throw new Error('Unavailable sticker canvas was accepted');
  };
  smoke.checkFoundationContracts = async () => {
    const localization = await import(new URL('locale.js', document.baseURI).href);
    const pixels = await import(new URL('sticker-pixels.js', document.baseURI).href);
    for (const catalogue of [null, [], 'text', { invalid: 1 }])
      await checkCatalogue(localization, catalogue);
    checkCanvas(pixels);
  };
})();
