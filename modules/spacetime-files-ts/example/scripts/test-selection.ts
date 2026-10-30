import assert from 'node:assert/strict';
import { VaultSelection } from '../src/selection';

const selection = new VaultSelection();
selection.setEntries([
  { type: 'folder', path: '/docs' },
  { type: 'file', path: '/a.txt' },
  { type: 'file', path: '/b.txt' },
  { type: 'file', path: '/c.txt' },
]);

assert.equal(selection.focus('/a.txt'), true);
assert.equal(selection.focusPath, '/a.txt');
assert.equal(selection.focus('/a.txt'), false);

selection.toggle('/a.txt');
selection.selectRange('/c.txt');
assert.deepEqual([...selection.selected], ['/a.txt', '/b.txt', '/c.txt']);

selection.toggle('/b.txt');
assert.deepEqual([...selection.selected], ['/a.txt', '/c.txt']);

selection.selected.clear();
selection.setAnchor('/c.txt');
selection.selectRange('/a.txt');
assert.deepEqual([...selection.selected], ['/a.txt', '/b.txt', '/c.txt']);

selection.selected.clear();
selection.selected.add('/a.txt');
selection.selected.add('/c.txt');
selection.setAnchor('/missing.txt');
selection.selectRange('/b.txt');
assert.deepEqual([...selection.selected], ['/a.txt', '/c.txt', '/b.txt']);

selection.focus('/docs');
selection.prune(
  new Set(['/b.txt', '/c.txt']),
  new Set(['/docs', '/b.txt', '/c.txt'])
);
assert.deepEqual([...selection.selected], ['/c.txt', '/b.txt']);
assert.equal(selection.focusPath, '/docs');

selection.prune(new Set(['/b.txt', '/c.txt']), new Set(['/b.txt', '/c.txt']));
assert.equal(selection.focusPath, null);

selection.clearFocus();
assert.equal(selection.focusPath, null);

console.log('files selection tests passed');
