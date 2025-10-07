# create-spacetime

## Overview

This package contains the official CLI tool for creating SpacetimeDB projects. The CLI allows you to quickly scaffold new SpacetimeDB applications with a template of your choice (React frontends with Rust or C# backends).

## Installation

To create a new SpacetimeDB project:

```bash
npm create spacetime@latest
```

## Usage

To create a project you can run the interactive setup or specify options directly.

```bash
npm create spacetime@latest my-spacetime-app -- -t rust
```

Skip interactive prompts with `-y` (defaults server language to Rust):

```bash
npm create spacetime@latest my-spacetime-app -- -y
```

The `-t` flag selects your server language template.

Available templates:

- `rust` - Rust server with React client
- `csharp` - C# server with React client

Create in your current directory using `.` as the project name:

```sh
npm create spacetime@latest .
```

Use a local SpacetimeDB server instead of Maincloud:

```bash
npm create spacetime@latest my-spacetime-app -- --local
```

## What You Get

Creates a full-stack SpacetimeDB app with server module and React frontend, including build scripts and example chat app.

## Requirements

- Node.js 18+ and npm 8+
- SpacetimeDB CLI (optional for creation, required for Maincloud deployment) - [spacetimedb.com/install](https://spacetimedb.com/install)

## Running The Project

After project creation:

```bash
cd my-spacetime-app
npm run dev
```

Your React app will be available at `http://localhost:5173`.

To deploy your SpacetimeDB module:

```bash
npm run local    # Local deployment
npm run deploy   # Deploy to Maincloud
```

## Learn More

- [SpacetimeDB Documentation](https://spacetimedb.com/docs)
