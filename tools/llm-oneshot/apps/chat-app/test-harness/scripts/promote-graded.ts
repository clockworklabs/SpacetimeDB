#!/usr/bin/env npx tsx
/**
 * Move graded apps from staging to final location
 * 
 * Usage:
 *   npm run promote -- <staging-path>
 *   npm run promote -- ../staging/typescript/opus-4-5/spacetime/chat-app-20260108-120000/
 * 
 * The script will:
 * 1. Verify GRADING_RESULTS.md exists (app has been graded)
 * 2. Move from staging/<lang>/<llm>/<backend>/<app> to <lang>/<llm>/<backend>/<app>
 */

import * as fs from 'fs';
import * as path from 'path';

const COLORS = {
  reset: '\x1b[0m',
  red: '\x1b[31m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
};

function log(color: keyof typeof COLORS, message: string) {
  console.log(`${COLORS[color]}${message}${COLORS.reset}`);
}

interface PathInfo {
  language: string;
  llm: string;
  backend: string;
  appName: string;
  stagingPath: string;
  finalPath: string;
}

function parsePathInfo(projectPath: string): PathInfo | null {
  const absolutePath = path.resolve(projectPath);
  const normalized = absolutePath.replace(/\\/g, '/');
  
  // Expected format: .../staging/<language>/<llm>/<backend>/<app-name>/
  const stagingMatch = normalized.match(/staging\/([^/]+)\/([^/]+)\/([^/]+)\/([^/]+)\/?$/);
  
  if (!stagingMatch) {
    return null;
  }
  
  const [, language, llm, backend, appName] = stagingMatch;
  const chatAppRoot = absolutePath.split('staging')[0];
  
  return {
    language,
    llm,
    backend,
    appName,
    stagingPath: absolutePath,
    finalPath: path.join(chatAppRoot, language, llm, backend, appName),
  };
}

function moveDirectory(src: string, dest: string): void {
  // Create destination parent directory
  const destParent = path.dirname(dest);
  if (!fs.existsSync(destParent)) {
    fs.mkdirSync(destParent, { recursive: true });
  }
  
  // Use fs.renameSync for same-volume moves (fast)
  // Falls back to copy+delete for cross-volume
  try {
    fs.renameSync(src, dest);
  } catch (e: any) {
    if (e.code === 'EXDEV') {
      // Cross-device move - need to copy
      copyRecursive(src, dest);
      fs.rmSync(src, { recursive: true, force: true });
    } else {
      throw e;
    }
  }
}

function copyRecursive(src: string, dest: string): void {
  const stats = fs.statSync(src);
  
  if (stats.isDirectory()) {
    fs.mkdirSync(dest, { recursive: true });
    for (const item of fs.readdirSync(src)) {
      copyRecursive(path.join(src, item), path.join(dest, item));
    }
  } else {
    fs.copyFileSync(src, dest);
  }
}

async function main() {
  const args = process.argv.slice(2);
  const projectPath = args.find(arg => !arg.startsWith('--'));
  const force = args.includes('--force');
  
  if (!projectPath) {
    console.log(`
Usage: npx tsx scripts/promote-graded.ts <staging-path> [options]

Options:
  --force     Promote even without GRADING_RESULTS.md

Promotes a graded app from staging to final location:
  staging/typescript/opus-4-5/spacetime/chat-app-20260108-120000/
       ↓
  typescript/opus-4-5/spacetime/chat-app-20260108-120000/

Examples:
  npx tsx scripts/promote-graded.ts ../staging/typescript/opus-4-5/spacetime/chat-app-20260108-120000/
`);
    process.exit(1);
  }
  
  const absolutePath = path.resolve(projectPath);
  
  // Parse path to extract components
  const pathInfo = parsePathInfo(absolutePath);
  
  if (!pathInfo) {
    log('red', '❌ Invalid path format. Expected: staging/<language>/<llm>/<backend>/<app-name>/');
    log('cyan', '   Example: staging/typescript/opus-4-5/spacetime/chat-app-20260108-120000/');
    process.exit(1);
  }
  
  log('blue', '\n' + '='.repeat(60));
  log('blue', '        PROMOTE GRADED APP');
  log('blue', '='.repeat(60));
  log('cyan', `📁 App: ${pathInfo.appName}`);
  log('cyan', `🌐 Language: ${pathInfo.language}`);
  log('cyan', `🤖 LLM: ${pathInfo.llm}`);
  log('cyan', `🔧 Backend: ${pathInfo.backend}`);
  log('blue', '='.repeat(60) + '\n');
  
  // Verify source exists
  if (!fs.existsSync(pathInfo.stagingPath)) {
    log('red', `❌ Source path does not exist: ${pathInfo.stagingPath}`);
    process.exit(1);
  }
  
  // Check for GRADING_RESULTS.md
  const gradingFile = path.join(pathInfo.stagingPath, 'GRADING_RESULTS.md');
  if (!fs.existsSync(gradingFile) && !force) {
    log('red', '❌ GRADING_RESULTS.md not found. App has not been graded.');
    log('yellow', '   Run grading first, or use --force to promote anyway.');
    process.exit(1);
  }
  
  if (!fs.existsSync(gradingFile)) {
    log('yellow', '⚠️  No GRADING_RESULTS.md found, but --force specified.');
  } else {
    log('green', '✅ GRADING_RESULTS.md found');
  }
  
  // Check if destination already exists
  if (fs.existsSync(pathInfo.finalPath)) {
    log('red', `❌ Destination already exists: ${pathInfo.finalPath}`);
    log('yellow', '   Remove it first or rename the source app.');
    process.exit(1);
  }
  
  // Perform the move
  log('cyan', `\n📦 Moving to: ${pathInfo.finalPath}`);
  
  try {
    moveDirectory(pathInfo.stagingPath, pathInfo.finalPath);
    log('green', '\n✅ Successfully promoted app!');
    log('cyan', `   From: ${pathInfo.stagingPath}`);
    log('cyan', `   To:   ${pathInfo.finalPath}`);
  } catch (e: any) {
    log('red', `\n❌ Failed to move: ${e.message}`);
    process.exit(1);
  }
  
  // Clean up empty parent directories in staging
  try {
    const backendDir = path.dirname(pathInfo.stagingPath);
    const llmDir = path.dirname(backendDir);
    const langDir = path.dirname(llmDir);
    
    for (const dir of [backendDir, llmDir, langDir]) {
      if (fs.existsSync(dir) && fs.readdirSync(dir).length === 0) {
        fs.rmdirSync(dir);
        log('cyan', `   Cleaned up empty directory: ${path.basename(dir)}`);
      }
    }
  } catch {
    // Ignore cleanup errors
  }
  
  log('green', '\n✅ Done!\n');
}

main().catch(e => {
  log('red', `\n❌ Error: ${e.message}`);
  process.exit(1);
});
