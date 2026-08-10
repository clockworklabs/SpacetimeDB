import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import * as ts from 'typescript';
import { describe, expect, it } from 'vitest';

const bindingsRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '..'
);

/**
 * Typecheck a snippet that mis-uses a TableRef, and return the diagnostics.
 *
 * Mirrors tests/query_error_message.test.ts: the compiler is driven directly so
 * the assertion is on what a user actually sees.
 */
function runTypecheck(source: string) {
  const tmpDir = mkdtempSync(path.join(tmpdir(), 'stdb-tableref-diag-'));
  const reproPath = path.join(tmpDir, 'repro.ts');
  writeFileSync(reproPath, source);

  try {
    const options: ts.CompilerOptions = {
      target: ts.ScriptTarget.ESNext,
      module: ts.ModuleKind.ESNext,
      strict: true,
      noEmit: true,
      skipLibCheck: true,
      forceConsistentCasingInFileNames: true,
      allowImportingTsExtensions: true,
      noImplicitAny: true,
      moduleResolution: ts.ModuleResolutionKind.Bundler,
      useDefineForClassFields: true,
      verbatimModuleSyntax: true,
      isolatedModules: true,
    };

    const host = ts.createCompilerHost(options);
    const program = ts.createProgram(
      [reproPath, path.join(bindingsRoot, 'src/server/sys.d.ts')],
      options,
      host
    );
    const diagnostics = ts.getPreEmitDiagnostics(program);
    return diagnostics.map(d =>
      ts.flattenDiagnosticMessageText(d.messageText, '\n')
    );
  } finally {
    rmSync(tmpDir, { recursive: true, force: true });
  }
}

/**
 * A TableRef must print as a NAME, not as its structure.
 *
 * This is a cost regression test, not a cosmetic one. When `TableRef` was
 * `Readonly<{ ... }>`, a constraint failure printed the whole shape — ten
 * members, with `cols` expanding through four levels of type functions — and
 * TypeScript's own truncation still left 637 characters here (414 in the case
 * observed in a real build).
 *
 * LLM-driven builds pay for that text on every subsequent turn of the session,
 * because the conversation is re-read on each call. More importantly, an error
 * made of structure rather than a name does not tell the reader what to change,
 * so the build tries again: the run that produced the 414-character version
 * needed 10 compile attempts where PostgreSQL needed 3.
 *
 * Declaring TableRef as an interface makes TypeScript print it by name. If
 * someone converts it back to an anonymous object type, this test fails.
 */
describe('TableRef diagnostics', () => {
  const source = `
import { t } from ${JSON.stringify(path.join(bindingsRoot, 'src/server/index.ts'))};
import { table } from ${JSON.stringify(path.join(bindingsRoot, 'src/lib/table.ts'))};
import { createTableRefFromDef } from ${JSON.stringify(path.join(bindingsRoot, 'src/lib/query.ts'))};
import type { AllUnique } from ${JSON.stringify(path.join(bindingsRoot, 'src/lib/constraints.ts'))};

const cartItem = table(
  { name: 'cart_item' },
  { id: t.u64().primaryKey().autoInc(), accountId: t.u64(), quantity: t.u32() }
);

const ref = createTableRefFromDef(cartItem as any, 'cartItem');
type Boom = AllUnique<typeof ref, ['accountId']>;
declare const b: Boom;
`;

  it('names the type instead of dumping its structure', () => {
    const messages = runTypecheck(source);
    const constraintError = messages.find(m =>
      m.includes("does not satisfy the constraint 'UntypedTableDef'")
    );

    expect(constraintError).toBeDefined();
    // The name, not the shape.
    expect(constraintError).toContain('TableRef<');
    expect(constraintError).not.toContain("type: \"table\"");
    expect(constraintError).not.toContain('accessorName');
  }, 15000);

  it('keeps the message short enough to be read', () => {
    const messages = runTypecheck(source);
    const constraintError = messages.find(m =>
      m.includes("does not satisfy the constraint 'UntypedTableDef'")
    )!;

    // Measured in this harness: 637 characters when TableRef was an anonymous
    // object type, 175 as an interface. The bound is deliberately loose — it
    // guards against a return to structural dumping, not against wording.
    expect(constraintError.length).toBeLessThan(250);
  }, 15000);
});
