import { describe, expect, test } from 'vitest';
import {
  SemanticVersion,
  _MINIMUM_CLI_VERSION,
  ensureMinimumVersionOrThrow,
} from '../src/version.ts';

describe('SemanticVersion', () => {
  describe('Minimum version check', () => {
    test('older versions throw an error', () => {
      let olderVersions: string[] = ['0.0.1', '0.1.0', '1.1.0'];
      if (_MINIMUM_CLI_VERSION.major > 0) {
        let olderVersion = _MINIMUM_CLI_VERSION.clone();
        olderVersion.major -= 1;
        olderVersions.push(olderVersion.toString());
      }
      if (_MINIMUM_CLI_VERSION.minor > 0) {
        let olderVersion = _MINIMUM_CLI_VERSION.clone();
        olderVersion.minor -= 1;
        olderVersions.push(olderVersion.toString());
      }
      if (_MINIMUM_CLI_VERSION.patch > 0) {
        let olderVersion = _MINIMUM_CLI_VERSION.clone();
        olderVersion.patch -= 1;
        olderVersions.push(olderVersion.toString());
      }
      if (!_MINIMUM_CLI_VERSION.preRelease) {
        let olderVersion = _MINIMUM_CLI_VERSION.clone();
        olderVersion.preRelease = ['alpha'];
        olderVersions.push(olderVersion.toString());
      }
      let olderVersion = _MINIMUM_CLI_VERSION.clone();
      if (olderVersion.preRelease != null) {
        if (typeof olderVersion.preRelease[0] === 'number') {
          olderVersion.preRelease[0] = olderVersion.preRelease[0] - 1;
        } else {
          olderVersion.preRelease[0] += 'alpha';
        }
      }
      const errorRegexp = new RegExp(
        '.*generated with an incompatible version.*'
      );
      for (const versionString of olderVersions) {
        expect(
          () => ensureMinimumVersionOrThrow(versionString),
          `Checking ${versionString}`
        ).toThrow(errorRegexp);
      }
    });

    test('newer versions do not throw', () => {
      let newerVersions: string[] = [_MINIMUM_CLI_VERSION.toString()];
      let newVersion = _MINIMUM_CLI_VERSION.clone();
      newVersion.major += 1;
      newerVersions.push(newVersion.toString());
      newVersion = _MINIMUM_CLI_VERSION.clone();
      newVersion.minor += 1;
      newerVersions.push(newVersion.toString());
      newVersion = _MINIMUM_CLI_VERSION.clone();
      newVersion.patch += 1;
      newerVersions.push(newVersion.toString());
      newVersion = _MINIMUM_CLI_VERSION.clone();
      if (newVersion.preRelease != null) {
        newVersion.preRelease = null;
        newerVersions.push(newVersion.toString());
      }
      const errorRegexp = new RegExp(
        '.*generated with an incompatible version.*'
      );
      for (const versionString of newerVersions) {
        expect(
          () => ensureMinimumVersionOrThrow(versionString),
          `Checking ${versionString}`
        ).not.toThrow();
      }
    });
  });
  describe('Parsing semantic version strings', () => {
    test('valid versions', () => {
      // This is a dummy test to ensure that the test suite runs
      // and that the TableCache is working as expected.
      // You can add more tests here to cover different scenarios.
      //parseVersionString('1.0.0');
      let tests: [string, SemanticVersion][] = [
        ['1.0.0', new SemanticVersion(1, 0, 0)],
        ['1.0.0-alpha', new SemanticVersion(1, 0, 0, ['alpha'])],
        ['1.0.0-alpha.1', new SemanticVersion(1, 0, 0, ['alpha', 1])],
        [
          '1.0.0-alpha.20.beta',
          new SemanticVersion(1, 0, 0, ['alpha', 20, 'beta']),
        ],
        ['1.0.0-alpha.beta', new SemanticVersion(1, 0, 0, ['alpha', 'beta'])],
        ['0.2.1', new SemanticVersion(0, 2, 1)],
        ['0.0.0', new SemanticVersion(0, 0, 0)],
        ['10.2.1', new SemanticVersion(10, 2, 1)],
        [
          '1.0.0+20130313144700',
          new SemanticVersion(1, 0, 0, null, '20130313144700'),
        ],
        ['1.0.0-alpha+001', new SemanticVersion(1, 0, 0, ['alpha'], '001')],
        [
          '1.0.0-alpha.beta+exp.sha.5114f85',
          new SemanticVersion(1, 0, 0, ['alpha', 'beta'], 'exp.sha.5114f85'),
        ],
        [
          '1.0.0+exp.sha.5114f85',
          new SemanticVersion(1, 0, 0, null, 'exp.sha.5114f85'),
        ],
      ];
      for (const [versionString, expectedVersion] of tests) {
        const parsedVersion = SemanticVersion.parseVersionString(versionString);
        expect(parsedVersion, `Parsing ${versionString}`).toEqual(
          expectedVersion
        );
      }
    });

    test('invalid version strings should throw an error', () => {
      let invalidTests: string[] = [
        '1.0', // Missing patch version
        '1', // Missing minor and patch versions
        '1.0.0-', // Trailing hyphen
        '1.0.0+', // Trailing plus
        '1.0.0-alpha..1', // Double dots in pre-release
        '1.0.0+build..info', // Double dots in build metadata
        '1.0.0-alpha!', // Invalid character in pre-release
        '1.0.0+build!', // Invalid character in build metadata
        'abc.def.ghi', // Completely invalid format
        '', // Empty string
      ];
      for (const versionString of invalidTests) {
        expect(() => SemanticVersion.parseVersionString(versionString)).toThrow(
          `Invalid version string: ${versionString}`
        );

        // Checking the minimum version should also fail.
        expect(() => ensureMinimumVersionOrThrow(versionString)).toThrow(
          `Invalid version string: ${versionString}`
        );
      }
    });
  });
  describe('Comparing SemanticVersions', () => {
    function normalizedCompare(
      version1: SemanticVersion,
      version2: SemanticVersion
    ): number {
      let c = version1.compare(version2);
      if (c < 0) {
        return -1;
      }
      if (c > 0) {
        return 1;
      }
      return 0;
    }
    test('test comparison order', () => {
      let cases: [string, string, number][] = [
        ['1.0.0', '1.0.0', 0],
        ['1.0.0', '1.0.1', -1],
        ['1.10.0', '1.2.1', 1],
        ['1.2.1', '1.10.0', -1],
        ['1.0.1', '1.0.0', 1],
        ['1.0.0-alpha', '1.0.0-alpha', 0],
        ['1.0.0-alpha', '1.0.0-beta', -1],
        ['1.0.0-beta', '1.0.0-alpha', 1],
        ['1.0.0-alpha', '1.0.0-alpha.beta', -1],
        ['1.0.0-1', '1.0.0-alpha.beta', -1],
        ['1.0.0-alpha.1', '1.0.0-alpha.2', -1],
        ['1.0.0-alpha.beta', '1.0.0-alpha', 1],
        ['2.0.0-alpha.beta', '2.0.0-alpha.beta', 0],
        ['2.0.0-alpha.beta+001', '2.0.0-alpha.beta+001', 0],
        ['2.0.0-alpha.beta+001', '2.0.0-alpha.beta+002', 0], // Build tags don't affect comparison
      ];
      for (const [left, right, expectedComparison] of cases) {
        const parsedLeft = SemanticVersion.parseVersionString(left);
        const parsedRight = SemanticVersion.parseVersionString(right);
        expect(
          normalizedCompare(parsedLeft, parsedRight),
          'Comparing ' + left + ' and ' + right
        ).toEqual(expectedComparison);
      }
    });
  });
});
