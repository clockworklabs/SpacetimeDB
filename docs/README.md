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

## License

This documentation repository is licensed under Apache 2.0. See LICENSE.txt for more details.
