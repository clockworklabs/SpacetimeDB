#!/usr/bin/env npx tsx
/**
 * Create a new project scaffold in the staging folder
 * 
 * Usage:
 *   npm run create -- --lang=typescript --llm=opus-4-5 --backend=spacetime
 *   npm run create -- --lang=rust --llm=gpt-5 --backend=spacetime
 * 
 * Creates:
 *   staging/<lang>/<llm>/<backend>/chat-app-YYYYMMDD-HHMMSS/
 *     ├── backend/
 *     └── client/
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

function getTimestamp(): string {
  const now = new Date();
  const pad = (n: number) => n.toString().padStart(2, '0');
  return `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}-${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
}

const VALID_LANGUAGES = ['typescript', 'rust', 'csharp'];
const VALID_BACKENDS = ['spacetime', 'postgres'];

async function main() {
  const args = process.argv.slice(2);
  
  const langArg = args.find(arg => arg.startsWith('--lang='));
  const llmArg = args.find(arg => arg.startsWith('--llm='));
  const backendArg = args.find(arg => arg.startsWith('--backend='));
  const nameArg = args.find(arg => arg.startsWith('--name='));
  
  const language = langArg?.split('=')[1];
  const llm = llmArg?.split('=')[1];
  const backend = backendArg?.split('=')[1];
  const customName = nameArg?.split('=')[1];
  
  if (!language || !llm || !backend) {
    console.log(`
Usage: npx tsx scripts/create-project.ts --lang=<language> --llm=<model> --backend=<type> [--name=<name>]

Required:
  --lang=<language>   Target language: ${VALID_LANGUAGES.join(', ')}
  --llm=<model>       LLM model name (e.g., opus-4-5, gpt-5, gemini-3-pro)
  --backend=<type>    Backend type: ${VALID_BACKENDS.join(', ')}

Optional:
  --name=<name>       Custom app name (default: chat-app-YYYYMMDD-HHMMSS)

Creates a new project folder in staging:
  staging/<lang>/<llm>/<backend>/<name>/

Examples:
  npx tsx scripts/create-project.ts --lang=typescript --llm=opus-4-5 --backend=spacetime
  npx tsx scripts/create-project.ts --lang=rust --llm=gpt-5 --backend=spacetime --name=my-test
`);
    process.exit(1);
  }
  
  // Validate language
  if (!VALID_LANGUAGES.includes(language)) {
    log('red', `❌ Invalid language: ${language}`);
    log('cyan', `   Valid options: ${VALID_LANGUAGES.join(', ')}`);
    process.exit(1);
  }
  
  // Validate backend
  if (!VALID_BACKENDS.includes(backend)) {
    log('red', `❌ Invalid backend: ${backend}`);
    log('cyan', `   Valid options: ${VALID_BACKENDS.join(', ')}`);
    process.exit(1);
  }
  
  const appName = customName || `chat-app-${getTimestamp()}`;
  
  // Build path: staging/<lang>/<llm>/<backend>/<app-name>
  const scriptDir = path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Z]:)/, '$1'));
  const harnessDir = path.dirname(scriptDir);
  const chatAppDir = path.dirname(harnessDir);
  const projectPath = path.join(chatAppDir, 'staging', language, llm, backend, appName);
  
  log('blue', '\n' + '='.repeat(60));
  log('blue', '        CREATE NEW PROJECT');
  log('blue', '='.repeat(60));
  log('cyan', `📁 App: ${appName}`);
  log('cyan', `🌐 Language: ${language}`);
  log('cyan', `🤖 LLM: ${llm}`);
  log('cyan', `🔧 Backend: ${backend}`);
  log('cyan', `📍 Path: ${projectPath}`);
  log('blue', '='.repeat(60) + '\n');
  
  // Check if already exists
  if (fs.existsSync(projectPath)) {
    log('red', `❌ Project already exists: ${projectPath}`);
    process.exit(1);
  }
  
  // Create directory structure
  const backendDir = path.join(projectPath, 'backend');
  const clientDir = path.join(projectPath, 'client');
  
  fs.mkdirSync(backendDir, { recursive: true });
  fs.mkdirSync(path.join(clientDir, 'src'), { recursive: true });
  
  log('green', '✅ Created directory structure');
  
  // Create a README placeholder
  const readmeContent = `# ${appName}

**Language:** ${language}
**LLM:** ${llm}
**Backend:** ${backend}
**Created:** ${new Date().toISOString()}

## Status

- [ ] Backend implemented
- [ ] Client implemented  
- [ ] Deployed and tested
- [ ] Graded

## Grading

Run grading with:
\`\`\`bash
cd ../test-harness
npm run grade -- ${path.relative(harnessDir, projectPath).replace(/\\/g, '/')} --level=5
\`\`\`

After grading, promote to final location:
\`\`\`bash
npm run promote -- ${path.relative(harnessDir, projectPath).replace(/\\/g, '/')}
\`\`\`
`;
  
  fs.writeFileSync(path.join(projectPath, 'README.md'), readmeContent);
  log('green', '✅ Created README.md');
  
  // Output next steps
  log('cyan', '\n📋 Next steps:');
  log('cyan', `   1. cd "${projectPath}"`);
  log('cyan', '   2. Implement backend and client');
  log('cyan', '   3. Deploy and test:');
  log('cyan', `      npm run deploy-test -- ${path.relative(harnessDir, projectPath).replace(/\\/g, '/')} --level=5`);
  log('cyan', '   4. After grading, promote:');
  log('cyan', `      npm run promote -- ${path.relative(harnessDir, projectPath).replace(/\\/g, '/')}`);
  
  log('green', '\n✅ Project created successfully!\n');
  
  // Output the path for easy use
  console.log(projectPath);
}

main().catch(e => {
  log('red', `\n❌ Error: ${e.message}`);
  process.exit(1);
});
