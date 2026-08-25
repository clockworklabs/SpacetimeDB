#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { releasePackages, spacetimedbVersion } from './release-packages.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const npmCommand = process.platform === 'win32' ? 'npm.cmd' : 'npm';
const npxCommand = process.platform === 'win32' ? 'npx.cmd' : 'npx';
const pnpmCommand = process.platform === 'win32' ? 'pnpm.cmd' : 'pnpm';
const spacetimeCommand =
  process.platform === 'win32' ? 'spacetime.exe' : 'spacetime';
const temporaryRoot = mkdtempSync(join(tmpdir(), 'stdb-submodules-consumer-'));
const packDirectory = join(temporaryRoot, 'packs');
mkdirSync(packDirectory);

function run(command, args, cwd = temporaryRoot) {
  const result = spawnSync(command, args, {
    cwd,
    encoding: 'utf8',
    shell: process.platform === 'win32',
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  if (result.status !== 0) {
    const detail = result.error?.message || result.stderr || result.stdout;
    throw new Error(`${command} ${args.join(' ')} failed:\n${detail.trim()}`);
  }
  return result.stdout;
}

function packageSpecifier(packageName, exportName) {
  return exportName === '.'
    ? packageName
    : `${packageName}${exportName.slice(1)}`;
}

try {
  const dependencies = {
    spacetimedb: spacetimedbVersion,
  };
  const importLines = [];
  let importIndex = 0;

  for (const packageDirectory of releasePackages) {
    const directory = resolve(root, packageDirectory);
    const manifest = JSON.parse(
      readFileSync(resolve(directory, 'package.json'), 'utf8')
    );
    const output = run(
      pnpmCommand,
      ['pack', '--json', '--pack-destination', packDirectory],
      directory
    );
    const result = JSON.parse(output);
    const tarball = resolve(result.filename);
    if (!existsSync(tarball))
      throw new Error(`npm pack did not create ${tarball}`);
    dependencies[manifest.name] = `file:./packs/${basename(tarball)}`;

    for (const exportName of Object.keys(
      manifest.exports ?? { '.': manifest.main }
    )) {
      const alias = `packageExport${importIndex}`;
      importLines.push(
        `import * as ${alias} from '${packageSpecifier(manifest.name, exportName)}';`
      );
      importLines.push(`void ${alias};`);
      importIndex += 1;
    }
  }

  writeFileSync(
    join(temporaryRoot, 'package.json'),
    `${JSON.stringify(
      {
        name: 'spacetimedb-submodules-consumer-check',
        private: true,
        type: 'module',
        dependencies,
        devDependencies: { typescript: '^5.9.3' },
      },
      null,
      2
    )}\n`
  );
  writeFileSync(
    join(temporaryRoot, 'tsconfig.json'),
    `${JSON.stringify(
      {
        compilerOptions: {
          allowImportingTsExtensions: true,
          module: 'ESNext',
          moduleResolution: 'Bundler',
          noEmit: true,
          skipLibCheck: true,
          strict: true,
          target: 'ES2022',
        },
        include: ['consumer.ts'],
      },
      null,
      2
    )}\n`
  );
  writeFileSync(
    join(temporaryRoot, 'consumer.ts'),
    `${importLines.join('\n')}\n`
  );
  mkdirSync(join(temporaryRoot, 'src'));
  writeFileSync(
    join(temporaryRoot, 'src', 'index.ts'),
    `
import { schema } from 'spacetimedb/server';
import * as apiKeys from '@spacetimedb/api-keys/submodule';
import * as auth from '@spacetimedb/auth/submodule';
import * as files from '@spacetimedb/files/submodule';
import * as grid from '@spacetimedb/grid/submodule';
import * as lobby from '@spacetimedb/lobby/submodule';
import * as posthog from '@spacetimedb/posthog/submodule';
import * as presence from '@spacetimedb/presence/submodule';
import * as rateLimit from '@spacetimedb/rate-limit/submodule';
import * as resend from '@spacetimedb/resend/submodule';
import * as stripe from '@spacetimedb/stripe/submodule';

const spacetimedb = schema({
  apiKeys,
  auth,
  files,
  grid,
  lobby,
  posthog,
  presence,
  rateLimit,
  resend,
  stripe,
});
export default spacetimedb;

export const init = spacetimedb.init(ctx => {
  apiKeys.installApiKeys(ctx.as.apiKeys);
  auth.installAuth(ctx.as.auth);
  files.installFiles(ctx.as.files);
  grid.installGrid(ctx.as.grid);
  lobby.installLobby(ctx.as.lobby);
  posthog.installPostHog(ctx.as.posthog);
  presence.installPresence(ctx.as.presence);
  rateLimit.installRateLimit(ctx.as.rateLimit);
  resend.installResend(ctx.as.resend);
  stripe.installStripe(ctx.as.stripe);
});
`
  );

  run(npmCommand, ['install', '--ignore-scripts', '--no-audit', '--no-fund']);
  if (process.argv.includes('--audit')) {
    const audit = spawnSync(npmCommand, ['audit', '--omit=dev', '--json'], {
      cwd: temporaryRoot,
      encoding: 'utf8',
      shell: process.platform === 'win32',
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let report;
    try {
      report = JSON.parse(audit.stdout);
    } catch {
      throw new Error(
        `npm audit did not return JSON${audit.stderr ? `:\n${audit.stderr.trim()}` : ''}`
      );
    }
    const counts = report.metadata?.vulnerabilities ?? {};
    const total = ['info', 'low', 'moderate', 'high', 'critical'].reduce(
      (sum, severity) => sum + Number(counts[severity] ?? 0),
      0
    );
    if (audit.status !== 0 || total > 0) {
      const summary = ['info', 'low', 'moderate', 'high', 'critical']
        .filter(severity => Number(counts[severity] ?? 0) > 0)
        .map(severity => `${severity}=${counts[severity]}`)
        .join(', ');
      throw new Error(`packed production dependency audit failed: ${summary}`);
    }
    console.log('Packed production dependency audit passed.');
  }
  run(npxCommand, ['tsc', '--project', 'tsconfig.json']);
  run(spacetimeCommand, ['build']);

  for (const packageDirectory of releasePackages) {
    const sourceManifest = JSON.parse(
      readFileSync(resolve(root, packageDirectory, 'package.json'), 'utf8')
    );
    const installedDirectory = resolve(
      temporaryRoot,
      'node_modules',
      ...sourceManifest.name.split('/')
    );
    const installedManifest = JSON.parse(
      readFileSync(resolve(installedDirectory, 'package.json'), 'utf8')
    );
    for (const value of Object.values(installedManifest.exports ?? {})) {
      const targets =
        typeof value === 'string' ? [value] : Object.values(value);
      for (const target of targets) {
        if (typeof target !== 'string') continue;
        if (!existsSync(resolve(installedDirectory, target))) {
          throw new Error(
            `${installedManifest.name} export is absent after install: ${target}`
          );
        }
      }
    }
  }

  console.log(
    `Consumer install check passed for ${releasePackages.length} packed packages, ${importIndex} exports, and a clean host-module build.`
  );
} finally {
  rmSync(temporaryRoot, { force: true, recursive: true });
}
