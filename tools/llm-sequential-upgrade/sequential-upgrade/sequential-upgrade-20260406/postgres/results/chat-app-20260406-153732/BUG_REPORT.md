# Bug Report — L12 Anonymous to Registered Migration (PostgreSQL) — Iteration 2

## Bug 1: Online users list broken — app crashes on load

The previous fix added session persistence for guest users, but the app now crashes immediately on page load with `TypeError: onlineUsers is not iterable`. The browser console also shows a 400 Bad Request from `GET /api/users/online` with the response body `{"error":"Invalid user ID"}`.

**Expected:** The online users list should load normally (as it did before the previous fix). The app should not crash on page load. The guest session persistence fix from the previous iteration should remain in place.
