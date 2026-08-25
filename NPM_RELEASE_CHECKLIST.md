# Publishing the Submodules to npm

These packages publish under the public `@spacetimedb` scope. Publishing is
manual until a dedicated release workflow and npm trusted publisher are
configured. The packages require the released SpacetimeDB 2.8 submodule APIs.

## Prerequisites

1. Install Node.js 22 or later and a current npm CLI.
2. Confirm you have write access to the `@spacetimedb` npm organization.
3. Enable two-factor authentication on the npm account.
4. Authenticate and verify the registry account:

   ```bash
   npm login
   npm whoami
   npm config get registry
   ```

   The registry must be `https://registry.npmjs.org/`.

5. Install and select the exact CLI used by the release gates:

   ```bash
   spacetime version install 2.8.3
   spacetime version use 2.8.3
   spacetime --version
   npm view spacetimedb@2.8.3 version
   ```

   Both the CLI tool and embedded library must report `2.8.3`. Package
   development dependencies resolve the SDK from this pnpm workspace.

## Release gates

Run from the repository root:

```bash
pnpm install --frozen-lockfile
pnpm components:check
pnpm components:build
pnpm components:consumer:check
pnpm --dir spacetime-stripe-ts run test:smoke
pnpm --dir spacetime-resend-ts run test:smoke
```

The release check validates metadata, exports, README structure, publishable
dependency ranges, lifecycle boundaries, forbidden Node-only imports, and the
contents of every npm tarball. The package checks run TypeScript validation and
all non-credentialed unit suites.

The build gate first verifies the released 2.8.3 CLI, then compiles 22 server
fixtures and regenerates and bundles all 12 browser examples. The Stripe and
Resend smoke suites publish dedicated local databases and use synthetic signed
webhooks, so they require a running local SpacetimeDB server but no provider
credentials. Stripe's `test:stripe:e2e` suite remains opt-in because it uses a
real Stripe sandbox and Stripe CLI session.

Before publishing, also confirm:

- The worktree contains only intended release changes.
- The commit to release is on the protected default branch.
- Every changed package has the intended version.
- `CHANGELOG` or release notes describe user-visible changes.
- No `.env`, credential, log, generated binding, example build, or
  `node_modules` file appears in `pnpm pack --json`.

## Versioning

For the first publication, use the reviewed version already recorded in the
manifest. npm never permits overwriting an existing name and version.

Check a package before choosing a version:

```bash
npm view @spacetimedb/crypto version
```

An npm `E404` means the package name has not been published. For an existing
package, update the version and defer Git tagging:

```bash
cd spacetime-crypto-ts
npm version patch --no-git-tag-version
```

Use `minor` or `major` when the change warrants it. If an internal dependency
receives a version outside a consumer's current range, update the consumer
manifest before publishing.

## Publish order

Publish dependency foundations before their consumers:

1. `@spacetimedb/crypto`
2. `@spacetimedb/rate-limit`
3. Packages with no unpublished internal dependency: `agents`, `cron`,
   `grid`, `lobby`, `posthog`, `presence`, and `retry`
4. `@spacetimedb/api-keys`, `@spacetimedb/files`, `@spacetimedb/auth`,
   `@spacetimedb/resend`, and `@spacetimedb/stripe`

Independent packages within steps 1-3 may be released in
any order. Wait for each foundation version to become visible through
`npm view` before publishing its consumers.

## Dry run and publish

Run these commands from the package directory. The explicit access flag is
important for the first publication of an organization-scoped public package.

```bash
pnpm pack --json
pnpm publish --dry-run --access public
pnpm publish --access public
```

Interactive publication requires 2FA. Do not pass an access token on the
command line or store it in the repository. npm also supports staged
publication (`npm stage publish`) when a separate 2FA approval step is desired.

Immediately verify the published package:

```bash
npm view @spacetimedb/crypto@0.1.0 --json
npm install --ignore-scripts @spacetimedb/crypto@0.1.0
```

For packages with `./submodule`, verify the installed package contains that
export and run a clean consumer typecheck before continuing to the next package.

## After publication

1. Tag the release commit using the repository's chosen tag convention.
2. Create release notes that list every published package and version.
3. Submit eligible packages to the SpacetimeDB submodule registry.
4. Configure npm trusted publishing for a future release workflow. Use Node
   22.14.0 or newer and npm 11.5.1 or newer in the publish job. Trusted
   publishing uses short-lived OIDC credentials and automatically records npm
   provenance for eligible public packages; the workflow needs
   `id-token: write`. New trusted-publisher configurations must explicitly
   allow `npm publish`, `npm stage publish`, or both.

References:

- [Publishing scoped public packages](https://docs.npmjs.com/creating-and-publishing-scoped-public-packages/)
- [npm two-factor authentication](https://docs.npmjs.com/about-two-factor-authentication/)
- [Trusted publishing](https://docs.npmjs.com/trusted-publishers/)
- [npm provenance](https://docs.npmjs.com/generating-provenance-statements/)
