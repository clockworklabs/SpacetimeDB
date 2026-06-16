# Bug Report

## Bug 1: Online presence shows a connected user as offline

**Feature:** Basic Chat

**Description:** Online presence does not reflect real connection state. With the same
user open in two browser tabs, closing one tab marks that user offline for everyone —
including themselves — even though the other tab is still connected and can send messages.

**Expected:** A user appears online whenever they have at least one connected session,
and only goes offline once their last session closes.

**Actual:** Closing one of a user's two sessions shows them offline, even though the
other session is still connected and sending messages.
