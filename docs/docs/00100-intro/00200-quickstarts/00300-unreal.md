---
title: Unreal
slug: /quickstarts/unreal
id: quickstart-unreal
---

import { InstallCardLink } from "@site/src/components/InstallCardLink";

# Unreal Quickstart

Get a SpacetimeDB Unreal Engine game running in under 5 minutes.

## Prerequisites

- [Unreal Engine 5.3+](https://www.unrealengine.com/download) installed
- [SpacetimeDB CLI](https://spacetimedb.com/install) installed

<InstallCardLink />

## Create your project

```bash
spacetime dev --template unreal my-spacetime-game
```

This command:
1. Creates a new Unreal project with SpacetimeDB plugin
2. Creates a SpacetimeDB module (Rust or C#)
3. Starts the local SpacetimeDB server
4. Publishes your module
5. Generates C++ bindings for Unreal

## Open in Unreal

1. Open the `my-spacetime-game/client/MySpacetimeGame.uproject` file
2. The SpacetimeDB plugin will be automatically loaded

## Project structure

```
my-spacetime-game/
├── spacetimedb/          # Your SpacetimeDB module
│   └── src/
│       └── lib.rs        # Server-side logic
├── client/               # Unreal project
│   ├── Source/
│   │   └── MySpacetimeGame/
│   │       └── ModuleBindings/  # Auto-generated types
│   └── Plugins/
│       └── SpacetimeDB/
└── README.md
```

## Next steps

- Edit the module to add your game tables and reducers
- Use the generated bindings in your C++ or Blueprints
- See the [Unreal Tutorial](/docs/tutorials/unreal) for a complete multiplayer game example
- Read the [Unreal SDK Reference](/sdks/unreal) for detailed API docs
