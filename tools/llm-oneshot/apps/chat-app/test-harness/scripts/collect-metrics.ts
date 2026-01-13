import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';

interface Metrics {
  projectPath: string;
  projectType: 'spacetime' | 'postgres' | 'unknown';
  compiles: boolean;
  compileError?: string;
  linesOfCode: {
    backend: number;
    frontend: number;
    total: number;
  };
  fileCount: {
    backend: number;
    frontend: number;
    total: number;
  };
  dependencies: {
    backend: string[];
    frontend: string[];
  };
  timestamp: string;
}

/**
 * Count lines of code in a directory (excluding node_modules, etc.)
 */
function countLines(dir: string): number {
  if (!fs.existsSync(dir)) return 0;
  
  let total = 0;
  const excludeDirs = ['node_modules', 'dist', 'build', '.git', 'module_bindings'];
  
  function walk(currentDir: string) {
    const entries = fs.readdirSync(currentDir, { withFileTypes: true });
    
    for (const entry of entries) {
      const fullPath = path.join(currentDir, entry.name);
      
      if (entry.isDirectory()) {
        if (!excludeDirs.includes(entry.name)) {
          walk(fullPath);
        }
      } else if (entry.isFile()) {
        const ext = path.extname(entry.name).toLowerCase();
        if (['.ts', '.tsx', '.js', '.jsx', '.css', '.html'].includes(ext)) {
          try {
            const content = fs.readFileSync(fullPath, 'utf-8');
            const lines = content.split('\n').filter(line => line.trim().length > 0);
            total += lines.length;
          } catch (e) {
            // Skip files that can't be read
          }
        }
      }
    }
  }
  
  walk(dir);
  return total;
}

/**
 * Count files in a directory
 */
function countFiles(dir: string): number {
  if (!fs.existsSync(dir)) return 0;
  
  let total = 0;
  const excludeDirs = ['node_modules', 'dist', 'build', '.git', 'module_bindings'];
  
  function walk(currentDir: string) {
    const entries = fs.readdirSync(currentDir, { withFileTypes: true });
    
    for (const entry of entries) {
      const fullPath = path.join(currentDir, entry.name);
      
      if (entry.isDirectory()) {
        if (!excludeDirs.includes(entry.name)) {
          walk(fullPath);
        }
      } else if (entry.isFile()) {
        const ext = path.extname(entry.name).toLowerCase();
        if (['.ts', '.tsx', '.js', '.jsx', '.css', '.html', '.json'].includes(ext)) {
          total++;
        }
      }
    }
  }
  
  walk(dir);
  return total;
}

/**
 * Get dependencies from package.json
 */
function getDependencies(packageJsonPath: string): string[] {
  if (!fs.existsSync(packageJsonPath)) return [];
  
  try {
    const content = JSON.parse(fs.readFileSync(packageJsonPath, 'utf-8'));
    const deps = Object.keys(content.dependencies || {});
    const devDeps = Object.keys(content.devDependencies || {});
    return [...new Set([...deps, ...devDeps])];
  } catch (e) {
    return [];
  }
}

/**
 * Detect project type
 */
function detectProjectType(projectPath: string): 'spacetime' | 'postgres' | 'unknown' {
  if (projectPath.includes('spacetime')) return 'spacetime';
  if (projectPath.includes('postgres')) return 'postgres';
  
  // Check for SpacetimeDB markers
  const backendPath = path.join(projectPath, 'backend');
  if (fs.existsSync(path.join(backendPath, 'spacetimedb'))) return 'spacetime';
  
  // Check for PostgreSQL markers (docker-compose with postgres)
  const dockerPath = path.join(projectPath, 'docker-compose.yml');
  if (fs.existsSync(dockerPath)) {
    const content = fs.readFileSync(dockerPath, 'utf-8');
    if (content.includes('postgres')) return 'postgres';
  }
  
  return 'unknown';
}

/**
 * Get backend and frontend paths based on project type
 */
function getPaths(projectPath: string, projectType: string): { backend: string; frontend: string } {
  if (projectType === 'spacetime') {
    return {
      backend: path.join(projectPath, 'backend', 'spacetimedb'),
      frontend: path.join(projectPath, 'client', 'src'),
    };
  } else {
    return {
      backend: path.join(projectPath, 'server'),
      frontend: path.join(projectPath, 'client'),
    };
  }
}

/**
 * Try to compile the project
 */
