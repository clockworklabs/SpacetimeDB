# Bug Report — L12 Anonymous to Registered Migration (PostgreSQL)

## Bug 1: Guest identity does not persist for the session

When a user joins as guest and then refreshes the page, the app returns them to the login screen. Clicking "Join as Guest" again assigns a brand new Guest-XXXX account with a different ID — the previous guest's messages and room memberships are orphaned.

**Expected:** Per the L12 spec, "anonymous users have a temporary identity that persists for their session." Refreshing the page in the same browser should restore the same guest user — same ID, same name, same room memberships, their previous messages still attributed to them.
