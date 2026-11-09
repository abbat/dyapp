smoke.checkDomContracts = async () => {
  const dom = await import(new URL('dom.js', document.baseURI).href);
  const root = document.createElement('div');
  const button = document.createElement('button');
  const input = document.createElement('input');
  root.append(button, input);
  const check = (condition, reason) => {
    if (!condition) throw new Error(reason);
  };
  check(dom.byId('create-identity', HTMLButtonElement).id === 'create-identity', 'DOM ID lookup');
  check(dom.query(root, 'button', HTMLButtonElement) === button, 'DOM required selector');
  check(dom.find(root, 'input', HTMLInputElement) === input, 'DOM optional selector');
  check(dom.find(root, '.absent', HTMLElement) === null, 'DOM optional absence');
  check(dom.all(root, 'button', HTMLButtonElement)[0] === button, 'DOM typed list');
  check(dom.all(root, '.absent', HTMLElement).length === 0, 'DOM empty list');
  const controls = dom.all(root, '*', HTMLButtonElement, HTMLInputElement);
  check(controls.length === 2 && controls[1] === input, 'DOM mixed control list');
  check(dom.eventDetail(new Event('plain')) === undefined, 'DOM plain event detail');
  check(
    dom.eventDetail(new CustomEvent('custom', { detail: 42 })) === 42,
    'DOM custom event detail',
  );
  const invalid = [
    () => dom.byId('dom-fixture-missing', HTMLElement),
    () => dom.byId('create-identity', HTMLInputElement),
    () => dom.query(root, '.absent', HTMLElement),
    () => dom.query(root, 'button', HTMLInputElement),
    () => dom.find(root, 'input', HTMLButtonElement),
    () => dom.all(root, '*', HTMLButtonElement),
  ];
  for (const run of invalid) {
    let rejected = false;
    try {
      run();
    } catch (error) {
      rejected = error instanceof Error && error.message.includes('element');
    }
    check(rejected, 'DOM boundary accepted a missing element or an incorrect kind');
  }
};
