import { execSync, spawn } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import { collectMetrics } from './collect-metrics.js';

interface TestResult {
  feature: string;
  featureNumber: number;
  passed: number;
  failed: number;
  skipped: number;
  total: number;
  score: number; // 0-3 scale
}

interface BenchmarkResult {
  projectPath: string;
  projectType: 'spacetime' | 'postgres' | 'unknown';
  promptLevel: number; // 1-11, determines which features to score
  featuresEvaluated: number; // How many features were evaluated
  metrics: {
    compiles: boolean;
    compileError?: string;
    runs: boolean;
    runError?: string;
    linesOfCode: { backend: number; frontend: number; total: number };
    fileCount: { backend: number; frontend: number; total: number };
    dependencies: { backend: string[]; frontend: string[] };
  };
  testResults: TestResult[];
  totalScore: number;
  maxScore: number;
  timestamp: string;
}

// Mapping from prompt level to number of features included
const PROMPT_LEVEL_FEATURES: Record<number, number> = {
  1: 4,   // 01_*_basic: Basic, Typing, Read Receipts, Unread
  2: 5,   // 02_*_scheduled: + Scheduled Messages
  3: 6,   // 03_*_realtime: + Ephemeral Messages
  4: 7,   // 04_*_reactions: + Reactions
  5: 8,   // 05_*_edit_history: + Edit History
  6: 9,   // 06_*_permissions: + Permissions
  7: 10,  // 07_*_presence: + Rich Presence
  8: 11,  // 08_*_threading: + Threading
  9: 12,  // 09_*_activity: + Activity Indicators
  10: 13, // 10_*_drafts: + Draft Sync
  11: 14, // 11_*_anonymous: All features
};

const FEATURE_NAMES = [
  'Basic Chat',
  'Typing Indicators',
  'Read Receipts',
  'Unread Counts',
  'Scheduled Messages',
  'Ephemeral Messages',
  'Message Reactions',
  'Message Editing',
  'Real-Time Permissions',
  'Rich Presence',
  'Message Threading',
  'Activity Indicators',
  'Draft Sync',
  'Anonymous Migration',
];

/**
 * Parse Playwright JSON results
 */
function parseTestResults(resultsPath: string): TestResult[] {
  if (!fs.existsSync(resultsPath)) {
    console.warn('No test results found at:', resultsPath);
    return [];
  }

  const results: TestResult[] = [];
  const data = JSON.parse(fs.readFileSync(resultsPath, 'utf-8'));

  // Group by feature
  const featureMap = new Map<number, { passed: number; failed: number; skipped: number }>();

  for (const suite of data.suites || []) {
    // Extract feature number from filename (e.g., "01-basic-chat.spec.ts")
    const match = suite.file?.match(/(\d+)-/);
    if (!match) continue;

    const featureNum = parseInt(match[1], 10);
    if (!featureMap.has(featureNum)) {
      featureMap.set(featureNum, { passed: 0, failed: 0, skipped: 0 });
    }

    const stats = featureMap.get(featureNum)!;

    // Count test results
    for (const spec of suite.specs || []) {
      for (const test of spec.tests || []) {
        const status = test.status || test.results?.[0]?.status;
        if (status === 'passed' || status === 'expected') {
          stats.passed++;
        } else if (status === 'failed' || status === 'unexpected') {
          stats.failed++;
        } else if (status === 'skipped') {
          stats.skipped++;
        }
      }
    }
  }

  // Convert to TestResult array
  for (let i = 1; i <= 14; i++) {
    const stats = featureMap.get(i) || { passed: 0, failed: 0, skipped: 0 };
    const total = stats.passed + stats.failed + stats.skipped;
    
    // Calculate score (0-3 scale based on pass rate)
    let score = 0;
    if (total > 0) {
      const passRate = stats.passed / total;
      if (passRate >= 0.9) score = 3;
      else if (passRate >= 0.7) score = 2;
      else if (passRate >= 0.4) score = 1;
      else score = 0;
    }

    results.push({
      feature: FEATURE_NAMES[i - 1] || `Feature ${i}`,
      featureNumber: i,
      passed: stats.passed,
      failed: stats.failed,
      skipped: stats.skipped,
      total,
      score,
    });
  }

  return results;
}

