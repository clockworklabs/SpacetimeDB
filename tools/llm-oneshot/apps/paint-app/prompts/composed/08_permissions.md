# Paint App - Permissions

Create a **real-time collaborative drawing app**.

**See `language/*.md` for language-specific setup, architecture, and constraints.**

## UI Requirements

Use SpacetimeDB brand styling (dark theme).

## Features

### Basic Drawing

* Users can set a display name and pick an avatar color
* Users can create canvases and join/leave them
* Basic drawing tools: freehand brush, eraser (visually removes strokes), color picker
* Adjustable brush size
* Clear canvas option (with confirmation)
* Real-time sync - see other users' strokes appear as they draw
* Strokes should remain visible during drawing (no flicker from polling)

### Live Cursors

* Show all collaborators' cursor positions in real-time
* Each cursor displays the user's name and avatar color
* Cursor icon reflects their current tool (brush, eraser, select, etc.)
* Show a small preview of their selected color next to the cursor
* Cursors smoothly animate and fade out when users go inactive

### Shapes

* Shape tools: rectangle, ellipse, line, arrow
* Shapes have stroke color, fill color, and stroke width options
* **Drag to draw shapes** with live preview while dragging
* Hold shift for perfect squares/circles
* Shapes are elements that can be selected and modified after creation

### Selection & Manipulation

* Selection tool (V) to click and select elements
* **Drag to move** selected elements
* **Drag corner/edge handles to resize** - 8 handles (4 corners + 4 edges)
* **Delete/Backspace** to delete selected elements
* **Escape** to deselect
* **See what others have selected** - other users' selections shown with their avatar color outline
* When someone is actively dragging an element, others see it move in real-time
* Multi-select with shift-click

### Layers with Locking

* Canvas supports multiple named layers
* Reorder layers by dragging
* Toggle layer visibility (eye icon)
* Adjust layer opacity
* **Layer locking** - click to lock a layer for editing; shows "Locked by [username]"
* Only the person who locked can edit that layer; others see a lock icon
* Auto-unlock when user leaves or after 5 minutes of inactivity

### Presence & Activity Status

* Show list of users currently on the canvas with their avatar colors
* Display each user's status: **active** (drawing), **idle** (not interacting), **away** (tab hidden)
* Show what tool each user has selected
* "X users viewing" count in the header
* Status updates automatically based on user activity
* Auto-set to "away" after 2 minutes of no interaction

### Comments & Feedback

* Drop comment pins anywhere on the canvas
* Comments anchor to canvas coordinates (move with pan/zoom)
* Threaded replies on each comment
* Mark comments as resolved (grays out but keeps visible)
* Show unresolved comment count in header
* Click comment in list to pan canvas to that location
* Comments sync in real-time to all collaborators

### Version History

* Auto-save a snapshot every 5 minutes while canvas is being edited
* Manual "Save Version" button with optional name/description
* Version history panel showing all snapshots with timestamps
* Preview any version (read-only view)
* Restore any version (creates a new "Restored from..." version)
* Show who triggered each save (auto-save vs manual + username)

### Permissions

* Canvas creator is the owner
* Owner can set collaborator roles: **viewer** (can only watch) or **editor** (can draw)
* Role changes take effect **instantly** - if you're demoted to viewer mid-stroke, it cancels
* Owner can remove users from the canvas (they're kicked immediately)
* Viewers see a "View Only" badge and all tools are disabled
* Editors can't change permissions or remove others
