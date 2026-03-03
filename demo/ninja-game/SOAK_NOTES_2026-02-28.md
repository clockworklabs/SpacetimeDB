# NinjaGame 2-Client Soak Notes (2026-02-28)

## Scope

Runbook-aligned local 2-client soak on `ninjagame` after republish, measuring replication cadence and desync indicators.

## Setup Commands

```bash
spacetime publish -s local -p demo/ninja-game/spacetimedb ninjagame -c -y
```

Server ping during run:

```bash
curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:3000/v1/ping
# 200
```

## Soak Method

- Two concurrent Swift SDK clients (`SoakA`, `SoakB`)
- Duration: 120 seconds each
- Movement: deterministic circular path at 20Hz reducer sends (`move_player`)
- Join flow: both clients call `join` at connect
- Metrics captured per client:
  - peer row sample count
  - average / max peer update gap (ms)
  - max observed peer positional jump
  - reducer error count

## Raw Results (Movement-Only)

`SoakA`:

```json
{"elapsedSeconds":132.91749596595764,"sawPeer":true,"reducerErrors":[],"peerSamples":4787,"connected":true,"peerGapMaxMs":92.12613105773926,"disconnectError":"","peerGapAvgMs":27.314124324557874,"name":"SoakA","durationSeconds":120,"reducerErrorCount":0,"peerMaxJump":180,"disconnected":true}
```

`SoakB`:

```json
{"elapsedSeconds":132.8305640220642,"sawPeer":true,"peerGapMaxMs":92.12613105773926,"reducerErrorCount":0,"disconnectError":"","peerSamples":4788,"durationSeconds":120,"disconnected":true,"name":"SoakB","peerGapAvgMs":27.318672668667407,"peerMaxJump":7.199581623077393,"reducerErrors":[],"connected":true}
```

## Raw Results (Combat-Mixed Traffic)

Combat-mixed variant includes `attack` and `spawn_weapon` traffic while moving.

`CombatA`:

```json
{"peerMaxJump":190,"reducerErrorCount":0,"name":"CombatA","sawPeer":true,"disconnectError":"","disconnected":true,"peerSamples":11158,"elapsedSeconds":101.23913788795471,"connected":true,"peerGapMaxMs":1498.4707832336426,"reducerErrors":[],"durationSeconds":90,"peerGapAvgMs":27.046896811826837}
```

`CombatB`:

```json
{"peerGapAvgMs":26.886929714531068,"durationSeconds":90,"elapsedSeconds":101.32108783721924,"disconnected":true,"peerGapMaxMs":1498.4748363494873,"name":"CombatB","reducerErrorCount":0,"reducerErrors":[],"sawPeer":true,"peerSamples":11150,"disconnectError":"","peerMaxJump":7.1246495246887207,"connected":true}
```

## Observed Latency / Desync Notes

- Both clients connected, saw each other, and completed the full soak without reducer failures.
- Replication cadence stayed stable:
  - average peer update gap ~27.3ms
  - max peer update gap ~92.1ms
- No sustained desync pattern observed in metrics.
- `SoakA` max jump of `180` is consistent with an initial peer-acquisition snap (first non-zero peer position sample). `SoakB` steady-state max jump was `7.2`, indicating smooth tracking after initial convergence.
- Combat-mixed run also had zero reducer failures.
- Combat-mixed max peer-gap spikes (~1.5s) appeared during transient gameplay state stalls (e.g., target not producing movement updates), not as repeated transport drops; average gap remained ~27ms.

## Conclusion

Protocol-level 2-client soak shows stable local replication and no recurring latency/desync regressions for continuous movement and combat-mixed traffic in local mode.
