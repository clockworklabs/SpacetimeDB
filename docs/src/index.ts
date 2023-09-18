#! /usr/bin/env node

import { DocConfig, DocSectionConfig, JumpLink } from "./types";

const { Command } = require("commander");
const clear = require("clear");
const figlet = require("figlet");
const path = require("path");
const fs = require("fs");
const fsExtra = require("fs-extra");

const cwd = process.cwd();
const DOCS_PATH = path.join(__dirname, "docs");
const CONFIG_PATH = path.join(cwd, "spacetime-docs.json");

function extractHeadersFromMarkdown(filePath) {
  const content = fs.readFileSync(filePath, "utf-8");
  const headers: JumpLink[] = [];
  const titleRegex = /^#\s+(.+)$/m;
  const headerMatch = content.match(titleRegex);
  const title = headerMatch ? headerMatch[1] : null;

  const headerRegex = /^(#+)\s+(.+)$/gm; // This captures the hashes and the header text
  let match;
  while ((match = headerRegex.exec(content))) {
    const depth = match[1].length; // Count of #'s indicate depth
    headers.push({
      title: match[2],
      route: match[2].toLowerCase().replace(/[^\w]+/g, "-"),
      depth: depth,
    });
  }

  return { title, jumpLinks: headers };
}
let config = {
  docPath: "",
  order: [] as any[],
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
  const rootDir = config.docPath;

  function processDirectory(dir) {
    const categoryFile = path.join(dir, "_category.json");
    if (!fs.existsSync(categoryFile)) return null;

    const category = fsExtra.readJSONSync(categoryFile);
    const docSectionConfig = {
      title: category.title,
      identifier: path.basename(dir),
      indexIdentifier: category.index.replace(".md", ""),
      comingSoon: category.disabled || false,
      tag: category.tag || undefined,
      hasPages: false,
      editUrl: encodeURIComponent(category.title) + "/" + category.index,
      jumpLinks: [],
      pages: [] as any[],
    };

    const items = fs.readdirSync(dir);
    const subSections: any[] = [];

    items.forEach((item) => {
      const itemPath = path.join(dir, item);
      const isDirectory = fs.statSync(itemPath).isDirectory();
      const isMarkdownFile = path.extname(item) === ".md";

      if (isDirectory) {
        const subSection = processDirectory(itemPath);
        if (subSection) {
          subSections.push(subSection);
        }
      } else if (isMarkdownFile && item !== "_category.json") {
        const { title, jumpLinks } = extractHeadersFromMarkdown(itemPath);
        const pageIdentifier = item.replace(".md", "");

        subSections.push({
          title: title || pageIdentifier, // Use the extracted title if available, otherwise fallback to the pageIdentifier
          identifier: pageIdentifier,
          indexIdentifier: pageIdentifier,
          hasPages: false,
          editUrl: encodeURIComponent(pageIdentifier) + ".md",
          jumpLinks: jumpLinks,
          pages: [],
        });
      }
    });

    if (subSections.length > 0) {
      docSectionConfig.hasPages = true;
      docSectionConfig.pages = subSections;
    }

    return docSectionConfig;
  }

  const docConfig = {
    sections: [] as any[],
    rootEditURL: config.editURLRoot,
  };

  const folders = fs.readdirSync(rootDir);
  folders.forEach((folder) => {
    const folderPath = path.join(rootDir, folder);
    if (fs.statSync(folderPath).isDirectory()) {
      const section = processDirectory(folderPath);
      if (section) {
        docConfig.sections.push(section);
      }
    }
  });

  docConfig.sections = docConfig.sections.sort((a: any, b: any) => {
    const orderA = config.order.indexOf(a.title);
    const orderB = config.order.indexOf(b.title);

    if (orderA === -1 && orderB === -1) return 0; // If both items are not in the order list, they remain in their current order.
    if (orderA === -1) return 1; // If only 'a' is not in the order list, 'b' comes first.
    if (orderB === -1) return -1; // If only 'b' is not in the order list, 'a' comes first.

    return orderA - orderB; // Otherwise, sort according to the order in the order list.
  });

  fs.writeFileSync(
    path.join(rootDir, "docs-config.ts"),
    `export const docsConfig = ${JSON.stringify(docConfig, null, 2)};`
  );
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
