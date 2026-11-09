import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';

const tools = process.env.DYAPP_WEB_TOOLS ?? '/opt/web-lint';
const require = createRequire(path.join(tools, 'package.json'));
const { parse: parseJs } = require('acorn');
const { parse: parseHtml } = require('parse5');
const css = require('css-tree');
const root = path.resolve(process.argv[2] ?? '.');
const desktop = path.join(root, 'clients/desktop');
const ui = path.join(desktop, 'ui');

function files(directory) {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((item) => {
    const filename = path.join(directory, item.name);
    if (item.isSymbolicLink()) throw new Error(`Unsupported source symlink: ${filename}`);
    return item.isDirectory() ? files(filename) : [filename];
  });
}
function walk(node, visit) {
  if (!node || typeof node !== 'object') return;
  if (node.type) visit(node);
  for (const child of Object.values(node)) {
    if (Array.isArray(child)) for (const item of child) walk(item, visit);
    else if (child && typeof child === 'object') walk(child, visit);
  }
}
function javascript(source) {
  return parseJs(source, { ecmaVersion: 'latest', sourceType: 'module' });
}
function html(source, visit) {
  const pending = [parseHtml(source)];
  while (pending.length) {
    const node = pending.pop();
    visit(node);
    pending.push(...(node.childNodes ?? []));
    if (node.content) pending.push(node.content);
  }
}
function entries() {
  const names = [];
  html(fs.readFileSync(path.join(ui, 'index.html'), 'utf8'), (node) => {
    const attrs = Object.fromEntries((node.attrs ?? []).map((a) => [a.name, a.value]));
    if (node.tagName === 'script' && attrs.type === 'module') {
      if (!attrs.src || attrs.src.includes('..') || /^[a-z]+:/i.test(attrs.src))
        throw new Error('Unsupported module entry');
      names.push(`ui/${attrs.src.replace(/^\.\//, '')}`);
    }
  });
  if (!names.length) throw new Error('No desktop module entries');
  return names;
}
function smokeSource() {
  const directory = path.join(desktop, 'ui-tests');
  const registration = fs.readFileSync(path.join(directory, 'mod.rs'), 'utf8');
  const section = registration.match(/const SCRIPTS:[\s\S]*?=\s*&\[([\s\S]*?)\];/);
  if (!section) throw new Error('Missing native smoke inventory');
  const inventory = section[1].replace(/\/\/[^\n]*/g, '').replace(/\/\*[\s\S]*?\*\//g, '');
  const names = [...inventory.matchAll(/include_str!\("([^"]+\.js)"\)/g)].map((m) => m[1]);
  if (inventory.replace(/include_str!\("[^"]+\.js"\)/g, '').replace(/[\s,]/g, ''))
    throw new Error('Unsupported native smoke inventory');
  if (names.some((name) => name.includes('..') || path.isAbsolute(name)))
    throw new Error('Unsupported native smoke path');
  if (!names.length || new Set(names).size !== names.length)
    throw new Error('Empty or duplicated native smoke inventory');
  const source = names
    .map((name) => fs.readFileSync(path.join(directory, name), 'utf8'))
    .join('\n');
  const edits = [];
  walk(javascript(source), (node) => {
    if (node.type !== 'ImportExpression') return;
    const expression = node.source;
    const url = expression.object;
    const name = url?.arguments?.[0]?.value;
    const base = url?.arguments?.[1];
    if (
      expression.type !== 'MemberExpression' ||
      expression.property.name !== 'href' ||
      url?.type !== 'NewExpression' ||
      url.callee.name !== 'URL' ||
      typeof name !== 'string' ||
      base?.object?.name !== 'document' ||
      base.property?.name !== 'baseURI' ||
      name.includes('..') ||
      !fs.existsSync(path.join(ui, name))
    )
      throw new Error('Unsupported smoke import');
    edits.push({
      start: expression.start,
      end: expression.end,
      text: JSON.stringify(`./ui/${name}`),
    });
  });
  return edits
    .sort((a, b) => b.start - a.start)
    .reduce((text, edit) => text.slice(0, edit.start) + edit.text + text.slice(edit.end), source);
}
function checkJs() {
  const build = path.join(root, 'build');
  fs.mkdirSync(build, { recursive: true });
  const output = fs.mkdtempSync(path.join(build, 'web-unused-'));
  for (const filename of files(ui).filter((name) => name.endsWith('.js'))) {
    const target = path.join(output, 'ui', path.relative(ui, filename));
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.copyFileSync(filename, target);
  }
  fs.writeFileSync(path.join(output, 'smoke-entry.js'), smokeSource());
  const manifest = [ui, desktop, root]
    .map((dir) => path.join(dir, 'package.json'))
    .find((name) => fs.existsSync(name));
  fs.writeFileSync(
    path.join(output, 'package.json'),
    manifest
      ? fs.readFileSync(manifest)
      : JSON.stringify({ name: 'dyapp-ui-analysis', private: true, type: 'module' }),
  );
  fs.writeFileSync(
    path.join(output, 'knip.json'),
    JSON.stringify({
      entry: [...entries(), 'smoke-entry.js'],
      project: ['ui/**/*.js'],
      includeEntryExports: true,
      treatConfigHintsAsErrors: true,
    }),
  );
  const result = spawnSync(
    process.execPath,
    [
      path.join(tools, 'node_modules/knip/bin/knip.js'),
      '--config',
      'knip.json',
      '--no-gitignore',
      '--no-progress',
      '--reporter',
      'json',
    ],
    { cwd: output, encoding: 'utf8' },
  );
  if (result.error) throw result.error;
  if (result.status !== 0) {
    console.error(`Unused JavaScript analysis failed (${result.status}); snapshot: ${output}`);
    console.error(result.stdout.trim(), result.stderr.trim());
    return false;
  }
  return true;
}
function collectAttributes(node, used) {
  for (const attr of node.attrs ?? []) {
    if (attr.name === 'class') for (const name of attr.value.split(/\s+/)) used.classes.add(name);
    if (attr.name === 'id') used.ids.add(attr.value);
  }
}
function references() {
  const used = { classes: new Set(), ids: new Set(), strings: new Set() };
  const tokens = (text) => text.match(/[-_\p{ID_Start}][-_\p{ID_Continue}]*/gu) ?? [];
  const collect = (source) =>
    walk(javascript(source), (node) => {
      let value;
      if (node.type === 'Literal' && typeof node.value === 'string') value = node.value;
      if (node.type === 'TemplateElement') value = node.value.cooked ?? node.value.raw;
      if (value !== undefined) for (const token of tokens(value)) used.strings.add(token);
    });
  for (const filename of [...files(ui), ...files(path.join(desktop, 'prototype'))]) {
    const source = fs.readFileSync(filename, 'utf8');
    if (filename.endsWith('.js')) collect(source);
    if (!filename.endsWith('.html')) continue;
    html(source, (node) => {
      collectAttributes(node, used);
      if (node.tagName === 'script' && !(node.attrs ?? []).some((a) => a.name === 'src'))
        collect((node.childNodes ?? []).map((n) => n.value ?? '').join(''));
    });
  }
  return used;
}
function checkCss() {
  const used = references();
  const filename = path.join(ui, 'style.css');
  const selectors = new Map();
  css.walk(
    css.parse(fs.readFileSync(filename, 'utf8'), {
      positions: true,
      onParseError(error) {
        throw error;
      },
    }),
    (node) => {
      if (node.type !== 'ClassSelector' && node.type !== 'IdSelector') return;
      const name = css.ident.decode(node.name);
      const names = node.type === 'ClassSelector' ? used.classes : used.ids;
      selectors.set(`${node.type === 'ClassSelector' ? '.' : '#'}${name}`, {
        used: names.has(name) || used.strings.has(name),
        line: node.loc.start.line,
      });
    },
  );
  const allow = JSON.parse(
    fs.readFileSync(path.join(root, 'tools/web-lint/runtime-selectors.json'), 'utf8'),
  ).selectors;
  if (!allow || typeof allow !== 'object' || Array.isArray(allow))
    throw new Error('Invalid runtime selector allowlist');
  let valid = true;
  for (const [selector, reason] of Object.entries(allow)) {
    if (
      typeof reason !== 'string' ||
      !reason.trim() ||
      !selectors.has(selector) ||
      selectors.get(selector).used
    ) {
      console.error(`Invalid or stale runtime selector allowance: ${selector}`);
      valid = false;
    }
  }
  for (const [selector, value] of selectors) {
    if (value.used || Object.hasOwn(allow, selector)) continue;
    console.error(`clients/desktop/ui/style.css:${value.line}: unreferenced selector ${selector}`);
    valid = false;
  }
  return valid;
}
try {
  const js = checkJs();
  const styles = checkCss();
  if (!js || !styles) process.exitCode = 1;
} catch (error) {
  console.error(`Web unused-code check failed: ${error.message}`);
  process.exitCode = 2;
}
