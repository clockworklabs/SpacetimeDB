import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { promisify } from 'node:util';
import ts from 'typescript';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

test('reference password helpers preserve full UTF-8 passwords with bounded asynchronous scrypt', async t => {
  for (const stack of ['postgres', 'mongodb']) {
    await t.test(stack, async () => {
      const source = readFileSync(join(STACK_BENCH_ROOT,
        `reference-apps/ecommerce/${stack}/server/src/auth.ts`), 'utf8');
      const code = ts.transpileModule(source.replace(/^import .*;\r?\n/gm, '')
        .replace(/^export /gm, ''), { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText;
      let derivations = 0;
      const observedCrypto = {
        ...crypto,
        scrypt: ((...args: Parameters<typeof crypto.scrypt>) => {
          derivations++;
          assert.equal(args[2], 64);
          assert.deepEqual(args[3], { N: 131072, r: 8, p: 1, maxmem: 256 * 1024 * 1024 });
          return crypto.scrypt(...args);
        }) as typeof crypto.scrypt,
      };
      // Execute the standalone fixture helper, without importing its server or database.
      const auth = new Function('crypto', 'promisify', `${code}; return { hashPassword, verifyPassword, validCredentials };`)(observedCrypto, promisify) as {
        hashPassword(password: string): Promise<string>;
        verifyPassword(password: string, stored: string): Promise<boolean>;
        validCredentials(username: unknown, password: unknown): boolean;
      };
      const password = '界'.repeat(24) + 'Ab';
      const wrongSuffix = '界'.repeat(24) + 'Cd';
      assert.equal(Buffer.byteLength(password), 74);
      assert(Buffer.from(password).subarray(0, 72).equals(Buffer.from(wrongSuffix).subarray(0, 72)));
      const pendingHash = auth.hashPassword(password);
      assert(pendingHash instanceof Promise);
      const stored = await pendingHash;
      assert.match(stored, /^scrypt-131072-8-1:[a-f0-9]{32}:[a-f0-9]{128}$/);
      assert.notEqual(await auth.hashPassword(password), stored, 'Each password gets a new salt');
      assert.equal(await auth.verifyPassword(password, stored), true);
      assert.equal(await auth.verifyPassword(wrongSuffix, stored), false);
      assert.equal(await auth.verifyPassword(password.slice(0, -1), stored), false);
      const beforeInvalid = derivations;
      for (const invalid of ['', 'a'.repeat(65), null, 5]) {
        assert.equal(auth.validCredentials('valid-user', invalid), false);
        await assert.rejects(auth.hashPassword(invalid as string));
        assert.equal(await auth.verifyPassword(invalid as string, stored), false);
      }
      for (const invalidHash of ['', 'salt:hash', stored + ':extra', stored + '\n', stored.replace('131072', '16384'), null]) {
        assert.equal(await auth.verifyPassword(password, invalidHash as string), false);
      }
      assert.equal(derivations, beforeInvalid, 'Invalid input must not start a costly derivation');
      assert.equal(auth.validCredentials('a'.repeat(48), '界'.repeat(64)), true);
      for (const invalidName of ['', 'a'.repeat(49), 'a b', 'ann\n', {}, null]) {
        assert.equal(auth.validCredentials(invalidName, password), false);
      }
    });
  }
});
