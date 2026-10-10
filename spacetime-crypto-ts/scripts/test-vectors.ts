// Verify the @noble/hashes wrappers and signature helpers against test vectors.
// Run via: pnpm test
//
// Sources:
//   SHA-256 vectors: NIST CAVS examples + RFC 6234 §8.5
//   HMAC-SHA256 vectors: RFC 4231 §4

import { sha256 } from '../src/sha256';
import { hmacSha256 } from '../src/hmac';
import {
  bytesToHex,
  hexToBytes,
  timingSafeEqual,
  base64ToBytes,
} from '../src/timing';
import {
  errors,
  verifyStripeSignature,
  verifyGithubSignature,
  verifySvixSignature,
  type VerifyResult,
} from '../src/vendors';

const enc = new TextEncoder();

let pass = 0,
  fail = 0;

function assertEq(label: string, got: string, expected: string): void {
  if (got === expected) {
    process.stdout.write(`  OK  ${label}\n`);
    pass++;
  } else {
    process.stdout.write(
      `  FAIL ${label}\n    got:      ${got}\n    expected: ${expected}\n`
    );
    fail++;
  }
}

function assert(label: string, cond: boolean): void {
  if (cond) {
    process.stdout.write(`  OK  ${label}\n`);
    pass++;
  } else {
    process.stdout.write(`  FAIL ${label}\n`);
    fail++;
  }
}

function assertResult(
  label: string,
  got: VerifyResult,
  expected: VerifyResult
): void {
  assertEq(label, JSON.stringify(got), JSON.stringify(expected));
}

const ok: VerifyResult = { ok: true };
const fails = (reason: keyof typeof errors): VerifyResult => ({
  ok: false,
  reason: errors[reason],
});

// SHA-256
process.stdout.write('SHA-256\n');
assertEq(
  'empty string',
  bytesToHex(sha256(enc.encode(''))),
  'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855'
);
assertEq(
  '"abc"',
  bytesToHex(sha256(enc.encode('abc'))),
  'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad'
);
assertEq(
  'NIST 448-bit message',
  bytesToHex(
    sha256(
      enc.encode('abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq')
    )
  ),
  '248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1'
);
// One million "a" characters exercises message-length packing in the padding.
const millionA = new Uint8Array(1_000_000);
millionA.fill(0x61);
assertEq(
  '1,000,000 × "a"',
  bytesToHex(sha256(millionA)),
  'cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0'
);

// HMAC-SHA256 (RFC 4231 test cases)
process.stdout.write('\nHMAC-SHA256 (RFC 4231)\n');

assertEq(
  'TC1: 20 × 0x0b key, "Hi There"',
  bytesToHex(
    hmacSha256(
      hexToBytes('0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b'),
      enc.encode('Hi There')
    )
  ),
  'b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7'
);

assertEq(
  'TC2: "Jefe" key, "what do ya want for nothing?"',
  bytesToHex(
    hmacSha256(enc.encode('Jefe'), enc.encode('what do ya want for nothing?'))
  ),
  '5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843'
);

assertEq(
  'TC3: 20 × 0xaa key, 50 × 0xdd data',
  bytesToHex(
    hmacSha256(
      hexToBytes('aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'),
      hexToBytes('dd'.repeat(50))
    )
  ),
  '773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe'
);

assertEq(
  'TC4: 25-byte key, 50 × 0xcd',
  bytesToHex(
    hmacSha256(
      hexToBytes('0102030405060708090a0b0c0d0e0f10111213141516171819'),
      hexToBytes('cd'.repeat(50))
    )
  ),
  '82558a389a443c0ea4cc819899f2083a85f0faa3e578f8077a2e3ff46729665b'
);

assertEq(
  'TC6: 131-byte key (forces hash-down), short data',
  bytesToHex(
    hmacSha256(
      hexToBytes('aa'.repeat(131)),
      enc.encode('Test Using Larger Than Block-Size Key - Hash Key First')
    )
  ),
  '60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54'
);

assertEq(
  'TC7: 131-byte key, long data',
  bytesToHex(
    hmacSha256(
      hexToBytes('aa'.repeat(131)),
      enc.encode(
        'This is a test using a larger than block-size key and a larger than block-size data. ' +
          'The key needs to be hashed before being used by the HMAC algorithm.'
      )
    )
  ),
  '9b09ffa71b942fcb27635fbcd5b0e944bfdc63644f0713938a7f51535c3a35e2'
);

// timingSafeEqual
process.stdout.write('\ntimingSafeEqual\n');
assert(
  'equal returns true',
  timingSafeEqual(new Uint8Array([1, 2, 3]), new Uint8Array([1, 2, 3]))
);
assert(
  'differing byte returns false',
  !timingSafeEqual(new Uint8Array([1, 2, 3]), new Uint8Array([1, 2, 4]))
);
assert(
  'different length returns false',
  !timingSafeEqual(new Uint8Array([1, 2, 3]), new Uint8Array([1, 2, 3, 4]))
);
assert(
  'two empty returns true',
  timingSafeEqual(new Uint8Array([]), new Uint8Array([]))
);

