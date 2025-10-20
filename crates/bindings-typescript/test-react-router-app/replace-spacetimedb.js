#!/usr/bin/env node

/**
 * Cross-platform replacement script
 * Equivalent to:
 * find src/module_bindings -type f -exec perl -pi -e 's#spacetimedb#../../../src/index#g' {}
 */

import fs from 'fs';
import path from 'path';

const ROOT = path.resolve('src/module_bindings');
const SEARCH = /spacetimedb/g;
const REPLACEMENT = '../../../src/index';

function replaceInFile(filePath) {
  try {
    let content = fs.readFileSync(filePath, 'utf8');
    if (SEARCH.test(content)) {
      const updated = content.replace(SEARCH, REPLACEMENT);
      fs.writeFileSync(filePath, updated, 'utf8');
      console.log(`✔ Updated: ${filePath}`);
    }
  } catch (err) {
    console.error(`✖ Error processing ${filePath}:`, err);
  }
}

function walkDir(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) walkDir(fullPath);
    else if (entry.isFile()) replaceInFile(fullPath);
  }
}

walkDir(ROOT);
console.log('✅ Replacement complete.');
