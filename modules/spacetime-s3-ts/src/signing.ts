import { hmacSha256, sha256, bytesToHex } from '@spacetimedb/crypto';

import type { Credentials } from './config.js';
const utf8 = new TextEncoder();
const hash = (text: string) => bytesToHex(sha256(utf8.encode(text)));
const mac = (key: Uint8Array, text: string) =>
  hmacSha256(key, utf8.encode(text));
const encode = (text: string) =>
  encodeURIComponent(text).replace(
    /[!'()*]/g,
    ch => '%' + ch.charCodeAt(0).toString(16).toUpperCase()
  );
const keyPath = (key: string) => '/' + key.split('/').map(encode).join('/');
function signingKey(secret: string, date: string, region: string) {
  return mac(
    mac(mac(mac(utf8.encode('AWS4' + secret), date), region), 's3'),
    'aws4_request'
  );
}
export function signPolicy(
  policy: string,
  secret: string,
  date: string,
  region: string
) {
  return bytesToHex(mac(signingKey(secret, date, region), policy));
}
function signature(
  canonical: string,
  date: string,
  region: string,
  credentials: Credentials
) {
  const scope = `${date.slice(0, 8)}/${region}/s3/aws4_request`;
  const value = ['AWS4-HMAC-SHA256', date, scope, hash(canonical)].join('\n');
  return bytesToHex(
    mac(
      signingKey(credentials.secretAccessKey, date.slice(0, 8), region),
      value
    )
  );
}
export function presignRequest(
  method: string,
  host: string,
  key: string,
  headers: Record<string, string>,
  date: string,
  region: string,
  credentials: Credentials,
  expires: number,
  extraQuery: Record<string, string> = {}
) {
  const signingHeaders: Record<string, string> = { ...headers, host };
  const names = Object.keys(signingHeaders).sort();
  const canonicalHeaders = names
    .map(
      name => `${name}:${signingHeaders[name].trim().replace(/\s+/g, ' ')}\n`
    )
    .join('');
  const scope = `${date.slice(0, 8)}/${region}/s3/aws4_request`;
  const query: Record<string, string> = {
    ...extraQuery,
    'X-Amz-Algorithm': 'AWS4-HMAC-SHA256',
    'X-Amz-Credential': `${credentials.accessKeyId}/${scope}`,
    'X-Amz-Date': date,
    'X-Amz-Expires': String(expires),
    'X-Amz-SignedHeaders': names.join(';'),
    'x-amz-content-sha256': 'UNSIGNED-PAYLOAD',
  };
  if (credentials.sessionToken)
    query['X-Amz-Security-Token'] = credentials.sessionToken;
  const canonicalQuery = Object.keys(query)
    .sort()
    .map(k => `${encode(k)}=${encode(query[k])}`)
    .join('&');
  const path = keyPath(key);
  const canonical = [
    method,
    path,
    canonicalQuery,
    canonicalHeaders,
    names.join(';'),
    'UNSIGNED-PAYLOAD',
  ].join('\n');
  const signed = signature(canonical, date, region, credentials);
  return `https://${host}${path}?${canonicalQuery}&X-Amz-Signature=${signed}`;
}
