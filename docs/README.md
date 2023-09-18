# Spacetime Docs CLI

## How to use:

1. run `yarn install`
2. run `yarn build`
3. run `npm i -g .`
4. run `spacetime-docs -h` in your terminal

## Specify Docs Out Directory

Create a `spacetime-docs.json` file in your project root and specify the `docPath` property.

```json
{
  "docPath": "./docs"
}
```

## Specify The Sidebar Order

In the `spacetime-docs.json` file in your project root add:

> This will respect the order when generating the docs.

```json
    "order": [
        "Overview",
        "Getting Started",
        "Cloud Testnet",
        "Unity Tutorial",
        "Server Module Languages",
        "Client SDK Languages",
        "Module ABI Reference",
        "HTTP API Reference",
        "WebScoket API Reference",
        "SATN Reference",
        "SQL Reference"
    ]
```

## Add tags

Tags will show up next to the section title in the sidebar. In the `_category.json` file for a section add:

```json
tag: "New" // Or anything else...
```
