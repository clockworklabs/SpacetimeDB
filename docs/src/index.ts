#! /usr/bin/env node

import { DocConfig, DocSectionConfig } from "./types";

const { Command } = require("commander");
const clear = require("clear");
const figlet = require("figlet");
const path = require("path");
const fs = require("fs");
const fsExtra = require("fs-extra");

const cwd = process.cwd();
const DOCS_PATH = path.join(__dirname, "docs");
const CONFIG_PATH = path.join(cwd, "spacetime-docs.json");

let config = {
  docPath: "",
  order: [],
  editURLRoot: "",
};

if (fs.existsSync(CONFIG_PATH)) {
  const configOpts = JSON.parse(fs.readFileSync(CONFIG_PATH, "utf8"));
  config.docPath = configOpts.docPath || path.join(cwd, "docs");
  config.order = configOpts.order || [];
  config.editURLRoot = configOpts.editURLRoot || "";
} else {
  config.docPath = path.join(cwd, "docs");
  config.order = [];
  config.editURLRoot = "";
}

clear();

console.log(figlet.textSync("spacetime-docs", { horizontalLayout: "full" }));

const program = new Command();

program.version("1.0.0").description("Spacetime Docs CLI");

program.command("generate").action(() => {
  let unorderedSections: DocSectionConfig[] = [];

  const dirs = fs
    .readdirSync(config.docPath, { withFileTypes: true })
    .filter((dirent: any) => dirent.isDirectory())
    .map((dirent: any) => dirent.name);

  for (const dir of dirs) {
    const section: DocSectionConfig = {
      title: dir,
      identifier: dir.toLowerCase().replace(" ", "-"),
      comingSoon: false,
      hasPages: false,
      editUrl: `/${dir}`,
      jumpLinks: [],
      pages: [],
    };

    // Check for subdirectories (pages)
    const subDirs = fs
      .readdirSync(path.join(config.docPath, dir), { withFileTypes: true })
      .filter((dirent: any) => dirent.isDirectory())
      .map((dirent: any) => dirent.name);

    if (subDirs.length > 0) {
      section.hasPages = true;
      for (const subDir of subDirs) {
        const page: DocSectionConfig = {
          title: subDir,
          identifier: subDir.toLowerCase(),
          comingSoon: false,
          hasPages: false,
          editUrl: `/${dir}/${subDir}`,
          jumpLinks: [],
        };
        if (section.pages) {
          section.pages.push(page);
        } else {
          section.pages = [page];
        }
      }
    }

    unorderedSections.push(section);
  }

  let sections = config.order
    .map((orderTitle) => {
      return unorderedSections.find((section) => section.title === orderTitle);
    })
    .filter(Boolean);

  if (sections.length === 0 || sections === undefined) {
    sections = [];
  }

  const docConfig: DocConfig = {
    //@ts-ignore
    sections: sections,
    rootEditURL: config.editURLRoot, // replace with your actual root edit URL
  };

  const configContent = `export const docConfig = ${JSON.stringify(
    docConfig,
    null,
    2
  )};`;
  fs.writeFileSync(path.join(cwd, "docs-config.ts"), configContent);
  console.log("docs-config.ts generated successfully!");
});

program
  .command("page")
  .argument("<routeName>", "The route to create the page in")
  .argument("<pageName>", "The name of the page")
  .action((route: string, pageName: string) => {
    const routePath = path.join(config.docPath, route);
    const pagePath = path.join(routePath, `${pageName}.md`);

    if (!fs.existsSync(routePath)) {
      console.log(`Route ${route} does not exist.`);
      return;
    }

    if (fs.existsSync(pagePath)) {
      console.log(`Page ${pageName} already exists in route ${route}.`);
      return;
    }

    fs.writeFileSync(pagePath, `# ${pageName}`);
    console.log(`Page ${pageName} created successfully in route ${route}.`);
  });

program
  .command("remove-route")
  .argument("<routeName>", "The route to remove")
  .action((route: string) => {
    const routePath = path.join(config.docPath, route);

    if (fs.existsSync(routePath)) {
      fsExtra.removeSync(routePath);
      console.log(`Successfully removed route: ${route}`);
    } else {
      console.log(`Route ${route} does not exist.`);
    }
  });

program
  .command("create-route")
  .argument("<routeName>", "The route to create")
  .option(
    "-p, --parent <parentRoute>",
    "Parent route under which to create the subroute"
  )
  .action((routeName: string, options: any) => {
    let routePath = path.join(config.docPath, routeName);

    // Check for parent option
    if (options.parent) {
      routePath = path.join(config.docPath, options.parent, routeName);
    }

    const titleName = routeName.charAt(0).toUpperCase() + routeName.slice(1);

    // Check if ./docs exists, if not create it
    if (!fs.existsSync(config.docPath)) {
      fs.mkdirSync(config.docPath);
    }

    // Create the route folder
    if (!fs.existsSync(routePath)) {
      fs.mkdirSync(routePath, { recursive: true }); // Ensure parent directories are created
    }

    // Create the index.md file inside the route folder
    const indexPath = path.join(routePath, "index.md");
    const categoryPath = path.join(routePath, "_category.json");

    if (!fs.existsSync(categoryPath)) {
      fs.writeFileSync(
        categoryPath,
        JSON.stringify({
          title: titleName,
          disabled: false,
          index: "index.md",
        })
      );
    } else {
      console.log(`_category.json already exists in ${titleName}`);
    }

    if (!fs.existsSync(indexPath)) {
      fs.writeFileSync(indexPath, "# Welcome to " + titleName);
    } else {
      console.log(`index.md already exists in ${titleName}`);
    }
  });

program.parse(process.argv);
