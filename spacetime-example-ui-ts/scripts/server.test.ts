import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import {
  discardStoredServerToken,
  exampleUiAssetsDir,
  loadServerToken,
  saveServerToken,
} from '../src/server';

const temporaryDirectories: string[] = [];

function temporaryTokenPath(): string {
  const directory = mkdtempSync(path.join(tmpdir(), 'stdb-example-ui-'));
  temporaryDirectories.push(directory);
  return path.join(directory, 'server-token');
}

afterEach(() => {
  for (const directory of temporaryDirectories.splice(0)) {
    rmSync(directory, { recursive: true, force: true });
  }
});

describe('example server support', () => {
  it('resolves the shared SVG assets', () => {
    for (const filename of ['brand.svg', 'logo.svg']) {
      expect(
        readFileSync(path.join(exampleUiAssetsDir, filename), 'utf8')
      ).toMatch(/^<svg/);
    }
  });

  it('prefers an environment token over a stored token', () => {
    const tokenPath = temporaryTokenPath();
    writeFileSync(tokenPath, 'stored-token\n');

    expect(loadServerToken(tokenPath, ' environment-token ')).toEqual({
      token: 'environment-token',
      source: 'environment',
    });
  });

  it('saves, loads, and discards a server token', () => {
    const tokenPath = temporaryTokenPath();

    expect(loadServerToken(tokenPath, undefined)).toEqual({
      token: undefined,
      source: 'none',
    });

    saveServerToken(tokenPath, ' generated-token ');
    expect(loadServerToken(tokenPath, undefined)).toEqual({
      token: 'generated-token',
      source: 'file',
    });

    discardStoredServerToken(tokenPath);
    expect(loadServerToken(tokenPath, undefined)).toEqual({
      token: undefined,
      source: 'none',
    });
  });
});