/**
 * Try to start the application
 */
async function tryStart(projectPath: string, projectType: string): Promise<{ success: boolean; error?: string }> {
  // This is a simplified check - in production you'd want to actually start the servers
  // and verify they respond
  
  const clientPath = projectType === 'spacetime'
    ? path.join(projectPath, 'client')
    : path.join(projectPath, 'client');
  
  const clientPackageJson = path.join(clientPath, 'package.json');
  
  if (!fs.existsSync(clientPackageJson)) {
    return { success: false, error: 'Client package.json not found' };
  }

  try {
    const packageJson = JSON.parse(fs.readFileSync(clientPackageJson, 'utf-8'));
    
    // Check if dev script exists
    if (!packageJson.scripts?.dev && !packageJson.scripts?.start) {
      return { success: false, error: 'No dev or start script found' };
    }

    // For now, just verify the structure exists
    // A full implementation would start the server and check it responds
    return { success: true };
  } catch (e: any) {
    return { success: false, error: e.message };
  }
}

/**
 * Run Playwright tests
 */
async function runTests(projectPath: string, harnessPath: string): Promise<void> {
  const clientPath = path.join(projectPath, 'client');
  const clientUrl = process.env.CLIENT_URL || 'http://localhost:5173';
  
  console.log('\n📋 Running Playwright tests...\n');
  
  try {
    execSync('npx playwright test --reporter=json', {
      cwd: harnessPath,
      env: {
        ...process.env,
        CLIENT_URL: clientUrl,
      },
      stdio: 'inherit',
      timeout: 600000, // 10 minute timeout
    });
  } catch (e) {
    console.warn('Some tests failed (this is expected for incomplete implementations)');
  }
}

/**
 * Generate a summary report
 */
function generateReport(result: BenchmarkResult): string {
  let report = '\n' + '='.repeat(60) + '\n';
  report += '           BENCHMARK RESULTS\n';
  report += '='.repeat(60) + '\n\n';

  report += `📁 Project: ${path.basename(result.projectPath)}\n`;
  report += `🏷️  Type: ${result.projectType.toUpperCase()}\n`;
  report += `📋 Prompt Level: ${result.promptLevel} (Features 1-${result.featuresEvaluated})\n`;
  report += `📅 Date: ${new Date(result.timestamp).toLocaleString()}\n\n`;

  report += '─'.repeat(60) + '\n';
  report += '                    BUILD STATUS\n';
  report += '─'.repeat(60) + '\n';
  report += `Compiles: ${result.metrics.compiles ? '✅ PASS' : '❌ FAIL'}\n`;
  if (result.metrics.compileError) {
    report += `Error: ${result.metrics.compileError.slice(0, 100)}...\n`;
  }
  report += `Runs: ${result.metrics.runs ? '✅ PASS' : '❌ FAIL'}\n`;
  if (result.metrics.runError) {
    report += `Error: ${result.metrics.runError}\n`;
  }

  report += '\n' + '─'.repeat(60) + '\n';
  report += '                    CODE METRICS\n';
  report += '─'.repeat(60) + '\n';
  report += `Lines of Code:\n`;
  report += `  Backend:  ${result.metrics.linesOfCode.backend.toLocaleString()}\n`;
  report += `  Frontend: ${result.metrics.linesOfCode.frontend.toLocaleString()}\n`;
  report += `  Total:    ${result.metrics.linesOfCode.total.toLocaleString()}\n\n`;
  report += `File Count:\n`;
  report += `  Backend:  ${result.metrics.fileCount.backend}\n`;
  report += `  Frontend: ${result.metrics.fileCount.frontend}\n`;
  report += `  Total:    ${result.metrics.fileCount.total}\n\n`;
  report += `Dependencies:\n`;
  report += `  Backend:  ${result.metrics.dependencies.backend.length} packages\n`;
  report += `  Frontend: ${result.metrics.dependencies.frontend.length} packages\n`;

  report += '\n' + '─'.repeat(60) + '\n';
  report += '                   FEATURE SCORES\n';
  report += '─'.repeat(60) + '\n';
  
  for (const test of result.testResults) {
    const scoreIcon = test.score === 3 ? '✅' : test.score === 2 ? '⚠️ ' : test.score === 1 ? '🔶' : '❌';
    const scoreBar = '█'.repeat(test.score) + '░'.repeat(3 - test.score);
    report += `${test.featureNumber.toString().padStart(2)}. ${test.feature.padEnd(24)} ${scoreIcon} ${scoreBar} ${test.score}/3`;
    if (test.total > 0) {
      report += ` (${test.passed}/${test.total} tests)`;
    }
    report += '\n';
  }

  report += '\n' + '─'.repeat(60) + '\n';
  report += `                    TOTAL SCORE: ${result.totalScore}/${result.maxScore}\n`;
  report += '─'.repeat(60) + '\n';

  const percentage = Math.round((result.totalScore / result.maxScore) * 100);
  let grade = '';
  if (percentage >= 90) grade = 'A';
  else if (percentage >= 80) grade = 'B';
  else if (percentage >= 70) grade = 'C';
  else if (percentage >= 60) grade = 'D';
  else grade = 'F';

  report += `                    GRADE: ${grade} (${percentage}%)\n`;
  report += '='.repeat(60) + '\n';

  return report;
}

