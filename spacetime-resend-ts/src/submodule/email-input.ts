import { errors } from './errors.js';
import { hasControlCharacter } from './text-validation.js';

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
  throw new Error(code);
}

function validateAddressList(
  values: string[] | undefined,
  tooMany: string,
  invalidAddress: string
): void {
  if (values === undefined) return;
  if (values.length > 100) fail(tooMany);
  for (const value of values) {
    if (
      value.length === 0 ||
      value.length > 320 ||
      hasControlCharacter(value)
    ) {
      fail(invalidAddress);
    }
  }
}

function validateJson(
  value: string | undefined,
  tooLarge: string,
  invalidJson: string,
  maxLength: number
): void {
  if (value === undefined) return;
  if (value.length > maxLength) fail(tooLarge);
  let parsed: unknown;
  try {
    parsed = JSON.parse(value);
  } catch {
    fail(invalidJson);
  }
  if (parsed === null || typeof parsed !== 'object') fail(invalidJson);
}

export function validateEmailInput(args: EmailInput): void {
  if (args.to.length === 0) fail(errors.sendEmailNoRecipients);
  validateAddressList(args.to, errors.toTooMany, errors.toInvalidAddress);
  validateAddressList(args.cc, errors.ccTooMany, errors.ccInvalidAddress);
  validateAddressList(args.bcc, errors.bccTooMany, errors.bccInvalidAddress);
  validateAddressList(
    args.replyTo,
    errors.replyToTooMany,
    errors.replyToInvalidAddress
  );
  const recipientCount =
    args.to.length + (args.cc?.length ?? 0) + (args.bcc?.length ?? 0);
  if (recipientCount > 100) fail(errors.sendEmailTooManyRecipients);
  if (
    args.from !== undefined &&
    (args.from.length === 0 ||
      args.from.length > 320 ||
      hasControlCharacter(args.from))
  )
    fail(errors.sendEmailInvalidFrom);
  if (
    args.subject.length === 0 ||
    args.subject.length > 998 ||
    hasControlCharacter(args.subject)
  ) {
    fail(errors.sendEmailInvalidSubject);
  }
  if (args.html === undefined && args.text === undefined)
    fail(errors.sendEmailMissingContent);
  if ((args.html?.length ?? 0) > 200_000) fail(errors.sendEmailHtmlTooLarge);
  if ((args.text?.length ?? 0) > 200_000) fail(errors.sendEmailTextTooLarge);
  validateJson(
    args.tagsJson,
    errors.tagsTooLarge,
    errors.tagsInvalidJson,
    16_384
  );
  validateJson(
    args.headersJson,
    errors.headersTooLarge,
    errors.headersInvalidJson,
    16_384
  );
  if (
    (args.scheduledAt?.length ?? 0) > 128 ||
    hasControlCharacter(args.scheduledAt ?? '')
  ) {
    fail(errors.sendEmailInvalidSchedule);
  }
}
