import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';

import { assertBinarySourceUnchanged, createBinaryProvenance, RUST_BUILDER_IMAGE,
  verifyBinaryProvenance } from '../container/binary-provenance.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { binarySourceIdentity, SOURCE_IDENTITY_SCHEME }
  from '../src/releases/release-source.js';

const SOURCE = { identityScheme: SOURCE_IDENTITY_SCHEME,
  revision: 'a'.repeat(40), sha256: 'b'.repeat(64), files: 12 } as const;

function fixture(): string {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-binaries-'));
  const bin = join(root, 'container', 'bin');
  mkdirSync(bin, { recursive: true });
  writeFileSync(join(bin, 'spacetimedb-cli'), Buffer.from([0x7f, 0x45, 0x4c, 0x46, 1]));
  writeFileSync(join(bin, 'spacetimedb-standalone'), Buffer.from([0x7f, 0x45, 0x4c, 0x46, 2]));
  return root;
}

function record(root: string) {
  const manifest = createBinaryProvenance(root, SOURCE);
  writeFileSync(join(root, 'container', 'spacetimedb-binaries.json'),
    `${JSON.stringify(manifest, null, 2)}\n`);
  return manifest;
}

test('binary provenance verifies both exact Linux binaries and their source identity', () => {
  const root = fixture();
  try {
    const manifest = record(root);
    assert.equal(manifest.builderImage, RUST_BUILDER_IMAGE);
    assert.deepEqual(verifyBinaryProvenance(root, { sourceSha256: SOURCE.sha256 }), manifest);

    writeFileSync(join(root, 'container', 'bin', 'spacetimedb-cli'),
      Buffer.from([0x7f, 0x45, 0x4c, 0x46, 3]));
    assert.throws(() => verifyBinaryProvenance(root, { sourceSha256: SOURCE.sha256 }),
      /spacetimedb-cli checksum does not match provenance/);

    writeFileSync(join(root, 'container', 'bin', 'spacetimedb-cli'),
      Buffer.from([0x7f, 0x45, 0x4c, 0x46, 1]));
    writeFileSync(join(root, 'container', 'bin', 'spacetimedb-standalone'),
      Buffer.from([0x7f, 0x45, 0x4c, 0x46, 4]));
    assert.throws(() => verifyBinaryProvenance(root, { sourceSha256: SOURCE.sha256 }),
      /spacetimedb-standalone checksum does not match provenance/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('binary provenance rejects stale source and an unbuilt clean checkout', () => {
  const root = fixture();
  try {
    record(root);
    assert.throws(() => verifyBinaryProvenance(root, { sourceSha256: 'c'.repeat(64) }),
      /do not match the selected release source/);
    const invalid = record(root);
    invalid.source.files = 0;
    writeFileSync(join(root, 'container', 'spacetimedb-binaries.json'), JSON.stringify(invalid));
    assert.throws(() => verifyBinaryProvenance(root, { sourceSha256: SOURCE.sha256 }),
      /source file count is invalid/);
    writeFileSync(join(root, 'container', 'spacetimedb-binaries.json'), JSON.stringify({
      schemaVersion: 1, status: 'unbuilt',
    }));
    assert.throws(() => verifyBinaryProvenance(root, { sourceSha256: SOURCE.sha256 }),
      /build-linux-cli\.sh/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('binary source identity uses canonical Git content across checkout line endings', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-binary-source-'));
  const cargo = join(root, 'Cargo.toml');
  const main = join(root, 'crates', 'cli', 'src', 'main.rs');
  mkdirSync(dirname(main), { recursive: true });
  writeFileSync(join(root, '.gitattributes'), '* text=auto\n');
  writeFileSync(cargo, '[workspace]\n');
  writeFileSync(main, 'fn main() {}\n');
  const git = (args: string[]): string => execFileSync('git', ['-C', root, ...args], {
    encoding: 'utf8', windowsHide: true,
  });
  try {
    git(['init', '--quiet']);
    git(['config', 'user.email', 'stack-bench@example.invalid']);
    git(['config', 'user.name', 'Stack Bench']);
    git(['config', 'core.autocrlf', 'false']);
    git(['add', '.gitattributes', 'Cargo.toml', 'crates/cli/src/main.rs']);
    git(['commit', '--quiet', '-m', 'fixture']);
    const before = binarySourceIdentity(root);

    git(['config', 'core.autocrlf', 'true']);
    rmSync(cargo);
    rmSync(main);
    git(['checkout', '--', 'Cargo.toml', 'crates/cli/src/main.rs']);
    assert.match(readFileSync(main, 'utf8'), /\r\n/);
    assert.equal(git(['status', '--porcelain=v1', '--', 'Cargo.toml', 'crates']).trim(), '');
    assert.equal(binarySourceIdentity(root).sha256, before.sha256);

    writeFileSync(main, 'fn main() { println!("changed"); }\n');
    assert.throws(() => binarySourceIdentity(root),
      /binary source paths are not clean/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('binary provenance refuses a source change during the build', () => {
  assert.doesNotThrow(() => assertBinarySourceUnchanged(SOURCE, { ...SOURCE }));
  assert.throws(() => assertBinarySourceUnchanged(SOURCE,
    { ...SOURCE, sha256: 'c'.repeat(64) }), /source changed during the build/);
  assert.throws(() => assertBinarySourceUnchanged(SOURCE,
    { ...SOURCE, revision: 'd'.repeat(40) }), /source changed during the build/);
  assert.throws(() => assertBinarySourceUnchanged(SOURCE,
    { ...SOURCE, files: SOURCE.files + 1 }), /source changed during the build/);
  assert.throws(() => createBinaryProvenance('unused',
    { ...SOURCE, files: 0 }), /file count must be a positive integer/);
});

test('controller verifies recorded binaries before it installs them', () => {
  const dockerfile = readFileSync(join(STACK_BENCH_ROOT, 'appliance',
    'Controller.Dockerfile'), 'utf8');
  assert.match(dockerfile, /ARG BINARY_SOURCE_SHA256/);
  const verify = dockerfile.indexOf('binary-provenance.js verify');
  const install = dockerfile.indexOf('install -m 0555 container/bin/spacetimedb-cli');
  assert.equal(verify >= 0 && install > verify, true);
  assert.doesNotMatch(dockerfile, /^COPY .*container\/bin\/spacetimedb-/m);
});

test('the Linux binary build uses the recorded builder digest', () => {
  const script = readFileSync(join(STACK_BENCH_ROOT, 'container',
    'build-linux-cli.sh'), 'utf8');
  assert.match(script, new RegExp(RUST_BUILDER_IMAGE.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
  assert.doesNotMatch(script, /STACK_BENCH_RUST_IMAGE/);
  assert.match(script, /PROVENANCE=.*binary-provenance\.js/);
  assert.match(script, /node "\$PROVENANCE" source/);
  assert.match(script, /node "\$PROVENANCE" record/);
});