// Base64 round-trip
process.stdout.write('\nbase64\n');
const b64Cases: Array<[string, string]> = [
  ['', ''],
  ['f', 'Zg=='],
  ['fo', 'Zm8='],
  ['foo', 'Zm9v'],
  ['foob', 'Zm9vYg=='],
  ['fooba', 'Zm9vYmE='],
  ['foobar', 'Zm9vYmFy'],
];
for (const [plain, b64] of b64Cases) {
  const decoded = new TextDecoder().decode(base64ToBytes(b64));
  assertEq(`decode("${b64}")`, decoded, plain);
}
assertEq(
  'decode unpadded Zg',
  new TextDecoder().decode(base64ToBytes('Zg')),
  'f'
);
for (const malformed of [
  'A',
  '=AAA',
  'A=AA',
  'AA=A',
  'AAAA=',
  'AA===',
  'Zh==',
  'Zm9=',
  'Zg==\n',
  '\u0100AAA',
  'AA\u0100A',
]) {
  let threw = false;
  try {
    base64ToBytes(malformed);
  } catch {
    threw = true;
  }
  assert(`reject malformed base64 ${JSON.stringify(malformed)}`, threw);
}

// Stripe signature round-trip
// Construct a header the way Stripe does, then verify it round-trips.
process.stdout.write('\nStripe webhook signature\n');
const stripeSecret = 'whsec_test_secret_1234567890';
const stripeBody = '{"id":"evt_test","type":"customer.created"}';
const stripeTs = '1700000000';
const stripeMac = hmacSha256(
  enc.encode(stripeSecret),
  enc.encode(`${stripeTs}.${stripeBody}`)
);
const stripeHeader = `t=${stripeTs},v1=${bytesToHex(stripeMac)}`;
const stripe = {
  rawBody: stripeBody,
  signatureHeader: stripeHeader,
  secret: stripeSecret,
  toleranceSeconds: Infinity,
};
assertResult('valid signature passes', verifyStripeSignature(stripe), ok);
assertResult(
  'mutated body fails',
  verifyStripeSignature({ ...stripe, rawBody: stripeBody + 'x' }),
  fails('signatureMismatch')
);
assertResult(
  'wrong secret fails',
  verifyStripeSignature({ ...stripe, secret: 'whsec_wrong' }),
  fails('signatureMismatch')
);
assertResult(
  'missing v1 entry fails',
  verifyStripeSignature({ ...stripe, signatureHeader: `t=${stripeTs}` }),
  fails('missingSignature')
);
assertResult(
  'non-numeric timestamp fails',
  verifyStripeSignature({
    ...stripe,
    signatureHeader: `t=soon,v1=${bytesToHex(stripeMac)}`,
  }),
  fails('invalidTimestamp')
);
assertResult(
  'old timestamp rejected when tolerance enforced',
  verifyStripeSignature({
    ...stripe,
    toleranceSeconds: 300,
    nowSeconds: Number(stripeTs) + 1000,
  }),
  fails('timestampOutsideTolerance')
);
assertResult(
  'multiple v1 entries: any match wins',
  verifyStripeSignature({
    ...stripe,
    signatureHeader: `t=${stripeTs},v1=deadbeef,v1=${bytesToHex(stripeMac)}`,
  }),
  ok
);

// GitHub signature round-trip
process.stdout.write('\nGitHub webhook signature\n');
const ghSecret = "It's a Secret to Everybody";
const ghBody = 'Hello, World!';
const ghMac = hmacSha256(enc.encode(ghSecret), enc.encode(ghBody));
const github = {
  rawBody: ghBody,
  signatureHeader: `sha256=${bytesToHex(ghMac)}`,
  secret: ghSecret,
};
assertResult('roundtrip passes', verifyGithubSignature(github), ok);
assertResult(
  'missing prefix fails',
  verifyGithubSignature({ ...github, signatureHeader: bytesToHex(ghMac) }),
  fails('missingSignature')
);
assertResult(
  'wrong secret fails',
  verifyGithubSignature({ ...github, secret: 'wrong' }),
  fails('signatureMismatch')
);
// Known vector from GitHub docs.
assertEq(
  'docs example body→hex',
  bytesToHex(hmacSha256(enc.encode(ghSecret), enc.encode(ghBody))),
  '757107ea0eb2509fc211221cce984b8a37570b6d7586c22c46f4379c8b043e17'
);

// Svix (Resend) signature round-trip
process.stdout.write('\nsvix (Resend) signature\n');
const svixSecretRaw = new Uint8Array(32).fill(0x42);
const toBase64 = (bytes: Uint8Array) => Buffer.from(bytes).toString('base64');
const svixId = 'msg_test_123';
const svixTs = '1700000000';
const svixBody = '{"event":"email.delivered"}';
const svixMac = hmacSha256(
  svixSecretRaw,
  enc.encode(`${svixId}.${svixTs}.${svixBody}`)
);
const svix = {
  rawBody: svixBody,
  svixId,
  svixTimestamp: svixTs,
  svixSignature: `v1,${toBase64(svixMac)}`,
  secret: `whsec_${toBase64(svixSecretRaw)}`,
  toleranceSeconds: Infinity,
};
assertResult('valid svix signature passes', verifySvixSignature(svix), ok);
assertResult(
  'mutated svix body fails',
  verifySvixSignature({ ...svix, rawBody: svixBody + 'x' }),
  fails('signatureMismatch')
);
assertResult(
  'invalid base64 secret fails',
  verifySvixSignature({ ...svix, secret: 'whsec_not base64' }),
  fails('invalidSecret')
);
assertResult(
  'no v1 signature fails',
  verifySvixSignature({ ...svix, svixSignature: 'v2,abc' }),
  fails('missingSignature')
);
assertResult(
  'old svix timestamp rejected',
  verifySvixSignature({
    ...svix,
    toleranceSeconds: 300,
    nowSeconds: Number(svixTs) + 301,
  }),
  fails('timestampOutsideTolerance')
);

// Summary
process.stdout.write(`\n${pass} pass, ${fail} fail\n`);
if (fail > 0) process.exit(1);
