# Bug Report

## Bug 1: Kicked users still see and receive the room's messages

**Feature:** Real-Time Permissions

**Description:** When an admin kicks a user from a room, the kicked user is shown a
"You have been kicked from this room" banner, but the room's existing messages remain
visible behind the banner, and new messages sent to the room after the kick continue to
appear in the kicked user's client. The kick only overlays the room in the UI — it does
not actually remove the user's access to the room's data.

**Expected:** A kicked user loses access to the room — its messages are no longer
available to them, and any message sent to the room after the kick does not reach them.

**Actual:** The kicked user still sees the room's existing messages and keeps receiving
new ones; only a banner is shown over the still-visible content.
