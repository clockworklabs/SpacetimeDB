## Configuration resource passed to [method SpacetimeDBClient.connect_db].
##
## Controls WebSocket behaviour, threading, authentication, reconnection
## strategy, and performance monitoring. Create one, tweak the members, and
## hand it to [code]connect_db()[/code].
class_name SpacetimeDBConnectionOptions
extends Resource

const CompressionPreference = SpacetimeDBConnection.CompressionPreference

## WebSocket payload compression mode. None, Gzip, and Brotli are all supported
## (Brotli decoded via Godot's built-in decoder).
var compression: CompressionPreference = CompressionPreference.NONE
## If [code]true[/code], BSATN deserialization runs on a background thread.
var threading: bool = true
## If [code]true[/code], the SDK requests a fresh token on every connection.
var one_time_token: bool = true
## If [code]false[/code], the acquired token is never persisted to disk.
## Typically paired with [member one_time_token] = [code]true[/code].
var save_token: bool = true
## Pre-set authentication token. If empty, the SDK will request one automatically.
var token: String = ""
## Enables verbose logging in the SDK's connection and client classes.
var debug_mode: bool = false
## Registers custom Godot [Performance] monitors for packet/byte throughput.
var monitor_mode: bool = false
## If [code]true[/code], subscribes in "light" mode: the server omits row data the
## client did not request, sending only the deltas needed to keep the cache current.
## Lower bandwidth; the trade-off is fewer fields available in transaction updates.
var light_mode: bool = false
## If [code]true[/code], the server waits for each transaction to be durably
## committed before sending its update (read-after-commit). Higher latency, stronger
## durability. Default [code]false[/code] matches SpacetimeDB's default.
var confirmed_reads: bool = false
## Maximum size in bytes of the WebSocket inbound buffer (default 2 MB).
var inbound_buffer_size: int = 1024 * 1024 * 2
## Maximum size in bytes of the WebSocket outbound buffer (default 2 MB).
var outbound_buffer_size: int = 1024 * 1024 * 2
## Interval in seconds between WebSocket keepalive pings. The peer sends a PING every
## interval and closes the connection — triggering auto-reconnect if enabled — when no
## PONG arrives before the next one, detecting a dead/half-open socket within ~2 intervals
## instead of waiting out the OS TCP timeout (minutes). [code]0.0[/code] disables keepalive.
var heartbeat_interval_seconds: float = 15.0

## Per-frame time budget in microseconds for applying parsed server messages.
## Higher values drain bursts (initial subscription, mass updates) faster at the
## cost of more frame time; lower values keep frames smoother but backlog longer.
## When [member auto_tune_frame_budget] is enabled this is the seed value; the
## runtime then adjusts it within [member frame_budget_min_us]/[member frame_budget_max_us].
var frame_budget_us: int = 4000
## Hard ceiling on messages applied per frame, regardless of the time budget.
## Safety backstop against unbounded drain; rarely the binding limit.
var max_messages_per_frame: int = 256

## If [code]true[/code], [member frame_budget_us] is auto-tuned at runtime by an
## fps feedback loop: ramp up while a backlog drains and fps stays healthy, back
## off when fps dips. Finds the largest safe budget for the current hardware/scene.
var auto_tune_frame_budget: bool = true
## Lower clamp for the auto-tuned budget (microseconds).
var frame_budget_min_us: int = 1000
## Upper clamp for the auto-tuned budget (microseconds).
var frame_budget_max_us: int = 8000
## Target fps the auto-tuner defends. [code]0[/code] = use [member Engine.physics_ticks_per_second].
var auto_tune_target_fps: int = 0

## If [code]true[/code], the client automatically reconnects after unintentional disconnects.
var auto_reconnect: bool = false
## Maximum reconnect attempts before giving up. [code]0[/code] means infinite.
var max_reconnect_attempts: int = 10
## Initial delay in seconds before the first reconnect attempt.
var reconnect_initial_delay: float = 1.0
## Maximum delay cap in seconds after exponential backoff.
var reconnect_max_delay: float = 30.0
## Multiplier applied to the delay after each failed attempt.
var reconnect_backoff_multiplier: float = 2.0
## Fraction of the computed delay used as random jitter ([code]0.0[/code]–[code]1.0[/code]).
var reconnect_jitter_fraction: float = 0.5


## Convenience setter — sets both [member inbound_buffer_size] and [member outbound_buffer_size].
func set_all_buffer_size(size: int) -> void:
	inbound_buffer_size = size
	outbound_buffer_size = size
