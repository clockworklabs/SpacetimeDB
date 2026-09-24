import assert from 'node:assert/strict';
import {
  SCOPE_BUILD,
  SCOPE_PLANT,
  SCOPE_TERRAFORM,
  clamp,
  colorFor,
  paintLinePoints,
  parsePresencePayload,
  parseScopes,
  permissionsFor,
  roleLabel,
  safePresenceColor,
  safePresenceCoordinate,
  safePresenceText,
  stepDirection,
  toolAllowedFor,
  worldPixelSize,
} from '../src/model.ts';

assert.equal(safePresenceColor('#59C6D6'), '#59c6d6');
assert.equal(safePresenceColor('" onload="alert(1)', '#59c6d6'), '#59c6d6');
assert.equal(safePresenceColor('red;position:fixed'), '');

assert.equal(safePresenceText('  Settler  ', 'Someone', 64), 'Settler');
assert.equal(safePresenceText('', 'Someone', 64), 'Someone');
assert.equal(safePresenceText('abcdef', 'Someone', 4), 'abcd');

assert.equal(safePresenceCoordinate(4.5, -1, 13), 4.5);
assert.equal(safePresenceCoordinate(Infinity, -1, 13), 0);
assert.equal(safePresenceCoordinate(99, -1, 13), 13);

assert.deepEqual(parseScopes('["colony:view", 7]'), ['colony:view', '7']);
assert.deepEqual(parseScopes('{"scope":"colony:view"}'), []);
assert.deepEqual(parseScopes('invalid'), []);
assert.equal(roleLabel([]), 'Viewer');
assert.equal(roleLabel([SCOPE_TERRAFORM]), 'Terraformer');
assert.equal(roleLabel([SCOPE_BUILD]), 'Builder');
assert.equal(roleLabel([SCOPE_PLANT]), 'Planter');
assert.equal(
  roleLabel([SCOPE_TERRAFORM, SCOPE_BUILD, SCOPE_PLANT]),
  'Collaborator'
);

assert.deepEqual(permissionsFor('holder', [SCOPE_BUILD]), {
  terraform: false,
  build: true,
  plant: false,
});
assert.deepEqual(permissionsFor('owner', []), {
  terraform: true,
  build: true,
  plant: true,
});
assert.equal(
  toolAllowedFor('holder', [SCOPE_BUILD], {
    id: 'dome',
    group: 'structure',
    label: 'Dome',
  }),
  true
);
assert.equal(
  toolAllowedFor('holder', [SCOPE_BUILD], {
    id: 'tree',
    group: 'nature',
    label: 'Tree',
  }),
  false
);

assert.match(colorFor('subject'), /^#[0-9a-f]{6}$/);
assert.equal(colorFor('subject'), colorFor('subject'));
assert.deepEqual(parsePresencePayload(), {
  name: 'Someone',
  role: '',
  color: '',
  cx: 0,
  cy: 0,
  onGrid: false,
});
assert.deepEqual(
  parsePresencePayload(
    JSON.stringify({
      name: '  Settler  ',
      role: 'Builder',
      color: '#59C6D6',
      cx: 99,
      cy: 4.5,
      onGrid: true,
    })
  ),
  {
    name: 'Settler',
    role: 'Builder',
    color: '#59c6d6',
    cx: 13,
    cy: 4.5,
    onGrid: true,
  }
);

assert.equal(clamp(9, 0, 5), 5);
assert.deepEqual(worldPixelSize(2, 3), { width: 208, height: 286 });
assert.equal(stepDirection(1, 1, 1, 0), 'n');
assert.equal(stepDirection(1, 1, 2, 1), 'e');
assert.equal(stepDirection(1, 1, 1, 2), 's');
assert.equal(stepDirection(1, 1, 0, 1), 'w');
assert.equal(stepDirection(1, 1, 3, 3), '');
assert.deepEqual(paintLinePoints({ x: 0, y: 0 }, { x: 2, y: 2 }), [
  { x: 1, y: 0 },
  { x: 2, y: 0 },
  { x: 2, y: 1 },
  { x: 2, y: 2 },
]);

console.log('api-keys model tests passed');
