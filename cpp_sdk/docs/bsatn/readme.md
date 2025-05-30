# bsatn spec

This directory contains the specification for the bsatn format. The grammar of the spec is roughly based on that of the spec for the WebAssembly binary format: https://webassembly.github.io/spec/core/binary/conventions.html

## Building

Dependencies:

- Katex, from npm: `npm i -g katex` or `yarn global add katex`
- wkhtmltopdf for building a pdf (not required, looks better viewed directly in browser)

```sh
make spec.html
```
