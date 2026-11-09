import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';

const require = createRequire(
  path.join(process.env.DYAPP_WEB_TOOLS ?? '/opt/web-lint', 'package.json'),
);
const css = require('css-tree');
const { parse: parseHtml } = require('parse5');
const root = path.resolve(process.argv[2] ?? '.');
const directories = ['clients/desktop/ui', 'clients/desktop/prototype'];
const problems = [];
const spacing = new Set();
const repeated = new Map();
const budget = JSON.parse(
  fs.readFileSync(path.join(root, 'tools/web-lint/css-budget.json'), 'utf8'),
);

function git(...args) {
  const result = spawnSync('git', args, { cwd: root, encoding: 'utf8' });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(result.stderr);
  return result.stdout.trim();
}

function checkBudgetHistory() {
  if (git('rev-parse', '--is-shallow-repository') !== 'false')
    throw new Error('Full Git history is required for the CSS spacing budget');
  const name = 'tools/web-lint/css-budget.json';
  const allowed = new Set(Object.keys(budget.spacing));
  for (const revision of git('log', '--format=%H', '--diff-filter=AM', '--', name)
    .split(/\r?\n/)
    .filter(Boolean)) {
    const previous = JSON.parse(git('show', `${revision}:${name}`)).spacing;
    for (const key of allowed) if (!Object.hasOwn(previous, key)) allowed.delete(key);
  }
  for (const key of Object.keys(budget.spacing))
    if (!allowed.has(key)) problems.push(`L2 spacing baseline may only shrink: ${key}`);
}

function files(directory) {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const name = path.join(directory, entry.name);
    if (entry.isSymbolicLink()) throw new Error(`Unsupported source symlink: ${name}`);
    return entry.isDirectory() ? files(name) : [name];
  });
}

function checkHtml(source, name) {
  const pending = [parseHtml(source, { sourceCodeLocationInfo: true })];
  while (pending.length) {
    const node = pending.pop();
    if (node.tagName === 'style' || node.attrs?.some((attr) => attr.name === 'style')) {
      problems.push(`${name}:${node.sourceCodeLocation.startLine}: static styles belong in CSS`);
    }
    pending.push(...(node.childNodes ?? []));
    if (node.content) pending.push(node.content);
  }
}

function checkSpaceToken(node, name) {
  if (!node.property.startsWith('--space-')) return;
  const value = css.generate(node.value).trim();
  if (value === '0') return;
  if (!/^\d+(?:\.\d+)?px$/.test(value)) {
    problems.push(
      `${name}:${node.loc.start.line}: computed spacing token needs checker support: ${node.property}`,
    );
  } else if (Number.parseFloat(value) % 4 !== 0) {
    problems.push(`${name}:${node.loc.start.line}: spacing token is outside the 4px grid`);
  }
}

function rawSpacing(value) {
  let raw = false;
  css.walk(value, (part) => {
    if ((part.type === 'Dimension' || part.type === 'Percentage') && Number(part.value) !== 0)
      raw = true;
    if (
      part.type === 'Identifier' &&
      part.name.startsWith('--') &&
      !part.name.startsWith('--space-')
    )
      raw = true;
  });
  return raw;
}

function repeat(signature, key) {
  if (!repeated.has(signature)) repeated.set(signature, new Set());
  repeated.get(signature).add(key);
}

function collectColors(value, key) {
  css.walk(value, (part) => {
    if (!['Hash', 'Identifier', 'Function'].includes(part.type)) return;
    if (part.type === 'Function' && part.name === 'var') return;
    if (part.type === 'Identifier' && /^(currentcolor|transparent)$/i.test(part.name)) return;
    if (css.lexer.matchType('color', part).matched) repeat(`color:${css.generate(part)}`, key);
  });
}

function checkCss(source, name) {
  const ast = css.parse(source, {
    positions: true,
    parseCustomProperty: true,
    onParseError(error) {
      throw error;
    },
  });
  css.walk(ast, {
    visit: 'Declaration',
    enter(node) {
      checkSpaceToken(node, name);
      if (node.property.startsWith('--')) {
        css.walk(node.value, (part) => {
          if (
            part.type === 'Function' &&
            part.name === 'var' &&
            part.children.first?.name === node.property
          )
            problems.push(
              `${name}:${node.loc.start.line}: CSS token self-reference: ${node.property}`,
            );
        });
        return;
      }
      const selector = this.rule?.prelude ? css.generate(this.rule.prelude) : '@';
      const value = css.generate(node.value);
      const key = `${name} ${selector} ${node.property}:${value}`;
      if (/^(?:(?:row|column)-gap|gap|margin(?:-.+)?)$/.test(node.property)) {
        if (rawSpacing(node.value)) spacing.add(key);
      }
      collectColors(node.value, key);
      if (
        /^font(?:-.+)?$/.test(node.property) &&
        !/^(inherit|initial|unset|normal)$/.test(value) &&
        !/^var\(--[^,)]+\)$/.test(value)
      ) {
        repeat(`${node.property}:${value}`, key);
      }
    },
  });
}

for (const directory of directories) {
  for (const filename of files(path.join(root, directory))) {
    const name = path.relative(root, filename);
    if (filename.endsWith('.html')) checkHtml(fs.readFileSync(filename, 'utf8'), name);
    if (filename.endsWith('.css')) checkCss(fs.readFileSync(filename, 'utf8'), name);
  }
}
checkBudgetHistory();
for (const finding of spacing) {
  if (!Object.hasOwn(budget.spacing, finding))
    problems.push(`L2 spacing token required: ${finding}`);
}
for (const [finding, reason] of Object.entries(budget.spacing)) {
  if (!reason.trim() || !spacing.has(finding))
    problems.push(`Remove stale CSS spacing allowance: ${finding}`);
}
for (const [value, places] of repeated) {
  if (places.size < 2) continue;
  const distinct = budget.distinctSemantics?.[value];
  if (
    distinct?.reason?.trim() &&
    distinct.declarations?.length === places.size &&
    distinct.declarations.every((key) => places.has(key))
  )
    continue;
  problems.push(
    `Repeated value needs a shared token or a documented difference in semantics: ${value}: ${[...places].join(', ')}`,
  );
}
for (const value of Object.keys(budget.distinctSemantics ?? {})) {
  if ((repeated.get(value)?.size ?? 0) < 2)
    problems.push(`Remove stale CSS semantic distinction: ${value}`);
}
for (const problem of problems) console.error(problem);
if (problems.length) process.exitCode = 1;
else console.log('Desktop CSS: styles owned by CSS, spacing tokens and font reuse checked');
