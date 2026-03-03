# SimpleModuleClient

SwiftUI client for the `demo/simple-module/spacetimedb` module.

## Run (Xcode or SwiftPM)

Open package in Xcode:

```bash
open Package.swift
```

or run from terminal:

```bash
swift run
```

If your module/database name differs from `simple-module-demo`, update the value in the app UI before connecting.

## Local Quick Start (Step 1/2/3)

In the app:

1. `Use Local Preset`
2. `Start Local Server`
3. `Publish Module`
4. `Connect`

Then test realtime reducers:

- `Add`
- `Add Sample`
- `Delete` (trash button per row)

`Bootstrap Local (Recommended)` runs steps 2-4 automatically.

## Maincloud Quick Test

1. `Use Maincloud Preset`
2. (Optional) `Load CLI Token` after running `spacetime login`
3. `Publish Maincloud Module`
4. `Connect`

## Troubleshooting

- `bad server response` on connect:
  publish the module for that server, then reconnect.
- `Reducer ... no such reducer`:
  server schema is stale for this database; publish again, then reconnect.
- local publish fails:
  confirm `demo/simple-module/spacetimedb` exists and `spacetime` CLI is installed.

## What You Can Test

- Add a person (`add` reducer)
- Delete a person from the list (`delete_person` reducer)
- Add sample rows quickly with `Add Sample` (UI action that calls `add`)
- Test against local server (`http://127.0.0.1:3000`) or Maincloud (`https://maincloud.spacetimedb.com`)
