// This public fixture is tests/fuzz/corpus/sticker_webp/lossy.webp, not peer media.
(() => {
  const name = 'Native smoke cats';
  const check = (ok, why) => {
    if (!ok) throw new Error(why);
  };
  const get = (id) => document.getElementById(`sticker-${id}`);
  const preventChooser = (event) => {
    if (event.target.id === 'sticker-file') event.preventDefault();
  };
  const archive = () => {
    const encode = (text) => new TextEncoder().encode(text);
    const image = Uint8Array.from(
      atob('UklGRi4AAABXRUJQVlA4ICIAAABwAQCdASoBAAEAAUAmJZQCdAFAAAD+/DeBV/fU6D4r4AAA'),
      (byte) => byte.charCodeAt(0),
    );
    const manifest = encode(
      JSON.stringify({
        format: 1,
        name,
        stickers: [
          {
            file: 'cat.webp',
            emoji: '😺',
            sha256: '83e7a26e05ed89fa9d6732336ca8cb93b0dceb591df75ebcb9bd72eece9bf82c',
          },
        ],
      }),
    );
    const entries = [
      ['manifest.json', manifest],
      ['cat.webp', image],
    ];
    const bytes = new Uint8Array(
      1024 +
        entries.reduce((total, [, data]) => total + 512 + Math.ceil(data.length / 512) * 512, 0),
    );
    let offset = 0;
    for (const [filename, data] of entries) {
      const header = bytes.subarray(offset, offset + 512);
      header.set(encode(filename));
      header.set(encode(`${data.length.toString(8).padStart(11, '0')}\0`), 124);
      header.fill(32, 148, 156);
      header[156] = 48;
      header.set(encode('ustar'), 257);
      const sum = header.reduce((total, byte) => total + byte, 0);
      header.set(encode(`${sum.toString(8).padStart(6, '0')}\0 `), 148);
      bytes.set(data, offset + 512);
      offset += 512 + Math.ceil(data.length / 512) * 512;
    }
    return bytes;
  };
  const select = (input, file) => {
    const transfer = new DataTransfer();
    if (file) transfer.items.add(file);
    input.files = transfer.files;
    input.dispatchEvent(new Event('change', { bubbles: true }));
    return window.stickerImported;
  };
  const ownership = async () => {
    const manager = document.getElementById('sticker-manager');
    manager.addEventListener('click', preventChooser, true);
    let imports = 0;
    const call = async (command) => {
      if (command === 'import_sticker_pack') imports += 1;
      return { entries: [], next: null, usedBytes: 0, unrecognizedFiles: 0 };
    };
    try {
      await window.stickerCollection.open(call);
      get('import').click();
      const old = get('file');
      window.stickerCollection.close();
      await window.stickerCollection.open(call);
      get('import').click();
      const latest = get('file');
      await select(old, new File([archive()], 'old-owner.tar'));
      check(imports === 0, 'Old file selection imported into a reopened manager');
      await select(latest, new File([archive()], 'current-owner.tar'));
      check(imports === 1, 'Current file selection did not import once');
    } finally {
      manager.removeEventListener('click', preventChooser, true);
      window.stickerCollection.close();
    }
  };
  const readback = async () => {
    const identityId = current.peerId;
    const page = await invoke('saved_sticker_packs', { identityId, after: null });
    const pack = page.entries.find((entry) => entry.name === name && !entry.error);
    check(pack, 'Imported pack did not survive native reopen');
    const content = await invoke('saved_sticker_page', { identityId, packId: pack.id, after: 0 });
    check(
      JSON.stringify(content.packId) === JSON.stringify(pack.id),
      'Native page returned another pack ID',
    );
    check(
      content.name === name &&
        content.total === 1 &&
        content.entries.length === 1 &&
        content.entries[0].emoji === '😺',
      'Native imported manifest changed',
    );
    const frame = content.entries[0].frame;
    check(
      frame.width > 0 &&
        frame.width <= 96 &&
        frame.height > 0 &&
        frame.height <= 96 &&
        frame.pixels.length === frame.width * frame.height * 4,
      'Native pack thumbnail is not bounded RGBA',
    );
    return pack.id;
  };
  const importPack = async () => {
    await ownership();
    const manager = document.getElementById('sticker-manager');
    manager.addEventListener('click', preventChooser, true);
    let imports = 0,
      importedId = null;
    const identityId = current.peerId;
    const call = async (command, args) => {
      const result = await invoke(command, args);
      if (command === 'import_sticker_pack') {
        imports += 1;
        importedId = result;
        check(args.identityId === identityId, 'Real file import changed owner');
      }
      return result;
    };
    try {
      await window.stickerCollection.open(call);
      get('import').click();
      await select(get('file'), null);
      check(imports === 0, 'Empty file selection invoked native import');
      get('import').click();
      await select(get('file'), new File([new ArrayBuffer(8 * 1024 * 1024 + 1)], 'too-large.tar'));
      check(
        imports === 0 && get('status').textContent === window.strings['stickers.tooLarge'],
        'Real oversized file reached native import',
      );
      get('import').click();
      await select(
        get('file'),
        new File([archive()], 'native-stickers.tar', { type: 'application/x-tar' }),
      );
      check(
        imports === 1 && importedId?.length === 32,
        `Real file change did not complete native import: ${get('reason').textContent}`,
      );
      check(
        JSON.stringify(await readback()) === JSON.stringify(importedId),
        'Native readback selected another pack',
      );
      await shot('stickers-file-import');
    } finally {
      manager.removeEventListener('click', preventChooser, true);
      window.stickerCollection.close();
    }
  };
  const restart = async () => {
    await readback();
    try {
      await window.stickerCollection.open();
      await shot('stickers-file-restart');
    } finally {
      window.stickerCollection.close();
    }
  };
  window.stickerFileChecks = { ownership, importPack, readback, restart };
})();
