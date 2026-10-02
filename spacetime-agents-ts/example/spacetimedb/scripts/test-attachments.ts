import {
  ATTACHMENT_COUNT_MAX,
  ATTACHMENT_TOTAL_BYTES_MAX,
  attachmentValidationError,
} from '../src/attachments';

let failures = 0;
function assert(cond: boolean, msg: string): void {
  if (!cond) {
    process.stderr.write(`  FAIL: ${msg}\n`);
    failures++;
  } else {
    process.stdout.write(`  ${msg}  OK\n`);
  }
}

const png = (length: number) => ({ mimeType: 'image/png', bytes: { length } });
assert(
  attachmentValidationError([png(10)]) === undefined,
  'accepts a supported attachment'
);
assert(
  attachmentValidationError([
    { mimeType: 'text/plain', bytes: { length: 10 } },
  ]) === 'agent.unsupported_attachment_mime:text/plain',
  'rejects an unsupported attachment type'
);
assert(
  attachmentValidationError(
    Array.from({ length: ATTACHMENT_COUNT_MAX + 1 }, () => png(1))
  ) ===
    `agent.too_many_attachments:${ATTACHMENT_COUNT_MAX + 1}/${ATTACHMENT_COUNT_MAX}`,
  'rejects too many attachments'
);
assert(
  attachmentValidationError([
    png(3_000_000),
    png(3_000_000),
    png(3_000_000),
    png(ATTACHMENT_TOTAL_BYTES_MAX - 9_000_000 + 1),
  ]) ===
    `agent.attachments_too_large:${ATTACHMENT_TOTAL_BYTES_MAX + 1}/${ATTACHMENT_TOTAL_BYTES_MAX}`,
  'rejects excessive aggregate attachment bytes'
);

if (failures > 0) {
  process.stderr.write(`\n${failures} test(s) failed.\n`);
  process.exit(1);
}
process.stdout.write('\nall attachment tests passed.\n');
