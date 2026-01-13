#!/usr/bin/env npx tsx
/**
 * Full Grading Pipeline
 * 
 * Runs all automated analysis and prepares structured output for LLM review.
 * 
 * Usage:
 *   npm run grade -- ../spacetime/chat-app-YYYYMMDD-HHMMSS/ --level=5
 */

import * as fs from 'fs';
import * as path from 'path';
import { execSync } from 'child_process';
import { collectMetrics } from './collect-metrics.js';

interface FullGradeResult {
  projectPath: string;
  projectName: string;
  projectType: 'spacetime' | 'postgres' | 'unknown';
  promptLevel: number;
  timestamp: string;
  
  // Automated metrics
  metrics: {
    compiles: boolean;
    compileError?: string;
    linesOfCode: { backend: number; frontend: number; total: number };
    fileCount: { backend: number; frontend: number; total: number };
    dependencies: { backend: string[]; frontend: string[] };
  };
  
  // AI pattern detection scores (0-3 per feature)
  patternAnalysis: {
    features: Array<{
      number: number;
      name: string;
      detected: boolean;
      confidence: string;
      score: number;
    }>;
    totalScore: number;
    maxScore: number;
  };
  
  // E2E test results (if app was running)
  e2eResults?: {
    ran: boolean;
    features: Array<{
      number: number;
      name: string;
      passed: number;
      failed: number;
      score: number;
    }>;
    totalScore: number;
    maxScore: number;
  };
  
  // Code content for LLM review
  codeBundle: {
    backend: Array<{ path: string; content: string }>;
    frontend: Array<{ path: string; content: string }>;
  };
}

// Feature patterns (simplified from ai-grade.ts)
const FEATURE_PATTERNS: Record<number, { name: string; patterns: RegExp[] }> = {
  1: { name: 'Basic Chat', patterns: [/send.*message/i, /create.*room/i, /table.*message/i] },
  2: { name: 'Typing Indicators', patterns: [/typing|isTyping/i, /typing.*indicator/i] },
  3: { name: 'Read Receipts', patterns: [/read.*receipt|seen.*by/i, /mark.*read/i] },
  4: { name: 'Unread Counts', patterns: [/unread.*count/i, /badge|notification/i] },
  5: { name: 'Scheduled Messages', patterns: [/scheduled?.*message|scheduleAt/i] },
  6: { name: 'Ephemeral Messages', patterns: [/ephemeral|disappear|expires?_at/i] },
  7: { name: 'Reactions', patterns: [/reaction|emoji/i, /add.*reaction/i] },
  8: { name: 'Message Editing', patterns: [/edit.*message/i, /message.*history/i] },
  9: { name: 'Permissions', patterns: [/admin|moderator|role/i, /kick|ban/i] },
  10: { name: 'Rich Presence', patterns: [/presence|status/i, /away|dnd|invisible/i] },
  11: { name: 'Threading', patterns: [/thread|reply.*to|parent.*id/i] },
  12: { name: 'Activity Indicators', patterns: [/activity|velocity|hot/i] },
  13: { name: 'Draft Sync', patterns: [/draft|unsent.*message/i] },
  14: { name: 'Anonymous Migration', patterns: [/anonymous|guest/i, /migrate|register/i] },
};

const PROMPT_LEVEL_FEATURES: Record<number, number> = {
  1: 4, 2: 5, 3: 6, 4: 7, 5: 8, 6: 9, 7: 10, 8: 11, 9: 12, 10: 13, 11: 14,
};

function detectProjectType(projectPath: string): 'spacetime' | 'postgres' | 'unknown' {
  if (projectPath.includes('spacetime')) return 'spacetime';
  if (projectPath.includes('postgres')) return 'postgres';
  return 'unknown';
}

