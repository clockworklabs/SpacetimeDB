#!/usr/bin/env npx tsx
/**
 * Full automation: Deploy app and run benchmark tests
 * 
 * Usage:
 *   npm run deploy-test -- ../spacetime/chat-app-YYYYMMDD-HHMMSS/ --level=5
 *   npm run deploy-test -- ../postgres/chat-app-YYYYMMDD-HHMMSS/ --level=8
 */

import { execSync, spawn, ChildProcess } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import * as http from 'http';

interface DeployResult {
  success: boolean;
  error?: string;
  clientUrl?: string;
  processes: ChildProcess[];
}

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

function detectProjectType(projectPath: string): 'spacetime' | 'postgres' | 'unknown' {
  if (projectPath.includes('spacetime')) return 'spacetime';
  if (projectPath.includes('postgres')) return 'postgres';
  
  const backendPath = path.join(projectPath, 'backend');
  if (fs.existsSync(path.join(backendPath, 'spacetimedb')) || 
      fs.existsSync(path.join(backendPath, 'src', 'schema.ts'))) {
    return 'spacetime';
  }
  
  const dockerPath = path.join(projectPath, 'docker-compose.yml');
  if (fs.existsSync(dockerPath)) {
    const content = fs.readFileSync(dockerPath, 'utf-8');
    if (content.includes('postgres')) return 'postgres';
  }
  
  const serverPath = path.join(projectPath, 'server');
  if (fs.existsSync(serverPath)) return 'postgres';
  
  return 'unknown';
}

async function waitForServer(url: string, timeout: number = 60000): Promise<boolean> {
  const start = Date.now();
  log('cyan', `⏳ Waiting for ${url} to be ready...`);
  
  while (Date.now() - start < timeout) {
    try {
      const response = await fetch(url, { 
        method: 'HEAD',
        signal: AbortSignal.timeout(2000),
      });
      if (response.ok || response.status === 404) {
        log('green', `✅ Server ready at ${url}`);
        return true;
      }
    } catch {
      // Server not ready yet
    }
    await new Promise(r => setTimeout(r, 1000));
  }
  
  log('red', `❌ Server not ready after ${timeout/1000}s`);
  return false;
}

async function installDeps(dir: string, name: string): Promise<boolean> {
  if (!fs.existsSync(path.join(dir, 'package.json'))) {
    log('yellow', `⚠️  No package.json in ${name}`);
    return true;
  }
  
  log('cyan', `📦 Installing ${name} dependencies...`);
  try {
    execSync('npm install', { 
      cwd: dir, 
      stdio: 'pipe',
      timeout: 180000,
    });
    return true;
  } catch (e: any) {
    log('red', `❌ Failed to install ${name} deps: ${e.message?.slice(0, 100)}`);
    return false;
  }
}

