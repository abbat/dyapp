import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';

const build = path.resolve('build');
fs.mkdirSync(build, { recursive: true });
const root = fs.mkdtempSync(path.join(build, 'css-fixtures-'));
for (const directory of ['clients/desktop/ui', 'clients/desktop/prototype', 'tools/web-lint']) {
  fs.mkdirSync(path.join(root, directory), { recursive: true });
}
fs.writeFileSync(
  path.join(root, 'tools/web-lint/css-budget.json'),
  JSON.stringify({ spacing: {} }),
);
const style = path.join(root, 'clients/desktop/ui/style.css');
const page = path.join(root, 'clients/desktop/prototype/index.html');
const check = (css, html = '<main>Fixture</main>') => {
  fs.writeFileSync(style, css);
  fs.writeFileSync(page, html);
  const result = spawnSync(process.execPath, ['tools/web-lint/check-css.mjs', root], {
    encoding: 'utf8',
  });
  if (result.error) throw result.error;
  return result;
};
assert.equal(
  check(':root { --space-2: 8px; } .a { margin: var(--space-2); color: #123456; font-size: 14px; }')
    .status,
  0,
);
for (const source of [
  '.a { margin: 7px; }',
  '.a { margin: var(--space-2, 7px); }',
  '.a { column-gap: calc(7px + var(--space-2)); }',
]) {
  const result = check(source);
  assert.equal(result.status, 1, `${source}: ${result.stderr}`);
  assert.match(result.stderr, /L2 spacing token required/);
}
assert.match(check(':root { --space-bad: 7px; }').stderr, /outside the 4px grid/);
assert.match(check('', '<div style="color:red"></div>').stderr, /static styles belong in CSS/);
assert.match(check('', '<style>.a { color:red; }</style>').stderr, /static styles belong in CSS/);
assert.match(check('.a { font-size: 14px; } .b { font-size: 14px; }').stderr, /Repeated value/);
assert.match(check('.a { color: #123456; } .b { background: #123456; }').stderr, /Repeated value/);
fs.writeFileSync(
  path.join(root, 'tools/web-lint/css-budget.json'),
  JSON.stringify({
    spacing: {},
    distinctSemantics: {
      'font-size:14px': {
        reason: 'Fixture: unrelated roles deliberately have independent sizes.',
        declarations: [
          'clients/desktop/ui/style.css .a font-size:14px',
          'clients/desktop/ui/style.css .b font-size:14px',
        ],
      },
    },
  }),
);
assert.equal(check('.a { font-size: 14px; } .b { font-size: 14px; }').status, 0);
assert.equal(
  check('.a { font-size: 14px; } .b { font-size: 14px; } .c { font-size: 14px; }').status,
  1,
);
assert.match(check('.a { font-size: 14px; }').stderr, /Remove stale CSS semantic distinction/);
console.log('Desktop CSS regression fixtures passed');
