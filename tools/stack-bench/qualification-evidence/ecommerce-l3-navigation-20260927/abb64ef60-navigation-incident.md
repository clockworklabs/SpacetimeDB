# Catalog navigation grading incident

Study: `993215036f6fc527cbeb47060ff33051eece01a71626acb5b352662084479cb0`; frozen controller `abb64ef60`.

Cancellation requested on 2026-09-26 after a confirmed shared scenario defect. At the last pre-cancel monitor, 12 attempts had started, 2 completed and 10 were running. Preserve all original scores and source. These scores must not be published as a verified stack comparison.

## Evidence

- `contracts/catalog-items.md` explicitly permits a separate catalog and requires `catalog-link` on the first page.
- `contracts/purchasing.md` says to use `catalog-link` to return to the catalog.
- `scenarios/01-buying.json` sets up buyer and watcher accounts but never opens the catalog. Check 3b immediately expects Espresso Machine stock on watcher. Grader actor creation navigates only to the application root. The shared sign-up action only waits for `current-user`.
- SpacetimeDB repetition 3 L2 `first-build-l2-grading/grading-selected-source-008.json` fails on that first stock observation, before any purchase.
- Saved screenshot `abb64ef60-stdb-r3-3b.png` shows the signed-in Home landing page. `first-build-l2/frontend/src/App.tsx` starts at `home`; the declared `catalog-link` opens `catalog`, which contains Espresso Machine and the required controls. `Features.tsx` puts cart and order controls on catalog/detail pages.
- The earlier fix added navigation only to `01-catalog-values.json`. It did not cover sibling scenarios. This is a grader integration defect, not evidence that a purchase failed or live stock failed.

## Work order

1. Confirm cancellation and resource recovery. Audit all 12 saved attempts, with completed and interrupted outcomes separate. Keep real application defects and grading limitations distinct.
2. Trace all selected L1–L3 scenario actors and explicit page transitions. Include signed-out observers, login, fresh clients, reload, catalog return, and staff/admin pages. Inventory affected check IDs before editing.
3. Extend existing Chromium behavioral coverage before implementation: catalog on root and on a separate page must work; dead links and missing controls must still fail; observers must not navigate or reload during live-update assertions; valid signed-out absence checks must first reach the correct surface.
4. Fix explicit scenario setup/navigation at the correct boundary using existing actions. Do not add invisible navigation in assertions, catch-and-retry failures, or expose grading details to generated apps.
5. Run focused E2E coverage and the affected saved-source replay. Run affected reference/defect controls and validate reuse for unchanged checks. Freeze and inspect the candidate before a fresh paid study.
6. Start the authorized Sol 6 medium four-stack, three-repetition, no-repair study with 12 slots only after these gates pass. Monitor, audit all attempts, and preserve prior studies separately.

No claim is made that every failure in this study came from this defect. Metadata integrity checks passed for the first two completed attempts; that does not establish behavioral grading fairness.

## Confirmed correction

- Saved SpacetimeDB r3 L2 source was replayed without model calls or source edits against the navigation candidate. Selected purchase-stock 3b passed 2/2; the original stopped at its first stock lookup. Evidence: state-volume `results/diagnostics/navigation-20260926/stdb-r3-purchase`, `navigation-stdb-replay-launch.json`, and `navigation-stdb-replay.log`. Recovery is clean; its stopped diagnostic controller was removed after preserving logs.
- Browser controls cover root and linked catalogs, a dead link, stale updates, and navigation after the purchase. Cart controls cover authentication/reload returning to home and a stale second browser.
- Review found that the signed-out purchase/ranking observer also needs a pre-write baseline. A new late-first-read control falsely passed before the baseline change (`purchase-baseline-before.log`). After adding initial stock/ranking observations, all five purchase cases meet their expected outcomes (`purchase-baseline-after.log`, `results/diagnostics/purchase-navigation-ePx0oJ`). This prevents a delayed initial fetch from being counted as live delivery. The scope stays within the changed scenarios.
- Scenario validation reports 0 errors and 0 warnings. Qualification remains pending. Historical results are unchanged.
