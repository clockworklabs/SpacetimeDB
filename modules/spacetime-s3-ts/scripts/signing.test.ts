import assert from 'node:assert/strict';
import { test } from 'node:test';
import { Hash } from '@smithy/hash-node';
import { SignatureV4 } from '@smithy/signature-v4';
import { signPolicy, presignRequest } from '../src/signing.js';

// Public credentials and a known signature from the AWS POST signing example.
const credentials = {
  accessKeyId: 'AKIAIOSFODNN7EXAMPLE',
  secretAccessKey: 'wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY',
};
test('AWS published POST policy signature', () => {
  const policy =
    'eyAiZXhwaXJhdGlvbiI6ICIyMDE1LTEyLTMwVDEyOjAwOjAwLjAwMFoiLA0KICAiY29uZGl0aW9ucyI6IFsNCiAgICB7ImJ1Y2tldCI6ICJzaWd2NGV4YW1wbGVidWNrZXQifSwNCiAgICBbInN0YXJ0cy13aXRoIiwgIiRrZXkiLCAidXNlci91c2VyMS8iXSwNCiAgICB7ImFjbCI6ICJwdWJsaWMtcmVhZCJ9LA0KICAgIHsic3VjY2Vzc19hY3Rpb25fcmVkaXJlY3QiOiAiaHR0cDovL3NpZ3Y0ZXhhbXBsZWJ1Y2tldC5zMy5hbWF6b25hd3MuY29tL3N1Y2Nlc3NmdWxfdXBsb2FkLmh0bWwifSwNCiAgICBbInN0YXJ0cy13aXRoIiwgIiRDb250ZW50LVR5cGUiLCAiaW1hZ2UvIl0sDQogICAgeyJ4LWFtei1tZXRhLXV1aWQiOiAiMTQzNjUxMjM2NTEyNzQifSwNCiAgICB7IngtYW16LXNlcnZlci1zaWRlLWVuY3J5cHRpb24iOiAiQUVTMjU2In0sDQogICAgWyJzdGFydHMtd2l0aCIsICIkeC1hbXotbWV0YS10YWciLCAiIl0sDQoNCiAgICB7IngtYW16LWNyZWRlbnRpYWwiOiAiQUtJQUlPU0ZPRE5ON0VYQU1QTEUvMjAxNTEyMjkvdXMtZWFzdC0xL3MzL2F3czRfcmVxdWVzdCJ9LA0KICAgIHsieC1hbXotYWxnb3JpdGhtIjogIkFXUzQtSE1BQy1TSEEyNTYifSwNCiAgICB7IngtYW16LWRhdGUiOiAiMjAxNTEyMjlUMDAwMDAwWiIgfQ0KICBdDQp9';
  assert.equal(
    signPolicy(policy, credentials.secretAccessKey, '20151229', 'us-east-1'),
    '8afdbf4008c03f22c2cd3cdb72e4afbb1f6a588f3255ac628749a66d7f09699e'
  );
});
test('query signatures match the official signer, including tokens, headers, keys and pagination', async () => {
  const signingDate = new Date('2026-10-15T12:00:00Z');
  const host = 'proof.s3.us-east-1.amazonaws.com';
  for (const sessionToken of [undefined, 'session+/=token']) {
    const creds = { ...credentials, sessionToken };
    const signer = new SignatureV4({
      credentials: creds,
      region: 'us-east-1',
      service: 's3',
      sha256: Hash.bind(null, 'sha256'),
      uriEscapePath: false,
    });
    for (const method of ['GET', 'HEAD', 'PUT', 'DELETE']) {
      for (const [key, path] of [
        ['final/test.txt', '/final/test.txt'],
        ['space +%/café//test.txt', '/space%20%2B%25/caf%C3%A9//test.txt'],
      ]) {
        const headers: Record<string, string> = {
          'x-amz-expected-bucket-owner': '123456789012',
        };
        if (method === 'PUT')
          Object.assign(headers, {
            'x-amz-copy-source': '/proof/temporary/test.txt',
            'x-amz-copy-source-if-match': '"etag"',
          });
        const actual = presignRequest(
          method,
          host,
          key,
          headers,
          '20261015T120000Z',
          'us-east-1',
          creds,
          60
        );
        const expected = await signer.presign(
          {
            method,
            protocol: 'https:',
            hostname: host,
            path,
            query: {},
            headers: {
              ...headers,
              host,
              'x-amz-content-sha256': 'UNSIGNED-PAYLOAD',
            },
          },
          {
            signingDate,
            expiresIn: 60,
            unhoistableHeaders: new Set(Object.keys(headers)),
          }
        );
        assert.deepEqual(
          Object.fromEntries(new URL(actual).searchParams),
          expected.query
        );
      }
    }
    const query = {
      'list-type': '2',
      prefix: 'app/final/',
      'max-keys': '16',
      'start-after': 'app/final/a +%/café',
      'encoding-type': 'url',
    };
    const actual = presignRequest(
      'GET',
      host,
      '',
      {},
      '20261015T120000Z',
      'us-east-1',
      creds,
      60,
      query
    );
    const expected = await signer.presign(
      {
        method: 'GET',
        protocol: 'https:',
        hostname: host,
        path: '/',
        query,
        headers: { host, 'x-amz-content-sha256': 'UNSIGNED-PAYLOAD' },
      },
      { signingDate, expiresIn: 60 }
    );
    assert.deepEqual(
      Object.fromEntries(new URL(actual).searchParams),
      expected.query
    );
  }
});
