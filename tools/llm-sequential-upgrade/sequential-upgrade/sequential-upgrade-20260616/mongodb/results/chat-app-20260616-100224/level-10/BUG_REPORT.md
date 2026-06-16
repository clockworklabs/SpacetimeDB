# Bug Report

## Bug 1: Room activity badge doesn't reset until the page is refreshed

**Feature:** Room Activity Indicators

**Description:** When a room becomes active or hot, its badge updates correctly in real time.
But when the room goes quiet, the badge does not decay on its own — it stays "Hot"/"Active"
until the page is manually refreshed.

**Expected:** The activity badge updates in real time as activity changes, including dropping
to a lower level (or clearing) when the room goes quiet, without a refresh.

**Actual:** Once set, the badge only resets after a manual page refresh.
