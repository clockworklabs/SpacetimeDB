# Bug Report

## Bug 1: Invited users are auto-added to private rooms without Accept/Decline choice

**Feature:** Private Rooms

**Description:** When a user is invited to a private room, they are automatically added as a member with no notification and no choice. The spec requires that invited users receive a notification and are presented with "Accept" and "Decline" buttons.

**Expected:** Invited user sees a notification containing the room name with "Accept" and "Decline" buttons. They are only added to the room if they accept.
**Actual:** Invited user is silently and immediately added to the private room with no notification or consent prompt.
