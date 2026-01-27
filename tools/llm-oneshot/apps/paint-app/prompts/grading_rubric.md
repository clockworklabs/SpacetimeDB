# Paint App Grading Rubric

## Scoring System

Each feature: **0–3 points**

| Score | Meaning                     |
| ----- | --------------------------- |
| 0     | Not implemented or broken   |
| 1     | Partial - major issues      |
| 2     | Mostly working - minor bugs |
| 3     | Fully working               |

---

## Feature 1: Basic Drawing (3 pts)

- [ ] User can set display name
- [ ] Can create/join canvases
- [ ] Brush tool draws strokes
- [ ] Eraser removes content
- [ ] Color picker works
- [ ] **Strokes sync in real-time to other users**

---

## Feature 2: Live Cursors (3 pts)

- [ ] See other users' cursor positions
- [ ] Cursors have user name + color
- [ ] Cursor shows current tool icon
- [ ] **Cursors update smoothly (not choppy)**
- [ ] Cursors fade when user inactive

---

## Feature 3: Shapes (3 pts)

- [ ] Rectangle tool works
- [ ] Ellipse tool works
- [ ] Line/arrow tool works
- [ ] Shapes have stroke/fill colors
- [ ] Shapes sync to other users

---

## Feature 4: Selection & Collaborative Awareness (3 pts)

- [ ] Can select elements
- [ ] Can move/resize selected elements
- [ ] Can delete selected elements
- [ ] **See other users' selections (colored outline)**
- [ ] Selection syncs in real-time

---

## Feature 5: Layers with Locking (3 pts)

- [ ] Can create multiple layers
- [ ] Can reorder layers
- [ ] Can toggle visibility
- [ ] **Lock layer shows "Locked by [user]"**
- [ ] Only locker can edit locked layer
- [ ] **Auto-unlock on leave/timeout**

---

## Feature 6: Presence & Activity Status (3 pts)

- [ ] Shows list of users on canvas
- [ ] **Shows status: active/idle/away**
- [ ] Shows each user's current tool
- [ ] Status updates automatically
- [ ] **Auto-away after inactivity**

---

## Feature 7: Comments (3 pts)

- [ ] Can drop comment pins on canvas
- [ ] Comments have threaded replies
- [ ] Can resolve/unresolve comments
- [ ] Click comment to pan to location
- [ ] Comments sync in real-time

---

## Feature 8: Version History (3 pts)

- [ ] Auto-saves versions periodically
- [ ] Can manually save named version
- [ ] Can view version list
- [ ] Can preview old versions
- [ ] Can restore old versions

---

## Feature 9: Permissions (3 pts)

- [ ] Owner can set viewer/editor roles
- [ ] Viewers cannot draw (tools disabled)
- [ ] **Role change takes effect instantly**
- [ ] Owner can kick users
- [ ] **Kicked users removed immediately**

---

## Feature 10: Follow Mode (3 pts)

- [ ] Can click user to follow them
- [ ] **Viewport syncs in real-time**
- [ ] Shows "Following [user]" indicator
- [ ] Manual pan/zoom stops following
- [ ] Multiple users can follow same person

---

## Feature 11: Activity Feed (3 pts)

- [ ] Shows real-time action log
- [ ] Actions have timestamps + user
- [ ] Click entry to pan to location
- [ ] **Updates in real-time as actions happen**
- [ ] Old entries auto-removed

---

## Feature 12: Private Canvases & Sharing (3 pts)

- [ ] Canvases private by default
- [ ] Can generate share link
- [ ] Can set link permissions (view/edit)
- [ ] Can invite by username
- [ ] Can revoke share link

---

## Feature 13: Canvas Chat (3 pts)

- [ ] Chat panel for collaborators
- [ ] Messages show user + timestamp
- [ ] **"User is typing..." indicator works**
- [ ] Chat history persists
- [ ] Notification badge for new messages

---

## Feature 14: Auto-Cleanup & Notifications (3 pts)

- [ ] Inactive canvases marked for deletion
- [ ] **Deletion happens after scheduled time**
- [ ] Warning before deletion
- [ ] Activity resets timer
- [ ] "Keep Forever" option works

---

## Feature 15: Text & Sticky Notes (3 pts)

- [ ] Text tool adds text to canvas
- [ ] Can change font size/color
- [ ] Sticky note tool works
- [ ] **"User is editing..." indicator while typing**
- [ ] Text syncs in real-time

---

## Summary

| Feature             | Max    | Score |
| ------------------- | ------ | ----- |
| 1. Basic Drawing    | 3      |       |
| 2. Live Cursors     | 3      |       |
| 3. Shapes           | 3      |       |
| 4. Selection        | 3      |       |
| 5. Layers + Locking | 3      |       |
| 6. Presence         | 3      |       |
| 7. Comments         | 3      |       |
| 8. Version History  | 3      |       |
| 9. Permissions      | 3      |       |
| 10. Follow Mode     | 3      |       |
| 11. Activity Feed   | 3      |       |
| 12. Sharing         | 3      |       |
| 13. Canvas Chat     | 3      |       |
| 14. Auto-Cleanup    | 3      |       |
| 15. Text & Stickies | 3      |       |
| **TOTAL**           | **45** |       |

## Key Differentiator Tests

These criteria specifically test SpacetimeDB strengths (bold items above):

1. **Real-time cursor smoothness** - Are cursor updates laggy?
2. **Layer lock speed** - Is locking instant?
3. **Permission enforcement** - Does demotion cancel in-progress action?
4. **Follow mode sync** - Does viewport follow without lag?
5. **Activity feed latency** - Do actions appear immediately?
6. **Scheduled deletion** - Does auto-cleanup actually fire?
7. **Typing indicators** - Do they appear/disappear correctly?

Score these separately to measure STDB advantage.
