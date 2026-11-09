// axe's WCAG 2.2 rules include disabled-by-default rules such as target-size.
smoke.accessibilityOptions = () => {
  if (window.axe?.version !== '4.14.0') throw new Error('axe-core 4.14.0 is required');
  const tags = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'];
  const rules = Object.fromEntries(
    window.axe.getRules(tags).map((rule) => [rule.ruleId, { enabled: true }]),
  );
  if (!rules['target-size']) throw new Error('axe WCAG 2.2 target-size rule is missing');
  return { runOnly: { type: 'tag', values: tags }, rules };
};
smoke.accessibility = async () => {
  const result = await window.axe.run(document, smoke.accessibilityOptions());
  const findings = [];
  for (const violation of result.violations) {
    for (const node of violation.nodes) {
      findings.push({
        rule: `axe:${violation.id}`,
        element: JSON.stringify(node.target),
        detail: node.failureSummary ?? violation.description,
      });
    }
  }
  for (const review of result.incomplete) {
    if (!['target-size', 'color-contrast'].includes(review.id)) continue;
    for (const node of review.nodes) {
      findings.push({
        rule: `axe-review:${review.id}`,
        element: JSON.stringify(node.target),
        detail: node.failureSummary ?? review.description,
      });
    }
  }
  return findings;
};
smoke.accessibilitySelfCheck = async () => {
  const fixture = document.createElement('div');
  fixture.id = 'criteria-axe-fixture';
  fixture.style.cssText =
    'position:fixed;top:0;left:0;z-index:2147483647;background:white;color:black';
  fixture.innerHTML =
    '<p id="criteria-axe-low-contrast" style="color:#eee;background:white;font:18px sans-serif">Unreadable label</p><div style="display:flex"><button id="criteria-axe-small-a">a</button><button id="criteria-axe-small-b">b</button></div>';
  for (const button of fixture.querySelectorAll('button')) {
    button.style.cssText =
      'box-sizing:border-box;width:12px;height:12px;min-width:0;min-height:0;padding:0;margin:0;border:0;background:white;color:black';
  }
  document.body.append(fixture);
  try {
    const result = await window.axe.run(fixture, smoke.accessibilityOptions());
    for (const id of ['color-contrast', 'target-size']) {
      if (!result.violations.some((finding) => finding.id === id))
        throw new Error(`axe did not reject the ${id} counterexample`);
    }
  } finally {
    fixture.remove();
  }
};
