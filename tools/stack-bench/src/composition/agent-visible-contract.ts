import { applyCredentialAliases } from './credential-aliases.js';
type StackApplicationInterface = 'http' | 'reducer';
const INTERFACE_SECTION = /<!-- interface:(http|reducer) -->([\s\S]*?)<!-- \/interface -->/g;

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

// Contract sources use internal grading terms. Coding agents receive only the
// public application requirements and stable interface names.
export function agentVisibleContractText(
  value: unknown,
  credentialAliases: Readonly<Record<string, string>> = {},
  applicationInterface: StackApplicationInterface | null = null,
): string {
  return applyCredentialAliases(selectedInterfaceText(value, applicationInterface), credentialAliases)
    .replace(/\bso (?:Stack Bench|the grader) can verify that\b/gi, 'so that')
    .replace(/\bso Stack Bench can verify\b/gi, 'to support')
    .replace(/The app is graded by an automated harness that locates elements \*\*only\*\* via\s+/gi,
      'Expose ')
    .replace(/What the harness needs/gi, 'Application interface')
    .replace(/public testing interface/gi, 'application interface')
    .replace(/testing interface/gi, 'application interface')
    .replace(/test interface/gi, 'application interface')
    .replace(/the test runner locates visible controls through exact `data-testid` attributes\./gi,
      'Use the following exact `data-testid` attributes on the corresponding visible controls.')
    .replace(/the runner locates visible controls through exact `data-testid` attributes\./gi,
      'Use the following exact `data-testid` attributes on the corresponding visible controls.')
    .replace(/the test runner also issues the same account writes as the UI\./gi,
      'Expose the same account writes used by the UI.')
    .replace(/the runner also issues the same account writes as the UI\./gi,
      'Expose the same account writes used by the UI.')
    .replace(/the test runner locates/gi, 'The application exposes')
    .replace(/the runner locates/gi, 'The application exposes')
    .replace(/the test runner also issues/gi, 'The application also exposes')
    .replace(/the runner also issues/gi, 'The application also exposes')
    .replace(/test runner/gi, 'application')
    .replace(/the runner/gi, 'the application')
    .replace(/test fixture/gi, 'provided data')
    .replace(/test handle/gi, 'interface value')
    .replace(/direct authorization tests/gi, 'direct authorization actions')
    .replace(/testing call/gi, 'application action')
    .replace(/\btest action\b/gi, 'application action')
    .replace(/Do not add another transport\s+only for Stack Bench\./gi,
      'Use the same transport as the visible application.')
    .replace(/Test ID/gi, 'Element ID')
    .replace(/Test id/gi, 'Element ID')
    .replace(/\bhooks\b/gi, 'controls')
    .replace(/\bhook\b/gi, 'control')
    .replace(/`data-testid` attributes/gi, '`id` attributes')
    .replace(/data-testid/gi, 'id')
    .replace(/harness/gi, 'runtime')
    .replace(/benchmark/gi, 'runtime')
    .replace(/\bStack Bench\b/gi, 'the application runtime')
    .replace(/\bgrader|grading\b/gi, 'runtime');
}

// Contract fragments use backticks for stable element IDs. Passwords and
// custom data attributes share the notation but are not element IDs.
export function contractControlIds(contractText: unknown): string[] {
  const matches = String(contractText ?? '').matchAll(
    /`([a-z][a-z0-9]*(?:-[a-z0-9]+)+)`/g,
  );
  const ids = [...matches]
    .map((match) => match[1])
    .filter((id): id is string => id !== undefined)
    .filter((id) => !id.startsWith('stackbench-') && !id.startsWith('data-'));
  return [...new Set(ids)].sort();
}
