# Bug Report

## Bug 1: Auto-away does not restore to "online" on user activity/window focus

**Feature:** Rich User Presence

**Description:** When the auto-away timer triggers and sets the user to "Away", returning to the window or typing does not automatically restore the status back to "Online". The user must manually switch status away and back to online.

**Expected:** When the user returns focus to the window or performs any activity (mouse move, keypress, click), status is automatically restored to "Online" and all other clients see the update immediately.
**Actual:** Status stays as "Away" after auto-away triggers, even after the user is actively using the app again.

## Bug 2: Top status selector and bottom online list are out of sync

**Feature:** Rich User Presence

**Description:** The status shown in the top selector (e.g. "Online") does not match what is displayed in the bottom online users list (e.g. "Away - Last active 6m ago") for the same user. They only sync after the user manually changes their status.

**Expected:** Both the top status selector and the bottom online list always reflect the same current status in real-time.
**Actual:** The two status displays are out of sync and only update independently when the user manually intervenes.