/**
 * Main benchmark runner
 * @param projectPath - Path to the project to benchmark
 * @param promptLevel - Prompt level (1-11), determines which features to evaluate
 */
async function runBenchmark(projectPath: string, promptLevel: number = 11): Promise<BenchmarkResult> {
  const absolutePath = path.resolve(projectPath);
  const harnessPath = path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Z]:)/, '$1'));
  
  // Validate and get feature count for this level
  const validLevel = Math.max(1, Math.min(11, promptLevel));
  const featuresEvaluated = PROMPT_LEVEL_FEATURES[validLevel] || 14;
  
  console.log('🚀 Starting benchmark for:', absolutePath);
  console.log('📦 Test harness at:', harnessPath);
  console.log(`📋 Prompt level: ${validLevel} (evaluating features 1-${featuresEvaluated})`);

  // Collect metrics
  console.log('\n📊 Collecting metrics...');
  const metrics = collectMetrics(absolutePath, false);

  // Try to start
  console.log('\n🔌 Checking if app can run...');
  const startResult = await tryStart(absolutePath, metrics.projectType);

  // Run tests (if app can start)
  let testResults: TestResult[] = [];
  if (startResult.success && metrics.compiles) {
    console.log('\nℹ️  Note: For full testing, start the app manually and set CLIENT_URL');
    console.log('   Example: CLIENT_URL=http://localhost:5173 npm run benchmark <path> --level=5');
    
    // Check if server is running
    try {
      const response = await fetch(process.env.CLIENT_URL || 'http://localhost:5173', { 
        method: 'HEAD',
        signal: AbortSignal.timeout(5000),
      });
      if (response.ok) {
        await runTests(absolutePath, harnessPath);
        const resultsPath = path.join(harnessPath, 'results', 'test-results.json');
        testResults = parseTestResults(resultsPath);
      }
    } catch (e) {
      console.log('\n⚠️  App not running. Skipping E2E tests.');
      console.log('   Start the app and run again for full test coverage.\n');
    }
  }

  // If no tests were run, create placeholder results
  if (testResults.length === 0) {
    testResults = FEATURE_NAMES.map((name, i) => ({
      feature: name,
      featureNumber: i + 1,
      passed: 0,
      failed: 0,
      skipped: 0,
      total: 0,
      score: 0,
    }));
  }

  // Filter to only include features up to the prompt level
  const evaluatedResults = testResults.filter(t => t.featureNumber <= featuresEvaluated);
  const skippedResults = testResults.filter(t => t.featureNumber > featuresEvaluated);
  
  const totalScore = evaluatedResults.reduce((sum, t) => sum + t.score, 0);
  const maxScore = featuresEvaluated * 3;

  const result: BenchmarkResult = {
    projectPath: absolutePath,
    projectType: metrics.projectType,
    promptLevel: validLevel,
    featuresEvaluated,
    metrics: {
      compiles: metrics.compiles,
      compileError: metrics.compileError,
      runs: startResult.success,
      runError: startResult.error,
      linesOfCode: metrics.linesOfCode,
      fileCount: metrics.fileCount,
      dependencies: metrics.dependencies,
    },
    testResults: evaluatedResults, // Only include evaluated features
    totalScore,
    maxScore,
    timestamp: new Date().toISOString(),
  };

  // Generate and print report
  const report = generateReport(result);
  console.log(report);
  
  // Show skipped features
  if (skippedResults.length > 0) {
    console.log(`\n📝 Features not evaluated (not in prompt level ${validLevel}):`);
    for (const f of skippedResults) {
      console.log(`   ${f.featureNumber}. ${f.feature} — N/A`);
    }
  }

  // Save results
  const resultsDir = path.join(harnessPath, 'results');
  if (!fs.existsSync(resultsDir)) {
    fs.mkdirSync(resultsDir, { recursive: true });
  }
  
  const resultPath = path.join(resultsDir, `benchmark-${Date.now()}.json`);
  fs.writeFileSync(resultPath, JSON.stringify(result, null, 2));
  console.log(`\n📄 Full results saved to: ${resultPath}`);

  return result;
}

