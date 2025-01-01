import fs from 'fs';
import path from 'path';
import nav from '../nav'; // Import the nav object directly

// Function to extract slugs from the nav object and prefix them with /docs
function extractSlugsFromNav(nav: { items: any[] }): string[] {
  const slugs: string[] = [];

  function traverseNav(items: any[]): void {
    items.forEach((item) => {
      if (item.type === 'page' && item.slug) {
        slugs.push(`/docs/${item.slug}`); // Prefix slugs with /docs
      } else if (item.type === 'section' && item.items) {
        traverseNav(item.items); // Recursively traverse sections
      }
    });
  }

  traverseNav(nav.items);
  return slugs;
}

// Function to extract links from markdown files with line numbers
function extractLinksFromMarkdown(filePath: string): { link: string; line: number }[] {
  const fileContent = fs.readFileSync(filePath, 'utf-8');
  const lines = fileContent.split('\n');
  const linkRegex = /\[([^\]]+)\]\(([^)]+)\)/g;

  const links: { link: string; line: number }[] = [];
  lines.forEach((lineContent, index) => {
    let match: RegExpExecArray | null;
    while ((match = linkRegex.exec(lineContent)) !== null) {
      links.push({ link: match[2], line: index + 1 }); // Add 1 to make line numbers 1-based
    }
  });

  return links;
}

// Resolve relative links based on the current file's location
function resolveLink(link: string, filePath: string): string {
  // If the link is absolute (starts with `/`), return it as is
  if (link.startsWith('/')) {
    return link;
  }
  // Resolve the relative link to an absolute path
  const fileDir = path.dirname(filePath);
  const resolvedPath = path.join(fileDir, link);
  // Convert to a normalized format (e.g., `/docs/...`)
  const relativePath = path.relative(path.resolve(__dirname, '../docs'), resolvedPath);
  return `/docs/${relativePath}`;
}

// Function to check if the links in .md files match the slugs in nav.ts
function checkLinks(): void {
  const brokenLinks: { file: string; link: string; line: number }[] = [];

  // Extract slugs from the nav object
  const validSlugs = extractSlugsFromNav(nav);

  console.log(`Extracted ${validSlugs.length} slugs from nav.ts`);

  // Get all .md files to check
  const mdFiles = getMarkdownFiles(path.resolve(__dirname, '../docs'));

  mdFiles.forEach((file) => {
    const links = extractLinksFromMarkdown(file);

    links.forEach(({ link, line }) => {
      // Exclude external links (starting with http://, https://, mailto:, etc.)
      if (/^([a-z][a-z0-9+.-]*):/.test(link)) {
        // Skip the external links
        return;
      }

      const siteLinks = ['/install', '/images'];
      for (const siteLink of siteLinks) {
        if (link.startsWith(siteLink)) {
          // Skip the site links
          return;
        }
      }

      // For now remove the fragment part of the link and check if it is a valid slug
      const fragmentIndex = link.indexOf('#');
      if (fragmentIndex !== -1) {
        link = link.substring(0, fragmentIndex);
        if (link === '') {
          // Skip references to the current file
          return;
        }
      }

      // Resolve the link to its absolute counterpart
      const resolvedLink = resolveLink(link, file);

      if (!validSlugs.includes(resolvedLink)) {
        brokenLinks.push({ file, link: resolvedLink, line });
      }
    });
  });

  if (brokenLinks.length > 0) {
    console.error(`\nFound ${brokenLinks.length} broken links:`);
    brokenLinks.forEach(({ file, link, line }) => {
      console.error(`File: ${file}:${line}, Link: ${link}`);
    });
    process.exit(1); // Exit with error if any invalid links are found
  } else {
    console.log('All links are valid!');
  }
}

// Function to get all markdown files recursively
function getMarkdownFiles(dir: string): string[] {
  let files: string[] = [];
  const items = fs.readdirSync(dir);

  items.forEach((item) => {
    const fullPath = path.join(dir, item);
    const stat = fs.lstatSync(fullPath);

    if (stat.isDirectory()) {
      files = files.concat(getMarkdownFiles(fullPath)); // Recurse into directories
    } else if (fullPath.endsWith('.md')) {
      files.push(fullPath);
    }
  });

  return files;
}

checkLinks();
