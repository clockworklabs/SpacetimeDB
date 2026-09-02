const ANSI = new RegExp(`${String.fromCharCode(27)}\\[[0-9;]*m|\\[\\d+m`, 'g');
const TEST_SELECTOR = /\[(?:data-testid|data-test|data-cy)\s*=\s*(?:"[^"]*"|'[^']*'|[^\]\s]+)\]/gi;
const LOCATOR_CALL = /\b(?:page\.)?(?:locator|getByTestId|getByRole|getByText|getByLabel|getByPlaceholder|getByAltText|getByTitle|waitForSelector)\((?:[^()]|\([^()]*\))*\)/gi;
const LOCAL_URL = /\b(?:https?:\/\/)?(?:localhost|127\.0\.0\.1|0\.0\.0\.0|host\.docker\.internal)(?::\d+)?(?:\/[^\s'"`)]+)?/gi;
const WINDOWS_PATH = /\b[A-Za-z]:[\\/][^\s'"`)]*/g;
const HARNESS_PATH = /\/(?:app|workspace|root|home|tmp|mnt|tools\/stack-bench)(?:\/[^\s'"`):]*)*/g;
const BEARER_CREDENTIAL = /\b(?:authorization\s*:\s*)?bearer\s+[A-Za-z0-9._~+/-]+=*/gi;
const NAMED_CREDENTIAL = /["']?(?:anthropic_api_key|claude_code_oauth_token|api[_-]?key|access[_-]?token|auth[_-]?token|oauth[_-]?token|password|secret)["']?\s*[:=]\s*(?:"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|[^\s,;}\]]+)/gi;
const PREFIXED_CREDENTIAL = /\b(?:sk|key)-[A-Za-z0-9_-]{16,}\b/g;
const URL_CREDENTIAL = /\b([a-z][a-z0-9+.-]*:\/\/)[^\s/@]+:[^\s/@]+@/gi;

function publicControlName(value: unknown): string | null {
  const raw = String(value ?? '');
  const match = raw.match(/\[(?:data-testid|data-test|data-cy)\s*=\s*(?:"([^"]+)"|'([^']+)'|([^\]\s]+))\]/i)
    ?? raw.match(/(?:getByTestId|locator)\(\s*["']([^"']+)["']/i)
    ?? raw.match(/\bcontrol,#([a-z0-9_.:-]+)/i);
  const name = match?.[1] ?? match?.[2] ?? match?.[3];
  if (!name || !/^[a-z0-9][a-z0-9_.:-]{0,79}$/i.test(name)) return null;
  return name.toLowerCase();
}

export function redactCredentials(value: unknown): string {
  return String(value ?? '')
    .replace(BEARER_CREDENTIAL, '[redacted credential]')
    .replace(NAMED_CREDENTIAL, '[redacted credential]')
    .replace(PREFIXED_CREDENTIAL, '[redacted credential]')
    .replace(URL_CREDENTIAL, '$1[redacted credential]@');
}

interface CleanOptions {
  callLog?: boolean;
}

function clean(value: unknown, limit: number, { callLog = true }: CleanOptions = {}): string {
  let raw = String(value ?? '').replace(ANSI, ' ');
  let reasons = '';
  if (callLog) {
    const callLogAt = raw.search(/Call log:/i);
    if (callLogAt !== -1) {
      const log = raw.slice(callLogAt);
      const selectedReasons = [...log.matchAll(/^\s*-\s+(.*)$/gm)]
        .map(match => match[1]?.trim() ?? '')
        .filter(line => line.length > 0)
        .filter(line => !/^(?:waiting for|retrying|attempting|scrolling|done scrolling|waiting \d|\d+\s*[×x]\s+)/i.test(line))
        .filter(line => !/^locator resolved to/i.test(line))
        .map(line => clean(line, 120, { callLog: false }))
        .filter(Boolean);
      reasons = [...new Set(selectedReasons)].slice(0, 2).join('; ');
      raw = raw.slice(0, callLogAt);
    }
  }

  let result = redactCredentials(raw)
    .replace(/^\s*at\s+.*$/gm, ' ')
    .replace(/\bfile:\/\/\/?[^\s'"`)]+/gi, 'a file')
    .replace(LOCAL_URL, 'the app')
    .replace(WINDOWS_PATH, 'a file')
    .replace(HARNESS_PATH, 'a file')
    .replace(TEST_SELECTOR, 'the control')
    .replace(LOCATOR_CALL, 'the control')
    .replace(/\b(?:within|after|timeout(?: of)?)\s+\d+(?:\.\d+)?\s*(?:ms|milliseconds?|s|seconds?)\b/gi,
      'in time')
    .replace(/\b\d+(?:\.\d+)?\s*(?:ms|milliseconds?|s|seconds?)\b/gi, '')
    .replace(/\bin time exceeded\b/gi, 'did not respond in time')
    .replace(/\s+/g, ' ')
    .trim();
  if (reasons) result = `${result}${result ? ' ' : ''}(${reasons})`;
  if (/^(?:setup failed|timeout(?: exceeded)?[.\s]*)$/i.test(result)) return '';
  return result.length > limit ? `${result.slice(0, limit - 3)}...` : result;
}

export function sanitiseDiagnostic(detail: unknown = '', limit = 220): string {
  return clean(detail, limit);
}

export function sanitiseConsoleError(detail: unknown = ''): string {
  return sanitiseDiagnostic(detail, 360).replaceAll('`', "'");
}

export function humaniseDiagnostic(detail: unknown = ''): string {
  const raw = String(detail ?? '');
  if (/selectOption/i.test(raw)) {
    const control = publicControlName(raw);
    return control
      ? `the ${control} control did not offer the requested choice`
      : 'the requested choice was not available';
  }
  const stateMismatch = raw.match(/expected\s+data-state\s+["']([^"']+)["'].*?got\s+["']([^"']+)["']/i);
  if (stateMismatch) {
    const control = publicControlName(raw);
    return `${control ? `the ${control} control` : 'the control'} showed "${stateMismatch[2]}" instead of "${stateMismatch[1]}"`;
  }
  const valueMismatch = raw.match(/expected\s+(?:the\s+)?control,#([a-z0-9_.:-]+).*?value\s+["']([^"']+)["'].*?got\s+["']([^"']+)["']/i)
    ?? raw.match(/(?:the\s+)?control,#([a-z0-9_.:-]+).*?expected\s+value\s+["']([^"']+)["'].*?got\s+["']([^"']+)["']/i);
  if (valueMismatch) {
    return `the ${valueMismatch[1]} control showed "${valueMismatch[3]}" instead of "${valueMismatch[2]}"`;
  }
  const sequenceMismatch = raw.match(/expected\s+(?:the\s+)?control,#([a-z0-9_.:-]+)\s+sequence\s+(.+),\s+saw\s+(.+?)(?:\s+\(|$)/i);
  if (sequenceMismatch) {
    return `the ${sequenceMismatch[1]} list showed ${sequenceMismatch[3]} instead of ${sequenceMismatch[2]}`;
  }
  const unexpectedControl = raw.match(/(?:the\s+)?control,#([a-z0-9_.:-]+)(?:\s+containing\s+(["'][^"']+["']))?\s+became\s+(?:available|visible)/i);
  if (unexpectedControl) {
    return unexpectedControl[2]
      ? `the ${unexpectedControl[1]} control showed ${unexpectedControl[2]} when it should not have`
      : `the ${unexpectedControl[1]} control was available when it should not have been`;
  }
  if (/still visible after/i.test(raw)) {
    const control = publicControlName(raw);
    return control ? `the ${control} control was still visible` : sanitiseDiagnostic(raw);
  }
  if (/not visible within/i.test(raw)) {
    const control = publicControlName(raw);
    return control ? `the ${control} control did not appear` : sanitiseDiagnostic(raw);
  }
  if (/missing, \d+ duplicated/i.test(raw)) {
    const match = raw.match(/(\d+) missing, (\d+) duplicated/i);
    const missing = Number(match?.[1]);
    const duplicated = Number(match?.[2]);
    const parts: string[] = [];
    if (missing) parts.push(`${missing} never arrived`);
    if (duplicated) parts.push(`${duplicated} arrived more than once`);
    return parts.join(' and ');
  }
  if (/order differs between/i.test(raw)) return 'the two users saw the messages in different orders';
  if (/unexpectedly contains/i.test(raw)) return 'it included something it should not have';
  if (/expected exactly (\d+)/i.test(raw)) {
    const match = raw.match(/expected exactly (\d+) .*?, found (\d+)/i);
    return match ? `there were ${match[2]} of them instead of ${match[1]}` : 'the wrong number of them appeared';
  }
  if (/ACCEPTED a write with a tampered/i.test(raw))
    return 'the server accepted a request that claimed to be from a different user';
  if (/intercepts pointer events/i.test(raw))
    return 'something invisible was covering the page and absorbing the clicks';
  if (/Page crashed/i.test(raw)) return 'the page crashed';
  if (/element is not (?:visible|enabled)/i.test(raw)) return 'the control was on screen but not usable';

  const setup = raw.match(/setup failed:\s*(.*)$/is);
  if (setup) {
    const setupDetail = setup[1] ?? '';
    const why = sanitiseDiagnostic(setupDetail);
    if (/current-user|signed-in|session/i.test(setupDetail))
      return 'signing in never completed, so nothing behind it could be reached';
    return why ? `the feature could not be set up: ${why}` : 'the feature could not be reached at all';
  }

  if (/Timeout/i.test(raw)) {
    const control = publicControlName(raw);
    if (control) return `the ${control} control did not become usable`;
    const why = sanitiseDiagnostic(raw).replace(/^timeout\b[:\s-]*/i, '').trim();
    return why ? `the app did not respond in time: ${why}` : 'the app did not respond in time';
  }

  const rest = sanitiseDiagnostic(raw);
  return rest ? `it did not behave as described: ${rest}` : 'it did not behave as described';
}
