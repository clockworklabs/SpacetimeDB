# Bug Report

## Bug 1: Scheduled messages API returns 500

**Feature:** Scheduled Messages (Feature 5)
**Severity:** Critical — feature completely non-functional

**Steps to reproduce:**
1. Open the app at http://localhost:6273
2. Observe: `GET /api/scheduled-messages?userId=2` returns 500 on page load
3. Try to schedule a message: `POST /api/scheduled-messages` returns 500

**Likely cause:** The `scheduled_messages` table was not created in the database. The schema migration for the upgrade may have run against the wrong PostgreSQL container (this happened in a prior iteration — `llm-sequential-upgrade-postgres-1` on port 6432 is the correct container, NOT `spacetime-web-postgres-1`).

**Fix required:**
- Verify the `scheduled_messages` table exists in `llm-sequential-upgrade-postgres-1`
- If missing, run the CREATE TABLE statement against the correct container
- Restart the Express server after schema is applied

## Bug 2: Scheduled time validation too restrictive

**Feature:** Scheduled Messages (Feature 5)
**Severity:** High — blocks usability

**Description:** The UI or server enforces a minimum scheduling window that requires messages to be scheduled hours in the future. This makes the feature impossible to test and is not a requirement.

**Fix required:** Allow scheduling messages at least 1 minute in the future. Remove any unreasonable minimum time restriction.