function getAllSourceFiles(dir: string): Array<{ path: string; content: string }> {
  const files: Array<{ path: string; content: string }> = [];
  const excludeDirs = ['node_modules', 'dist', 'build', '.git', 'module_bindings'];
  const extensions = ['.ts', '.tsx', '.js', '.jsx'];
  
  function walk(currentDir: string, relativePath: string = '') {
    if (!fs.existsSync(currentDir)) return;
    
    try {
      const entries = fs.readdirSync(currentDir, { withFileTypes: true });
      for (const entry of entries) {
        const fullPath = path.join(currentDir, entry.name);
        const relPath = path.join(relativePath, entry.name);
        
        if (entry.isDirectory() && !excludeDirs.includes(entry.name)) {
          walk(fullPath, relPath);
        } else if (entry.isFile() && extensions.some(ext => entry.name.endsWith(ext))) {
          try {
            const content = fs.readFileSync(fullPath, 'utf-8');
            // Skip very large files
            if (content.length < 50000) {
              files.push({ path: relPath, content });
            }
          } catch {
            // Skip files that can't be read
          }
        }
      }
    } catch {
      // Skip directories that can't be read
    }
  }
  
  walk(dir);
  return files;
}

function analyzePatterns(content: string, promptLevel: number): FullGradeResult['patternAnalysis'] {
  const featuresEvaluated = PROMPT_LEVEL_FEATURES[promptLevel] || 14;
  const features: FullGradeResult['patternAnalysis']['features'] = [];
  
  for (let i = 1; i <= featuresEvaluated; i++) {
    const pattern = FEATURE_PATTERNS[i];
    if (!pattern) continue;
    
    let matchCount = 0;
    for (const regex of pattern.patterns) {
      if (regex.test(content)) matchCount++;
    }
    
    const detected = matchCount >= 2;
    let confidence: string;
    let score: number;
    
    if (matchCount >= pattern.patterns.length) {
      confidence = 'high';
      score = 3;
    } else if (matchCount >= 2) {
      confidence = 'medium';
      score = 2;
    } else if (matchCount >= 1) {
      confidence = 'low';
      score = 1;
    } else {
      confidence = 'none';
      score = 0;
    }
    
    features.push({
      number: i,
      name: pattern.name,
      detected,
      confidence,
      score,
    });
  }
  
  return {
    features,
    totalScore: features.reduce((sum, f) => sum + f.score, 0),
    maxScore: featuresEvaluated * 3,
  };
}

