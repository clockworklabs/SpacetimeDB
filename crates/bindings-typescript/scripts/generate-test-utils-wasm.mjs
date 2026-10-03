import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  renameSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
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
const checkOnly = args.length === 1 && args[0] === '--check';
if (args.length > 0 && !checkOnly) {
  throw new Error(`Unknown arguments: ${args.join(' ')}`);
}

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const workspaceRoot = resolve(packageRoot, '../..');
const outputDir = resolve(
  packageRoot,
  'src/server/test-utils/portable-datastore-wasm'
);
const buildArtifact = resolve(
  workspaceRoot,
  `target/wasm32-unknown-unknown/release/${OUTPUT_NAME}.wasm`
);

function run(command, args, options = {}) {
  return execFileSync(command, args, {
    cwd: workspaceRoot,
    encoding: 'utf8',
    stdio: options.capture ? ['ignore', 'pipe', 'inherit'] : 'inherit',
  });
}

function findArtifactDifferences(generatedDir) {
  const differences = [];
  const checkedInFiles = existsSync(outputDir)
    ? readdirSync(outputDir).sort()
    : [];

  for (const file of OUTPUT_FILES) {
    const checkedInPath = resolve(outputDir, file);
    if (!existsSync(checkedInPath)) {
      differences.push(`${file} is missing`);
    } else if (
      !readFileSync(checkedInPath).equals(
        readFileSync(resolve(generatedDir, file))
      )
    ) {
      differences.push(`${file} is out of date`);
    }
  }

  for (const file of checkedInFiles) {
    if (!OUTPUT_FILES.includes(file)) {
      differences.push(`${file} is unexpected`);
    }
  }

  return differences;
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

run('cargo', [
  'build',
  '--locked',
  '--release',
  '--target',
  'wasm32-unknown-unknown',
  '-p',
  'spacetimedb-portable-datastore-wasm',
]);

const tempDir = mkdtempSync(join(tmpdir(), 'spacetimedb-test-utils-wasm-'));
try {
  run('wasm-bindgen', [
    buildArtifact,
    '--target',
    'nodejs',
    '--out-dir',
    tempDir,
    '--out-name',
    OUTPUT_NAME,
  ]);

  const generatedJs = resolve(tempDir, `${OUTPUT_NAME}.js`);
  const generatedCjs = resolve(tempDir, `${OUTPUT_NAME}.cjs`);
  writeFileSync(
    generatedJs,
    `${readFileSync(generatedJs, 'utf8').trimEnd()}\n`
  );
  renameSync(generatedJs, generatedCjs);

  if (checkOnly) {
    const differences = findArtifactDifferences(tempDir);
    if (differences.length > 0) {
      console.error('Portable datastore Wasm artifacts are not up to date:');
      for (const difference of differences) {
        console.error(`  - ${difference}`);
      }
      console.error('\nRegenerate them from the repository root with:');
      console.error(
        '  pnpm --dir crates/bindings-typescript generate:test-utils-wasm'
      );
      process.exitCode = 1;
    } else {
      console.log('Portable datastore Wasm artifacts are up to date.');
    }
  } else {
    rmSync(outputDir, { force: true, recursive: true });
    mkdirSync(outputDir, { recursive: true });
    for (const file of OUTPUT_FILES) {
      copyFileSync(resolve(tempDir, file), resolve(outputDir, file));
    }
  }
} finally {
  rmSync(tempDir, { force: true, recursive: true });
}

if (!checkOnly) {
  console.log(`Generated portable datastore Wasm artifacts in ${outputDir}`);
}
