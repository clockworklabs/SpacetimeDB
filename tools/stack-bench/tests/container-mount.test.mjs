import assert from 'node:assert/strict';
import test from 'node:test';

import { dockerMountArguments } from '../container-mount.mjs';

test('container mounts distinguish host binds from release-owned volumes', () => {
  assert.deepEqual(dockerMountArguments({
    kind: 'bind', source: 'C:\\bench\\app', target: '/app', readOnly: false,
  }), ['--mount', 'type=bind,src=C:\\bench\\app,dst=/app']);
  assert.deepEqual(dockerMountArguments({
    kind: 'volume', source: 'stack-bench-release-deps', target: '/release-deps', readOnly: true,
  }), ['--mount', 'type=volume,src=stack-bench-release-deps,dst=/release-deps,readonly']);
});

test('container mount validation rejects ambiguous or unsafe declarations', () => {
  assert.throws(() => dockerMountArguments(null), /must be an object/);
  assert.throws(() => dockerMountArguments({
    kind: 'secret', source: 'x', target: '/x', readOnly: true,
  }), /unsupported/);
  assert.throws(() => dockerMountArguments({
    kind: 'volume', source: '../x', target: '/x', readOnly: true,
  }), /volume name/);
  assert.throws(() => dockerMountArguments({
    kind: 'bind', source: '/host', target: '/../escape', readOnly: true,
  }), /normalized absolute/);
  assert.throws(() => dockerMountArguments({
    kind: 'bind', source: '/host', target: '/x', readOnly: 'yes', extra: true,
  }), /unknown fields/);
});
