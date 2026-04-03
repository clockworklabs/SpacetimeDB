# Bug Report

## Bug 1: Users always appear as "invisible" until they manually change status

**Feature:** Rich User Presence

**Description:** When a user joins a room, their status dot shows as invisible/grey for all other members. The status is not initialized to "online" on connect — it only reflects reality after the user manually selects a status from the selector.

**Expected:** Users should default to "online" (green dot) when they connect. Their status should be visible to others immediately upon joining.
**Actual:** All users show as invisible/grey until they explicitly change their status via the selector.
