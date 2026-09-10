// Findings: the closed catalog of ways an action can fail. An executor never
// fails with a sentence; it fails with a kind and its fields, and every reader
// renders the finding from one template. Fields are things the coding agent
// already has or the application produced: contract control names, action
// ids, actor labels, numbers, counts, HTTP statuses. Repair reports can include
// measured values and required results, but not scenario scripts or algorithms.
// A scenario's own probe text is never a field. `detail` is never rendered.

import { z } from 'zod';

type Immutable<T> = T extends object ? { readonly [K in keyof T]: Immutable<T[K]> } : T;
type SchemaFinding = z.infer<typeof findingSchema>;
export type NumberExpectation = Immutable<z.infer<typeof expectationSchema>>;
export type Operation = Immutable<z.infer<typeof operationSchema>>;
export type LifecycleTarget = z.infer<typeof targetSchema>;
export type FindingFields = {
  [F in SchemaFinding as F['kind']]: keyof F['fields'] extends never
    ? Record<string, never> : Immutable<F['fields']>;
};
export type FindingKind = keyof FindingFields;
// The harness could not measure. Never repair feedback.
export type InconclusiveFindingKind =
  | 'assertion-without-action' | 'unknown-action' | 'action-without-parameters'
  | 'no-session' | 'unresolved-action' | 'replay-unavailable'
  | 'forgery-unverifiable' | 'not-observed' | 'transport-incomplete' | 'nothing-contended'
  | 'no-backend-control' | 'control-refused' | 'database-write-failed'
  | 'stock-read-unavailable'
  | 'unsupported-backend' | 'app-directory-unknown' | 'invalid-input';
export type FailedFindingKind = Exclude<FindingKind, InconclusiveFindingKind>;
export type FailedFindingFields = Pick<FindingFields, FailedFindingKind>;
export type InconclusiveFindingFields = Pick<FindingFields, InconclusiveFindingKind>;
export type FindingStatus = 'failed' | 'inconclusive';
export type Finding = { [K in FindingKind]: { readonly kind: K; readonly fields: FindingFields[K] } }[FindingKind];

type Renderers<Fields> = { readonly [K in keyof Fields]: (fields: Fields[K]) => string };

const control = (name: string): string => `the ${name} control`;
const scopedControl = (f: { control?: string; scope?: string }): string =>
  `${f.control ? control(f.control) : 'a control'}${f.scope ? ` inside the ${f.scope} control` : ''}`;
const names = (values: readonly string[]): string => values.join(', ');
const http = (status: number | null): string => status ? `HTTP ${status}` : 'no server response';
const operation = (value: Operation | null): string => {
  if (!value) return '';
  const parts = [value.reducer ? `the ${value.reducer} reducer` : null,
    value.path ? `${value.method} ${value.path}` : null].filter(Boolean);
  return parts.length ? `; the application interface names ${parts.join(' or ')}` : '';
};
const expectation = (value: NumberExpectation): string => [
  value.equals === undefined ? null : `exactly ${value.equals}`,
  value.atLeast === undefined ? null : `at least ${value.atLeast}`,
  value.atMost === undefined ? null : `at most ${value.atMost}`,
].filter(Boolean).join(' and ') || 'a number';
const target = (value: LifecycleTarget): string =>
  value === 'app-server' ? 'the application server' : 'the database runtime';

