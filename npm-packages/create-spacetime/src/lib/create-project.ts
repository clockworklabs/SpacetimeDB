import fs from "fs-extra";
import path from "path";
import { spawn, sync } from "cross-spawn";
import ora from "ora";
import chalk from "chalk";
import degit from "degit";

import { getTemplate, isValidTemplate } from "../templates/index.js";
import { PackageManager, getInstallCommand, getRunCommand } from "../utils/package.js";
import {
  SPACETIME_VERSIONS,
  SERVER_CONFIG,
  TIMEOUTS,
  SPACETIME_SDK_PACKAGE,
} from "../constants.js";

export interface CreateProjectOptions {
  name: string;
  root: string;
  template: string;
  packageManager: PackageManager;
  useLocal?: boolean;
  autoSetup?: boolean;
}

export async function createProject(options: CreateProjectOptions): Promise<boolean> {
  const { name, root, template, packageManager, useLocal = false, autoSetup = false } = options;

  if (!name || !root || !template || !packageManager) {
    console.error(chalk.red("Missing required parameters for project creation"));
    return false;
  }

  try {
    console.log(`\nCreating ${chalk.bold(name)} in ${chalk.bold(root)}...`);

    if (isValidTemplate(template)) {
      await setupProject(root, name, template, useLocal, packageManager);
      await installDependencies(root, packageManager);

      if (autoSetup) {
        await setupSpacetimeDB(root, name, useLocal, packageManager);
      }

      console.log();
      console.log(chalk.green("Project created successfully."));
      console.log();
      printInstructions(name, root, packageManager, autoSetup, useLocal, template);
    } else {
      console.log(chalk.gray(`Downloading GitHub template: ${template}`));
      await setupGitHubTemplate(root, name, template);

      const clientDir = path.join(root, "client");
      if (await fs.pathExists(clientDir)) {
        await installDependencies(root, packageManager);
      } else {
        const packageJsonPath = path.join(root, "package.json");
        if (await fs.pathExists(packageJsonPath)) {
          await runCommand(packageManager, getInstallCommand(packageManager), root, 600000);
        }
      }

      console.log(
        chalk.gray("Auto-setup skipped for custom template. You may need to configure manually."),
      );
      console.log();
      printInstructions(name, root, packageManager, autoSetup, useLocal, template);
    }
    return true;
  } catch (error) {
    console.error(chalk.red("Failed to create project:"));
    console.error(chalk.red(error instanceof Error ? error.message : String(error)));

    await fs
      .remove(root)
      .catch(() => console.warn(chalk.yellow(`Warning: Manual cleanup needed: ${root}`)));

    return false;
  }
}

async function setupProject(
  root: string,
  name: string,
  templateKey: string,
  useLocal: boolean,
  packageManager: PackageManager,
) {
  const template = getTemplate(templateKey)!;

  const serverDir = path.join(root, "server");
  const clientDir = path.join(root, "client");

  await fs.ensureDir(clientDir);

  try {
    const clientEmitter = degit(template.clientRepository);
    await clientEmitter.clone(clientDir);
  } catch (error) {
    throw new Error(`Failed to clone client template from ${template.clientRepository}: ${error}`);
  }

  try {
    const serverEmitter = degit(template.serverRepository);
    await serverEmitter.clone(serverDir);
  } catch (error) {
    throw new Error(`Failed to clone server template from ${template.serverRepository}: ${error}`);
  }
  try {
    await createRootPackageJson(root, name, packageManager);
    await updateClientPackageJson(root, name);
    await configureServer(root, name, template.serverLanguage);
    await updateClientConfig(root, name, useLocal);
  } catch (error) {
    throw new Error(`Project configuration failed: ${error}`);
  }
}

async function setupGitHubTemplate(root: string, name: string, templateInput: string) {
  let repoPath = templateInput;
  if (templateInput.startsWith("https://github.com/")) {
    repoPath = templateInput.replace("https://github.com/", "");
  }

  try {
    const emitter = degit(repoPath);
    await emitter.clone(root);

    const packageJsonPath = path.join(root, "package.json");
    if (await fs.pathExists(packageJsonPath)) {
      const packageJson = await fs.readJSON(packageJsonPath);
      packageJson.name = name;
      await fs.writeJSON(packageJsonPath, packageJson, { spaces: 2 });
    }

    console.log(chalk.green(`Successfully downloaded ${repoPath}`));
  } catch {
    throw new Error(`Failed to clone GitHub template "${repoPath}". Please use owner/repo format.`);
  }
}

async function createRootPackageJson(root: string, name: string, packageManager: PackageManager) {
  const packageJson = {
    name,
    version: "0.1.0",
    private: true,
    scripts: {
      dev: `cd client && ${getRunCommand(packageManager, "dev")}`,
      build: `cd server && spacetime build && cd ../client && ${getRunCommand(packageManager, "build")}`,
      deploy: `${getRunCommand(packageManager, "build")} && spacetime publish --project-path server --server maincloud ${name} && spacetime generate --project-path server --lang typescript --out-dir client/src/module_bindings`,
      local: `${getRunCommand(packageManager, "build")} && spacetime publish --project-path server --server local ${name} --yes && spacetime generate --project-path server --lang typescript --out-dir client/src/module_bindings`,
    },
    workspaces: ["client"],
  };

  await fs.writeJSON(path.join(root, "package.json"), packageJson, { spaces: 2 });
}

