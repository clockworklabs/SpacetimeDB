const fs = require('fs');
const path = require('path');

const root = path.resolve(__dirname, '../../..');
const profile = process.env.PROFILE || 'debug';
const targetDir = path.join(root, 'target', profile);

const candidates =
  process.platform === 'win32'
    ? ['spacetimedb_test_runtime_node.dll']
    : process.platform === 'darwin'
      ? ['libspacetimedb_test_runtime_node.dylib']
      : ['libspacetimedb_test_runtime_node.so'];

const source = candidates.map(name => path.join(targetDir, name)).find(fs.existsSync);
if (!source) {
  throw new Error(
    `Could not find native test runtime artifact in ${targetDir}. Tried: ${candidates.join(', ')}`
  );
}

fs.copyFileSync(source, path.join(__dirname, 'spacetimedb_test_runtime_node.node'));
