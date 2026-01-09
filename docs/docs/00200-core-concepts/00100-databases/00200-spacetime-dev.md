---
title: "spacetime dev"
slug: /databases/developing
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# `spacetime dev`

This guide covers how to create a new SpacetimeDB database module project.

## Prerequisites

First, [install the SpacetimeDB CLI](https://spacetimedb.com/install).

## Interactive Development with `spacetime dev`

The fastest way to get started developing a database module is with `spacetime dev`. This interactive command guides you through creating a new SpacetimeDB project with hot-reloading - whenever you save changes to your module code, SpacetimeDB automatically rebuilds and republishes your module.

:::caution
`spacetime dev` is currently an unstable command and may change in the future.
:::

### Getting Started

Run the command from your terminal:

```bash
spacetime dev
```

**First Time Setup**

If no SpacetimeDB project is found in the current directory, you'll be guided through creating a new one:

**Step 1: Project Name**
Enter a name for your project (e.g., `my-project`)

**Step 2: Project Path**
Choose where to create the project files (defaults to `./<project-name>`)

**Step 3: Select a Client Type**
Choose how you want to develop:

- **React** - React web app with TypeScript server (recommended for web apps)
- **Use Template** - Choose from built-in templates or clone from GitHub
- **None** - Server module only

**Existing Projects**

If you run `spacetime dev` in a directory with an existing SpacetimeDB project (containing a `spacetimedb/` directory), it will skip setup and enter development mode directly, connecting to your database and watching for file changes.

### Client Type Options

#### React

Creates a full-stack React web application with:

- TypeScript server module
- React frontend with SpacetimeDB client SDK
- Pre-configured hot-reloading for both client and server

#### Use Template
Choose from several built-in templates:

- `basic-typescript` - Basic TypeScript client and server stubs
- `basic-c-sharp` - Basic C# client and server stubs
- `basic-rust` - Basic Rust client and server stubs
- `basic-react` - React web app with TypeScript server
- `quickstart-chat-rust` - Complete Rust chat implementation
- `quickstart-chat-c-sharp` - Complete C# chat implementation
- `quickstart-chat-typescript` - Complete TypeScript chat implementation

You can also clone an existing project by entering a GitHub repository (`owner/repo`) or git URL.

#### None

Creates a server module only, without any client code. You'll choose your server language:

- **TypeScript** - Server module in TypeScript
- **Rust** - Server module in Rust
- **C#** - Server module in C#

The server code will be created in a `spacetimedb/` subdirectory within your project.

### What Happens Next

After completing setup, `spacetime dev`:

- Starts a local SpacetimeDB server
- Creates a new database
- Builds and publishes your module to the database
- Watches your source files for changes
- Automatically rebuilds and republishes when you save changes

Your database will be available at `https://maincloud.spacetimedb.com`.

### Project Structure

After initialization, your project will contain:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```text
my-project/
├── spacetimedb/            # Server module code
│   ├── package.json
│   ├── tsconfig.json
│   └── src/
│       └── index.ts
├── src/                    # Client code
│   └── module_bindings/    # Generated client bindings
├── package.json
├── tsconfig.json
└── README.md
```

</TabItem>
<TabItem value="csharp" label="C#">

```text
my-project/
├── spacetimedb/            # Server module code
│   ├── StdbModule.csproj
│   └── Lib.cs
├── module_bindings/        # Generated client bindings
├── client.csproj
├── Program.cs
└── README.md
```

</TabItem>
<TabItem value="rust" label="Rust">

```text
my-project/
├── spacetimedb/            # Server module code
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs
├── src/                    # Client code
│   └── module_bindings/    # Generated client bindings
├── Cargo.toml
├── .gitignore
└── README.md
```

</TabItem>
</Tabs>

## Alternative: Manual Project Creation

If you prefer more control over the development workflow, you can create a database module project manually and use the standard build and publish workflow.

### Create a New Project with `spacetime init`

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```bash
spacetime init --lang typescript --project-path ./my-project my-project
cd my-project
```

This creates a new TypeScript project with:

- A `package.json` configured for SpacetimeDB
- A `src/index.ts` with a sample module
- Sample table and reducer definitions

</TabItem>
<TabItem value="csharp" label="C#">

```bash
spacetime init --lang csharp --project-path ./my-project my-project
cd my-project
```

This creates a new C# project with:

- A `StdbModule.csproj` configured for SpacetimeDB
- A `Lib.cs` with a sample module
- Sample table and reducer definitions

</TabItem>
<TabItem value="rust" label="Rust">

```bash
spacetime init --lang rust --project-path ./my-project my-project
cd my-project
```

This creates a new Rust project with:

- A `Cargo.toml` configured for SpacetimeDB
- A `src/lib.rs` with a sample module
- Sample table and reducer definitions

</TabItem>
</Tabs>

## Next Steps

After creating your database module:

- Learn about [Tables](/tables), [Reducers](/functions/reducers), and [Procedures](/functions/procedures)
- [Build and publish your module](/databases/building-publishing)
