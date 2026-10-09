import {
  copyFileSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

const WASM_BINDGEN_VERSION = 'wasm-bindgen 0.2.104';
const OUTPUT_NAME = 'spacetimedb_portable_datastore_wasm';
const OUTPUT_FILES = [
  `${OUTPUT_NAME}.cjs`,
  `${OUTPUT_NAME}.d.ts`,
  `${OUTPUT_NAME}_bg.wasm`,
];

const args = process.argv.slice(2);
const cleanBuild = args.length === 1 && args[0] === '--clean';
if (args.length > 0 && !cleanBuild) {
  throw new Error(`Unknown arguments: ${args.join(' ')}`);
}

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const workspaceRoot = resolve(packageRoot, '../..');
const outputDir = resolve(
  packageRoot,
  'src/server/test-utils/portable-datastore-wasm'
);

function run(command, args, options = {}) {
  return execFileSync(command, args, {
    cwd: workspaceRoot,
    env: { ...process.env, ...options.env },
    encoding: 'utf8',
    stdio: options.capture ? ['ignore', 'pipe', 'inherit'] : 'inherit',
  });
}

let installedVersion;
try {
  installedVersion = run('wasm-bindgen', ['--version'], {
    capture: true,
  }).trim();
} catch {
  throw new Error(
    `wasm-bindgen-cli ${WASM_BINDGEN_VERSION.split(' ')[1]} is required; see crates/bindings-typescript/DEVELOP.md`
  );
}

if (installedVersion !== WASM_BINDGEN_VERSION) {
  throw new Error(
    `Expected ${WASM_BINDGEN_VERSION}, found ${installedVersion}. See crates/bindings-typescript/DEVELOP.md`
  );
}

const tempDir = mkdtempSync(join(tmpdir(), 'spacetimedb-test-utils-wasm-'));
try {
  const cargoTargetDir = cleanBuild
    ? resolve(tempDir, 'target')
    : resolve(workspaceRoot, 'target/test-utils-wasm');
  const generatedDir = resolve(tempDir, 'generated');
  const buildArtifact = resolve(
    cargoTargetDir,
    `wasm32-unknown-unknown/release/${OUTPUT_NAME}.wasm`
  );
  const cargoHome = process.env.CARGO_HOME ?? resolve(homedir(), '.cargo');
  const rustSysroot = run('rustc', ['--print', 'sysroot'], {
    capture: true,
  }).trim();
  const rustFlags = [
    '--cfg',
    'tokio_unstable',
    `--remap-path-prefix=${workspaceRoot}=/workspace`,
    `--remap-path-prefix=${cargoHome}=/cargo`,
    `--remap-path-prefix=${rustSysroot}=/rust`,
  ].join('\x1f');

  run(
    'cargo',
    [
      'build',
      '--locked',
      '--release',
      '--target',
      'wasm32-unknown-unknown',
      '-p',
      'spacetimedb-portable-datastore-wasm',
    ],
    {
      env: {
        CARGO_TARGET_DIR: cargoTargetDir,
        CARGO_ENCODED_RUSTFLAGS: rustFlags,
        RUSTFLAGS: '',
        SOURCE_DATE_EPOCH: '0',
      },
    }
  );

  mkdirSync(generatedDir);
  run('wasm-bindgen', [
    buildArtifact,
    '--target',
    'nodejs',
    '--out-dir',
    generatedDir,
    '--out-name',
    OUTPUT_NAME,
  ]);

  const generatedJs = resolve(generatedDir, `${OUTPUT_NAME}.js`);
  const generatedCjs = resolve(generatedDir, `${OUTPUT_NAME}.cjs`);
  writeFileSync(
    generatedJs,
    `${readFileSync(generatedJs, 'utf8').trimEnd()}\n`
  );
  renameSync(generatedJs, generatedCjs);

  const stagedOutputDir = mkdtempSync(
    join(dirname(outputDir), '.portable-datastore-wasm-')
  );
  try {
    for (const file of OUTPUT_FILES) {
      copyFileSync(resolve(generatedDir, file), resolve(stagedOutputDir, file));
    }
    rmSync(outputDir, { force: true, recursive: true });
    renameSync(stagedOutputDir, outputDir);
  } finally {
    rmSync(stagedOutputDir, { force: true, recursive: true });
  }
} finally {
  rmSync(tempDir, { force: true, recursive: true });
}

console.log(
  `Generated portable datastore Wasm artifacts in ${outputDir} (${cleanBuild ? 'clean build' : 'cached build'})`
);
