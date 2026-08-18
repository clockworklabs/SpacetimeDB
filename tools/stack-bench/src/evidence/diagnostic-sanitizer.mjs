const ANSI = /\x1B\[[0-9;]*m|\[\d+m/g;
const TEST_SELECTOR = /\[(?:data-testid|data-test|data-cy)\s*=\s*(?:"[^"]*"|'[^']*'|[^\]\s]+)\]/gi;
const LOCATOR_CALL = /\b(?:page\.)?(?:locator|getByTestId|getByRole|getByText|getByLabel|getByPlaceholder|getByAltText|getByTitle|waitForSelector)\((?:[^()]|\([^()]*\))*\)/gi;
const LOCAL_URL = /\b(?:https?:\/\/)?(?:localhost|127\.0\.0\.1|0\.0\.0\.0|host\.docker\.internal)(?::\d+)?(?:\/[^\s'"`)]+)?/gi;
const WINDOWS_PATH = /\b[A-Za-z]:[\\/][^\s'"`)]*/g;
const HARNESS_PATH = /\/(?:app|workspace|root|home|tmp|mnt|tools\/stack-bench)(?:\/[^\s'"`):]*)*/g;

function redactSecrets(value) {
  return value
    .replace(/\b(?:authorization\s*:\s*)?bearer\s+[A-Za-z0-9._~+\/-]+=*/gi, 'credential [redacted]')
    .replace(/\b(?:api[_-]?key|access[_-]?token|auth[_-]?token|password)\s*[=:]\s*[^\s,;]+/gi,
      match => `${match.split(/[=:]/, 1)[0]}=[redacted]`)
    .replace(/\b(?:sk|key)-[A-Za-z0-9_-]{16,}\b/g, '[redacted credential]');
}

function clean(value, limit, { callLog = true } = {}) {
  let raw = String(value ?? '').replace(ANSI, ' ');
  let reasons = '';
  if (callLog) {
    const callLogAt = raw.search(/Call log:/i);
    if (callLogAt !== -1) {
      const log = raw.slice(callLogAt);
      reasons = [...log.matchAll(/^\s*-\s+(.*)$/gm)]
        .map(match => match[1].trim())
        .filter(line => !/^(?:waiting for|retrying|attempting|scrolling|done scrolling|waiting \d|\d+\s*[×x]\s+)/i.test(line))
        .filter(line => !/^locator resolved to/i.test(line))
        .map(line => clean(line, 120, { callLog: false }))
        .filter(Boolean);
      reasons = [...new Set(reasons)].slice(0, 2).join('; ');
      raw = raw.slice(0, callLogAt);
    }
  }

  let result = redactSecrets(raw)
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
    .replace(/\s+/g, ' ')
    .trim();
  if (reasons) result = `${result}${result ? ' ' : ''}(${reasons})`;
  if (/^(?:setup failed|timeout(?: exceeded)?[.\s]*)$/i.test(result)) return '';
  return result.length > limit ? `${result.slice(0, limit - 3)}...` : result;
}

export function sanitiseDiagnostic(detail = '', limit = 220) {
  return clean(detail, limit);
}

export function sanitiseConsoleError(detail = '') {
  return sanitiseDiagnostic(detail, 360).replaceAll('`', "'");
}

export function humaniseDiagnostic(detail = '') {
  const raw = String(detail ?? '');
  if (/still visible after/i.test(raw)) return 'it was still showing when it should have disappeared';
  if (/not visible within/i.test(raw)) return 'it never appeared';
  if (/missing, \d+ duplicated/i.test(raw)) {
    const match = raw.match(/(\d+) missing, (\d+) duplicated/i);
    const parts = [];
    if (Number(match?.[1])) parts.push(`${match[1]} never arrived`);
    if (Number(match?.[2])) parts.push(`${match[2]} arrived more than once`);
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
    const why = sanitiseDiagnostic(setup[1]);
    if (/current-user|signed-in|session/i.test(setup[1]))
      return 'signing in never completed, so nothing behind it could be reached';
    return why ? `the feature could not be set up: ${why}` : 'the feature could not be reached at all';
  }

  if (/Timeout/i.test(raw)) {
    const why = sanitiseDiagnostic(raw).replace(/^timeout\b[:\s-]*/i, '').trim();
    return why ? `the app did not respond in time: ${why}` : 'the app did not respond in time';
  }

  const rest = sanitiseDiagnostic(raw);
  return rest ? `it did not behave as described: ${rest}` : 'it did not behave as described';
}