export const FAILED_FINDINGS: Renderers<FailedFindingFields> = {
  'control-missing': f => `${scopedControl(f)}${f.filtered ? ' matching the requested entry' : ''} did not appear`,
  'control-present': f => `${control(f.control)} was shown when it must not be`,
  'control-available': f => `${control(f.control)} stayed available to ${f.actor}`,
  'control-not-ready': f => `${control(f.control)} never became usable for ${names(f.actors)}`,
  'control-blocked': f => `${scopedControl(f)} is covered by another element`,
  'control-empty': f => `${control(f.control)} is empty`,
  'control-unreadable': f => `${control(f.control)} is missing or unreadable for ${names(f.actors)}`,
  'value-mismatch': f => `${control(f.control)} does not show the required value`,
  'text-unexpected': f => `${control(f.control)} shows text that must not appear`,
  'value-unstable': f => `${control(f.control)} changed while nothing happened`,
  'clients-disagree': f => `${names(f.actors)} see different values in ${control(f.control)}`,
  'number-missing': f => `${control(f.control)} shows no number`,
  'number-mismatch': f => `${control(f.control)} reads ${f.observed ?? 'no number'}, `
    + `expected ${expectation(f.expected)}`,
  'count-mismatch': f => `${f.observed} ${f.control} entries shown, expected ${f.expected}`,
  'order-mismatch': f => f.actors?.length
    ? `${names(f.actors)} see ${control(f.control)} entries in different orders`
    : `${control(f.control)} entries are not in the required order`,
  'entries-missing': f => `of ${f.expected} entries, ${f.missing} missing and ${f.duplicated} duplicated`,
  'actors-with-control': f => `${f.observed} actor(s) hold ${control(f.control)}, expected ${f.expected}`,
  'too-many-per-actor': f => `an actor holds more than ${f.maxEach} of ${control(f.control)}`,
  'clicks-failed': f => `${f.failed} of ${f.total} simultaneous clicks on ${control(f.control)} did not go through`,
  'choice-missing': f => `${scopedControl(f)} did not offer the required choice`,
  'page-timeout': f => f.control
    ? `${scopedControl(f)} did not become available in time`
    : 'the page did not respond in time',
  'page-crashed': () => 'the page crashed',
  'page-error': f => `${f.control ? control(f.control) : 'the page'} did not behave as required`,
  'app-control-failed': f => `${target(f.target)} could not ${f.mode}`,
  'script-failed': f => `${f.script} failed`,
  'script-invalid': f => `${f.script} is not a script inside the application directory`,
  'action-failed': f => `the ${f.action} step did not complete`,
  'call-refused': f => `the ${f.action} action was refused for ${f.actor} (${http(f.status)})${operation(f.operation)}`,
  'call-accepted': f => f.required === 'validation-refused'
    ? `the ${f.action} action accepted invalid input from ${f.actor}`
    : `the ${f.action} action was accepted for ${f.actor}, who must be refused`,
  'call-error': f => `the ${f.action} action returned ${http(f.status)} for ${f.actor}; `
    + `this does not meet the ${f.required === 'validation-refused' ? 'input-error' : 'access-error'} status contract${operation(f.operation)}`,
  'concurrent-calls-mismatch': f => `${f.accepted} of ${f.fired} simultaneous ${f.action} calls were accepted, expected ${f.expected}`,
  'interface-missing': f => `${control(f.control)} exposes no ${f.attribute} for the ${f.action} action`,
  'interface-invalid': f => f.missing?.length
    ? `${f.attribute} for the ${f.action} action is missing ${names(f.missing)}`
    : f.unexpected?.length
      ? `${f.attribute} for the ${f.action} action contains unexpected ${names(f.unexpected)}`
      : `${f.attribute} for the ${f.action} action is not valid`,
  'replay-accepted': f => `a request replayed as ${f.actor}, who must be refused, was accepted (${http(f.status)})`,
  'replay-error': f => `the replayed ${f.action ? `${f.action} action` : 'request'} returned ${http(f.status)}; this does not meet the access-error status contract`,
  'forgery-accepted': f => `a request with a tampered ${f.field} was accepted (${http(f.status)})`,
  'forgery-error': f => `the tampered request returned ${http(f.status)}; this does not meet the access-error status contract`,
  'message-delivered': f => `private data was delivered to unauthorized actor ${f.actor}`,
  'stock-interface-missing': f => f.missingRow
    ? `the required ${f.missingRow} row was not found in the stock data interface; check the original starting data and the item/warehouse links`
    : 'the stock data interface (item, warehouse, stock) is not available in the database',
};