async function runFullGrade(projectPath: string, promptLevel: number): Promise<FullGradeResult> {
  const absolutePath = path.resolve(projectPath);
  const projectType = detectProjectType(absolutePath);
  const projectName = path.basename(absolutePath);
  
  console.log('\n🎯 FULL GRADING PIPELINE');
  console.log('='.repeat(50));
  console.log(`📁 Project: ${projectName}`);
  console.log(`🏷️  Type: ${projectType}`);
  console.log(`📋 Level: ${promptLevel}`);
  console.log('='.repeat(50));
  
  // Step 1: Collect metrics
  console.log('\n📊 Step 1: Collecting metrics...');
  const metrics = collectMetrics(absolutePath, true); // Skip compile for speed
  console.log(`   ✓ Backend: ${metrics.linesOfCode.backend} LOC`);
  console.log(`   ✓ Frontend: ${metrics.linesOfCode.frontend} LOC`);
  
  // Step 2: Collect code files
  console.log('\n📂 Step 2: Bundling source code...');
  const backendPath = projectType === 'spacetime' 
    ? path.join(absolutePath, 'backend')
    : path.join(absolutePath, 'server');
  const frontendPath = path.join(absolutePath, 'client');
  
  const backendFiles = getAllSourceFiles(backendPath);
  const frontendFiles = getAllSourceFiles(frontendPath);
  console.log(`   ✓ Backend: ${backendFiles.length} files`);
  console.log(`   ✓ Frontend: ${frontendFiles.length} files`);
  
  // Step 3: Pattern analysis
  console.log('\n🔍 Step 3: Analyzing code patterns...');
  const allContent = [...backendFiles, ...frontendFiles].map(f => f.content).join('\n');
  const patternAnalysis = analyzePatterns(allContent, promptLevel);
  console.log(`   ✓ Pattern score: ${patternAnalysis.totalScore}/${patternAnalysis.maxScore}`);
  
  // Step 4: Check for E2E results
  console.log('\n🧪 Step 4: Checking for E2E test results...');
  let e2eResults: FullGradeResult['e2eResults'] | undefined;
  const harnessPath = path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Z]:)/, '$1'));
  const resultsDir = path.join(harnessPath, 'results');
  
  // Look for recent benchmark results
  if (fs.existsSync(resultsDir)) {
    const benchmarkFiles = fs.readdirSync(resultsDir)
      .filter(f => f.startsWith('benchmark-') && f.endsWith('.json'))
      .sort()
      .reverse();
    
    if (benchmarkFiles.length > 0) {
      try {
        const latestResult = JSON.parse(
          fs.readFileSync(path.join(resultsDir, benchmarkFiles[0]), 'utf-8')
        );
        
        // Check if it's for this project
        if (latestResult.projectPath === absolutePath || 
            latestResult.projectPath?.includes(projectName)) {
          e2eResults = {
            ran: true,
            features: latestResult.testResults || [],
            totalScore: latestResult.totalScore || 0,
            maxScore: latestResult.maxScore || 0,
          };
          console.log(`   ✓ Found E2E results: ${e2eResults.totalScore}/${e2eResults.maxScore}`);
        }
      } catch {
        // Ignore parse errors
      }
    }
  }
  
  if (!e2eResults) {
    console.log('   ⚠ No E2E results found (run benchmark first for full accuracy)');
  }
  
  const result: FullGradeResult = {
    projectPath: absolutePath,
    projectName,
    projectType,
    promptLevel,
    timestamp: new Date().toISOString(),
    metrics: {
      compiles: metrics.compiles,
      compileError: metrics.compileError,
      linesOfCode: metrics.linesOfCode,
      fileCount: metrics.fileCount,
      dependencies: metrics.dependencies,
    },
    patternAnalysis,
    e2eResults,
    codeBundle: {
      backend: backendFiles,
      frontend: frontendFiles,
    },
  };
  
  // Save full result
  if (!fs.existsSync(resultsDir)) {
    fs.mkdirSync(resultsDir, { recursive: true });
  }
  
  const resultPath = path.join(resultsDir, `full-grade-${Date.now()}.json`);
  fs.writeFileSync(resultPath, JSON.stringify(result, null, 2));
  
  // Create a markdown summary for easy reading
  const summaryPath = path.join(resultsDir, `grade-summary-${projectName}.md`);
  const summary = generateMarkdownSummary(result);
  fs.writeFileSync(summaryPath, summary);
  
  console.log('\n✅ Grading complete!');
  console.log(`📄 Full results: ${resultPath}`);
  console.log(`📋 Summary: ${summaryPath}`);
  
  // Print summary to console
  console.log('\n' + summary);
  
  return result;
}

