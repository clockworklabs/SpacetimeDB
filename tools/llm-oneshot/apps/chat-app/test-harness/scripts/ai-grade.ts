#!/usr/bin/env npx tsx
/**
 * AI-Assisted Grading
 * 
 * Analyzes generated code to predict feature completeness and quality.
 * Uses static analysis + pattern matching to detect implemented features.
 * 
 * Usage:
 *   npm run ai-grade -- ../spacetime/chat-app-YYYYMMDD-HHMMSS/ --level=5
 */

import * as fs from 'fs';
import * as path from 'path';

interface FeatureAnalysis {
  feature: string;
  featureNumber: number;
  detected: boolean;
  confidence: 'high' | 'medium' | 'low' | 'none';
  evidence: string[];
  score: number; // 0-3 predicted score
}

interface AIGradeResult {
  projectPath: string;
  projectType: 'spacetime' | 'postgres' | 'unknown';
  promptLevel: number;
  features: FeatureAnalysis[];
  totalScore: number;
  maxScore: number;
  codeQuality: {
    hasTypes: boolean;
    hasErrorHandling: boolean;
    hasValidation: boolean;
    complexity: 'simple' | 'moderate' | 'complex';
  };
  timestamp: string;
}

// Feature detection patterns
const FEATURE_PATTERNS: Record<number, {
  name: string;
  backend: RegExp[];
  frontend: RegExp[];
  required: number; // How many patterns must match
}> = {
  1: {
    name: 'Basic Chat',
    backend: [
      /reducer.*send.*message/i,
      /reducer.*create.*room/i,
      /table.*message/i,
      /table.*room/i,
      /table.*user/i,
    ],
    frontend: [
      /sendMessage|send_message/i,
      /createRoom|create_room/i,
      /<input.*message/i,
      /room.*list|roomList/i,
    ],
    required: 3,
  },
  2: {
    name: 'Typing Indicators',
    backend: [
      /typing|isTyping|is_typing/i,
      /reducer.*typing/i,
      /table.*typing/i,
    ],
    frontend: [
      /typing.*indicator|typingIndicator/i,
      /is.*typing|isTyping/i,
      /setTyping|set_typing/i,
    ],
    required: 2,
  },
  3: {
    name: 'Read Receipts',
    backend: [
      /read.*receipt|readReceipt|message_read/i,
      /seen.*by|seenBy|read_at/i,
      /reducer.*mark.*read/i,
    ],
    frontend: [
      /read.*receipt|readReceipt/i,
      /seen.*by|seenBy/i,
      /mark.*read|markRead/i,
    ],
    required: 2,
  },
  4: {
    name: 'Unread Counts',
    backend: [
      /unread.*count|unreadCount/i,
      /last.*read|lastRead/i,
    ],
    frontend: [
      /unread.*count|unreadCount/i,
      /badge|notification.*count/i,
      /unread.*message/i,
    ],
    required: 2,
  },
  5: {
    name: 'Scheduled Messages',
    backend: [
      /scheduled?.*message|scheduleAt/i,
      /schedule.*reducer|scheduled.*table/i,
      /scheduledAt|scheduled_at/i,
    ],
    frontend: [
      /schedule.*message|scheduledMessage/i,
      /datetime.*local|time.*picker/i,
      /pending.*message/i,
    ],
    required: 2,
  },
  6: {
    name: 'Ephemeral Messages',
    backend: [
      /ephemeral|disappear|expires?_at|ttl/i,
      /delete.*after|auto.*delete/i,
    ],
    frontend: [
      /ephemeral|disappear/i,
      /countdown|timer.*expire/i,
      /ttl|time.*live/i,
    ],
    required: 2,
  },
  7: {
    name: 'Reactions',
    backend: [
      /reaction|emoji/i,
      /reducer.*react|add.*reaction/i,
      /table.*reaction/i,
    ],
    frontend: [
      /reaction|emoji.*picker/i,
      /add.*reaction|react.*message/i,
      /👍|👎|❤️|😀/,
    ],
    required: 2,
  },
  8: {
    name: 'Message Editing',
    backend: [
      /edit.*message|update.*message/i,
      /message.*history|edit.*history/i,
      /edited.*at|updated_at/i,
    ],
    frontend: [
      /edit.*message|editMessage/i,
      /\(edited\)|edited.*indicator/i,
      /show.*history|viewHistory/i,
    ],
    required: 2,
  },
  9: {
    name: 'Permissions',
    backend: [
      /admin|moderator|role/i,
      /kick|ban|remove.*user/i,
      /permission|can.*send/i,
    ],
    frontend: [
      /admin|moderator/i,
      /kick|ban/i,
      /permission|role/i,
    ],
    required: 2,
  },
  10: {
    name: 'Rich Presence',
    backend: [
      /presence|status.*online|away|dnd/i,
      /last.*active|lastActive/i,
      /invisible|do.*not.*disturb/i,
    ],
    frontend: [
      /presence|status.*indicator/i,
      /online|away|busy|offline/i,
      /last.*seen|lastSeen/i,
    ],
    required: 2,
  },
  11: {
    name: 'Threading',
    backend: [
      /thread|reply.*to|parent.*id/i,
      /thread.*id|replyTo/i,
    ],
    frontend: [
      /thread|reply/i,
      /parent.*message|thread.*view/i,
      /reply.*count|replyCount/i,
    ],
    required: 2,
  },
  12: {
    name: 'Activity Indicators',
    backend: [
      /activity|velocity|hot|active/i,
      /message.*count.*recent/i,
    ],
    frontend: [
      /activity.*indicator|activityIndicator/i,
      /hot|active|quiet/i,
      /activity.*badge/i,
    ],
    required: 2,
  },
  13: {
    name: 'Draft Sync',
    backend: [
      /draft|unsent.*message/i,
      /save.*draft|draft.*table/i,
    ],
    frontend: [
      /draft|autosave/i,
      /sync.*draft|saveDraft/i,
    ],
    required: 2,
  },
  14: {
    name: 'Anonymous Migration',
    backend: [
      /anonymous|guest|temp.*user/i,
      /migrate|register|upgrade.*account/i,
      /merge.*identity|link.*account/i,
    ],
    frontend: [
      /anonymous|guest/i,
      /register|sign.*up|create.*account/i,
      /migrate|upgrade/i,
    ],
    required: 2,
  },
};

