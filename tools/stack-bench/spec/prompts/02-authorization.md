# Level 2 — Authorization

Adds ownership and access control to the app from level 1. Everything from level 1 must
keep working.

## Features

### Private rooms

- A room creator can mark a room **private** at creation time
- A private room's messages are readable only by its members. A non-member must not be able
  to read them by any means the app offers — not by entering the room, not by any link, and
  not through any request the client can make.
- Public rooms remain readable by any signed-in user

### Membership

- A room creator is its **owner**
- An owner can **invite** another user to a private room; the invitee gains access
- An owner can **remove** a member. Removal takes effect immediately: the removed user stops
  receiving that room's messages without reloading, and loses read access to its history.
- A removed user can be re-invited

### Permissions

- Only an owner sees owner controls (invite, remove, delete room)
- A non-owner must not be able to perform an owner action, including by issuing the request
  directly rather than through the UI
- Only the author of a message can delete it; a deleted message disappears live for everyone
- Attempting an unpermitted action shows a visible error and changes nothing
