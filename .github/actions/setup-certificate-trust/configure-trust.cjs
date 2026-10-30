const fs = require('node:fs');
const path = require('node:path');
const { X509Certificate } = require('node:crypto');
const { execFileSync } = require('node:child_process');
const blocked = new Set([
  // emSign ECC Root CA - G3
  '86:A1:EC:BA:08:9C:4A:8D:3B:BE:27:34:C6:12:BA:34:1D:81:3E:04:3C:F9:E8:A8:62:CD:5C:57:A3:6B:BE:6B',
  // AAA Certificate Services (cross-sign path)
  'D7:A7:A0:FB:5D:7E:27:31:D7:71:E9:48:4E:BC:DE:F7:1D:5F:0C:3E:0A:29:48:78:2B:C8:3E:E0:EA:69:9E:F4',
]);
const directory = path.join(process.env.RUNNER_TEMP, 'filtered-ca');
fs.mkdirSync(directory, { recursive: true });
// Preserve Node and OS trust anchors for existing HTTPS endpoints.
const roots = [...require('node:tls').rootCertificates];
for (const file of ['/etc/ssl/cert.pem', '/etc/ssl/certs/ca-certificates.crt']) {
  if (fs.existsSync(file)) roots.push(...fs.readFileSync(file, 'utf8').match(/-----BEGIN CERTIFICATE-----[\s\S]*?-----END CERTIFICATE-----/g) || []);
}
if (process.platform === 'darwin') {
  roots.push(...execFileSync('security', ['find-certificate', '-a', '-p', '/System/Library/Keychains/SystemRootCertificates.keychain'], { encoding: 'utf8' }).match(/-----BEGIN CERTIFICATE-----[\s\S]*?-----END CERTIFICATE-----/g) || []);
}
const certificates = [...new Set(roots)].filter(pem => {
  if (!blocked.has(new X509Certificate(pem).fingerprint256)) return true;
  // Native macOS TLS clients also need an explicit Keychain distrust entry.
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
// AWS CLI uses its own CA bundle unless AWS_CA_BUNDLE overrides it.
for (const variable of ['SSL_CERT_FILE', 'CURL_CA_BUNDLE', 'GIT_SSL_CAINFO', 'AWS_CA_BUNDLE']) {
  fs.appendFileSync(process.env.GITHUB_ENV, `${variable}=${bundle}\n`);
}
fs.appendFileSync(process.env.GITHUB_ENV, `SSL_CERT_DIR=${emptyDirectory}\n`);
console.log(`Configured ${certificates.length} trusted roots in ${bundle}`);
