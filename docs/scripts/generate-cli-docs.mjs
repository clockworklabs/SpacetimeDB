#!/usr/bin/env node
import { createWriteStream, promises as fs } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';

function getRepoRoot() {
  const __filename = fileURLToPath(import.meta.url);
  const __dirname = path.dirname(__filename);
  return path.resolve(__dirname, '../..');
}

function runCargoAndAppend({ cwd, outFilePath }) {
  return new Promise((resolve, reject) => {
    const outStream = createWriteStream(outFilePath, { flags: 'a' });

    const child = spawn(
      'cargo',
      ['run', '--features', 'markdown-docs', '-p', 'spacetimedb-cli'],
      {
        cwd,
        stdio: ['ignore', 'pipe', 'inherit'],
      },
    );

    child.on('error', err => {
      outStream.end();
      reject(err);
    });

    child.stdout.pipe(outStream);

    child.on('close', code => {
      outStream.end();
      if (code === 0) resolve();
      else reject(new Error(`cargo exited with code ${code ?? 'unknown'}`));
    });
  });
}

/**
 * Escape `<PLACEHOLDER>` patterns that appear outside backtick code spans
 * so MDX doesn't treat them as JSX tags.
 *
 * Splits each line on backtick boundaries, and only escapes in the non-code
 * segments. E.g. `<FOO>` (inside backticks) is left alone, but bare <FOO>
 * becomes \<FOO\>.
 */
function escapeAngleBracketsOutsideBackticks(content) {
  return content
    .split('\n')
    .map(line => {
      // Split on backtick-delimited code spans, alternating: text, code, text, code, ...
      const parts = line.split('`');
      for (let i = 0; i < parts.length; i += 2) {
        // Even indices are outside backticks
        parts[i] = parts[i].replace(/<([A-Z][A-Z0-9_-]*(?:\s+[A-Z][A-Z0-9_-]*)*)>/g, '\\<$1\\>');
      }
      return parts.join('`');
    })
    .join('\n');
}

async function main() {
  const repoRoot = getRepoRoot();

  const outFile = path.join(
    repoRoot,
    'docs',
    'docs',
    '00300-resources',
    '00200-reference',
    '00100-cli-reference',
    '00100-cli-reference.md',
  );

  const header = `---\ntitle: CLI Reference\nslug: /cli-reference\n---\n\n`;

  await fs.writeFile(outFile, header, 'utf8');
  await runCargoAndAppend({ cwd: repoRoot, outFilePath: outFile });

  // Post-process: escape angle-bracket placeholders outside backtick spans for MDX
  const raw = await fs.readFile(outFile, 'utf8');
  await fs.writeFile(outFile, escapeAngleBracketsOutsideBackticks(raw), 'utf8');
}

main().catch(err => {
  console.error(err?.stack ?? String(err));
  process.exit(1);
});
