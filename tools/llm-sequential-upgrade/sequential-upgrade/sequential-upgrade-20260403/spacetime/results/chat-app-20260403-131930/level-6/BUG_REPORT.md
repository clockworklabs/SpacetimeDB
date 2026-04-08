# Bug Report

## Bug 1: No Edit button on messages

**Feature:** Message Editing with History

**Description:** Hovering over own messages does not reveal an "Edit" button. Users have no way to edit their messages.

**Expected:** A button with text "Edit" appears on hover over messages sent by the current user.
**Actual:** No Edit button appears.

## Bug 2: Edit history panel does not update in real-time

**Feature:** Message Editing with History

**Description:** When user A edits a message while user B has that message's history panel open, B's history panel does not update to show the new edit. It only shows versions from before the panel was opened.

**Expected:** The history panel updates in real-time as new edits are made.
**Actual:** History panel is static — new edits do not appear until the panel is closed and reopened.
