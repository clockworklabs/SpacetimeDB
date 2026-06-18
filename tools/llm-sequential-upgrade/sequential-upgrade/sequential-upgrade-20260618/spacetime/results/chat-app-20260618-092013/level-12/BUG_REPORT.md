# Bug Report

## Bug 1: Drafts do not sync across sessions in real-time

**Feature:** Draft Sync

**Description:** When a user has the same room open in two sessions (for example, two
browser tabs signed in as the same user), editing the message draft in one session does
not update the message input in the other session while it stays on that room. The message
input is only populated from the saved draft when a room is selected; after that, updates to
the draft made from another session arrive in the local data but are not reflected in the
input field.

**Expected:** A draft edited in one session appears in the other session's message input in
real-time while both are viewing the same room, so the user can resume typing where they left
off on any device without having to switch rooms.

**Actual:** The second session's message input does not change when the draft is updated from
another session; it only picks up the updated draft after switching away from and back to the
room.
