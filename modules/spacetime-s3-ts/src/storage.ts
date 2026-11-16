import { fromByteArray } from 'base64-js';
import { TimeDuration, Timestamp } from 'spacetimedb';
import { XMLParser, XMLValidator } from 'fast-xml-parser';
import type { ProcedureCtx, InferSchema } from 'spacetimedb/server';
import type submodule from './submodule.js';
import {
  credentialExpiry,
  S3Error,
  type Credentials,
  type ResolvedConfig,
} from './config.js';
import { presignRequest, signPolicy } from './signing.js';
import type { FileRecord } from './model.js';

export type Http = ProcedureCtx<InferSchema<typeof submodule>>['http'];
export const PAGE_SIZE = 16;
const stamp = (millis: number) =>
  new Date(millis).toISOString().replace(/[:-]|\.\d{3}/g, '');
// RFC 5987 value for a Content-Disposition filename* parameter.
const filenameParam = (name: string) =>
  encodeURIComponent(name).replace(
    /['()*]/g,
    ch => `%${ch.charCodeAt(0).toString(16).toUpperCase()}`
  );
function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}
function parseXml(text: string): Record<string, unknown> {
  // The native client buffers the response before this parsing limit applies.
  if (
    text.length > 131072 ||
    /<!DOCTYPE|<!ENTITY/i.test(text) ||
    XMLValidator.validate(text) !== true
  )
    throw new S3Error('s3.invalid_response');
  const parsed: unknown = new XMLParser({
    parseTagValue: false,
    isArray: name => name === 'Contents',
  }).parse(text);
  if (!record(parsed)) throw new S3Error('s3.invalid_response');
  return parsed;
}

