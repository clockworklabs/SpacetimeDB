# Paint App - SpacetimeDB Rust

A real-time collaborative drawing application built with SpacetimeDB (Rust backend) and a Rust/axum web client.

## Features

### Basic Drawing
- Set display name and avatar color
- Create, join, and leave canvases
- Freehand brush and eraser tools
- Color picker for stroke and fill
- Adjustable brush size
- Clear canvas with confirmation
- Real-time sync across all users

### Live Cursors
- See collaborators' cursor positions in real-time
- Each cursor displays user's name and avatar color
- Cursor icon reflects current tool
- Smooth cursor animations

### Shapes
- Rectangle, ellipse, line, and arrow tools
- Stroke color, fill color, and stroke width options
- Hold Shift for perfect squares/circles
- Shapes can be selected and modified

### Selection & Collaborative Awareness
- Selection tool to click elements
- Move, resize, and delete selected elements
- See what others have selected (colored outline)
- Real-time element movement sync

### Layers with Locking
- Multiple named layers per canvas
- Reorder layers by drag
- Toggle layer visibility (eye icon)
- Adjust layer opacity
- Layer locking - shows "Locked by [username]"
- Auto-unlock after 5 minutes of inactivity

### Presence & Activity Status
- User list with avatar colors
- Status indicators: active, idle, away
- Tool selection display per user
- "X users viewing" count
- Auto-away after 2 minutes of inactivity

### Comments & Feedback
- Drop comment pins on canvas
- Threaded replies
- Mark comments as resolved
- Click comment to pan to location

### Version History
- Auto-save snapshots every 5 minutes
- Manual "Save Version" with optional name
- Preview and restore any version
- Shows who triggered each save

### Permissions
- Canvas creator is owner
- Viewer and editor roles
- Instant role changes
- Remove users from canvas
- View-only badge for viewers

### Follow Mode
- Click user to follow their viewport
- Real-time viewport sync
- "Following [username]" indicator
- Unfollow on manual pan/zoom

### Activity Feed
- Real-time activity log
- Click entry to pan to location
- Last 100 entries kept

### Sharing & Private Canvases
- Private by default
- Generate share links
- Set link permissions (view/edit)
- Invite by username
- Revoke share links

### Canvas Chat
- Real-time chat per canvas
- Typing indicators
- Persistent message history
- Notification badge

### Auto-Cleanup & Notifications
- 30-day inactivity cleanup
- 7-day warning before deletion
- "Keep Forever" option
- Notification center

### Text & Sticky Notes
- Text tool with font sizes
- Sticky note tool
- Double-click to edit
- Selectable/movable like shapes

## Project Structure

```
paint-app-20260112-150000/
├── backend/                    # SpacetimeDB Rust module
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs              # Tables, reducers, lifecycle hooks
├── client/                     # Rust axum web client
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs             # Axum server, API routes
│       ├── index.html          # Embedded web UI
│       └── module_bindings/    # Generated (spacetime generate)
│           └── mod.rs
└── README.md
```

## Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [SpacetimeDB CLI](https://spacetimedb.com/install)

## Deployment

### 1. Start SpacetimeDB Server

```bash
spacetime start
```

### 2. Publish the Backend Module

```bash
cd backend
spacetime publish paint-app --project-path .
```

To clear the database and republish:
```bash
spacetime publish paint-app --clear-database -y --project-path .
```

### 3. Generate Client Bindings

```bash
spacetime generate --lang rust --out-dir ../client/src/module_bindings --project-path .
```

### 4. Build and Run the Client

```bash
cd ../client
cargo run
```

The client will:
1. Connect to SpacetimeDB at `http://localhost:3000`
2. Start an axum web server at `http://localhost:8080`
3. Auto-open your browser

## Configuration

### Backend Module Name
Edit `client/src/main.rs`:
```rust
const MODULE_NAME: &str = "paint-app";
```

### SpacetimeDB URI
```rust
const SPACETIMEDB_URI: &str = "http://localhost:3000";
```

### Client Port
```rust
const CLIENT_PORT: u16 = 8080;
```

## UI Controls

### Keyboard Shortcuts
- `B` - Brush tool
- `E` - Eraser tool
- `V` - Select tool
- `R` - Rectangle
- `O` - Ellipse
- `L` - Line
- `A` - Arrow
- `T` - Text
- `S` - Sticky note
- `C` - Comment

### Drawing
- Click and drag to draw/create shapes
- Hold Shift for perfect squares/circles
- Use color pickers for stroke and fill
- Adjust brush size with slider

### Right Sidebar Tabs
- **Users** - Online collaborators
- **Chat** - Canvas-specific chat
- **Activity** - Recent actions
- **Comments** - Comment threads
- **Versions** - Version history

## Architecture

### Backend (SpacetimeDB)
- **Tables**: User, Canvas, CanvasMember, Layer, Element, Stroke, UserSelection, Comment, Version, ChatMessage, TypingIndicator, ActivityEntry, Notification
- **Scheduled Tables**: ScheduledCleanup, ScheduledAutoSave, ScheduledInactivity, ScheduledLayerUnlock
- **Reducers**: 40+ reducers for all user actions
- **Lifecycle Hooks**: client_connected, client_disconnected

### Client (Rust + axum)
- **axum** - HTTP server for web UI
- **spacetimedb-sdk** - SpacetimeDB connection
- **Embedded HTML/CSS/JS** - Single-file web app
- **Polling** - 500ms data refresh rate

## SpacetimeDB Brand Colors

The UI uses official SpacetimeDB brand colors:
- Primary accent: `#4cf490` (STDB Green)
- Background: `#060606`
- Surface: `#141416`
- Border: `#202126`
- Text: `#f4f6fc`

## View Server Logs

```bash
spacetime logs paint-app
```

## Troubleshooting

### Connection Issues
1. Ensure `spacetime start` is running
2. Check if module is published: `spacetime sql paint-app "SELECT * FROM user"`
3. Verify URI in client matches server

### Binding Generation Fails
1. Ensure backend compiles: `cd backend && cargo check`
2. Check SpacetimeDB CLI version matches crate versions

### Client Won't Compile
1. Regenerate bindings after backend changes
2. Check for Rust compiler errors in module_bindings

## License

MIT
