## SpacetimeDB Documentation

This repository contains the markdown files which are used to display documentation on our [website](https://spacetimedb.com/docs).

### Making Edits

To make changes to our docs, you can open a pull request in this repository. You can typically edit the files directly using the GitHub web interface, but you can also clone our repository and make your edits locally. To do this you can follow these instructions:

1. Fork our repository
2. Clone your fork:

```bash
git clone ssh://git@github.com/<username>/spacetime-docs
```

3. Make your edits to the docs that you want to make + test them locally
4. Commit your changes:

```bash
git add .
git commit -m "A specific description of the changes I made and why"
```

5. Push your changes to your fork as a branch

```bash
git checkout -b a-branch-name-that-describes-my-change
git push -u origin a-branch-name-that-describes-my-change
```

6. Go to our GitHub and open a PR that references your branch in your fork on your GitHub

> NOTE! If you make a change to `nav.ts` you will have to run `npm run build` to generate a new `docs/nav.js` file.

#### CLI Reference Section
1. Make sure that https://github.com/clockworklabs/SpacetimeDB/pull/2276 is included in your `spacetimedb-cli` binary
1. Run `cargo run --features markdown-docs -p spacetimedb-cli > cli-reference.md`

We currently don't properly render markdown backticks and bolding that are inside of headers, so do these two manual replacements to make them look okay (these have only been tested on Linux):
```bash
sed -i'' -E 's!^(##) `(.*)`$!\1 \2!' docs/cli-reference.md
sed -i'' -E 's!^(######) \*\*(.*)\*\*$!\1 <b>\2</b>!' docs/cli-reference.md
```

### Checking Links

We have a CI job which validates internal links. You can run it locally with `npm run check-links`. This will print any internal links (i.e. links to other docs pages) whose targets do not exist, including fragment links (i.e. `#`-ey links to anchors).

## License

This documentation repository is licensed under Apache 2.0. See LICENSE.txt for more details.