function detectProjectType(projectPath: string): 'spacetime' | 'postgres' | 'unknown' {
  if (projectPath.includes('spacetime')) return 'spacetime';
  if (projectPath.includes('postgres')) return 'postgres';
  return 'unknown';
}

function getAllSourceFiles(dir: string, extensions: string[]): string[] {
  const files: string[] = [];
  const excludeDirs = ['node_modules', 'dist', 'build', '.git', 'module_bindings'];
  
  function walk(currentDir: string) {
    if (!fs.existsSync(currentDir)) return;
    
    const entries = fs.readdirSync(currentDir, { withFileTypes: true });
    for (const entry of entries) {
      const fullPath = path.join(currentDir, entry.name);
      if (entry.isDirectory() && !excludeDirs.includes(entry.name)) {
        walk(fullPath);
      } else if (entry.isFile() && extensions.some(ext => entry.name.endsWith(ext))) {
        files.push(fullPath);
      }
    }
  }
  
  walk(dir);
  return files;
}

function analyzeFeature(
  featureNum: number,
  backendContent: string,
  frontendContent: string
): FeatureAnalysis {
  const pattern = FEATURE_PATTERNS[featureNum];
  if (!pattern) {
    return {
      feature: `Feature ${featureNum}`,
      featureNumber: featureNum,
      detected: false,
      confidence: 'none',
      evidence: [],
      score: 0,
    };
  }
  
  const evidence: string[] = [];
  let backendMatches = 0;
  let frontendMatches = 0;
  
  // Check backend patterns
  for (const regex of pattern.backend) {
    const matches = backendContent.match(regex);
    if (matches) {
      backendMatches++;
      evidence.push(`Backend: ${matches[0].slice(0, 50)}`);
    }
  }
  
  // Check frontend patterns
  for (const regex of pattern.frontend) {
    const matches = frontendContent.match(regex);
    if (matches) {
      frontendMatches++;
      evidence.push(`Frontend: ${matches[0].slice(0, 50)}`);
    }
  }
  
  const totalMatches = backendMatches + frontendMatches;
  const detected = totalMatches >= pattern.required;
  
  let confidence: 'high' | 'medium' | 'low' | 'none';
  let score: number;
  
  if (totalMatches >= pattern.backend.length + pattern.frontend.length - 1) {
    confidence = 'high';
    score = 3;
  } else if (totalMatches >= pattern.required + 1) {
    confidence = 'medium';
    score = 2;
  } else if (totalMatches >= pattern.required) {
    confidence = 'low';
    score = 1;
  } else if (totalMatches > 0) {
    confidence = 'low';
    score = 1;
  } else {
    confidence = 'none';
    score = 0;
  }
  
  return {
    feature: pattern.name,
    featureNumber: featureNum,
    detected,
    confidence,
    evidence: evidence.slice(0, 5), // Limit evidence
    score,
  };
}

