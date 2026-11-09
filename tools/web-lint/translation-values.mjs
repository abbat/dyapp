// Finite string references with lexical scopes, object aliases and native contracts.
import { createRequire } from 'node:module';
const require = createRequire(`${process.env.DYAPP_WEB_TOOLS ?? '/opt/web-lint'}/package.json`);
const { parse } = require('acorn');
const functionNode = (node) =>
  /^(?:FunctionDeclaration|FunctionExpression|ArrowFunctionExpression)$/.test(node.type);
const union = (...sets) => new Set(sets.flatMap((value) => [...value]));
const scope = (parent) => ({ parent, cells: new Map(), guards: [] });
function lookup(env, name) {
  while (env) {
    if (env.cells.has(name)) return env.cells.get(name);
    env = env.parent;
  }
}
function declare(env, name) {
  if (!env.cells.has(name)) env.cells.set(name, { defs: [], fn: null });
  return env.cells.get(name);
}
function children(node) {
  return Object.values(node)
    .flatMap((value) => (Array.isArray(value) ? value : [value]))
    .filter((value) => value && typeof value === 'object' && value.type);
}
class ValueGraph {
  constructor(source) {
    this.envs = new WeakMap();
    this.functionScopes = new WeakMap();
    this.nodes = [];
    this.visit(
      parse(source, { ecmaVersion: 'latest', sourceType: 'module', locations: true }),
      scope(null),
    );
    this.bindInitial();
    this.bindCalls();
    this.collectWrites();
  }
  visit(node, env) {
    if (node.type === 'BlockStatement') env = scope(env);
    this.envs.set(node, env);
    this.nodes.push(node);
    if (node.type === 'IfStatement') {
      this.visit(node.test, env);
      const positive = scope(env);
      positive.guards.push(node.test);
      this.visit(node.consequent, positive);
      if (node.alternate) this.visit(node.alternate, env);
      return;
    }
    if (functionNode(node)) {
      this.visitFunction(node, env);
      return;
    }
    if (node.type === 'VariableDeclarator') this.visitDeclaration(node, env);

    for (const child of children(node)) this.visit(child, env);
  }
  functionTargets(cell, seen = new Set()) {
    if (!cell || seen.has(cell)) return new Set();
    seen = new Set(seen).add(cell);
    const found = new Set(cell.fn ? [cell.fn] : []);
    for (const def of cell.defs) {
      if (functionNode(def)) found.add(def);
      if (def.type === 'Identifier')
        for (const fn of this.functionTargets(lookup(this.envs.get(def), def.name), seen))
          found.add(fn);
    }
    return found;
  }
  callees(node) {
    if (node.callee.type !== 'Identifier') return new Set();
    return this.functionTargets(lookup(this.envs.get(node), node.callee.name));
  }
  returns(fn) {
    if (fn.body.type !== 'BlockStatement') return [fn.body];
    const found = [];
    function scan(node) {
      if (functionNode(node)) return;
      if (node.type === 'ReturnStatement') found.push(node.argument);
      for (const child of children(node)) scan(child);
    }
    scan(fn.body);
    return found;
  }
  propertyNodes(object, name, seen) {
    if (object.type === 'ArrayExpression') {
      if (Number.isInteger(name)) return [object.elements[name]].filter(Boolean);
      return object.elements.filter(Boolean);
    }
    const result = object.properties
      .filter(
        (p) =>
          p.type === 'Property' &&
          ((!p.computed && (p.key.name ?? p.key.value) === name) ||
            (p.computed && p.key.value === name)),
      )
      .map((p) => p.value);
    for (const spread of object.properties.filter((p) => p.type === 'SpreadElement'))
      for (const origin of this.objects(spread.argument, seen))
        result.push(...this.propertyNodes(origin, name, seen));
    return result;
  }
  objects(node, seen = new Set()) {
    if (!node || seen.has(node)) return new Set();
    seen = new Set(seen).add(node);
    if (['ObjectExpression', 'ArrayExpression'].includes(node.type)) return new Set([node]);
    if (node.type === 'Identifier')
      return union(
        ...(lookup(this.envs.get(node), node.name)?.defs ?? []).map((n) => this.objects(n, seen)),
      );
    if (node.type === 'ConditionalExpression')
      return union(this.objects(node.consequent, seen), this.objects(node.alternate, seen));
    if (node.type === 'CallExpression') {
      return union(
        ...[...this.callees(node)].flatMap((fn) =>
          this.returns(fn).map((n) => this.objects(n, seen)),
        ),
      );
    }
    if (node.type === 'MemberExpression') {
      const name = node.computed ? node.property.value : node.property.name;
      return union(
        ...[...this.objects(node.object, seen)].flatMap((origin) =>
          this.propertyNodes(origin, name, seen).map((n) => this.objects(n, seen)),
        ),
      );
    }
    return new Set();
  }
  product(left, right, after = '') {
    const result = new Set();
    for (const a of left)
      for (const b of right) {
        result.add(a + b + after);
        if (result.size > 2000) throw Error('Finite translation domain too large');
      }
    return result;
  }
  strings(node, seen = new Set()) {
    if (!node || seen.has(node)) return new Set();
    seen = new Set(seen).add(node);
    const get = (n) => this.strings(n, seen);
    if (node.type === 'Literal')
      return typeof node.value === 'string' ? new Set([node.value]) : new Set();
    if (node.type === 'Identifier') return this.identifierStrings(node, get);
    if (node.type === 'ArrayExpression') return union(...node.elements.map(get));
    if (node.type === 'ConditionalExpression')
      return union(get(node.consequent), get(node.alternate));
    if (node.type === 'LogicalExpression') return union(get(node.left), get(node.right));
    if (node.type === 'BinaryExpression' && node.operator === '+')
      return this.product(get(node.left), get(node.right));
    if (node.type === 'TemplateLiteral') return this.templateStrings(node, get);
    if (node.type === 'CallExpression') {
      return union(...[...this.callees(node)].flatMap((fn) => this.returns(fn).map(get)));
    }
    if (node.type === 'MemberExpression') return this.memberStrings(node, get);
    return new Set();
  }
  bindInitial() {
    for (const node of this.nodes) {
      if (
        node.type === 'AssignmentExpression' &&
        node.operator === '=' &&
        node.left.type === 'Identifier'
      )
        lookup(this.envs.get(node), node.left.name)?.defs.push(node.right);
      if (node.type === 'CallExpression') {
        for (const fn of this.callees(node))
          fn.params.forEach((param, index) => {
            const id = param.type === 'AssignmentPattern' ? param.left : param;
            if (id.type === 'Identifier' && node.arguments[index])
              lookup(this.functionScopes.get(fn), id.name).defs.push(node.arguments[index]);
          });
      }
      this.bindArrayCallback(node);
      if (node.type === 'ForOfStatement' && node.left.type === 'VariableDeclaration') {
        const id = node.left.declarations[0].id;
        if (id.type === 'Identifier')
          lookup(this.envs.get(node.left), id.name)?.defs.push(node.right);
      }
    }
  }
  bindArrayCallback(node) {
    if (
      node.type === 'CallExpression' &&
      node.callee.type === 'MemberExpression' &&
      ['map', 'forEach', 'flatMap', 'filter'].includes(node.callee.property.name) &&
      node.arguments[0] &&
      functionNode(node.arguments[0])
    ) {
      const fn = node.arguments[0],
        first = fn.params[0];
      if (first?.type === 'Identifier')
        lookup(this.functionScopes.get(fn), first.name).defs.push(node.callee.object);
    }
  }
  bindCalls() {
    let changed;
    do {
      changed = false;
      for (const node of this.nodes) {
        if (node.type !== 'CallExpression') continue;
        for (const fn of this.callees(node)) {
          if (this.bindArguments(fn, node.arguments)) changed = true;
        }
      }
    } while (changed);
  }
  bindArguments(fn, args) {
    let changed = false;
    fn.params.forEach((param, index) => {
      const id = param.type === 'AssignmentPattern' ? param.left : param;
      const argument = args[index];
      if (id.type !== 'Identifier' || !argument) return;
      const cell = lookup(this.functionScopes.get(fn), id.name);
      if (!cell.defs.includes(argument)) {
        cell.defs.push(argument);
        changed = true;
      }
    });
    return changed;
  }
  collectWrites() {
    this.writes = new Map();
    for (const node of this.nodes) {
      if (
        node.type !== 'AssignmentExpression' ||
        node.operator !== '=' ||
        node.left.type !== 'MemberExpression'
      )
        continue;
      const name = node.left.computed ? node.left.property.value : node.left.property.name;
      for (const origin of this.objects(node.left.object)) {
        if (!this.writes.has(origin)) this.writes.set(origin, new Map());
        const fields = this.writes.get(origin);
        if (!fields.has(name)) fields.set(name, []);
        fields.get(name).push(node.right);
      }
    }
  }
  nativeKeys(node, domains) {
    const prefix = node.quasis[0].value.cooked;
    const values = Object.hasOwn(domains, prefix) ? domains[prefix] : null;
    if (!values || !node.expressions.length) return [];
    const expected = prefix === 'profile.value.' ? [prefix, '.', ''] : [prefix, ''];
    if (
      expected.length !== node.quasis.length ||
      expected.some((part, index) => part !== node.quasis[index].value.cooked)
    )
      throw Error(`Unsupported native translation template: ${prefix}`);
    return values;
  }
  analyze(nativeDomains) {
    const used = new Set(),
      unresolved = [];
    for (const node of this.nodes) {
      if (node.type === 'Literal' && typeof node.value === 'string') used.add(node.value);
      if (node.type !== 'TemplateLiteral') continue;
      const native = this.nativeKeys(node, nativeDomains);
      const entries = union(this.strings(node), new Set(native));
      for (const value of entries) used.add(value);
      if (!entries.size && node.quasis.some((q) => q.value.cooked.includes('.')))
        unresolved.push({
          line: node.loc.start.line,
          parts: node.quasis.map((q) => q.value.cooked),
        });
    }
    return { used: [...used], unresolved };
  }
  visitFunction(node, env) {
    if (node.type === 'FunctionDeclaration') declare(env, node.id.name).fn = node;
    const inside = scope(env);
    this.functionScopes.set(node, inside);
    for (const param of node.params) {
      const id = param.type === 'AssignmentPattern' ? param.left : param;
      if (id.type !== 'Identifier') continue;
      const cell = declare(inside, id.name);
      if (param.type === 'AssignmentPattern') {
        cell.defs.push(param.right);
        this.visit(param.right, inside);
      }
    }
    this.visit(node.body, inside);
    return;
  }
  visitDeclaration(node, env) {
    if (node.type !== 'VariableDeclarator') return;
    if (node.id.type === 'Identifier') {
      const cell = declare(env, node.id.name);
      if (node.init) cell.defs.push(node.init);
      if (node.init && functionNode(node.init)) cell.fn = node.init;
    }
    this.visitArrayDeclaration(node, env);
    this.visitObjectDeclaration(node, env);
  }
  visitArrayDeclaration(node, env) {
    if (node.id.type === 'ArrayPattern') {
      node.id.elements.forEach((id, index) => {
        if (id?.type !== 'Identifier') return;
        const cell = declare(env, id.name);
        if (!node.init) return;
        const member = {
          type: 'MemberExpression',
          computed: true,
          object: node.init,
          property: { type: 'Literal', value: index },
        };
        this.envs.set(member, env);
        cell.defs.push(member);
      });
    }
  }
  visitObjectDeclaration(node, env) {
    if (node.id.type === 'ObjectPattern' && node.init) {
      for (const property of node.id.properties) {
        if (property.type !== 'Property' || property.value.type !== 'Identifier') continue;
        const member = {
          type: 'MemberExpression',
          computed: false,
          object: node.init,
          property: property.key,
        };
        this.envs.set(member, env);
        declare(env, property.value.name).defs.push(member);
      }
    }
  }
  identifierStrings(node, get) {
    const cell = lookup(this.envs.get(node), node.name);
    const base = union(...(cell?.defs ?? []).map(get));
    for (let env = this.envs.get(node); env; env = env.parent) {
      for (const test of env.guards) {
        if (
          test.type !== 'CallExpression' ||
          test.callee.type !== 'MemberExpression' ||
          test.callee.property.name !== 'includes' ||
          test.arguments[0]?.type !== 'Identifier' ||
          !cell ||
          lookup(this.envs.get(test), test.arguments[0].name) !== cell
        )
          continue;
        const allowed = get(test.callee.object);
        if (allowed.size)
          return base.size ? new Set([...base].filter((v) => allowed.has(v))) : allowed;
      }
    }
    return base;
  }
  templateStrings(node, get) {
    let result = new Set([node.quasis[0].value.cooked]);
    node.expressions.forEach((expr, index) => {
      result = this.product(result, get(expr), node.quasis[index + 1].value.cooked);
    });
    return result;
  }
  memberStrings(node, get) {
    const name = node.computed ? node.property.value : node.property.name;
    return union(
      ...[...this.objects(node.object)]
        .flatMap((origin) => [
          ...this.propertyNodes(origin, name, new Set()),
          ...(this.writes.get(origin)?.get(name) ?? []),
        ])
        .map(get),
    );
  }
}
export function analyze(source, nativeDomains = {}) {
  return new ValueGraph(source).analyze(nativeDomains);
}
