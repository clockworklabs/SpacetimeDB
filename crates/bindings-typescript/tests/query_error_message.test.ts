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

function runTypecheck(semijoinPredicateExpr: string) {
  const tmpDir = mkdtempSync(path.join(tmpdir(), 'stdb-query-diag-'));
  const reproPath = path.join(tmpDir, 'repro.ts');

  const imports = {
    query: path.join(bindingsRoot, 'src/lib/query.ts'),
    moduleBindings: path.join(
      bindingsRoot,
      'test-app/src/module_bindings/index.ts'
    ),
    sys: path.join(bindingsRoot, 'src/server/sys.d.ts'),
  };

  const source = `
import { and } from ${JSON.stringify(imports.query)};
import { tables } from ${JSON.stringify(imports.moduleBindings)};

tables.player
  .leftSemijoin(tables.unindexed_player, (l, r) => ${semijoinPredicateExpr})
  .build();
`;

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
    const program = ts.createProgram([reproPath, imports.sys], options, host);
    const diagnostics = ts.getPreEmitDiagnostics(program);
    const output = diagnostics
      .map(d => ts.flattenDiagnosticMessageText(d.messageText, '\n'))
      .join('\n');

    return {
      status: diagnostics.length === 0 ? 0 : 1,
      output,
    };
  } finally {
    rmSync(tmpDir, { recursive: true, force: true });
  }
}

describe('query builder diagnostics', () => {
  const messageStart =
    'Cannot combine predicates from different table scopes with and/or.';
  const messageHint = 'move extra predicates to .where(...)';

  it('reports a clear message for free-floating and(...) in semijoin predicates', () => {
    const { status, output } = runTypecheck('and(l.id.eq(r.id), r.id.eq(5))');
    expect(status).not.toBe(0);
    expect(output).toContain(messageStart);
    expect(output).toContain(messageHint);
  });

  it('reports a clear message for method-style .and(...) in semijoin predicates', () => {
    const { status, output } = runTypecheck('l.id.eq(r.id).and(r.id.eq(5))');
    expect(status).not.toBe(0);
    expect(output).toContain(messageStart);
    expect(output).toContain(messageHint);
  });
});
