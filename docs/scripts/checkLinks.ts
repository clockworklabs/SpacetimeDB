import fs from 'fs';
import path from 'path';
import nav from '../nav'; // Import the nav object directly

// Function to map slugs to file paths from nav.ts
function extractSlugToPathMap(nav: { items: any[] }): Map<string, string> {
  const slugToPath = new Map<string, string>();

  function traverseNav(items: any[]): void {
    items.forEach((item) => {
      if (item.type === 'page' && item.slug && item.path) {
        const resolvedPath = path.resolve(__dirname, '../docs', item.path);
        slugToPath.set(`/docs/${item.slug}`, resolvedPath);
      } else if (item.type === 'section' && item.items) {
        traverseNav(item.items); // Recursively traverse sections
      }
    });
  }

  traverseNav(nav.items);
  return slugToPath;
}

// Function to assert that all files in slugToPath exist
function validatePathsExist(slugToPath: Map<string, string>): void {
  slugToPath.forEach((filePath, slug) => {
    if (!fs.existsSync(filePath)) {
      throw new Error(`File not found: ${filePath} (Referenced by slug: ${slug})`);
    }
  });
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

// Function to resolve relative links based on the current file's location
function resolveLink(link: string, filePath: string): string {
  if (link.startsWith('#')) {
    // If the link is a fragment, resolve it to the current file
    const currentSlug = `/docs/${path.relative(
      path.resolve(__dirname, '../docs'),
      filePath
    ).replace(/\\/g, '/')}`.replace(/\.md$/, ''); // Normalize to slug format
    return `${currentSlug}${link}`;
  }

  if (link.startsWith('/')) {
    return link; // Absolute links are already resolved
  }

  const fileDir = path.dirname(filePath);
  const resolvedPath = path.join(fileDir, link);
  const relativePath = path.relative(path.resolve(__dirname, '../docs'), resolvedPath);
  return `/docs/${relativePath}`; // Ensure resolved links are prefixed with /docs
}

// Function to extract headings from a markdown file
function extractHeadingsFromMarkdown(filePath: string): string[] {
  if (!fs.existsSync(filePath) || !fs.lstatSync(filePath).isFile()) {
    return []; // Return an empty list if the file does not exist or is not a file
  }

  const fileContent = fs.readFileSync(filePath, 'utf-8');
  const headingRegex = /^(#{1,6})\s+(.*)$/gm; // Match markdown headings like # Heading
  const headings: string[] = [];
  let match: RegExpExecArray | null;

  while ((match = headingRegex.exec(fileContent)) !== null) {
    const heading = match[2].trim(); // Extract the heading text
    const slug = heading
      .toLowerCase()
      .replace(/[^\w\- ]+/g, '') // Remove special characters
      .replace(/\s+/g, '-'); // Replace spaces with hyphens
    headings.push(slug);
  }

  return headings;
}

// Function to check if the links in .md files match the slugs in nav.ts and validate fragments
function checkLinks(): void {
  const brokenLinks: { file: string; link: string; line: number }[] = [];

  // Extract the slug-to-path mapping from nav.ts
  const slugToPath = extractSlugToPathMap(nav);

  // Validate that all paths in slugToPath exist
  validatePathsExist(slugToPath);

  console.log(`Validated ${slugToPath.size} paths from nav.ts`);

  // Extract valid slugs
  const validSlugs = Array.from(slugToPath.keys());

  // Get all .md files to check
  const mdFiles = getMarkdownFiles(path.resolve(__dirname, '../docs'));

  mdFiles.forEach((file) => {
    const links = extractLinksFromMarkdown(file);

    links.forEach(({ link, line }) => {
      // Exclude external links (starting with http://, https://, mailto:, etc.)
      if (/^([a-z][a-z0-9+.-]*):/.test(link)) {
        return; // Skip external links
      }

      const siteLinks = ['/install', '/images'];
      for (const siteLink of siteLinks) {
        if (link.startsWith(siteLink)) {
          return; // Skip site links
        }
      }

      // Resolve the link
      const resolvedLink = resolveLink(link, file);

      // Split the resolved link into base and fragment
      const [baseLink, fragmentRaw] = resolvedLink.split('#');
      const fragment: string | null = fragmentRaw || null;

      // Check if the base link matches a valid slug
      if (!validSlugs.includes(baseLink)) {
        brokenLinks.push({ file, link: resolvedLink, line });
        return;
      }

      // Validate the fragment, if present
      if (fragment) {
        const targetFile = slugToPath.get(baseLink);
        if (targetFile) {
          const targetHeadings = extractHeadingsFromMarkdown(targetFile);

          if (!targetHeadings.includes(fragment)) {
            brokenLinks.push({ file, link: resolvedLink, line });
          } else {
            console.log(`Found valid link: ${file}:${line} ${resolvedLink}`);
          }
        }
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