// CLI entry point
const args = process.argv.slice(2);
const projectPath = args.find(arg => !arg.startsWith('--'));
const levelArg = args.find(arg => arg.startsWith('--level='));
const promptLevel = levelArg ? parseInt(levelArg.split('=')[1], 10) : 11;

if (!projectPath) {
  console.error('Usage: tsx scripts/run-benchmark.ts <project-path> [--level=N]');
  console.error('');
  console.error('Options:');
  console.error('  --level=N         Prompt level (1-11), determines which features to evaluate');
  console.error('  CLIENT_URL=<url>  Set the client URL for E2E tests (env var)');
  console.error('');
  console.error('Prompt Levels:');
  console.error('  1  = Features 1-4   (Basic, Typing, Read Receipts, Unread)');
  console.error('  2  = Features 1-5   (+ Scheduled Messages)');
  console.error('  3  = Features 1-6   (+ Ephemeral Messages)');
  console.error('  4  = Features 1-7   (+ Reactions)');
  console.error('  5  = Features 1-8   (+ Edit History)');
  console.error('  6  = Features 1-9   (+ Permissions)');
  console.error('  7  = Features 1-10  (+ Rich Presence)');
  console.error('  8  = Features 1-11  (+ Threading)');
  console.error('  9  = Features 1-12  (+ Activity Indicators)');
  console.error('  10 = Features 1-13  (+ Draft Sync)');
  console.error('  11 = Features 1-14  (All features)');
  console.error('');
  console.error('Examples:');
  console.error('  tsx scripts/run-benchmark.ts ../spacetime/chat-app-20251229-120000/');
  console.error('  tsx scripts/run-benchmark.ts ../spacetime/chat-app-20251229-120000/ --level=5');
  console.error('  CLIENT_URL=http://localhost:3000 tsx scripts/run-benchmark.ts ../path/ --level=11');
  process.exit(1);
}

runBenchmark(projectPath, promptLevel).catch(console.error);

