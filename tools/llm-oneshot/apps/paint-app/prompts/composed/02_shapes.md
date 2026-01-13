# Paint App - Shapes

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