async function updateClientPackageJson(root: string, name: string) {
  const clientPackagePath = path.join(root, "client/package.json");
  try {
    const clientPackage = await fs.readJSON(clientPackagePath);
    clientPackage.name = `${name}-client`;

    if (clientPackage.dependencies?.[SPACETIME_SDK_PACKAGE]) {
      clientPackage.dependencies[SPACETIME_SDK_PACKAGE] = SPACETIME_VERSIONS.SDK;
    }

    await fs.writeJSON(clientPackagePath, clientPackage, { spaces: 2 });
  } catch (error) {
    console.warn(chalk.gray("Warning: Could not update client package.json"), error);
  }
}

async function updateDeployScript(
  root: string,
  deploymentName: string,
  packageManager: PackageManager,
) {
  const packagePath = path.join(root, "package.json");
  try {
    const packageJson = await fs.readJSON(packagePath);
    packageJson.scripts.deploy = `${getRunCommand(packageManager, "build")} && spacetime publish --project-path server --server maincloud ${deploymentName} && spacetime generate --project-path server --lang typescript --out-dir client/src/module_bindings`;
    await fs.writeJSON(packagePath, packageJson, { spaces: 2 });
  } catch (error) {
    console.warn(chalk.gray("Warning: Could not update deploy script"), error);
  }
}

async function configureServer(root: string, name: string, serverLanguage: string) {
  const serverDir = path.join(root, "server");

  if (serverLanguage === "rust") {
    await configureRustServer(serverDir, name);
  }
}

// existing rust example server configs need to be updated to work with the project setup
async function configureRustServer(serverDir: string, name: string) {
  const safeName = name.replace(/[^a-zA-Z0-9_]/g, "_");
  const cargoPath = path.join(serverDir, "Cargo.toml");

  try {
    if (await fs.pathExists(cargoPath)) {
      let content = await fs.readFile(cargoPath, "utf-8");
      content = content.replace(/^name = .*$/m, `name = "${safeName}"`);
      content = content.replace(/edition\.workspace = true/g, 'edition = "2021"');
      content = content.replace(/log\.workspace = true/g, 'log = "0.4"');
      content = content.replace(
        /spacetimedb = \{ path = ".*" \}/g,
        `spacetimedb = "${SPACETIME_VERSIONS.CLI}"`,
      );
      content = content.replace(
        /spacetimedb-lib = \{ path = ".*" \}/g,
        `spacetimedb-lib = "${SPACETIME_VERSIONS.CLI}"`,
      );
      await fs.writeFile(cargoPath, content);
    }
  } catch (error) {
    console.warn(chalk.gray("Warning: Could not update Cargo.toml"), error);
  }
}