function analyzeCodeQuality(content: string): AIGradeResult['codeQuality'] {
  const hasTypes = /:\s*(string|number|boolean|void|Promise|Identity|bigint)\b/.test(content) ||
                   /interface\s+\w+|type\s+\w+\s*=/.test(content);
  
  const hasErrorHandling = /try\s*{|catch\s*\(|\.catch\(|throw\s+new/.test(content) ||
                           /SenderError|Error\(/.test(content);
  
  const hasValidation = /if\s*\(\s*!|\.length\s*[<>=]|trim\(\)|\.test\(/.test(content) ||
                        /validate|isValid|required/.test(content);
  
  const lines = content.split('\n').length;
  const complexity: 'simple' | 'moderate' | 'complex' = 
    lines < 500 ? 'simple' : lines < 1500 ? 'moderate' : 'complex';
  
  return { hasTypes, hasErrorHandling, hasValidation, complexity };
}

function generateReport(result: AIGradeResult): string {
  const COLORS = {
    reset: '\x1b[0m',
    red: '\x1b[31m',
    green: '\x1b[32m',
    yellow: '\x1b[33m',
    blue: '\x1b[34m',
    cyan: '\x1b[36m',
    dim: '\x1b[2m',
  };
  
  let report = '\n' + '='.repeat(60) + '\n';
  report += '        AI-ASSISTED CODE ANALYSIS\n';
  report += '='.repeat(60) + '\n\n';
  
  report += `📁 Project: ${path.basename(result.projectPath)}\n`;
  report += `🏷️  Type: ${result.projectType.toUpperCase()}\n`;
  report += `📋 Level: ${result.promptLevel}\n`;
  report += `📅 Date: ${new Date(result.timestamp).toLocaleString()}\n\n`;
  
  report += '─'.repeat(60) + '\n';
  report += '                 CODE QUALITY\n';
  report += '─'.repeat(60) + '\n';
  report += `TypeScript Types: ${result.codeQuality.hasTypes ? '✅' : '❌'}\n`;
  report += `Error Handling: ${result.codeQuality.hasErrorHandling ? '✅' : '❌'}\n`;
  report += `Input Validation: ${result.codeQuality.hasValidation ? '✅' : '❌'}\n`;
  report += `Complexity: ${result.codeQuality.complexity}\n\n`;
  
  report += '─'.repeat(60) + '\n';
  report += '              FEATURE DETECTION\n';
  report += '─'.repeat(60) + '\n';
  
  const confidenceIcons: Record<string, string> = {
    high: '✅',
    medium: '⚠️ ',
    low: '🔶',
    none: '❌',
  };
  
  for (const feature of result.features) {
    const icon = confidenceIcons[feature.confidence];
    const scoreBar = '█'.repeat(feature.score) + '░'.repeat(3 - feature.score);
    report += `${feature.featureNumber.toString().padStart(2)}. ${feature.feature.padEnd(24)} ${icon} ${scoreBar} ${feature.score}/3`;
    report += ` (${feature.confidence})\n`;
    
    if (feature.evidence.length > 0) {
      report += `    ${COLORS.dim}${feature.evidence[0]}${COLORS.reset}\n`;
    }
  }
  
  report += '\n' + '─'.repeat(60) + '\n';
  report += `           PREDICTED SCORE: ${result.totalScore}/${result.maxScore}\n`;
  report += '─'.repeat(60) + '\n';
  
  const percentage = Math.round((result.totalScore / result.maxScore) * 100);
  let grade = '';
  if (percentage >= 90) grade = 'A';
  else if (percentage >= 80) grade = 'B';
  else if (percentage >= 70) grade = 'C';
  else if (percentage >= 60) grade = 'D';
  else grade = 'F';
  
  report += `           PREDICTED GRADE: ${grade} (${percentage}%)\n`;
  report += '='.repeat(60) + '\n';
  
  report += `\n${COLORS.yellow}⚠️  This is a static analysis prediction.${COLORS.reset}\n`;
  report += `${COLORS.cyan}   Run E2E tests for accurate scoring: npm run benchmark${COLORS.reset}\n`;
  
  return report;
}

async function main() {
  const args = process.argv.slice(2);
  const projectPath = args.find(arg => !arg.startsWith('--'));
  const levelArg = args.find(arg => arg.startsWith('--level='));
  const promptLevel = levelArg ? parseInt(levelArg.split('=')[1], 10) : 11;
  const jsonOutput = args.includes('--json');
  
  if (!projectPath) {
    console.log(`
AI-Assisted Grading - Analyze code to predict feature completeness

Usage: npx tsx scripts/ai-grade.ts <project-path> [options]

Options:
  --level=N     Prompt level (1-11) to evaluate
  --json        Output results as JSON

Examples:
  npx tsx scripts/ai-grade.ts ../spacetime/chat-app-20260105-120000/ --level=5
  npx tsx scripts/ai-grade.ts ../postgres/chat-app-20260105-120000/ --json
`);
    process.exit(1);
  }
  
  const absolutePath = path.resolve(projectPath);
  const projectType = detectProjectType(absolutePath);
  
  // Get all source files
  const backendPath = projectType === 'spacetime' 
    ? path.join(absolutePath, 'backend')
    : path.join(absolutePath, 'server');
  const frontendPath = path.join(absolutePath, 'client');
  
  const backendFiles = getAllSourceFiles(backendPath, ['.ts', '.tsx', '.js', '.jsx']);
  const frontendFiles = getAllSourceFiles(frontendPath, ['.ts', '.tsx', '.js', '.jsx']);
  
  // Read all content
  const backendContent = backendFiles.map(f => {
    try { return fs.readFileSync(f, 'utf-8'); } catch { return ''; }
  }).join('\n');
  
  const frontendContent = frontendFiles.map(f => {
    try { return fs.readFileSync(f, 'utf-8'); } catch { return ''; }
  }).join('\n');
  
  // Analyze features
  const PROMPT_LEVEL_FEATURES: Record<number, number> = {
    1: 4, 2: 5, 3: 6, 4: 7, 5: 8, 6: 9, 7: 10, 8: 11, 9: 12, 10: 13, 11: 14,
  };
  
  const featuresEvaluated = PROMPT_LEVEL_FEATURES[promptLevel] || 14;
  const features: FeatureAnalysis[] = [];
  
  for (let i = 1; i <= featuresEvaluated; i++) {
    features.push(analyzeFeature(i, backendContent, frontendContent));
  }
  
  const totalScore = features.reduce((sum, f) => sum + f.score, 0);
  const maxScore = featuresEvaluated * 3;
  
  const result: AIGradeResult = {
    projectPath: absolutePath,
    projectType,
    promptLevel,
    features,
    totalScore,
    maxScore,
    codeQuality: analyzeCodeQuality(backendContent + frontendContent),
    timestamp: new Date().toISOString(),
  };
  
  if (jsonOutput) {
    console.log(JSON.stringify(result, null, 2));
  } else {
    console.log(generateReport(result));
  }
  
  // Save results
  const resultsDir = path.join(path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Z]:)/, '$1')), 'results');
  if (!fs.existsSync(resultsDir)) {
    fs.mkdirSync(resultsDir, { recursive: true });
  }
  
  const resultPath = path.join(resultsDir, `ai-grade-${Date.now()}.json`);
  fs.writeFileSync(resultPath, JSON.stringify(result, null, 2));
  
  if (!jsonOutput) {
    console.log(`\n📄 Results saved to: ${resultPath}\n`);
  }
}

main().catch(e => {
  console.error(`Error: ${e.message}`);
  process.exit(1);
});
