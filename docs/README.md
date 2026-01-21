# SpacetimeDB Documentation

This repository contains the markdown files which are used to display
documentation on our [website](https://spacetimedb.com/docs).
This documentation is built using [Docusaurus](https://docusaurus.io/).

## Making Edits

To make changes to our docs, you can open a pull request in this repository.
You can typically edit the files directly using the GitHub web interface, but
you can also clone our repository and make your edits locally.

### Instructions

1. Fork our repository
2. Clone your fork:

```bash
git clone ssh://git@github.com/<username>/SpacetimeDB
cd SpacetimeDB/docs
```

3. Make your edits to the docs that you want to make + test them locally (See [Testing Locally](#testing-locally))
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

6. Go to our GitHub and open a PR that references your branch in your fork on
   your GitHub

### CLI Reference Section

To regenerate the CLI reference section, run `pnpm generate-cli-docs`.

### Docusaurus Documentation

For more information on how to use Docusaurus, see the
[Docusaurus documentation](https://docusaurus.io/docs).

### Testing Locally

#### Installation

1. Make sure you have [Node.js](https://nodejs.org/) installed
   (version 22 or higher is recommended).
2. Clone the repository and navigate to the `docs` directory.
3. Install the dependencies: `pnpm install`
4. Run the development server: `pnpm dev`, which will start a local server and open a browser window.
   All changes you make to the markdown files will be reflected live in the browser.

### Adding new pages

All of our directory and file names are prefixed with a five-digit number which determines how they're sorted.
We started with the hundreds place as the smallest significant digit, to allow using the tens and ones places to add new pages between.
When adding a new page in between two existing pages, choose a number which:

- Doesn't use any more significant figures than it needs to.
- Is approximately halfway between the previous and next page.

For example, if you want to add a new page between `00300-foo` and `00400-bar`, name it `00350-baz`. To add a new page between `00350-baz` and `00400-bar`, prefer `00370-quux` or `00380-quux`, rather than `00375-quux`, to avoid populating the ones place.

To add a new page after all previous pages, use the smallest multiple of 100 larger than all other pages. For example, if the highest-numbered existing page is `01350-abc`, create `01400-def`.

### Best practices

- Use relative links for linking between documentation pages, as this will ensure
  that links work correctly with versioning and localization.

## License

This documentation repository is licensed under Apache 2.0.
See LICENSE.txt for more details