export function storage(
  config: ResolvedConfig,
  install: bigint,
  credentials: Credentials,
  http: Http,
  now: () => Timestamp
) {
  const host = `${config.bucket}.s3.${config.region}.amazonaws.com`;
  const temporaryKey = (id: bigint) =>
    `${config.prefix}temporary/${install}-${id}`;
  const finalKey = (id: bigint) => `${config.prefix}final/${install}-${id}`;
  const ownerHeader = { 'x-amz-expected-bucket-owner': config.accountId };
  let checked = false;
  function signed(
    method: string,
    key: string,
    headers: Record<string, string>,
    seconds: number,
    query: Record<string, string> = {}
  ) {
    const millis = Number(now().microsSinceUnixEpoch / 1000n);
    const ttl = Math.min(
      seconds,
      Math.floor((credentialExpiry(credentials, millis) - millis) / 1000)
    );
    if (ttl < 1) throw new S3Error('s3.credentials_expiring');
    return {
      url: presignRequest(
        method,
        host,
        key,
        headers,
        stamp(millis),
        config.region,
        credentials,
        ttl,
        query
      ),
      expiresAt: new Timestamp(BigInt(millis + ttl * 1000) * 1000n),
    };
  }
  function send(
    method: string,
    key: string,
    headers: Record<string, string> = {},
    query: Record<string, string> = {}
  ) {
    headers = { ...headers, ...ownerHeader };
    const { url } = signed(method, key, headers, 60, query);
    let response;
    try {
      response = http.fetch(url, {
        method,
        headers,
        timeout: new TimeDuration(
          BigInt(config.httpTimeoutSeconds) * 1_000_000n
        ),
      });
    } catch {
      throw new S3Error(`s3.${method.toLowerCase()}_transport_failed`);
    }
    if (!response.ok && !(method === 'HEAD' && response.status === 404))
      throw new S3Error(`s3.${method.toLowerCase()}_http_${response.status}`);
    const version = response.headers.get('x-amz-version-id');
    if (
      (version && version !== 'null') ||
      response.headers.get('x-amz-delete-marker') === 'true'
    )
      throw new S3Error('s3.versioned_bucket');
    return response;
  }
  function verifyBucket() {
    if (checked) return;
    const version = parseXml(
      send('GET', '', {}, { versioning: '' }).text()
    ).VersioningConfiguration;
    if (version !== '' && (!record(version) || version.Status !== undefined))
      throw new S3Error('s3.versioned_bucket');
    const access = parseXml(
      send('GET', '', {}, { publicAccessBlock: '' }).text()
    ).PublicAccessBlockConfiguration;
    if (
      !record(access) ||
      [
        'BlockPublicAcls',
        'IgnorePublicAcls',
        'BlockPublicPolicy',
        'RestrictPublicBuckets',
      ].some(key => access[key] !== 'true')
    )
      throw new S3Error('s3.public_access_not_blocked');
    checked = true;
  }
  function head(key: string, file: FileRecord) {
    const response = send('HEAD', key, { 'x-amz-checksum-mode': 'ENABLED' });
    if (response.status === 404) return undefined;
    const etag = response.headers.get('etag');
    if (
      !etag ||
      response.headers.get('content-length') !== String(file.size) ||
      response.headers.get('x-amz-checksum-sha256') !== file.checksum ||
      response.headers.get('x-amz-server-side-encryption') !== 'AES256'
    )
      throw new S3Error('s3.object_mismatch');
    return etag;
  }
  return {
    verifyBucket,
    upload(file: FileRecord) {
      const millis = Number(now().microsSinceUnixEpoch / 1000n),
        date = stamp(millis);
      const expires = Math.min(
        Number(file.uploadExpiresAt.microsSinceUnixEpoch / 1000n),
        credentialExpiry(credentials, millis)
      );
      // A retried reservation can be about to expire.
      if (expires < millis + 1000) throw new S3Error('s3.upload_expired');
      const expiresAt = new Timestamp(BigInt(expires) * 1000n);
      const fields: Record<string, string> = {
        key: temporaryKey(file.id),
        'Content-Type': file.contentType,
        'Content-Disposition': 'attachment',
        'x-amz-algorithm': 'AWS4-HMAC-SHA256',
        'x-amz-date': date,
        'x-amz-credential': `${credentials.accessKeyId}/${date.slice(0, 8)}/${config.region}/s3/aws4_request`,
        'x-amz-checksum-algorithm': 'SHA256',
        'x-amz-checksum-sha256': file.checksum,
        'x-amz-server-side-encryption': 'AES256',
      };
      if (credentials.sessionToken)
        fields['x-amz-security-token'] = credentials.sessionToken;
      const policy = fromByteArray(
        new TextEncoder().encode(
          JSON.stringify({
            expiration: expiresAt.toDate().toISOString(),
            conditions: [
              { bucket: config.bucket },
              ...Object.entries(fields).map(([key, value]) => ({
                [key]: value,
              })),
              ['content-length-range', file.size, file.size],
            ],
          })
        )
      );
      fields.policy = policy;
      fields['x-amz-signature'] = signPolicy(
        policy,
        credentials.secretAccessKey,
        date.slice(0, 8),
        config.region
      );
      return {
        url: `https://${host}/`,
        fields: Object.entries(fields).map(([name, value]) => ({
          name,
          value,
        })),
        expiresAt,
      };
    },
    copyVerified(file: FileRecord) {
      verifyBucket();
      if (head(finalKey(file.id), file)) return;
      const etag = head(temporaryKey(file.id), file);
      if (!etag) throw new S3Error('s3.upload_missing');
      const result = parseXml(
        send('PUT', finalKey(file.id), {
          'x-amz-copy-source': `/${config.bucket}/${temporaryKey(file.id)}`,
          'x-amz-copy-source-if-match': etag,
          'x-amz-server-side-encryption': 'AES256',
        }).text()
      );
      if (
        !record(result.CopyObjectResult) ||
        typeof result.CopyObjectResult.ETag !== 'string'
      )
        throw new S3Error('s3.copy_failed');
      if (!head(finalKey(file.id), file)) throw new S3Error('s3.copy_missing');
    },
    removeFile(id: bigint) {
      verifyBucket();
      send('DELETE', finalKey(id));
      send('DELETE', temporaryKey(id));
    },
    removeOrphan(key: string) {
      if (!key.startsWith(`${config.prefix}final/`))
        throw new S3Error('s3.invalid_key');
      verifyBucket();
      send('DELETE', key);
    },
    download(file: FileRecord) {
      // Bind the account in the query so a browser needs no custom GET headers.
      return signed('GET', finalKey(file.id), {}, config.downloadSeconds, {
        'x-amz-expected-bucket-owner': config.accountId,
        // Opening the URL saves the file under its original name.
        'response-content-disposition': `attachment; filename*=UTF-8''${filenameParam(file.filename)}`,
      });
    },
    listFinal(after: string) {
      verifyBucket();
      const result = parseXml(
        send(
          'GET',
          '',
          {},
          {
            'list-type': '2',
            prefix: `${config.prefix}final/`,
            'max-keys': String(PAGE_SIZE),
            'start-after': after,
            'encoding-type': 'url',
          }
        ).text()
      ).ListBucketResult;
      if (
        !record(result) ||
        !['true', 'false'].includes(String(result.IsTruncated))
      )
        throw new S3Error('s3.invalid_list');
      const entries: unknown = result.Contents ?? [];
      if (!Array.isArray(entries) || entries.length > PAGE_SIZE)
        throw new S3Error('s3.invalid_list');
      const keys = entries.map((entry: unknown) => {
        if (!record(entry) || typeof entry.Key !== 'string')
          throw new S3Error('s3.invalid_key');
        let key: string;
        try {
          key = decodeURIComponent(entry.Key);
        } catch {
          throw new S3Error('s3.invalid_key');
        }
        if (!key.startsWith(`${config.prefix}final/`))
          throw new S3Error('s3.invalid_key');
        return key;
      });
      if (result.IsTruncated === 'true' && keys.length === 0)
        throw new S3Error('s3.invalid_list');
      return {
        keys,
        next: result.IsTruncated === 'true' ? keys[keys.length - 1] : '',
      };
    },
  };
}
