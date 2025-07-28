import fs from 'fs';
import path from 'path';
import nav from '../nav'; // Import the nav object directly
import GitHubSlugger from 'github-slugger';
import { unified } from 'unified';
import remarkParse from 'remark-parse';
import { visit } from 'unist-util-visit';

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

// Function to extract links and images from markdown files with line numbers
function extractLinksAndImagesFromMarkdown(filePath: string): { link: string; type: 'image' | 'link'; line: number }[] {
  const content = fs.readFileSync(filePath, 'utf-8');
  const tree = unified().use(remarkParse).parse(content);

  const results: { link: string; type: 'image' | 'link'; line: number }[] = [];

  visit(tree, ['link', 'image', 'definition'], (node: any) => {
    const link = node.url;
    const line = node.position?.start?.line ?? 0;
    if (link) {
      results.push({ link, type: node.type === 'image' ? 'image' : 'link', line });
    }
  });

  return results;
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

// Function to check if the links in .md files match the slugs in nav.ts and validate fragments/images
function checkLinks(): void {
  const brokenLinks: { file: string; link: string; type: 'image' | 'link'; line: number }[] = [];
  let totalFiles = 0;
  let totalLinks = 0;
  let validLinks = 0;
  let invalidLinks = 0;
  let totalFragments = 0;
  let validFragments = 0;
  let invalidFragments = 0;

  // Extract the slug-to-path mapping from nav.ts
  const slugToPath = extractSlugToPathMap(nav);

  // Validate that all paths in slugToPath exist
  validatePathsExist(slugToPath);

  console.log(`Validated ${slugToPath.size} paths from nav.ts`);

  // Extract valid slugs
  const validSlugs = Array.from(slugToPath.keys());

  // Hacky workaround because the slug for the root is /docs/index. No other slugs have a /index at the end.
  validSlugs.push('/docs');

  // Reverse map from file path to slug for current file resolution
  const pathToSlug = new Map<string, string>();
  slugToPath.forEach((filePath, slug) => {
    pathToSlug.set(filePath, slug);
  });

  // Get all .md files to check
  const mdFiles = getMarkdownFiles(path.resolve(__dirname, '../docs'));

  totalFiles = mdFiles.length;

  mdFiles.forEach((file) => {
    const linksAndImages = extractLinksAndImagesFromMarkdown(file);
    totalLinks += linksAndImages.length;

    const currentSlug = pathToSlug.get(file) || '';

    linksAndImages.forEach(({ link, type, line }) => {
      // Exclude external links (starting with http://, https://, mailto:, etc.)
      if (/^([a-z][a-z0-9+.-]*):/.test(link)) {
        return; // Skip external links
      }

      if (!link.startsWith('/docs')) {
        return; // Skip site links
      }

      // Resolve the link
      const resolvedLink = resolveLink(link, currentSlug);

      if (type === 'image') {
        // Validate image paths
        const normalizedLink = resolvedLink.startsWith('/') ? resolvedLink.slice(1) : resolvedLink;
        const imagePath = path.resolve(__dirname, '../', normalizedLink);

        if (!fs.existsSync(imagePath)) {
          brokenLinks.push({ file, link: resolvedLink, type: 'image', line });
          invalidLinks += 1;
        } else {
          validLinks += 1;
        }
        return;
      }

      // Split the resolved link into base and fragment
      let [baseLink, fragmentRaw] = resolvedLink.split('#');
      if (baseLink.endsWith('/')) {
        baseLink = baseLink.slice(0, -1);
      }
      const fragment: string | null = fragmentRaw || null;

      if (fragment) {
        totalFragments += 1;
      }

      // Check if the base link matches a valid slug
      if (!validSlugs.includes(baseLink)) {
        brokenLinks.push({ file, link: resolvedLink, type: 'link', line });
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
            brokenLinks.push({ file, link: resolvedLink, type: 'link', line });
            invalidFragments += 1;
            invalidLinks += 1;
          } else {
            validFragments += 1;
          }
        }
      }
    });
  });

  if (brokenLinks.length > 0) {
    console.error(`\nFound ${brokenLinks.length} broken links/images:`);
    brokenLinks.forEach(({ file, link, type, line }) => {
      const typeLabel = type === 'image' ? 'Image' : 'Link';
      console.error(`${typeLabel}: ${file}:${line}, Path: ${link}`);
    });
  } else {
    console.log('All links and images are valid!');
  }

  // Print statistics
  console.log('\n=== Validation Statistics ===');
  console.log(`Total markdown files processed: ${totalFiles}`);
  console.log(`Total links/images processed: ${totalLinks}`);
  console.log(`  Valid: ${validLinks}`);
  console.log(`  Invalid: ${invalidLinks}`);
  console.log(`Total links with fragments processed: ${totalFragments}`);
  console.log(`  Valid links with fragments: ${validFragments}`);
  console.log(`  Invalid links with fragments: ${invalidFragments}`);
  console.log('===============================');

  if (brokenLinks.length > 0) {
    process.exit(1); // Exit with an error code if there are broken links
  }
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

  const slugger = new GitHubSlugger();
  while ((match = headingRegex.exec(fileContent)) !== null) {
    const heading = match[2].trim(); // Extract the heading text
    const slug = slugger.slug(heading); // Slugify the heading text
    headings.push(slug);
  }

  return headings;
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
