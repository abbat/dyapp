// Dialog criteria for the WebKit prototype screenshot runner.
smoke.criteria = (() => {
  const visible = (element) =>
    element.getClientRects().length > 0 && getComputedStyle(element).visibility === 'visible';
  const name = (element) => {
    if (element.id) return `#${element.id}`;
    if (!element.parentElement) return element.tagName.toLowerCase();
    const siblings = [...element.parentElement.children].filter(
      (node) => node.tagName === element.tagName,
    );
    return `${name(element.parentElement)}>${element.tagName.toLowerCase()}:nth-of-type(${siblings.indexOf(element) + 1})`;
  };
  const defaults = (view) =>
    [...view.querySelectorAll('button.primary, button[data-default]')].filter(visible);
  const titleFinding = (dialog) => {
    const title = document.getElementById(dialog.getAttribute('aria-labelledby'));
    if (!title || !visible(title))
      return { rule: 'D1', element: name(dialog), detail: 'missing visible dialog title' };
    const range = document.createRange();
    range.selectNodeContents(title);
    const lines = new Set(
      [...range.getClientRects()]
        .filter((rect) => rect.width && rect.height)
        .map((rect) => Math.round(rect.top)),
    );
    return lines.size > 2
      ? { rule: 'D1', element: name(title), detail: `${lines.size} title lines` }
      : null;
  };
  const dismissalFindings = (dialog) => {
    const findings = [];
    for (const button of dialog.querySelectorAll('button')) {
      if (!visible(button) || !/(?:^|-)cancel(?:-|$)/.test(button.id)) continue;
      const key = button.dataset.t ?? button.dataset.i18n;
      if (
        key !== 'action.cancel' ||
        button.textContent.trim() !== window.strings?.['action.cancel']
      )
        findings.push({
          rule: 'D2',
          element: name(button),
          detail: 'dismissal must use the translated action.cancel key',
        });
    }
    return findings;
  };
  const dialogs = ({ titles = true } = {}) => {
    const findings = [];
    const active = [...document.querySelectorAll('[role="dialog"], dialog[open]')].filter(visible);
    for (const dialog of active) {
      const title = titleFinding(dialog);
      if (titles && title) findings.push(title);
      findings.push(...dismissalFindings(dialog));
      const count = defaults(dialog).length;
      if (count > 1 || (count === 0 && dialog.dataset.dialogKind !== 'choice'))
        findings.push({ rule: 'D3', element: name(dialog), detail: `${count} default buttons` });
    }
    const buttons = defaults(document);
    if (!active.length && buttons.length > 1)
      findings.push({
        rule: 'D3',
        element: name(buttons[1]),
        detail: `${buttons.length} default buttons in the view`,
      });
    return findings;
  };
  const selfCheck = () => {
    const fixture = document.createElement('div');
    fixture.id = 'criteria-fixture';
    fixture.setAttribute('role', 'dialog');
    fixture.setAttribute('aria-labelledby', 'criteria-fixture-title');
    fixture.style.cssText = 'position:fixed;top:0;left:0;z-index:2147483647;background:white';
    fixture.innerHTML =
      '<h2 id="criteria-fixture-title">Title</h2><button id="criteria-fixture-cancel" data-t="action.cancel"></button><button class="primary">Act</button>';
    fixture.querySelector('button').textContent = window.strings['action.cancel'];
    document.body.append(fixture);
    const assert = (expected) => {
      const found = dialogs().filter((finding) => finding.element.startsWith('#criteria-fixture'));
      if (found.map((finding) => finding.rule).join(',') !== expected)
        throw new Error(
          `dialog criterion regression: expected ${expected}, got ${JSON.stringify(found)}`,
        );
    };
    try {
      assert('');
      fixture.querySelector('h2').innerHTML = 'One<br>Two<br>Three';
      assert('D1');
      fixture.querySelector('h2').textContent = 'Title';
      fixture.querySelector('button').dataset.t = 'wrong.cancel';
      assert('D2');
      fixture.querySelector('button').dataset.t = 'action.cancel';
      const duplicate = fixture.querySelector('.primary').cloneNode(true);
      fixture.append(duplicate);
      assert('D3');
      duplicate.remove();
      fixture.querySelector('.primary').remove();
      assert('D3');
      fixture.dataset.dialogKind = 'choice';
      assert('');
    } finally {
      fixture.remove();
    }
  };
  return { dialogs, selfCheck };
})();
