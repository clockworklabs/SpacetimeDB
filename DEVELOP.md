## Enforce minimum npm package age
Please run this before doing TypeScript work or running tests in our repo:
```bash
pnpm config set --global minimumReleaseAge 1440
```
This will close most of the surface area for npm supply chain attacks.

## Install Git hooks
Please run:
```bash
git-hooks/install-hooks.sh
```
**Note that this removes everything in `.git/hooks`, and doesn't work if you're in a submodule**.

This will install our git hooks, for instance running formatting on commit.