export const INCONCLUSIVE_FINDINGS: Renderers<InconclusiveFindingFields> = {
  'assertion-without-action': f => `no ${f.action} ran before this assertion`,
  'unknown-action': f => `the track names no ${f.action} action`,
  'action-without-parameters': f => `the ${f.action} action declares no named parameters`,
  'no-session': f => `no session found for ${f.actor}${f.action ? `, so the ${f.action} action could not be issued` : ''}`,
  'unresolved-action': f => `could not resolve where to send ${f.action ? `the ${f.action} action` : 'the action'} for this backend`,
  'replay-unavailable': f => `could not issue the replay as ${f.actor}`,
  'forgery-unverifiable': f => `could not verify the forgery refusal for ${f.actor}`,
  'not-observed': f => `the expected data could not be observed reaching ${f.actor}`,
  'transport-incomplete': () => 'transport evidence is incomplete; absence cannot be established',
  'nothing-contended': () => 'the requests never contended',
  'no-backend-control': f => `no control over ${target(f.target)} was supplied`,
  'control-refused': f => `control over ${target(f.target)} was refused on this host`,
  'database-write-failed': () => 'the direct database write did not complete',
  'stock-read-unavailable': () => 'stored stock could not be measured',
  'unsupported-backend': f => `${f.backend} does not support this step`,
  'app-directory-unknown': () => 'the application directory is unknown',
  'invalid-input': () => 'the step input is invalid',
};

export const FAILED_FINDING_KINDS = Object.freeze(Object.keys(FAILED_FINDINGS).sort()) as readonly FailedFindingKind[];
export const INCONCLUSIVE_FINDING_KINDS = Object.freeze(
  Object.keys(INCONCLUSIVE_FINDINGS).sort()) as readonly InconclusiveFindingKind[];
export const FINDING_KINDS = Object.freeze([...FAILED_FINDING_KINDS, ...INCONCLUSIVE_FINDING_KINDS].sort()) as readonly FindingKind[];

export function finding<K extends FindingKind>(kind: K, fields: FindingFields[K]): Finding {
  return { kind, fields } as Finding;
}

export function findingStatus(value: Finding): FindingStatus {
  return value.kind in FAILED_FINDINGS ? 'failed' : 'inconclusive';
}

// Full diagnostic sentence for private evidence readers.
export function renderFinding(value: Finding): string {
  const renderers = { ...FAILED_FINDINGS, ...INCONCLUSIVE_FINDINGS } as Renderers<FindingFields>;
  return (renderers[value.kind] as (fields: unknown) => string)(value.fields);
}

// A repair needs the failed observation, including exact amounts when measured.
// Keep implementation advice out even when private diagnostics contain it.
export function renderRepairFinding(value: Finding): string {
  if (value.kind === 'stock-interface-missing' && value.fields.missingRow) {
    const { missingRow, item, warehouse } = value.fields;
    const name = missingRow === 'item' ? item : missingRow === 'warehouse' ? warehouse : undefined;
    const context = missingRow === 'stock'
      ? [item ? `item ${JSON.stringify(item)}` : null, warehouse ? `warehouse ${JSON.stringify(warehouse)}` : null]
        .filter(Boolean).join(' and ') : '';
    return `the required ${missingRow}${name ? ` ${JSON.stringify(name)}` : ''} row was not found in the stock data interface${context ? ` for ${context}` : ''}`;
  }
  return renderFinding(value);
}

const controlSchema = z.strictObject({ control: z.string() });
const actorSchema = z.strictObject({ actor: z.string() });
const actionSchema = z.strictObject({ action: z.string() });
const statusSchema = z.strictObject({ status: z.number().nullable() });
const detailSchema = z.strictObject({ detail: z.string().optional() });
const operationSchema = z.strictObject({
  reducer: z.string().nullable(),
  path: z.string().nullable(),
  method: z.string(),
});
const expectationSchema = z.strictObject({
  equals: z.number().optional(),
  atLeast: z.number().optional(),
  atMost: z.number().optional(),
});
const targetSchema = z.enum(['app-server', 'backend-runtime']);

