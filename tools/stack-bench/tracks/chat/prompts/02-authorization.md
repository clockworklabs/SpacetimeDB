# Level 2 — Authorization and People

Adds ownership, access control, and a social layer to the app from level 1.
Everything from level 1 must keep working.

## Features

### Private rooms

- A room creator can mark a room **private** at creation time
- A private room's messages are readable only by its members. A non-member must not be able
  to read them by any means the app offers — not by entering the room, not by any link, and
  not through any request the client can make.
- A private room does not appear in a non-member's room list at all — its existence and its
  name are members-only
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

### Profiles

- Every account has a **profile**: display name, a short bio, and a joined-date.
  The owner can edit their display name and bio.
- Clicking a username anywhere it appears (a message, the member list) opens that
  person's **profile panel**
- A profile panel is **live for as long as it is open**: if the person edits their
  bio while someone else is looking at their profile, the viewer sees the new bio
  without reopening the panel or reloading
- The panel can be closed; a closed panel is gone — reopening it shows current data

### Friends

- A signed-in user can **send a friend request** from a profile panel
- The recipient sees the pending request and can **accept** or **decline**; the
  sender sees which of these happened
- Each user has a **friends list** showing their friends and each friend's
  **online/offline presence**
- The friends list is live: a request accepted while the list is open appears in
  it without a reload, and a friend signing in or out flips their presence for
  everyone whose list shows them
- Either side can **unfriend**; the row disappears live from both lists
- Friend state is one relationship, not two: A's list and B's list can never
  disagree about whether they are friends
