import { createHash } from 'node:crypto';
import { mkdir, readdir, readFile, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, '../..');
const sourceDir = path.join(repoRoot, 'skills');
const outputDir = path.join(
  repoRoot,
  'docs/static/.well-known/agent-skills'
);

function readFrontmatter(markdown, sourcePath) {
  const match = markdown.match(/^---\n([\s\S]*?)\n---\n/);
  if (!match) {
    throw new Error(`${sourcePath} is missing YAML frontmatter`);
  }

  const frontmatter = {};
  for (const line of match[1].split('\n')) {
    const field = line.match(/^([a-zA-Z0-9_-]+):\s*(.*)$/);
    if (field) {
      frontmatter[field[1]] = field[2].replace(/^"(.*)"$/, '$1');
    }
  }

  if (!frontmatter.name || !frontmatter.description) {
    throw new Error(`${sourcePath} must declare name and description`);
  }

  return frontmatter;
}

await rm(outputDir, { recursive: true, force: true });
await mkdir(outputDir, { recursive: true });

const skills = [];

for (const entry of (await readdir(sourceDir, { withFileTypes: true })).sort(
  (a, b) => a.name.localeCompare(b.name)
)) {
  if (!entry.isDirectory()) {
    continue;
  }

  const skillPath = path.join(sourceDir, entry.name, 'SKILL.md');
  const markdown = await readFile(skillPath, 'utf8');
  const metadata = readFrontmatter(markdown, skillPath);

  const skillOutputDir = path.join(outputDir, metadata.name);
  await mkdir(skillOutputDir, { recursive: true });
  await writeFile(path.join(skillOutputDir, 'SKILL.md'), markdown);

  skills.push({
    name: metadata.name,
    type: 'skill-md',
    description: metadata.description,
    url: `/docs/.well-known/agent-skills/${metadata.name}/SKILL.md`,
    digest: `sha256:${createHash('sha256').update(markdown).digest('hex')}`,
  });
}

await writeFile(
  path.join(outputDir, 'index.json'),
  `${JSON.stringify(
    {
      $schema: 'https://schemas.agentskills.io/discovery/0.2.0/schema.json',
      skills,
    },
    null,
    2
  )}\n`
);

