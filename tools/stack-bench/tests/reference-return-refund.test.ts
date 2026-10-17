import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import ts from 'typescript';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

test('reference item and bundle refunds prorate discounted mixed orders and respect prior refunds', () => {
  // Compile the actual pure arithmetic from each standalone reference without
  // importing its server startup or requiring a database for these edge cases.
  for (const path of ['mongodb/server/src/credit.ts', 'postgres/server/src/credit.ts',
    'spacetime/backend/spacetimedb/src/index.ts']) {
    const source = ts.createSourceFile(path, readFileSync(join(STACK_BENCH_ROOT,
      'reference-apps/ecommerce', path), 'utf8'), ts.ScriptTarget.Latest, true);
    const declaration = source.statements.find(statement => ts.isFunctionDeclaration(statement)
      && statement.name?.text === 'refundForReturn');
    assert(declaration, path);
    const code = ts.transpileModule(declaration.getText(source).replace(/^export /, ''),
      { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;
    const refund = new Function(`${code}; return refundForReturn;`)() as
      (total: number, refunded: number, gross: number, returned: number, allReturned: boolean) => number;
    assert.equal(refund(80, 0, 100, 40, false), 32, path); // Only the returned bundle's discounted share.
    assert.equal(refund(80, 70, 100, 40, false), 10, path); // Staff already refunded most of the order.
    assert.equal(refund(80, 80, 100, 40, false), 0, path); // A physical return cannot refund twice.
    assert.equal(refund(0, 0, 0, 0, false), 0, path); // Free goods must not produce NaN.
    const first = refund(19.99, 0, 30, 10, false);
    const second = refund(19.99, first, 30, 10, false);
    const final = refund(19.99, first + second, 30, 10, true);
    assert.deepEqual([first, second, final], [6.66, 6.66, 6.67], path);
    assert.equal(Math.round((first + second + final) * 100), 1999, path);
    assert.equal(refund(80, 70, 100, 40, true), 10, path);
    assert.equal(refund(80, 80, 100, 40, true), 0, path);
    assert.equal(refund(0, 0, 0, 0, true), 0, path);
  }
});