export const findingSchema = z.discriminatedUnion('kind', [
  z.strictObject({ kind: z.literal('control-missing'), fields: controlSchema.extend({ scope: z.string().optional(), filtered: z.boolean().optional() }) }),
  z.strictObject({ kind: z.literal('control-present'), fields: controlSchema }),
  z.strictObject({ kind: z.literal('control-available'), fields: z.strictObject({ control: z.string(), actor: z.string() }) }),
  z.strictObject({ kind: z.literal('control-not-ready'), fields: z.strictObject({ control: z.string(), actors: z.array(z.string()) }) }),
  z.strictObject({ kind: z.literal('control-blocked'), fields: z.strictObject({ control: z.string().optional(), scope: z.string().optional(), detail: z.string().optional() }) }),
  z.strictObject({ kind: z.literal('control-empty'), fields: controlSchema }),
  z.strictObject({ kind: z.literal('control-unreadable'), fields: z.strictObject({ control: z.string(), actors: z.array(z.string()) }) }),
  z.strictObject({ kind: z.literal('value-mismatch'), fields: controlSchema }),
  z.strictObject({ kind: z.literal('text-unexpected'), fields: controlSchema }),
  z.strictObject({ kind: z.literal('value-unstable'), fields: controlSchema }),
  z.strictObject({ kind: z.literal('clients-disagree'), fields: z.strictObject({ control: z.string(), actors: z.array(z.string()) }) }),
  z.strictObject({ kind: z.literal('number-missing'), fields: controlSchema }),
  z.strictObject({ kind: z.literal('number-mismatch'), fields: z.strictObject({ control: z.string(), observed: z.number().nullable(), expected: expectationSchema }) }),
  z.strictObject({ kind: z.literal('count-mismatch'), fields: z.strictObject({ control: z.string(), observed: z.number(), expected: z.number() }) }),
  z.strictObject({ kind: z.literal('order-mismatch'), fields: z.strictObject({ control: z.string(), actors: z.array(z.string()).optional() }) }),
  z.strictObject({ kind: z.literal('entries-missing'), fields: z.strictObject({ expected: z.number(), missing: z.number(), duplicated: z.number() }) }),
  z.strictObject({ kind: z.literal('actors-with-control'), fields: z.strictObject({ control: z.string(), observed: z.number(), expected: z.number() }) }),
  z.strictObject({ kind: z.literal('too-many-per-actor'), fields: z.strictObject({ control: z.string(), maxEach: z.number() }) }),
  z.strictObject({ kind: z.literal('clicks-failed'), fields: z.strictObject({ control: z.string(), failed: z.number(), total: z.number(), detail: z.string().optional() }) }),
  z.strictObject({ kind: z.literal('choice-missing'), fields: z.strictObject({ control: z.string().optional(), scope: z.string().optional(), detail: z.string().optional() }) }),
  z.strictObject({ kind: z.literal('page-timeout'), fields: z.strictObject({ control: z.string().optional(), scope: z.string().optional(), detail: z.string().optional() }) }),
  z.strictObject({ kind: z.literal('page-crashed'), fields: detailSchema }),
  z.strictObject({ kind: z.literal('page-error'), fields: z.strictObject({ control: z.string().optional(), scope: z.string().optional(), detail: z.string().optional() }) }),
  z.strictObject({ kind: z.literal('app-control-failed'), fields: z.strictObject({ mode: z.string(), target: targetSchema, detail: z.string().optional() }) }),
  z.strictObject({ kind: z.literal('script-failed'), fields: z.strictObject({ script: z.string(), detail: z.string().optional() }) }),
  z.strictObject({ kind: z.literal('script-invalid'), fields: z.strictObject({ script: z.string() }) }),
  z.strictObject({ kind: z.literal('action-failed'), fields: actionSchema }),
  z.strictObject({ kind: z.literal('call-refused'), fields: z.strictObject({ action: z.string(), actor: z.string(), status: z.number().nullable(), operation: operationSchema.nullable() }) }),
  z.strictObject({ kind: z.literal('call-accepted'), fields: z.strictObject({ action: z.string(), actor: z.string(), status: z.number().nullable(), required: z.enum(['refused', 'validation-refused']) }) }),
  z.strictObject({ kind: z.literal('call-error'), fields: z.strictObject({ action: z.string(), actor: z.string(), status: z.number().nullable(), required: z.enum(['refused', 'validation-refused']), operation: operationSchema.nullable() }) }),
  z.strictObject({ kind: z.literal('concurrent-calls-mismatch'), fields: z.strictObject({ action: z.string(), expected: z.number(), accepted: z.number(), fired: z.number(), detail: z.string().optional() }) }),
  z.strictObject({ kind: z.literal('interface-missing'), fields: z.strictObject({ control: z.string(), action: z.string(), attribute: z.string() }) }),
  z.strictObject({ kind: z.literal('interface-invalid'), fields: z.strictObject({ action: z.string(), attribute: z.string(), missing: z.array(z.string()).optional(), unexpected: z.array(z.string()).optional(), detail: z.string().optional() }) }),
  z.strictObject({ kind: z.literal('replay-accepted'), fields: z.strictObject({ actor: z.string(), status: z.number().nullable(), action: z.string().optional() }) }),
  z.strictObject({ kind: z.literal('replay-error'), fields: z.strictObject({ status: z.number().nullable(), action: z.string().optional() }) }),
  z.strictObject({ kind: z.literal('forgery-accepted'), fields: z.strictObject({ status: z.number().nullable(), field: z.string() }) }),
  z.strictObject({ kind: z.literal('forgery-error'), fields: statusSchema }),
  z.strictObject({ kind: z.literal('message-delivered'), fields: actorSchema }),
  z.strictObject({ kind: z.literal('stock-interface-missing'), fields: detailSchema.extend({
    missingRow: z.enum(['item', 'warehouse', 'stock']).optional(),
    item: z.string().optional(), warehouse: z.string().optional(),
  }) }),
  z.strictObject({ kind: z.literal('assertion-without-action'), fields: actionSchema }),
  z.strictObject({ kind: z.literal('unknown-action'), fields: actionSchema }),
  z.strictObject({ kind: z.literal('action-without-parameters'), fields: actionSchema }),
  z.strictObject({ kind: z.literal('no-session'), fields: z.strictObject({ actor: z.string(), action: z.string().optional() }) }),
  z.strictObject({ kind: z.literal('unresolved-action'), fields: z.strictObject({ action: z.string().optional() }) }),
  z.strictObject({ kind: z.literal('replay-unavailable'), fields: z.strictObject({ actor: z.string(), detail: z.string().optional() }) }),
  z.strictObject({ kind: z.literal('forgery-unverifiable'), fields: z.strictObject({ actor: z.string(), detail: z.string().optional() }) }),
  z.strictObject({ kind: z.literal('not-observed'), fields: actorSchema }),
  z.strictObject({ kind: z.literal('transport-incomplete'), fields: z.strictObject({}) }),
  z.strictObject({ kind: z.literal('nothing-contended'), fields: detailSchema }),
  z.strictObject({ kind: z.literal('no-backend-control'), fields: z.strictObject({ target: targetSchema }) }),
  z.strictObject({ kind: z.literal('control-refused'), fields: z.strictObject({ target: targetSchema }) }),
  z.strictObject({ kind: z.literal('database-write-failed'), fields: detailSchema }),
  z.strictObject({ kind: z.literal('stock-read-unavailable'), fields: detailSchema }),
  z.strictObject({ kind: z.literal('unsupported-backend'), fields: z.strictObject({ backend: z.string() }) }),
  z.strictObject({ kind: z.literal('app-directory-unknown'), fields: z.strictObject({}) }),
  z.strictObject({ kind: z.literal('invalid-input'), fields: detailSchema }),
]);

export function isFinding(value: unknown): value is Finding {
  return findingSchema.safeParse(value).success;
}