async function updateClientConfig(root: string, moduleName: string, useLocal: boolean) {
  const targetUri = useLocal ? SERVER_CONFIG.LOCAL_URI : SERVER_CONFIG.MAINCLOUD_URI;
  const appPath = path.join(root, "client/src/main.tsx");

  try {
    if (await fs.pathExists(appPath)) {
      let content = await fs.readFile(appPath, "utf-8");
      content = content.replace(
        /\.withModuleName\(['"`][^'"`]*['"`]\)/g,
        `.withModuleName('${moduleName}')`,
      );
      content = content.replace(
        /\.withUri\(['"`]ws:\/\/localhost:3000['"`]\)/g,
        `.withUri('${targetUri}')`,
      );
      await fs.writeFile(appPath, content);
    }
  } catch (error) {
    console.warn(chalk.gray("Warning: Could not update client config"), error);
  }
}

async function installDependencies(root: string, packageManager: PackageManager): Promise<void> {
  const clientDir = path.join(root, "client");
  if (!(await fs.pathExists(clientDir))) {
    throw new Error(`Client directory not found: ${clientDir}`);
  }
  await runCommand(packageManager, getInstallCommand(packageManager), clientDir, 600000);
}

async function setupSpacetimeDB(
  root: string,
  name: string,
  useLocal: boolean,
  packageManager: PackageManager,
) {
  const target = useLocal ? "local" : "Maincloud";

  const moduleName = useLocal ? name : `${name}-${Date.now()}`;
  const spinner = ora(`Setting up SpacetimeDB (${target})...`).start();

  try {
    if (useLocal) {
      await ensureLocalServer(spinner);
    } else {
      spinner.text = "Preparing Maincloud deployment...";
    }

    spinner.text = "Building server module...";
    spinner.stop();
    console.log();
    await runCommand("spacetime", ["build"], path.join(root, "server"));
    spinner.start(`Publishing to ${target}...`);
    spinner.stop();
    console.log();
    const publishArgs = [
      "publish",
      "--project-path",
      "server",
      "--server",
      useLocal ? "local" : "maincloud",
    ];

    if (!useLocal) {
      publishArgs.push("--yes");
    }
    publishArgs.push(moduleName);

    await runCommand("spacetime", publishArgs, root);
    spinner.start("Generating module bindings...");
    spinner.stop();
    console.log();
    await runCommand(
      "spacetime",
      [
        "generate",
        "--lang",
        "typescript",
        "--project-path",
        "server",
        "--out-dir",
        "client/src/module_bindings",
        "--yes",
      ],
      root,
    );

    console.log(chalk.green("SpacetimeDB setup completed"));
    console.log(chalk.gray(`Database: ${chalk.bold(moduleName)} (${target})`));

    if (!useLocal) {
      console.log(chalk.gray(`Dashboard: ${chalk.blue("https://spacetimedb.com/profile")}`));
    }

    await updateClientConfig(root, moduleName, useLocal);

    if (!useLocal) {
      await updateDeployScript(root, moduleName, packageManager);
    }
  } catch {
    spinner.stop();
    console.log(chalk.red(`SpacetimeDB setup failed (${target})`));
    console.log();
    console.log(chalk.yellow("You can set it up manually later:"));
    console.log(chalk.cyan("   cd server && spacetime build"));
    console.log(chalk.cyan(`   ${getRunCommand(packageManager, useLocal ? "local" : "deploy")}`));
    console.log();
  }
}

async function ensureLocalServer(spinner: any) {
  spinner.text = "Checking local SpacetimeDB server...";
  const isRunning = await checkLocalServer();

  if (!isRunning) {
    spinner.text = "Starting local SpacetimeDB server...";
    const child = spawn("spacetime", ["start"], {
      detached: true,
      stdio: "ignore",
    });

    child.unref();

    const cleanup = () => {
      if (child && !child.killed) {
        try {
          child.kill();
        } catch {
          // process cleanup failed, continue anyway
        }
      }
    };

    process.once("exit", cleanup);
    process.once("SIGINT", cleanup);
    process.once("SIGTERM", cleanup);

    await new Promise((resolve) => setTimeout(resolve, TIMEOUTS.SERVER_START_DELAY));

    if (!(await checkLocalServer())) {
      throw new Error("Failed to start local SpacetimeDB server");
    }
  }

  spinner.text = "Logging in to local server...";

  try {
    const result = sync("spacetime", ["login", "--server-issued-login", "local"], {
      stdio: "pipe",
      encoding: "utf8",
      timeout: TIMEOUTS.COMMAND_TIMEOUT,
      windowsHide: true,
    });

    if (result.status !== 0) {
      console.warn(chalk.yellow("Warning: Could not log in to local server"));
    }
  } catch {
    console.warn(chalk.yellow("Warning: Could not log in to local server"));
  }
}

async function checkLocalServer(): Promise<boolean> {
  try {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), TIMEOUTS.SERVER_START_DELAY);

    const response = await fetch(
      `http://localhost:${SERVER_CONFIG.LOCAL_PORT}/v1/identity/public-key`,
      {
        signal: controller.signal,
      },
    );

    clearTimeout(timeout);
    return response.ok;
  } catch {
    return false;
  }
}

function printInstructions(
  name: string,
  root: string,
  packageManager: PackageManager,
  autoSetup: boolean,
  useLocal: boolean,
  template: string,
) {
  const projectDir = path.relative(process.cwd(), root);

  const isGitHubTemplate = !isValidTemplate(template);

  if (isGitHubTemplate) {
    console.log();
    console.log(`Learn more at: ${chalk.bold("https://spacetimedb.com/docs/getting-started")}`);
    return;
  }

  let message = "To get started with development:\n\n";

  if (projectDir && projectDir !== ".") {
    message += `  cd ${projectDir.includes(" ") ? `"${projectDir}"` : projectDir}\n`;
  }

  if (autoSetup) {
    message += `  ${getRunCommand(packageManager, "dev")}\n`;
  } else {
    message += `  ${getRunCommand(packageManager, useLocal ? "local" : "deploy")}\n`;
    message += `  ${getRunCommand(packageManager, "dev")}\n`;
  }

  message += `\nLearn more at: ${chalk.bold("https://spacetimedb.com/docs")}\n`;

  console.log();
  console.log();
  console.log(message);
}

function runCommand(
  command: string,
  args: string[],
  cwd: string,
  timeoutMs: number = 300000, // 5 min
): Promise<void> {
  const SAFE_COMMANDS = ["spacetime", "npm", "yarn", "pnpm", "bun"];
  if (!SAFE_COMMANDS.includes(command)) {
    throw new Error(`Unsafe command: ${command}`);
  }

  return new Promise((resolve, reject) => {
    const childProcess = spawn(command, args, { cwd, stdio: "inherit" });

    const timeout = setTimeout(() => {
      childProcess.kill("SIGTERM");
      reject(new Error(`Command timeout: ${command}`));
    }, timeoutMs);

    childProcess.on("close", (code) => {
      clearTimeout(timeout);
      if (code === 0) {
        resolve();
      } else {
        reject(new Error(`Command failed: ${command} (exit code ${code})`));
      }
    });

    childProcess.on("error", (error) => {
      clearTimeout(timeout);
      reject(error);
    });
  });
}
