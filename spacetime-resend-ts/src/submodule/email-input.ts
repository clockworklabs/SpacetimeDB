import { hasControlCharacter } from './text-validation';

export type EmailInput = {
  from?: string | undefined;
  to: string[];
  subject: string;
  html?: string | undefined;
  text?: string | undefined;
  cc?: string[] | undefined;
  bcc?: string[] | undefined;
  replyTo?: string[] | undefined;
  tagsJson?: string | undefined;
  headersJson?: string | undefined;
  scheduledAt?: string | undefined;
};

function fail(code: string): never {
  throw new Error(`resend.${code}`);
}

function validateAddressList(
  values: string[] | undefined,
  field: string
): void {
  if (values === undefined) return;
  if (values.length > 100) fail(`${field}_too_many`);
  for (const value of values) {
    if (
      value.length === 0 ||
      value.length > 320 ||
      hasControlCharacter(value)
    ) {
      fail(`${field}_invalid_address`);
    }
  }
}

function validateJson(
  value: string | undefined,
  field: string,
  maxLength: number
): void {
  if (value === undefined) return;
  if (value.length > maxLength) fail(`${field}_too_large`);
  let parsed: unknown;
  try {
    parsed = JSON.parse(value);
  } catch {
    fail(`${field}_invalid_json`);
  }
  if (parsed === null || typeof parsed !== 'object')
    fail(`${field}_invalid_json`);
}

export function validateEmailInput(args: EmailInput): void {
  if (args.to.length === 0) fail('send_email_no_recipients');
  validateAddressList(args.to, 'to');
  validateAddressList(args.cc, 'cc');
  validateAddressList(args.bcc, 'bcc');
  validateAddressList(args.replyTo, 'reply_to');
  const recipientCount =
    args.to.length + (args.cc?.length ?? 0) + (args.bcc?.length ?? 0);
  if (recipientCount > 100) fail('send_email_too_many_recipients');
  if (
    args.from !== undefined &&
    (args.from.length === 0 ||
      args.from.length > 320 ||
      hasControlCharacter(args.from))
  )
    fail('send_email_invalid_from');
  if (
    args.subject.length === 0 ||
    args.subject.length > 998 ||
    hasControlCharacter(args.subject)
  ) {
    fail('send_email_invalid_subject');
  }
  if (args.html === undefined && args.text === undefined)
    fail('send_email_missing_content');
  if ((args.html?.length ?? 0) > 200_000) fail('send_email_html_too_large');
  if ((args.text?.length ?? 0) > 200_000) fail('send_email_text_too_large');
  validateJson(args.tagsJson, 'tags', 16_384);
  validateJson(args.headersJson, 'headers', 16_384);
  if (
    (args.scheduledAt?.length ?? 0) > 128 ||
    hasControlCharacter(args.scheduledAt ?? '')
  ) {
    fail('send_email_invalid_schedule');
  }
}