async function deploySpacetime(projectPath: string): Promise<DeployResult> {
  const processes: ChildProcess[] = [];
  const backendPath = path.join(projectPath, 'backend');
  const clientPath = path.join(projectPath, 'client');
  
  // Check if SpacetimeDB is running
  log('cyan', '🔍 Checking SpacetimeDB server...');
  try {
    execSync('spacetime version', { stdio: 'pipe' });
  } catch {
    return { success: false, error: 'SpacetimeDB CLI not found. Install from https://spacetimedb.com', processes };
  }
  
  // Install deps
  if (!await installDeps(backendPath, 'backend')) {
    return { success: false, error: 'Backend dependency installation failed', processes };
  }
  if (!await installDeps(clientPath, 'client')) {
    return { success: false, error: 'Client dependency installation failed', processes };
  }
  
  // Publish module
  const moduleName = `benchmark-${Date.now()}`;
  log('cyan', `🚀 Publishing SpacetimeDB module as "${moduleName}"...`);
  try {
    execSync(`spacetime publish ${moduleName} --clear-database -y --project-path "${backendPath}"`, {
      stdio: 'inherit',
      timeout: 120000,
    });
  } catch (e: any) {
    return { success: false, error: `Failed to publish module: ${e.message}`, processes };
  }
  
  // Generate bindings
  log('cyan', '🔧 Generating client bindings...');
  const bindingsPath = path.join(clientPath, 'src', 'module_bindings');
  try {
    if (!fs.existsSync(bindingsPath)) {
      fs.mkdirSync(bindingsPath, { recursive: true });
    }
    execSync(`spacetime generate --lang typescript --out-dir "${bindingsPath}" --project-path "${backendPath}"`, {
      stdio: 'inherit',
      timeout: 60000,
    });
  } catch (e: any) {
    log('yellow', `⚠️  Bindings generation failed (may already exist): ${e.message?.slice(0, 50)}`);
  }
  
  // Update client config with module name
  const configPath = path.join(clientPath, 'src', 'config.ts');
  if (fs.existsSync(configPath)) {
    let config = fs.readFileSync(configPath, 'utf-8');
    config = config.replace(/MODULE_NAME\s*=\s*['"][^'"]*['"]/, `MODULE_NAME = '${moduleName}'`);
    fs.writeFileSync(configPath, config);
  }
  
  // Start client dev server
  log('cyan', '🌐 Starting client dev server...');
  const clientProcess = spawn('npm', ['run', 'dev'], {
    cwd: clientPath,
    stdio: 'pipe',
    shell: true,
    detached: false,
  });
  processes.push(clientProcess);
  
  // Wait for client to be ready
  const clientReady = await waitForServer('http://localhost:5173', 30000);
  if (!clientReady) {
    return { success: false, error: 'Client dev server failed to start', processes };
  }
  
  return { success: true, clientUrl: 'http://localhost:5173', processes };
}

async function deployPostgres(projectPath: string): Promise<DeployResult> {
  const processes: ChildProcess[] = [];
  const serverPath = path.join(projectPath, 'server');
  const clientPath = path.join(projectPath, 'client');
  
  // Install deps
  if (!await installDeps(serverPath, 'server')) {
    return { success: false, error: 'Server dependency installation failed', processes };
  }
  if (!await installDeps(clientPath, 'client')) {
    return { success: false, error: 'Client dependency installation failed', processes };
  }
  
  // Start Docker if docker-compose exists
  const dockerPath = path.join(projectPath, 'docker-compose.yml');
  if (fs.existsSync(dockerPath)) {
    log('cyan', '🐳 Starting Docker containers...');
    try {
      execSync('docker compose up -d', { cwd: projectPath, stdio: 'inherit', timeout: 120000 });
      await new Promise(r => setTimeout(r, 5000)); // Wait for DB to be ready
    } catch (e: any) {
      log('yellow', `⚠️  Docker compose failed: ${e.message?.slice(0, 50)}`);
    }
  }
  
  // Run migrations if needed
  const packageJson = JSON.parse(fs.readFileSync(path.join(serverPath, 'package.json'), 'utf-8'));
  if (packageJson.scripts?.['db:push'] || packageJson.scripts?.migrate) {
    log('cyan', '🗃️  Running database migrations...');
    try {
      const migrateCmd = packageJson.scripts?.['db:push'] ? 'npm run db:push' : 'npm run migrate';
      execSync(migrateCmd, { cwd: serverPath, stdio: 'inherit', timeout: 60000 });
    } catch (e: any) {
      log('yellow', `⚠️  Migration failed: ${e.message?.slice(0, 50)}`);
    }
  }
  
  // Start server
  log('cyan', '🖥️  Starting backend server...');
  const serverProcess = spawn('npm', ['run', 'dev'], {
    cwd: serverPath,
    stdio: 'pipe',
    shell: true,
    detached: false,
  });
  processes.push(serverProcess);
  
  // Wait for server
  const serverReady = await waitForServer('http://localhost:3001', 30000);
  if (!serverReady) {
    // Try common alternative ports
    const altReady = await waitForServer('http://localhost:3000', 5000);
    if (!altReady) {
      log('yellow', '⚠️  Backend server may not be ready, continuing...');
    }
  }
  
  // Start client
  log('cyan', '🌐 Starting client dev server...');
  const clientProcess = spawn('npm', ['run', 'dev'], {
    cwd: clientPath,
    stdio: 'pipe',
    shell: true,
    detached: false,
  });
  processes.push(clientProcess);
  
  // Wait for client - try both common ports
  let clientUrl = 'http://localhost:5173';
  let clientReady = await waitForServer(clientUrl, 30000);
  if (!clientReady) {
    clientUrl = 'http://localhost:5174';
    clientReady = await waitForServer(clientUrl, 10000);
  }
  
  if (!clientReady) {
    return { success: false, error: 'Client dev server failed to start', processes };
  }
  
  return { success: true, clientUrl, processes };
}

