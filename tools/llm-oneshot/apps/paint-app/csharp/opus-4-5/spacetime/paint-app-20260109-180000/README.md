# SpacetimeDB Paint App (C#)

A real-time collaborative paint/drawing application built with SpacetimeDB for the backend and .NET MAUI for the Windows client.

## Features

### Basic Drawing
- ✅ Users can set display names
- ✅ Create and join canvases
- ✅ Pencil/brush tool with adjustable size and opacity
- ✅ Eraser tool
- ✅ Color picker for stroke and fill colors
- ✅ Real-time sync of strokes

### Cursor Presence
- ✅ Real-time cursor positions from other users
- ✅ Each cursor shows user's name and selected tool
- ✅ Distinct colors per user

### Shapes & Fill
- ✅ Rectangle, ellipse, and line shape tools
- ✅ Fill tool for enclosed areas
- ✅ Stroke and fill color support

### Selection & Transform
- ✅ Selection tool
- ✅ Move, resize, and rotate elements
- ✅ Delete selected elements

### Undo/Redo
- ✅ Per-user undo/redo stack
- ✅ Keyboard shortcuts (Ctrl+Z, Ctrl+Y)

### Layers
- ✅ Multiple layers per canvas
- ✅ Create, rename, and delete layers
- ✅ Toggle layer visibility
- ✅ Layer opacity control
- ✅ Real-time layer sync

### Comments
- ✅ Comment pins at specific locations
- ✅ Comment threads with replies
- ✅ Resolve/unresolve comments
- ✅ Real-time comment sync

### Version History
- ✅ Manual save points with names
- ✅ Auto-save at regular intervals
- ✅ View saved versions list
- ✅ Restore previous versions

### Permissions
- ✅ Canvas creator can set user roles (viewer/editor)
- ✅ Viewers cannot draw
- ✅ Permission changes take effect immediately
- ✅ Kick users from canvas

### Text Tool
- ✅ Add text anywhere on canvas
- ✅ Font size and color selection

### Zoom & Pan
- ✅ Zoom in/out with buttons
- ✅ Fit-to-screen button
- ✅ Viewport tracking for follow mode

### Private Canvases
- ✅ Create private canvases
- ✅ Invite collaborators by identity
- ✅ Accept/decline invitations

### Image Import
- ✅ Import images as canvas elements
- ✅ Position and resize imported images

### Templates
- ✅ Save canvas as template
- ✅ Create canvas from template
- ✅ Public and private templates

## Prerequisites

- **.NET 8 SDK** - Required for SpacetimeDB WASM module (the wasi-wasm target)
- **.NET 9 SDK** - For the MAUI client (or update client to target net8.0)
- **SpacetimeDB CLI** - For publishing the module
- **Windows 10/11** - For running the MAUI client

## Project Structure

```
paint-app-20260109-180000/
├── backend/           # SpacetimeDB C# module
│   ├── backend.csproj
│   ├── Tables.cs      # Database table definitions
│   └── Reducers.cs    # All reducer implementations
└── client/            # .NET MAUI Windows client
    ├── client.csproj
    ├── MauiProgram.cs
    ├── App.xaml(.cs)
    ├── MainPage.xaml(.cs)
    └── Platforms/Windows/
```

## Deployment Status

✅ **SpacetimeDB Server**: Running on `http://127.0.0.1:3000`  
✅ **Backend Module**: Published as `paint-app`  
✅ **Client Bindings**: Generated in `client/module_bindings/`  
✅ **Client Code**: Compiles successfully (C# code is error-free)  
⚠️ **Client Build**: Requires Visual Studio 2022 with UWP workload for full build

## Prerequisites

- **.NET 8 SDK** - For SpacetimeDB WASM module (wasi-wasm target)
- **.NET 9 SDK** - For the MAUI client
- **SpacetimeDB CLI** - For publishing the module
- **Visual Studio 2022** - Required for building MAUI Windows app (UWP workload needed for resource packaging)
- **Windows 10/11** - For running the MAUI client

## Building and Running

### 1. Start SpacetimeDB Server

```bash
spacetime start
```

### 2. Build and Publish Backend Module (via Docker)

The backend requires .NET 8 with `wasi-experimental` workload. Using Docker:

```bash
cd backend
docker build -t paint-app-builder .
docker run --rm -v "${PWD}:/host" paint-app-builder cp /output/dotnet.wasm /host/backend.wasm
spacetime publish paint-app --bin-path backend.wasm
```

Or if you have .NET 8 with wasi-experimental workload installed locally:

```bash
cd backend
dotnet publish -c Release
spacetime publish paint-app --bin-path bin/Release/net8.0/wasi-wasm/AppBundle/dotnet.wasm
```

### 3. Generate Client Bindings

```bash
spacetime generate --lang csharp --out-dir ../client/module_bindings --bin-path backend.wasm
```

### 4. Build and Run Client

**IMPORTANT**: The MAUI Windows client requires Visual Studio 2022 for full build.

**Option A: Using Visual Studio 2022** (Recommended)
1. Open `client/client.csproj` in Visual Studio 2022
2. Build and run from the IDE

**Option B: Using VS Developer Command Prompt**
```bash
cd client
devenv client.csproj /build Debug
```

**Option C: Using `dotnet` CLI** (may fail on resource packaging)
```bash
cd client
dotnet restore
dotnet build -f net8.0-windows10.0.19041.0
```

Note: The `dotnet` CLI build may fail due to Windows App SDK's PRI resource generation requiring Visual Studio's MSBuild tools. The C# code compiles successfully - only the resource packaging step fails.

## UI Theme

The app uses a **SpacetimeDB cosmic theme**:
- Deep space background (`#0a0a0f`)
- Purple accents (`#6366f1`)
- Cyan accents (`#22d3ee`)
- Subtle glow effects on interactive elements

## SpacetimeDB Tables

| Table | Description |
|-------|-------------|
| `user` | User profiles and online status |
| `canvas` | Canvas metadata |
| `canvas_member` | Canvas membership and roles |
| `layer` | Canvas layers |
| `stroke` | Brush/eraser strokes |
| `shape` | Rectangle, ellipse, line shapes |
| `text_element` | Text on canvas |
| `image_element` | Imported images |
| `fill` | Fill tool operations |
| `cursor` | Real-time cursor positions |
| `selection` | User selections |
| `undo_action` | Undo/redo history |
| `comment` | Canvas comments |
| `comment_reply` | Comment replies |
| `canvas_version` | Version snapshots |
| `template` | Canvas templates |
| `invitation` | Private canvas invitations |
| `viewport` | User viewport for follow mode |
| `autosave_job` | Scheduled auto-save tasks |

## License

MIT
