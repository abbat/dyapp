import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { createRequire } from 'node:module';
import { analyze } from './translation-values.mjs';

const require = createRequire(`${process.env.DYAPP_WEB_TOOLS ?? '/opt/web-lint'}/package.json`);
const { parse } = require('parse5');

function htmlKeys(source) {
  const used = new Set();
  const pending = [parse(source)];
  while (pending.length) {
    const node = pending.pop();
    for (const attribute of node.attrs ?? [])
      if (['data-i18n', 'data-i18n-label', 'data-i18n-placeholder'].includes(attribute.name))
        used.add(attribute.value);
    pending.push(...(node.childNodes ?? []));
    if (node.content) pending.push(node.content);
  }
  return used;
}

function dictionary(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function validateDomains(domains) {
  for (const [prefix, values] of Object.entries(domains)) {
    if (
      !prefix.endsWith('.') ||
      !Array.isArray(values) ||
      !values.length ||
      values.some((value) => typeof value !== 'string' || !value.startsWith(prefix))
    )
      throw new Error(`Invalid finite translation domain: ${prefix}`);
  }
}

function validateInput(input) {
  if (!dictionary(input)) throw new Error('Invalid translation checker input');
  for (const field of ['sources', 'domains', 'english', 'russian'])
    if (!dictionary(input[field])) throw new Error(`Invalid translation field: ${field}`);
  if (!Array.isArray(input.direct) || input.direct.some((key) => typeof key !== 'string'))
    throw new Error('Invalid direct native translation references');
  for (const field of ['sources', 'english', 'russian'])
    if (Object.values(input[field]).some((value) => typeof value !== 'string'))
      throw new Error(`Invalid translation strings: ${field}`);
  if (!Object.keys(input.english).length || !Object.keys(input.sources).length)
    throw new Error('Empty translation catalogue or source inventory');
  validateDomains(input.domains);
}

function javascriptKeys(file, source, domains, keys) {
  const result = analyze(source, domains);
  const errors = result.unresolved
    .filter((item) => item.parts[0] && keys.some((key) => key.startsWith(item.parts[0])))
    .map((item) => `${file}:${item.line}: unresolved translation expression`);
  return { used: result.used, errors };
}

function sourceKeys(sources, domains, keys) {
  const used = new Set();
  const errors = [];
  for (const [file, source] of Object.entries(sources)) {
    if (file.endsWith('.html')) {
      for (const key of htmlKeys(source)) used.add(key);
    } else if (file.endsWith('.js')) {
      const result = javascriptKeys(file, source, domains, keys);
      for (const key of result.used) used.add(key);
      errors.push(...result.errors);
    } else {
      throw new Error(`Unsupported translation source: ${file}`);
    }
  }
  return { used, errors };
}

export function checkTranslations(input) {
  validateInput(input);
  const { sources, domains, direct, english, russian } = input;
  const keys = Object.keys(english);
  const { used, errors } = sourceKeys(sources, domains, keys);
  for (const key of direct) used.add(key);
  for (const key of new Set([...keys, ...Object.keys(russian)])) {
    if (!Object.hasOwn(english, key) || !Object.hasOwn(russian, key))
      errors.push(`Desktop catalogue parity differs: ${key}`);
    else if (!used.has(key)) errors.push(`Unused desktop translation key: ${key}`);
  }
  return errors.sort();
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  try {
    const input = JSON.parse(fs.readFileSync(0, 'utf8'));
    console.log(JSON.stringify(checkTranslations(input)));
  } catch (error) {
    console.error(`Desktop translation check failed: ${error.message}`);
    process.exitCode = 2;
  }
}
