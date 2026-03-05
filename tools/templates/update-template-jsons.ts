/**
 * Updates .template.json in each template folder with builtWith derived from
 * the slug. Run from SpacetimeDB repo root.
 *
 * Writes to templates/<slug>/.template.json. Commit those changes to keep
 * template metadata in sync with spacetimedb.com.
 *
 * Usage: pnpm run update-jsons (from tools/templates/)
 */

import { readFile, readdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, '../..');
const TEMPLATES_DIR = path.join(REPO_ROOT, 'templates');

/** Framework slugs that can be derived from template folder names. Must match spacetimedb.com BUILT_WITH keys. */
const FRAMEWORK_SLUGS = new Set([
    'react', 'nextjs', 'vue', 'nuxt', 'svelte', 'angular', 'tanstack', 'remix',
    'browser', 'bun', 'deno', 'nodejs', 'spacetimedb', 'tailwind', 'vite',
]);

function deriveBuiltWith(slug: string): string[] {
    const parts = slug.split('-');
    const result = new Set<string>();

    for (const part of parts) {
        if (FRAMEWORK_SLUGS.has(part)) {
            result.add(part);
        }
    }

    result.add('spacetimedb');
    return [...result];
}

export async function updateTemplateJsons(): Promise<void> {
    let entries: import('node:fs').Dirent[];
    try {
        entries = await readdir(TEMPLATES_DIR, { withFileTypes: true });
    } catch (err) {
        console.warn(`Could not read templates dir: ${TEMPLATES_DIR}`, err);
        return;
    }

    const dirs = entries
        .filter((e): e is import('node:fs').Dirent => e.isDirectory() && !e.name.startsWith('.'))
        .map(e => e.name);

    let updated = 0;
    for (const slug of dirs) {
        const jsonPath = path.join(TEMPLATES_DIR, slug, '.template.json');
        let jsonRaw: string;
        try {
            jsonRaw = await readFile(jsonPath, 'utf-8');
        } catch {
            continue;
        }

        let meta: Record<string, unknown>;
        try {
            meta = JSON.parse(jsonRaw);
        } catch {
            console.warn(`Skipping ${slug}: invalid JSON`);
            continue;
        }

        const builtWith = Array.isArray(meta.builtWith) && meta.builtWith.length > 0
            ? meta.builtWith as string[]
            : deriveBuiltWith(slug);

        const { image: _image, ...rest } = meta;
        const updatedMeta = { ...rest, builtWith };

        const updatedJson = JSON.stringify(updatedMeta, null, 2) + '\n';
        if (updatedJson !== jsonRaw) {
            await writeFile(jsonPath, updatedJson);
            console.log(`Updated ${slug}/.template.json`);
            updated++;
        }
    }

    console.log(`Updated ${updated} template JSON(s)`);
}

const isMain =
    import.meta.url === `file://${process.argv[1]}` ||
    fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);

if (isMain) {
    updateTemplateJsons().catch(err => {
        console.error(err);
        process.exit(1);
    });
}
