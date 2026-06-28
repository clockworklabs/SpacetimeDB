## Per-request round-trip latency tracker for the SpacetimeDB client. Records the
## microsecond gap between sending a request (reducer call, procedure call,
## one-off query, subscribe) and receiving its matching response, bucketed by
## category. Read it via [method SpacetimeDBClient.get_stats] for connection
## diagnostics ("why is this reducer slow"). Main-thread only — the client records
## sends and responses on the main thread (response handling runs during the
## physics-frame drain), so no locking is needed.
##
## Cost per request: one [method Time.get_ticks_usec] plus two dictionary ops —
## negligible against the network round-trip, so tracking is always on.
class_name SpacetimeDBStats
extends RefCounted

## Request kinds tracked separately. Closed set → enum, not StringName.
enum Category { REDUCER, PROCEDURE, ONE_OFF, SUBSCRIBE }

## Cap on outstanding (unanswered) sends retained before the oldest is dropped.
## A request that never gets a response (timeout, disconnect) would otherwise leak
## a pending entry forever; eviction bounds the memory and keeps in_flight honest.
const MAX_PENDING: int = 4096


## Rolling latency stats for one [enum Category]. POD record — fields plus a derived
## average; the client never mutates it directly, it goes through [method record_response].
class Tracker extends RefCounted:
	var count: int = 0
	var min_usec: int = 0
	var max_usec: int = 0
	var total_usec: int = 0
	var last_usec: int = 0
	var in_flight: int = 0


	func avg_usec() -> int:
		return total_usec / count if count > 0 else 0

# request_id -> send timestamp (usec) and request_id -> Category int. Split into two
# dicts (rather than one record per request) to avoid a per-call object allocation
# on the reducer hot path; both are transient, keyed by the same id, popped together.
var _pending_usec: Dictionary[int, int] = { }
var _pending_cat: Dictionary[int, int] = { }
var _trackers: Array[Tracker] = []


func _init() -> void:
	_trackers.resize(Category.size())
	for i: int in Category.size():
		_trackers[i] = Tracker.new()


## Records that a request of [param category] went out under [param request_id].
func record_send(request_id: int, category: Category) -> void:
	if _pending_usec.size() >= MAX_PENDING:
		_drop_oldest_pending()
	_pending_usec[request_id] = Time.get_ticks_usec()
	_pending_cat[request_id] = category
	_trackers[category].in_flight += 1


## Records the response for [param request_id], folding its latency into the matching
## tracker. No-op if the id was never recorded or was already evicted.
func record_response(request_id: int) -> void:
	if not _pending_usec.has(request_id):
		return
	var sent: int = _pending_usec[request_id]
	var category: int = _pending_cat[request_id]
	_pending_usec.erase(request_id)
	_pending_cat.erase(request_id)

	var latency: int = Time.get_ticks_usec() - sent
	var t: Tracker = _trackers[category]
	if t.count == 0 or latency < t.min_usec:
		t.min_usec = latency
	if latency > t.max_usec:
		t.max_usec = latency
	t.total_usec += latency
	t.last_usec = latency
	t.count += 1
	if t.in_flight > 0:
		t.in_flight -= 1


## Live [Tracker] for [param category]. Treat as read-only.
func get_tracker(category: Category) -> Tracker:
	return _trackers[category]


## Clears every counter and all pending sends.
func reset() -> void:
	_pending_usec.clear()
	_pending_cat.clear()
	for i: int in Category.size():
		_trackers[i] = Tracker.new()


## One-line-per-category debug summary, latencies in milliseconds.
func summary() -> String:
	var lines: PackedStringArray = []
	for name: String in Category:
		var t: Tracker = _trackers[Category[name]]
		lines.append(
			"%s: n=%d avg=%.2fms min=%.2fms max=%.2fms last=%.2fms in_flight=%d" % [
				name.to_lower(),
				t.count,
				t.avg_usec() / 1000.0,
				t.min_usec / 1000.0,
				t.max_usec / 1000.0,
				t.last_usec / 1000.0,
				t.in_flight,
			],
		)
	return "\n".join(lines)


# Drops the oldest pending send (first inserted key) and decrements its category
# in_flight so a permanently-lost request doesn't inflate the gauge.
func _drop_oldest_pending() -> void:
	if _pending_usec.is_empty():
		return
	# First key = oldest (dict iteration is insertion-order). Grab it without
	# allocating the full keys() Array just to read index 0.
	var oldest: int = -1
	for k: int in _pending_usec:
		oldest = k
		break
	var category: int = _pending_cat.get(oldest, -1)
	_pending_usec.erase(oldest)
	_pending_cat.erase(oldest)
	if category >= 0 and _trackers[category].in_flight > 0:
		_trackers[category].in_flight -= 1
