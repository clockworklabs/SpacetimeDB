import { applyCredentialAliases } from './credential-aliases.js';
type StackApplicationInterface = 'http' | 'reducer';
const INTERFACE_SECTION = /<!-- interface:(http|reducer) -->([\s\S]*?)<!-- \/interface -->/g;
const INTERNAL_LANGUAGE = /\b(?:Stack Bench|benchmark|grader|grading|harness|test runner|test fixture|testing (?:hooks|interface)|test action|test IDs?)\b|data-testid/i;

function selectedInterfaceText(value: unknown, selected: StackApplicationInterface | null): string {
  const source = String(value ?? '');
  let sections = 0;
  const output = source.replace(INTERFACE_SECTION, (_section, kind, content: string) => {
    sections += 1;
    return kind === selected ? content : '';
  });
  if (/<!--\s*(?:interface:|\/interface)/.test(output)) {
    throw new Error('interface-scoped contract has invalid markers');
  }
  if (sections && !selected) {
    throw new Error('interface-scoped contract requires a selected application interface');
  }
  return output.replace(/\n{3,}/g, '\n\n');
}

// Agent-facing documents are authored as agent-facing text. Reject accidental
// benchmark disclosure instead of trying to rewrite prose at runtime.
export function agentVisibleContractText(
  value: unknown,
  credentialAliases: Readonly<Record<string, string>> = {},
  applicationInterface: StackApplicationInterface | null = null,
): string {
  const text = applyCredentialAliases(
    selectedInterfaceText(value, applicationInterface), credentialAliases);
  const disclosure = text.match(INTERNAL_LANGUAGE)?.[0];
  if (disclosure) {
    throw new Error(`agent-facing text contains internal language ${JSON.stringify(disclosure)}`);
  }
  return text;
}

// Contract fragments declare stable element IDs in the first column of a table.
export function contractControlIds(contractText: unknown): string[] {
  const matches = String(contractText ?? '').matchAll(
    /^\s*\|\s*`([a-z][a-z0-9]*(?:-[a-z0-9]+)+)`\s*\|/gm,
  );
  const ids = [...matches]
    .map((match) => match[1])
    .filter((id): id is string => id !== undefined);
  return [...new Set(ids)].sort();
}
