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
    expect(constraintError).not.toContain('type: "table"');
    expect(constraintError).not.toContain('accessorName');
    expect(constraintError.length).toBeLessThan(250);
  }, 15000);
});
