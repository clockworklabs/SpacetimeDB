import * as assert from 'node:assert/strict';
import type { FileSummary } from '../src/codegen/app/types';
import {
  ARCHIVE_ENTRY_COUNT_MAX,
  ARCHIVE_FILE_COUNT_MAX,
  ARCHIVE_TOTAL_BYTES_MAX,
  archiveSelectionError,
} from '../src/downloads';
import { FileViewer } from '../src/viewer';

const fileWithSize = (size: bigint): FileSummary => ({ size }) as FileSummary;

assert.equal(archiveSelectionError([fileWithSize(1024n)]), undefined);
assert.match(
  archiveSelectionError(
    Array.from({ length: ARCHIVE_FILE_COUNT_MAX + 1 }, () => fileWithSize(0n))
  ) ?? '',
  /at most 250 files/
);
assert.match(
  archiveSelectionError([], ARCHIVE_ENTRY_COUNT_MAX + 1) ?? '',
  /1000 entry limit/
);
assert.match(
  archiveSelectionError([fileWithSize(BigInt(ARCHIVE_TOTAL_BYTES_MAX) + 1n)]) ??
    '',
  /64 MiB limit/
);

console.log('files download tests passed');

type TestElement = {
  classList: {
    add(name: string): void;
    remove(name: string): void;
    contains(name: string): boolean;
  };
  innerHTML: string;
  style: { display: string };
  textContent: string | null;
  title: string;
  disabled: boolean;
  querySelector(): null;
};

function testElement(): TestElement {
  const classes = new Set<string>();
  return {
    classList: {
      add: name => classes.add(name),
      remove: name => classes.delete(name),
      contains: name => classes.has(name),
    },
    innerHTML: '',
    style: { display: '' },
    textContent: null,
    title: '',
    disabled: false,
    querySelector: () => null,
  };
}

async function testClosingViewerCancelsPendingLoad(): Promise<void> {
  const ids = [
    'lightbox',
    'lb-stage',
    'lb-title',
    'lb-meta',
    'lb-prev',
    'lb-next',
    'lb-out',
    'lb-in',
    'lb-fit',
    'lb-full',
  ];
  const elements = new Map(ids.map(id => [id, testElement()]));
  const previousDocument = globalThis.document;
  Object.defineProperty(globalThis, 'document', {
    configurable: true,
    value: {
      getElementById(id: string) {
        return elements.get(id) ?? null;
      },
    },
  });

  let resolveBlob: ((blob: Blob) => void) | undefined;
  const viewer = new FileViewer({
    loadBlob: () =>
      new Promise(resolve => {
        resolveBlob = resolve;
      }),
    download: async () => undefined,
    iconHtml: () => '',
  });
  const row = {
    path: '/notes.txt',
    mimeType: 'text/plain',
    size: 5n,
    visibility: 'owner',
    updatedAt: { microsSinceUnixEpoch: 1_000n },
  } as FileSummary;

  try {
    const opening = viewer.open(row.path, [row]);
    viewer.close();
    resolveBlob?.(new Blob(['hello'], { type: 'text/plain' }));
    await opening;
    assert.equal(elements.get('lb-stage')?.innerHTML, '');
    assert.equal(viewer.path, null);
  } finally {
    Object.defineProperty(globalThis, 'document', {
      configurable: true,
      value: previousDocument,
    });
  }
}

await testClosingViewerCancelsPendingLoad();
console.log('files viewer tests passed');
