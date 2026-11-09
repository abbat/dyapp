import assert from 'node:assert/strict';
import { analyze } from './translation-values.mjs';
import { checkTranslations } from './check-translations.mjs';

function fixture(source, keys, domains = {}, file = 'ui/entry.js') {
  const catalogue = Object.fromEntries(keys.map((key) => [key, key]));
  return {
    sources: { [file]: source },
    domains,
    direct: [],
    english: { ...catalogue },
    russian: { ...catalogue },
  };
}
const cases = [
  ['const [key]=["real","unused"];strings[`a.\x24{key}`];', ['a.real'], ['a.unused']],
  [
    'const text=(key="default")=>strings[`a.\x24{key}`];text();text("saved");',
    ['a.default', 'a.saved'],
  ],
  [
    'const first=k=>strings[`a.\x24{k}`];const second=k=>strings[`b.\x24{k}`];function pick(action){action("one");}pick(first);pick(second);',
    ['a.one', 'b.one'],
    ['a.orphan', 'b.orphan'],
  ],
  [
    'let code=input; if(["yes","no"].includes(code)) strings[`a.\x24{code}`];',
    ['a.yes', 'a.no'],
    ['a.other'],
  ],
  ['const choices=["foo","bar"];choices.map(key=>strings[`a.\x24{key}`]);', ['a.foo', 'a.bar']],
  [
    'const state={status:"saved"};const prepared={state};const {state:alias}=prepared;strings[`a.\x24{alias.status}`];',
    ['a.saved'],
  ],
  [
    'const text = key => strings[`a.\x24{key}`]; text("real"); function unrelated(key) {} unrelated("fake");',
    ['a.real'],
    ['a.fake'],
  ],
  [
    'const real={status:"ready"};const other={status:"orphan"}; function text(v){return strings[`a.\x24{v.status}`];} text(real);',
    ['a.ready'],
    ['a.orphan'],
  ],
  [
    'const view={status:"old"};const alias=view;alias.status="new"; const text=()=>strings[`a.\x24{view.status}`];text();',
    ['a.old', 'a.new'],
  ],
  [
    'function outer(){const key="outer"; function inner(){const key="inner";return strings[`a.\x24{key}`];}return inner();}outer();',
    ['a.inner'],
    ['a.outer'],
  ],
  ['let text="";text=text+"x"; const key="ready"; strings[`a.\x24{key}`];', ['a.ready']],
];
for (const [source, included, excluded = []] of cases) {
  const result = new Set(analyze(source).used);
  for (const key of included) assert(result.has(key), key);
  for (const key of excluded) assert(!result.has(key), key);
}
const gates = [
  {
    source: "strings['a.used'];",
    keys: ['a.used'],
    errors: [],
  },
  {
    source: "const text=key=>strings[`a.\x24{key}`];text('used');",
    keys: ['a.used', 'a.orphan'],
    errors: ['Unused desktop translation key: a.orphan'],
  },
  {
    source: 'strings[`state.\x24{incoming}`];',
    keys: ['state.ready', 'state.orphan'],
    domains: {
      'state.': ['state.ready'],
    },
    errors: ['Unused desktop translation key: state.orphan'],
  },
  {
    source: "const other='orphan';",
    keys: ['state.ready'],
    domains: {
      'state.': ['state.ready'],
    },
    errors: ['Unused desktop translation key: state.ready'],
  },
  {
    source: '<button data-i18n-label="a.used"></button>',
    file: 'ui/index.html',
    keys: ['a.used'],
    errors: [],
  },
  {
    source: '<button data-i18n-other="a.used"></button>',
    file: 'ui/index.html',
    keys: ['a.used'],
    errors: ['Unused desktop translation key: a.used'],
  },
  {
    source: "// strings['a.used'];",
    keys: ['a.used'],
    errors: ['Unused desktop translation key: a.used'],
  },
  {
    source: "const unused='orphan';strings[`state.\x24{incoming}`];",
    keys: ['state.ready', 'state.orphan'],
    domains: {
      'state.': ['state.ready'],
    },
    errors: ['Unused desktop translation key: state.orphan'],
  },
];
for (const item of gates) {
  assert.deepEqual(
    checkTranslations(fixture(item.source, item.keys, item.domains, item.file)),
    item.errors,
  );
}
const unresolved = checkTranslations(fixture('strings[`a.\x24{external}`];', ['a.used']));
assert(unresolved.some((error) => error.includes('unresolved translation expression')));
assert.throws(() => checkTranslations(fixture('const (', ['a.used'])), SyntaxError);
const parity = fixture('strings["a.used"];', ['a.used']);
parity.russian.extra = 'extra';
assert(checkTranslations(parity).some((error) => error.includes('parity differs: extra')));
assert.throws(() => checkTranslations({}), /Invalid translation field/);
assert.throws(
  () => checkTranslations(fixture('strings["a.used"];', ['a.used'], { 'a.': ['wrong.prefix'] })),
  /Invalid finite/,
);
const malformed = 'strings[`state.\x24{incoming}.\x24{other}`];';
assert.throws(
  () => checkTranslations(fixture(malformed, ['state.ready'], { 'state.': ['state.ready'] })),
  /Unsupported native translation/,
);
console.log(`PASS ${cases.length + gates.length} translation parser fixtures`);
