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

## License

This documentation repository is licensed under Apache 2.0.
See LICENSE.txt for more details
