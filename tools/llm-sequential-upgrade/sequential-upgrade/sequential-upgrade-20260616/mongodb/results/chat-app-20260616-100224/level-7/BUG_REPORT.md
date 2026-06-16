# Bug Report

## Bug 1: Offline users aren't shown, so there's no "last active" presence

**Feature:** Rich User Presence

**Description:** The presence list only shows currently-online users. Offline users do
not appear anywhere, so the required "Last active X minutes ago" indicator for offline
users is never displayed.

**Expected:** Offline users appear in the presence list with a "Last active X minutes ago" timestamp.

**Actual:** Only online users are listed; offline users and their last-active time are not shown at all.

## Bug 2: "Last active" time is stuck at "just now"

**Feature:** Rich User Presence

**Description:** When a last-active time is shown, it always reads "just now" and never
increments. Observed: a user was set to "away" and the tab left untouched on a stable
connection for over two minutes, yet the label still read "just now".

**Expected:** The indicator reflects elapsed time — e.g. "Last active 3 minutes ago".

**Actual:** It permanently displays "just now" regardless of how much time has passed.

## Bug 3: Status does not auto-change to "away" after inactivity

**Feature:** Rich User Presence

**Description:** A user who stays inactive (no mouse or keyboard input) is never
automatically set to "away" — the status remains "online" indefinitely.

**Expected:** After a period of inactivity the status auto-changes to "away", and returns
to "online" when the user is active again.

**Actual:** The status stays "online" no matter how long the user is inactive.
