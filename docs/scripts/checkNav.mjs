#!/usr/bin/env node

import fs from 'fs';
import path from 'path';
import { fileURLToPath, pathToFileURL } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const navPath = '../docs/nav.mjs';

const mdListPath = process.argv[2];
if (!mdListPath) {
    console.error('Usage: node checkNav.mjs <md-list-file>');
    process.exit(1);
}

const navFile = path.resolve(__dirname, navPath);
const nav = await import(pathToFileURL(navFile).href).then(mod => mod.default);

const extractPathsFromNav = (items) =>
    items.filter(item => item.type === 'page').map(page => page.path);

const navPaths = extractPathsFromNav(nav.items);
const navPathSet = new Set(navPaths);

const expectedMdPaths = fs.readFileSync(path.resolve(__dirname, mdListPath), 'utf8')
    .split('\n')
    .map(line => line.trim())
    .filter(Boolean);
const expectedPathSet = new Set(expectedMdPaths);

const missingInNav = expectedMdPaths.filter(p => !navPathSet.has(p));
const extraInNav = navPaths.filter(p => !expectedPathSet.has(p));

let failed = false;

if (missingInNav.length > 0) {
    console.error('❌ These docs are missing from nav:');
    missingInNav.forEach(p => console.error(`- ${p}`));
    failed = true;
}

if (extraInNav.length > 0) {
    console.error('❌ These docs are listed in nav but not found under docs/:');
    extraInNav.forEach(p => console.error(`- ${p}`));
    failed = true;
}

if (!failed) {
    console.log('✅ nav list matches filesystem.');
} else {
    process.exit(1);
}
