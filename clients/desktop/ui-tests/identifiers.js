(() => {
  const checkShortIdentifierReplacement = async (code, formatted, check) => {
    for (const short of ['@3f9c2a', '@3f9c2a']) {
      code.textContent = short;
      await formatted(short);
      check(
        code.textContent === short &&
          code.querySelectorAll('.identifier-chunk').length === 1 &&
          !code.querySelector('wbr'),
        'Equal short identifier replacement lost its chunk',
      );
    }
  };
  const checkIdentifierContent = async (code, selection, formatted, check) => {
    check(typeof Intl.Segmenter === 'function', 'Target engine lacks grapheme segmentation');
    const raw = '0123456789abcdef'.repeat(4);
    code.textContent = raw;
    await formatted(raw);
    check(
      code.classList.contains('identifier') && code.getAttribute('aria-label'),
      'Identifier has no shared semantics',
    );
    check(
      code.textContent === raw && code.querySelectorAll('wbr').length === 7,
      'Identifier chunking changed raw text',
    );
    const range = document.createRange();
    range.selectNodeContents(code);
    check(range.toString() === raw, 'Identifier copy range is not raw');
    selection.removeAllRanges();
    selection.addRange(range);
    check(selection.toString() === raw, 'Visual identifier selection changed copied text');
    code.textContent = raw; // Replacing a value with the same text must restore lost break nodes.
    await formatted(raw);
    await checkShortIdentifierReplacement(code, formatted, check);
    const unicode = '👩‍👩‍👧‍👦'.repeat(9) + 'e\u0301'.repeat(9);
    code.textContent = unicode;
    await formatted(unicode);
    const chunks = [...code.querySelectorAll('.identifier-chunk')];
    const segmenter = new Intl.Segmenter(undefined, { granularity: 'grapheme' });
    check(
      chunks.map((node) => [...segmenter.segment(node.textContent)].length).join() === '8,8,2',
      'Identifier split a grapheme cluster',
    );
    const style = getComputedStyle(code);
    const gap = Number.parseFloat(
      getComputedStyle(document.documentElement).getPropertyValue('--space-3'),
    );
    check(
      Number.parseFloat(style.marginTop) >= gap &&
        Number.parseFloat(style.marginBottom) >= gap &&
        style.display === 'block',
      'Identifier spacing differs from shared token',
    );
    const markup = '<img src=x onerror=alert(1)>'.repeat(2);
    code.textContent = markup;
    await formatted(markup);
    check(
      !code.querySelector('img') && code.textContent === markup,
      'Identifier interpreted markup',
    );
  };
  const checkScopedIdentifiers = async (formatted, check) => {
    const { installIdentifiers } = await import(new URL('identifiers.js', document.baseURI).href);
    const container = document.createElement('div');
    const scoped = document.createElement('code');
    const outside = document.createElement('code');
    const raw = 'abcdef01'.repeat(8);
    scoped.textContent = raw;
    outside.textContent = raw;
    container.append(scoped, outside);
    installIdentifiers(appApi.strings, scoped);
    check(
      scoped.textContent === raw && scoped.querySelectorAll('wbr').length === 7,
      'Scoped identifier was not formatted',
    );
    const next = 'fedcba98'.repeat(8);
    const completed = formatted(next, scoped);
    scoped.textContent = next;
    await completed;
    check(
      scoped.textContent === next && scoped.querySelectorAll('wbr').length === 7,
      'Scoped identifier replacement lost formatting',
    );
    check(
      outside.textContent === raw &&
        !outside.classList.contains('identifier') &&
        !outside.querySelector('wbr'),
      'Scoped formatter changed an outside identifier',
    );
  };
  smoke.checkIdentifiers = async () => {
    const selection = window.getSelection();
    const savedRanges = Array.from({ length: selection.rangeCount }, (_, index) =>
      selection.getRangeAt(index).cloneRange(),
    );
    const box = document.createElement('div');
    box.style.maxWidth = '32ch';
    const code = document.createElement('code');
    box.append(code);
    document.body.append(box);
    const check = (ok, reason) => {
      if (!ok) throw Error(reason);
    };
    const formatted = (raw, target = code) =>
      new Promise((resolve, reject) => {
        const ready = () => target.textContent === raw && target.querySelector('.identifier-chunk');
        if (ready()) {
          resolve();
          return;
        }
        const observer = new MutationObserver(() => {
          if (ready()) {
            observer.disconnect();
            clearTimeout(deadline);
            resolve();
          }
        });
        observer.observe(target, { attributes: true, childList: true, subtree: true });
        const deadline = setTimeout(() => {
          observer.disconnect();
          reject(Error('Identifier format deadline'));
        }, 10000);
      });
    try {
      await checkIdentifierContent(code, selection, formatted, check);
      await checkScopedIdentifiers(formatted, check);
    } finally {
      selection.removeAllRanges();
      for (const range of savedRanges) selection.addRange(range);
      box.remove();
    }
  };
})();
