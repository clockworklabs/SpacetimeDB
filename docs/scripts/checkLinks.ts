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

// Function to resolve relative links using slugs
function resolveLink(link: string, currentSlug: string): string {
  if (link.startsWith('#')) {
    // If the link is a fragment, resolve it to the current slug
    return `${currentSlug}${link}`;
  }

  if (link.startsWith('/')) {
    // Absolute links are returned as-is
    return link;
  }

  // Resolve relative links based on slug
  const currentSlugDir = path.dirname(currentSlug);
  const resolvedSlug = path.normalize(path.join(currentSlugDir, link)).replace(/\\/g, '/');
  return resolvedSlug.startsWith('/docs') ? resolvedSlug : `/docs${resolvedSlug}`;
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
  let totalFiles = 0;
  let totalLinks = 0;
  let validLinks = 0;
  let invalidLinks = 0;
  let totalFragments = 0;
  let validFragments = 0;
  let invalidFragments = 0;
  let currentFileFragments = 0;

  // Extract the slug-to-path mapping from nav.ts
  const slugToPath = extractSlugToPathMap(nav);

  // Validate that all paths in slugToPath exist
  validatePathsExist(slugToPath);

  console.log(`Validated ${slugToPath.size} paths from nav.ts`);

  // Extract valid slugs
  const validSlugs = Array.from(slugToPath.keys());

  // Reverse map from file path to slug for current file resolution
  const pathToSlug = new Map<string, string>();
  slugToPath.forEach((filePath, slug) => {
    pathToSlug.set(filePath, slug);
  });

  // Get all .md files to check
  const mdFiles = getMarkdownFiles(path.resolve(__dirname, '../docs'));

  totalFiles = mdFiles.length;

  mdFiles.forEach((file) => {
    const links = extractLinksFromMarkdown(file);
    totalLinks += links.length;

    const currentSlug = pathToSlug.get(file) || '';

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
      const resolvedLink = resolveLink(link, currentSlug);
      
      // Split the resolved link into base and fragment
      const [baseLink, fragmentRaw] = resolvedLink.split('#');
      const fragment: string | null = fragmentRaw || null;

      if (fragment) {
        totalFragments += 1;
      }

      // Check if the base link matches a valid slug
      if (!validSlugs.includes(baseLink)) {
        brokenLinks.push({ file, link: resolvedLink, line });
        invalidLinks += 1;
        return;
      } else {
        validLinks += 1;
      }

      // Validate the fragment, if present
      if (fragment) {
        const targetFile = slugToPath.get(baseLink);
        if (targetFile) {
          const targetHeadings = extractHeadingsFromMarkdown(targetFile);

          if (!targetHeadings.includes(fragment)) {
            brokenLinks.push({ file, link: resolvedLink, line });
            invalidFragments += 1;
            invalidLinks += 1;
          } else {
            validFragments += 1;
            if (baseLink === currentSlug) {
              currentFileFragments += 1;
            }
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
  } else {
    console.log('All links are valid!');
  }

  // Print statistics
  console.log('\n=== Link Validation Statistics ===');
  console.log(`Total markdown files processed: ${totalFiles}`);
  console.log(`Total links processed: ${totalLinks}`);
  console.log(`  Valid links: ${validLinks}`);
  console.log(`  Invalid links: ${invalidLinks}`);
  console.log(`Total links with fragments processed: ${totalFragments}`);
  console.log(`  Valid links with fragments: ${validFragments}`);
  console.log(`  Invalid links with fragments: ${invalidFragments}`);
  console.log(`Fragments referring to the current file: ${currentFileFragments}`);
  console.log('=================================');
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
