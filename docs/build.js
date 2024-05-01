// @ts-check
import { build } from 'tsup';
import { createContext, Script } from 'node:vm';
import { readFile, writeFile, rm } from 'node:fs/promises';
import { inspect } from 'node:util';

await build({ entry: { out: 'nav.ts' }, clean: true, format: 'esm' });

// Read the file
const nav = await readFile('dist/out.js', 'utf8');

// Remove this line
// export {
// nav
// };
const final = nav.replace(/export {[^}]*};/, '') + '\nnav;';

// Execute the code
const context = createContext();
const script = new Script(final);
const out = script.runInContext(context);

await writeFile(
    'docs/nav.js',
    'module.exports = ' +
        inspect(out, { depth: null, compact: false, breakLength: 120 })
);

await rm('dist/out.js', { recursive: true });
