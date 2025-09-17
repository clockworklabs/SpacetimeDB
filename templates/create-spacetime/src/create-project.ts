import fs from "fs-extra";
import path from "path";
import { spawn } from "cross-spawn";
import ora from "ora";
import chalk from "chalk";
import degit from "degit";
import { getTemplate } from "./templates/index.js";
import { PackageManager, getInstallCommand, getRunCommand } from "./utils/packageManager.js";
import { SPACETIME_VERSIONS, SERVER_CONFIG } from "./config.js";

const SPACETIME_SDK_PACKAGE = "@clockworklabs/spacetimedb-sdk";

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

    await setupProject(root, name, template, useLocal, packageManager);
    await installDependencies(root, packageManager);

    if (autoSetup) {
      await setupSpacetimeDB(root, name, useLocal, packageManager);
    }

    console.log();
    console.log(chalk.green("Project created successfully."));

    printInstructions(name, root, packageManager, autoSetup, useLocal);
    return true;
  } catch (error) {
    console.error(chalk.red("Failed to create project:"));
    console.error(chalk.red(error instanceof Error ? error.message : String(error)));

    try {
      await fs.remove(root);
      console.log(chalk.gray("Cleaned up incomplete project files"));
    } catch (cleanupError) {
      console.warn(chalk.yellow("Warning: Failed to clean up project files"));
      console.warn(chalk.gray(`You may need to manually remove: ${root}`));
      console.error("Cleanup error details:", cleanupError);
    }

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

async function createRootPackageJson(root: string, name: string, packageManager: PackageManager) {
  const packageJson = {
    name,
    version: "0.1.0",
    private: true,
    scripts: {
      dev: `cd client && ${getRunCommand(packageManager, "dev")}`,
      build: `cd server && spacetime build && cd ../client && ${getRunCommand(packageManager, "build")}`,
      deploy: `${getRunCommand(packageManager, "build")} && spacetime publish --project-path server --server maincloud ${name} && spacetime generate --project-path server --lang typescript --out-dir client/src/module_bindings`,
      local: `${getRunCommand(packageManager, "build")} && spacetime publish --project-path server --server local ${name} && spacetime generate --project-path server --lang typescript --out-dir client/src/module_bindings`,
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

async function configureServer(root: string, name: string, serverLanguage: string) {
  const serverDir = path.join(root, "server");

  if (serverLanguage === "rust") {
    await configureRustServer(serverDir, name);
  } else if (serverLanguage === "C#") {
    await configureCsharpServer(serverDir);
  }
}

async function updateConfigFile(
  filePath: string,
  updates: Record<string, string>,
  warnMessage: string,
): Promise<void> {
  try {
    if (!(await fs.pathExists(filePath))) {
      console.warn(
        chalk.gray(`Warning: ${path.basename(filePath)} not found, skipping configuration`),
      );
      return;
    }

    let content = await fs.readFile(filePath, "utf-8");

    for (const [pattern, replacement] of Object.entries(updates)) {
      const regex = new RegExp(pattern, "g");
      content = content.replace(regex, replacement);
    }

    await fs.writeFile(filePath, content);
  } catch (error) {
    console.warn(chalk.gray(warnMessage), error);
  }
}

// existing rust and C# example servers configs need to be updated to work with the project setup
async function configureRustServer(serverDir: string, name: string) {
  const safeName = name.replace(/[^a-zA-Z0-9_]/g, "_");

  await updateConfigFile(
    path.join(serverDir, "Cargo.toml"),
    {
      "^name = .*$": `name = "${safeName}"`,
      "edition\\.workspace = true": 'edition = "2021"',
      "log\\.workspace = true": 'log = "0.4"',
      'spacetimedb = \\{ path = ".*" \\}': `spacetimedb = "${SPACETIME_VERSIONS.CLI}"`,
      'spacetimedb-lib = \\{ path = ".*" \\}': `spacetimedb-lib = "${SPACETIME_VERSIONS.CLI}"`,
    },
    "Warning: Could not update Cargo.toml",
  );
}

async function configureCsharpServer(serverDir: string) {
  await updateConfigFile(
    path.join(serverDir, "StdbModule.csproj"),
    {
      '<PackageReference Include="SpacetimeDB\\.Runtime" Version="[^"]*" \\/>': `<PackageReference Include="SpacetimeDB.Runtime" Version="${SPACETIME_VERSIONS.RUNTIME}" />`,
    },
    "Warning: Could not update .csproj file",
  );
}

async function updateClientConfig(root: string, name: string, useLocal: boolean) {
  const targetUri = useLocal ? SERVER_CONFIG.LOCAL_URI : SERVER_CONFIG.MAINCLOUD_URI;

  await updateConfigFile(
    path.join(root, "client/src/App.tsx"),
    {
      "\\.withModuleName\\(['\"`][^'\"`]*['\"`]\\)": `.withModuleName('${name}')`,
      "\\.withUri\\(['\"`]ws:\\/\\/localhost:3000['\"`]\\)": `.withUri('${targetUri}')`,
    },
    "Warning: Could not update client config",
  );
}

async function installDependencies(root: string, packageManager: PackageManager): Promise<void> {
  const args = getInstallCommand(packageManager);
  const clientDir = path.join(root, "client");

  if (!(await fs.pathExists(clientDir))) {
    throw new Error(`Client directory not found: ${clientDir}`);
  }

  return new Promise((resolve, reject) => {
    const child = spawn(packageManager, args, {
      cwd: clientDir,
      stdio: "inherit",
    });

    const timeout = setTimeout(() => {
      child.kill("SIGTERM");
      reject(new Error(`${packageManager} install timeout`));
    }, 600000); // 10 min

    child.on("close", (code) => {
      clearTimeout(timeout);
      if (code !== 0) {
        reject(new Error(`${packageManager} install failed with exit code ${code}`));
        return;
      }
      resolve();
    });

    child.on("error", (error) => {
      clearTimeout(timeout);
      reject(new Error(`Failed to spawn ${packageManager}: ${error.message}`));
    });
  });
}

async function setupSpacetimeDB(
  root: string,
  name: string,
  useLocal: boolean,
  packageManager: PackageManager,
) {
  const target = useLocal ? "local" : "Maincloud";
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
    await runCommand(
      "spacetime",
      [
        "publish",
        "--project-path",
        "server",
        "--server",
        useLocal ? "local" : "maincloud",
        "--yes",
        name,
      ],
      root,
    );
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
    console.log(chalk.gray(`Database: ${chalk.bold(name)} (${target})`));

    if (!useLocal) {
      console.log(chalk.gray(`Dashboard: ${chalk.blue("https://spacetimedb.com/profile")}`));
    }
  } catch (error) {
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
    spawn("spacetime", ["start"], { detached: true, stdio: "ignore" });

    await new Promise((resolve) => setTimeout(resolve, 3000));

    if (!(await checkLocalServer())) {
      throw new Error("Failed to start local SpacetimeDB server");
    }
  }
}

// check if local SpacetimeDB server is running reliably
async function checkLocalServer(): Promise<boolean> {
  try {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 3000);

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
) {
  const projectDir = path.relative(process.cwd(), root);

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

function runCommand(command: string, args: string[], cwd: string): Promise<void> {
  const SAFE_COMMANDS = ["spacetime", "npm", "yarn", "pnpm", "bun"];
  if (!SAFE_COMMANDS.includes(command)) {
    throw new Error(`Unsafe command: ${command}`);
  }

  return new Promise((resolve, reject) => {
    const childProcess = spawn(command, args, { cwd, stdio: "inherit" });

    const timeout = setTimeout(() => {
      childProcess.kill("SIGTERM");
      reject(new Error(`Command timeout: ${command}`));
    }, 300000); // 5 min

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