function generateMarkdownSummary(result: FullGradeResult): string {
  let md = `# Grade Summary: ${result.projectName}\n\n`;
  md += `- **Type:** ${result.projectType}\n`;
  md += `- **Prompt Level:** ${result.promptLevel}\n`;
  md += `- **Date:** ${new Date(result.timestamp).toLocaleString()}\n\n`;
  
  md += `## Metrics\n\n`;
  md += `| Metric | Value |\n|--------|-------|\n`;
  md += `| Backend LOC | ${result.metrics.linesOfCode.backend} |\n`;
  md += `| Frontend LOC | ${result.metrics.linesOfCode.frontend} |\n`;
  md += `| Total LOC | ${result.metrics.linesOfCode.total} |\n`;
  md += `| Backend Files | ${result.metrics.fileCount.backend} |\n`;
  md += `| Frontend Files | ${result.metrics.fileCount.frontend} |\n`;
  md += `| Compiles | ${result.metrics.compiles ? '✅' : '❌'} |\n\n`;
  
  md += `## Pattern Analysis Score: ${result.patternAnalysis.totalScore}/${result.patternAnalysis.maxScore}\n\n`;
  md += `| # | Feature | Detected | Confidence | Score |\n`;
  md += `|---|---------|----------|------------|-------|\n`;
  
  for (const f of result.patternAnalysis.features) {
    const icon = f.score === 3 ? '✅' : f.score === 2 ? '⚠️' : f.score === 1 ? '🔶' : '❌';
    md += `| ${f.number} | ${f.name} | ${f.detected ? 'Yes' : 'No'} | ${f.confidence} | ${icon} ${f.score}/3 |\n`;
  }
  
  if (result.e2eResults) {
    md += `\n## E2E Test Score: ${result.e2eResults.totalScore}/${result.e2eResults.maxScore}\n\n`;
    md += `| # | Feature | Passed | Failed | Score |\n`;
    md += `|---|---------|--------|--------|-------|\n`;
    
    for (const f of result.e2eResults.features) {
      const icon = f.score === 3 ? '✅' : f.score === 2 ? '⚠️' : f.score === 1 ? '🔶' : '❌';
      md += `| ${f.number} | ${f.name} | ${f.passed} | ${f.failed} | ${icon} ${f.score}/3 |\n`;
    }
  }
  
  // Calculate final score
  const patternPct = Math.round((result.patternAnalysis.totalScore / result.patternAnalysis.maxScore) * 100);
  const e2ePct = result.e2eResults 
    ? Math.round((result.e2eResults.totalScore / result.e2eResults.maxScore) * 100)
    : null;
  
  md += `\n## Summary\n\n`;
  md += `- **Pattern Analysis:** ${patternPct}%\n`;
  if (e2ePct !== null) {
    md += `- **E2E Tests:** ${e2ePct}%\n`;
    const avgPct = Math.round((patternPct + e2ePct) / 2);
    md += `- **Combined:** ${avgPct}%\n`;
  }
  
  md += `\n---\n\n`;
  md += `## Code Files for Review\n\n`;
  md += `### Backend (${result.codeBundle.backend.length} files)\n\n`;
  for (const f of result.codeBundle.backend.slice(0, 10)) {
    md += `- \`${f.path}\`\n`;
  }
  if (result.codeBundle.backend.length > 10) {
    md += `- ... and ${result.codeBundle.backend.length - 10} more\n`;
  }
  
  md += `\n### Frontend (${result.codeBundle.frontend.length} files)\n\n`;
  for (const f of result.codeBundle.frontend.slice(0, 10)) {
    md += `- \`${f.path}\`\n`;
  }
  if (result.codeBundle.frontend.length > 10) {
    md += `- ... and ${result.codeBundle.frontend.length - 10} more\n`;
  }
  
  return md;
}

// CLI
const args = process.argv.slice(2);
const projectPath = args.find(arg => !arg.startsWith('--'));
const levelArg = args.find(arg => arg.startsWith('--level='));
const promptLevel = levelArg ? parseInt(levelArg.split('=')[1], 10) : 11;

if (!projectPath) {
  console.log(`
Full Grading Pipeline - Automated analysis + LLM review preparation

Usage: npm run grade -- <project-path> [--level=N]

Examples:
  npm run grade -- ../spacetime/chat-app-20260105-120000/ --level=5
  npm run grade -- ../postgres/chat-app-20260105-120000/ --level=8
`);
  process.exit(1);
}

runFullGrade(projectPath, promptLevel).catch(e => {
  console.error(`Error: ${e.message}`);
  process.exit(1);
});
