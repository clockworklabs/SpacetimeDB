---
name: archive-evidence
description: Copy a finished StackBench campaign out of Docker with a verified manifest, record it in the ledger, and clean Docker safely
metadata:
  author: clockworklabs
  role: operator
---

# Archive campaign evidence

Results live in the `stack-bench-state` Docker volume. A Docker reset or a
broken VM disk destroys them, so the volume is never the record. The record is
an archive folder on the host, one directory per campaign, with a manifest.
Never quote a number that is not in a verified archive.

Layout:

```
<archive>/
  LEDGER.md                   one row per campaign, newest first
  campaigns/<job-id>/         the campaign directory, byte for byte
    ARCHIVE.json              job, plan, release, images, counts, status
    MANIFEST.sha256           sha256 of every file, computed inside Docker
    regrades/<tag>/           regrade outputs that bear on this campaign
```

Statuses: `current` (latest valid result), `superseded` (valid, replaced),
`invalid` (a defect made it wrong; keep the row, delete the data),
`diagnostic` (smoke runs, partial campaigns). A row is never deleted.

Archive only when no campaign or regrade is running; copying gigabytes
disturbs grading timing.

## Archive a campaign

1. Confirm it finished: every attempt `completed` or `invalid`, none pending.

   ```sh
   J=job-<id>
   MSYS_NO_PATHCONV=1 docker run --rm --entrypoint node -v stack-bench-state:/s <controller-image> -e 'const s=JSON.parse(require("fs").readFileSync("/s/results/campaigns/'$J'/state.json"));console.log(JSON.stringify((s.payload??s).summary))'
   ```

2. Build the manifest inside Docker, from the volume:

   ```sh
   MSYS_NO_PATHCONV=1 docker run --rm -v stack-bench-state:/s alpine sh -c "cd /s/results/campaigns/$J && find . -type f ! -name MANIFEST.sha256 -print0 | sort -z | xargs -0 sha256sum" > /tmp/$J.sha256
   ```

3. Copy the directory out with a read-only transfer container:

   ```sh
   A=<archive>/campaigns
   MSYS_NO_PATHCONV=1 docker create --name sb-archive-transfer --mount type=volume,source=stack-bench-state,target=/state,readonly alpine true
   MSYS_NO_PATHCONV=1 docker cp sb-archive-transfer:/state/results/campaigns/$J $A/$J
   docker rm sb-archive-transfer
   mv /tmp/$J.sha256 $A/$J/MANIFEST.sha256
   ```

4. Verify the copy. Every line must say `OK`:

   ```sh
   (cd $A/$J && sha256sum -c --quiet MANIFEST.sha256) && echo VERIFIED
   ```

5. Write `ARCHIVE.json` (`jobId`, `planId`, `releaseCommit`, `controllerImage`,
   `buildImage`, `summary`, `files`, `bytes`, `manifestSha256`, `archivedAt`,
   `status`) and add the ledger row. Mark any campaign it replaces `superseded`.

Archive a regrade the same way into `campaigns/<job-id>/regrades/<tag>/`.

## Invalidate

When a fix shows earlier results were wrong: set the row to `invalid` with the
reason and the fixing commit, keep the data until the replacement is archived
and verified, then delete the campaign folder. The manifest hash in the row
still identifies what was deleted.

## Clean Docker

Only after everything worth keeping is archived and verified, and nothing is
running:

1. Recover any interrupted run first ([recover-run](../recover-run/SKILL.md)),
   then remove exited `sb-*` containers. Keep `stack-bench-npm-cache` and the
   dashboard container.
2. In the volume, delete `results/campaigns/<job>` and `results/jobs/<id>` only
   for rows that are archived and verified, `invalid`, or `diagnostic`.
3. Remove `stack-bench-controller:*` images not named by a `current` or
   `superseded` row, then `docker builder prune`.
4. Never delete `controller-home/`, `results/plans`, or `secrets/`. Never run
   `docker volume rm stack-bench-state`.

Show the operator the exact deletion list and get a yes before deleting
campaign data.