function tryCompile(projectPath: string, projectType: string): { success: boolean; error?: string } {
  const paths = getPaths(projectPath, projectType);
  
  // Try frontend
  const frontendPackageJson = projectType === 'spacetime' 
    ? path.join(projectPath, 'client', 'package.json')
    : path.join(projectPath, 'client', 'package.json');
  
  if (fs.existsSync(frontendPackageJson)) {
    const frontendDir = path.dirname(frontendPackageJson);
    try {
      // Install dependencies
      execSync('npm install', { cwd: frontendDir, stdio: 'pipe', timeout: 120000 });
      
      // Try to build
      const packageJson = JSON.parse(fs.readFileSync(frontendPackageJson, 'utf-8'));
      if (packageJson.scripts?.build) {
        execSync('npm run build', { cwd: frontendDir, stdio: 'pipe', timeout: 120000 });
      } else if (packageJson.scripts?.['type-check']) {
        execSync('npm run type-check', { cwd: frontendDir, stdio: 'pipe', timeout: 60000 });
      }
    } catch (e: any) {
      return { success: false, error: e.message || 'Frontend compilation failed' };
    }
  }
  
  // Try backend
  const backendPackageJson = projectType === 'spacetime'
    ? path.join(projectPath, 'backend', 'package.json')
    : path.join(projectPath, 'server', 'package.json');
  
  if (fs.existsSync(backendPackageJson)) {
    const backendDir = path.dirname(backendPackageJson);
    try {
      execSync('npm install', { cwd: backendDir, stdio: 'pipe', timeout: 120000 });
      
      const packageJson = JSON.parse(fs.readFileSync(backendPackageJson, 'utf-8'));
      if (packageJson.scripts?.build) {
        execSync('npm run build', { cwd: backendDir, stdio: 'pipe', timeout: 120000 });
      }
    } catch (e: any) {
      return { success: false, error: e.message || 'Backend compilation failed' };
    }
  }
  
  return { success: true };
}

/**
 * Collect all metrics for a project
 */
export function collectMetrics(projectPath: string, skipCompile = false): Metrics {
  const absolutePath = path.resolve(projectPath);
  const projectType = detectProjectType(absolutePath);
  const paths = getPaths(absolutePath, projectType);
  
  // Compile check
  let compiles = true;
  let compileError: string | undefined;
  
  if (!skipCompile) {
    const compileResult = tryCompile(absolutePath, projectType);
    compiles = compileResult.success;
    compileError = compileResult.error;
  }
  
  // Lines of code
  const backendLOC = countLines(paths.backend);
  const frontendLOC = countLines(paths.frontend);
  
  // File count
  const backendFiles = countFiles(paths.backend);
  const frontendFiles = countFiles(paths.frontend);
  
  // Dependencies
  const backendDeps = getDependencies(
    projectType === 'spacetime'
      ? path.join(absolutePath, 'backend', 'package.json')
      : path.join(absolutePath, 'server', 'package.json')
  );
  const frontendDeps = getDependencies(
    projectType === 'spacetime'
      ? path.join(absolutePath, 'client', 'package.json')
      : path.join(absolutePath, 'client', 'package.json')
  );
  
  return {
    projectPath: absolutePath,
    projectType,
    compiles,
    compileError,
    linesOfCode: {
      backend: backendLOC,
      frontend: frontendLOC,
      total: backendLOC + frontendLOC,
    },
    fileCount: {
      backend: backendFiles,
      frontend: frontendFiles,
      total: backendFiles + frontendFiles,
    },
    dependencies: {
      backend: backendDeps,
      frontend: frontendDeps,
    },
    timestamp: new Date().toISOString(),
  };
}

// CLI entry point
if (import.meta.url === `file://${process.argv[1]}`) {
  const projectPath = process.argv[2];
  
  if (!projectPath) {
    console.error('Usage: tsx scripts/collect-metrics.ts <project-path>');
    process.exit(1);
  }
  
  const skipCompile = process.argv.includes('--skip-compile');
  const metrics = collectMetrics(projectPath, skipCompile);
  
  console.log('\n=== Project Metrics ===\n');
  console.log(`Path: ${metrics.projectPath}`);
  console.log(`Type: ${metrics.projectType}`);
  console.log(`Compiles: ${metrics.compiles ? '✅ Yes' : '❌ No'}`);
  if (metrics.compileError) {
    console.log(`Compile Error: ${metrics.compileError.slice(0, 200)}...`);
  }
  console.log(`\nLines of Code:`);
  console.log(`  Backend:  ${metrics.linesOfCode.backend}`);
  console.log(`  Frontend: ${metrics.linesOfCode.frontend}`);
  console.log(`  Total:    ${metrics.linesOfCode.total}`);
  console.log(`\nFile Count:`);
  console.log(`  Backend:  ${metrics.fileCount.backend}`);
  console.log(`  Frontend: ${metrics.fileCount.frontend}`);
  console.log(`  Total:    ${metrics.fileCount.total}`);
  console.log(`\nDependencies:`);
  console.log(`  Backend:  ${metrics.dependencies.backend.length} packages`);
  console.log(`  Frontend: ${metrics.dependencies.frontend.length} packages`);
  
  // Save to file
  const outputPath = path.join(path.dirname(projectPath), 'metrics.json');
  fs.writeFileSync(outputPath, JSON.stringify(metrics, null, 2));
  console.log(`\nMetrics saved to: ${outputPath}`);
}

