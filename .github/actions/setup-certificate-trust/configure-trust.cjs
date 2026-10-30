// GitHub-hosted images can contain stale certificate stores. Build a CA bundle
// for subsequent CI steps that rejects the compromised eMudhra/emSign chains.
// Checkout and action downloads performed before this setup still use the
// runner's original trust configuration.

const fs = require('node:fs');
const path = require('node:path');
const { X509Certificate } = require('node:crypto');
const { execFileSync } = require('node:child_process');
// The compromised intermediates can chain to either of these roots, so remove
// both. SHA-256 fingerprints identify the exact certificates, regardless of name.
const blocked = new Set([
  // emSign ECC Root CA - G3
  '86:A1:EC:BA:08:9C:4A:8D:3B:BE:27:34:C6:12:BA:34:1D:81:3E:04:3C:F9:E8:A8:62:CD:5C:57:A3:6B:BE:6B',
  // AAA Certificate Services (cross-sign path)
  'D7:A7:A0:FB:5D:7E:27:31:D7:71:E9:48:4E:BC:DE:F7:1D:5F:0C:3E:0A:29:48:78:2B:C8:3E:E0:EA:69:9E:F4',
]);
const directory = path.join(process.env.RUNNER_TEMP, 'filtered-ca');
fs.mkdirSync(directory, { recursive: true });
// Node and command-line tools start with different root stores. Preserve the
// other trusted roots from both so they can share one filtered bundle.
const roots = [...require('node:tls').rootCertificates];
for (const file of ['/etc/ssl/cert.pem', '/etc/ssl/certs/ca-certificates.crt']) {
  if (fs.existsSync(file)) roots.push(...fs.readFileSync(file, 'utf8').match(/-----BEGIN CERTIFICATE-----[\s\S]*?-----END CERTIFICATE-----/g) || []);
}
// macOS also keeps system roots in the Keychain rather than just PEM files.
if (process.platform === 'darwin') {
  roots.push(...execFileSync('security', ['find-certificate', '-a', '-p', '/System/Library/Keychains/SystemRootCertificates.keychain'], { encoding: 'utf8' }).match(/-----BEGIN CERTIFICATE-----[\s\S]*?-----END CERTIFICATE-----/g) || []);
}
// Deduplicate the combined stores and exclude both blocked roots.
const certificates = [...new Set(roots)].filter(pem => {
  if (!blocked.has(new X509Certificate(pem).fingerprint256)) return true;
  // Native macOS clients use Keychain trust rather than this PEM bundle.
  // Explicitly deny the same roots there for the lifetime of this hosted VM.
  if (process.platform === 'darwin') {
    const file = path.join(directory, 'blocked.pem');
    fs.writeFileSync(file, pem);
    execFileSync('sudo', ['security', 'add-trusted-cert', '-d', '-r', 'deny', '-k', '/Library/Keychains/System.keychain', file]);
  }
  return false;
});
const bundle = path.join(directory, 'ca-bundle.pem');
fs.writeFileSync(bundle, certificates.join('\n') + '\n');
// Stop OpenSSL from loading blocked roots through its default CA directory.
const emptyDirectory = path.join(directory, 'empty');
fs.mkdirSync(emptyDirectory, { recursive: true });
// Export the bundle to later steps: OpenSSL/Python, curl, Git, and AWS CLI each
// recognize different variables. AWS CLI otherwise uses its own bundled roots.
// The workflow sets NODE_OPTIONS=--use-openssl-ca so action Node runtimes also
// honor SSL_CERT_FILE; GitHub forbids setting NODE_OPTIONS through GITHUB_ENV.
for (const variable of ['SSL_CERT_FILE', 'CURL_CA_BUNDLE', 'GIT_SSL_CAINFO', 'AWS_CA_BUNDLE']) {
  fs.appendFileSync(process.env.GITHUB_ENV, `${variable}=${bundle}\n`);
}
fs.appendFileSync(process.env.GITHUB_ENV, `SSL_CERT_DIR=${emptyDirectory}\n`);
console.log(`Configured ${certificates.length} trusted roots in ${bundle}`);
