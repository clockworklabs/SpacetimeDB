import { FILE_BYTES_MAX } from '@spacetimedb/files/constants';

export const ATTACHMENT_ALLOWED_MIMES = new Set([
  'image/png',
  'image/jpeg',
  'image/webp',
  'image/gif',
]);

export const ATTACHMENT_COUNT_MAX = 4;
export const ATTACHMENT_TOTAL_BYTES_MAX = 12_000_000;

export interface AttachmentInput {
  mimeType: string;
  bytes: ArrayLike<number>;
}

export function attachmentValidationError(
  attachments: readonly AttachmentInput[]
): string | undefined {
  if (attachments.length > ATTACHMENT_COUNT_MAX) {
    return `agent.too_many_attachments:${attachments.length}/${ATTACHMENT_COUNT_MAX}`;
  }

  let totalBytes = 0;
  for (const attachment of attachments) {
    if (!ATTACHMENT_ALLOWED_MIMES.has(attachment.mimeType)) {
      return `agent.unsupported_attachment_mime:${attachment.mimeType}`;
    }
    if (attachment.bytes.length > FILE_BYTES_MAX) {
      return `agent.attachment_too_large:${attachment.bytes.length}/${FILE_BYTES_MAX}`;
    }
    totalBytes += attachment.bytes.length;
    if (totalBytes > ATTACHMENT_TOTAL_BYTES_MAX) {
      return `agent.attachments_too_large:${totalBytes}/${ATTACHMENT_TOTAL_BYTES_MAX}`;
    }
  }
  return undefined;
}