function cleanup(processes: ChildProcess[]) {
  log('cyan', '\n🧹 Cleaning up processes...');
  for (const proc of processes) {
    try {
      if (proc.pid) {
        process.kill(-proc.pid, 'SIGTERM');
      }
    } catch {
      try {
        proc.kill('SIGTERM');
      } catch {
        // Process already dead
      }
    }
  }
}

async function main() {
  const args = process.argv.slice(2);
  const projectPath = args.find(arg => !arg.startsWith('--'));
  const levelArg = args.find(arg => arg.startsWith('--level='));
  const promptLevel = levelArg ? parseInt(levelArg.split('=')[1], 10) : 11;
  const keepRunning = args.includes('--keep');
  
  if (!projectPath) {
    console.log(`
Usage: npx tsx scripts/deploy-and-test.ts <project-path> [options]

Options:
  --level=N     Prompt level (1-11) to evaluate
  --keep        Keep servers running after tests complete

Examples:
  npx tsx scripts/deploy-and-test.ts ../spacetime/chat-app-20260105-120000/ --level=5
  npx tsx scripts/deploy-and-test.ts ../postgres/chat-app-20260105-120000/ --level=8 --keep
`);
    process.exit(1);
  }
  
  const absolutePath = path.resolve(projectPath);
  const projectType = detectProjectType(absolutePath);
  
  log('blue', '\n' + '='.repeat(60));
  log('blue', '        AUTOMATED DEPLOY & TEST');
  log('blue', '='.repeat(60));
  log('cyan', `📁 Project: ${path.basename(absolutePath)}`);
  log('cyan', `🏷️  Type: ${projectType.toUpperCase()}`);
  log('cyan', `📋 Level: ${promptLevel}`);
  log('blue', '='.repeat(60) + '\n');
  
  if (projectType === 'unknown') {
    log('red', '❌ Could not detect project type (spacetime or postgres)');
    process.exit(1);
  }
  
  // Deploy
  let deployResult: DeployResult;
  if (projectType === 'spacetime') {
    deployResult = await deploySpacetime(absolutePath);
  } else {
    deployResult = await deployPostgres(absolutePath);
  }
  
  if (!deployResult.success) {
    log('red', `\n❌ Deployment failed: ${deployResult.error}`);
    cleanup(deployResult.processes);
    process.exit(1);
  }
  
  log('green', `\n✅ Deployment successful! App running at ${deployResult.clientUrl}`);
  
  // Run benchmark
  log('cyan', '\n📊 Running benchmark tests...\n');
  
  const harnessPath = path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Z]:)/, '$1'));
  
  try {
    execSync(`npx tsx scripts/run-benchmark.ts "${absolutePath}" --level=${promptLevel}`, {
      cwd: path.dirname(harnessPath),
      stdio: 'inherit',
      env: {
        ...process.env,
        CLIENT_URL: deployResult.clientUrl,
      },
      timeout: 600000, // 10 minute timeout
    });
  } catch (e: any) {
    log('yellow', '\n⚠️  Some tests may have failed');
  }
  
  // Cleanup or keep running
  if (keepRunning) {
    log('green', `\n✅ App still running at ${deployResult.clientUrl}`);
    log('cyan', 'Press Ctrl+C to stop...\n');
    
    process.on('SIGINT', () => {
      cleanup(deployResult.processes);
      process.exit(0);
    });
    
    // Keep process alive
    await new Promise(() => {});
  } else {
    cleanup(deployResult.processes);
    log('green', '\n✅ Done!\n');
  }
}

main().catch(e => {
  log('red', `\n❌ Error: ${e.message}`);
  process.exit(1);
});
