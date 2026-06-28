## Handle returned by [method SpacetimeDBClient.subscribe].
##
## Tracks a subscription's lifecycle from registration through server
## confirmation and eventual unsubscription. Connect to [signal applied] or
## [code]await[/code] [method wait_for_applied] to know when the initial
## row snapshot has been processed.
class_name SpacetimeDBSubscription
extends RefCounted

## Emitted when the server confirms the subscription and the initial rows are applied.
signal applied
## Emitted when the subscription is ended (unsubscribed or errored).
signal end
signal _applied_or_timeout(timeout: bool)
signal _ended_or_timeout(timeout: bool)

## Client-assigned query set id.
var query_id: int = -1
## The SQL queries registered with this subscription.
var queries: PackedStringArray
## Immediate error from subscribe, or [constant OK].
var error: Error = OK
## Human-readable error from a [SubscriptionErrorMessage], if any.
var error_message: String = ""
## Subscription lifecycle, single source of truth. PENDING until the server
## confirms ([constant State.ACTIVE]) or it unsubscribes/errors
## ([constant State.ENDED]). States are mutually exclusive by construction.
enum State { PENDING, ACTIVE, ENDED }
## [code]true[/code] after [signal applied] fires and before [signal end] fires.
var active: bool:
	get:
		return _state == State.ACTIVE
## [code]true[/code] after [signal end] fires.
var ended: bool:
	get:
		return _state == State.ENDED
var _client: SpacetimeDBClient
var _state: State = State.PENDING
var _apply_timer: SceneTreeTimer
var _end_timer: SceneTreeTimer


static func create(
		p_client: SpacetimeDBClient,
		p_query_id: int,
		p_queries: PackedStringArray,
) -> SpacetimeDBSubscription:
	var subscription: SpacetimeDBSubscription = SpacetimeDBSubscription.new()
	subscription._client = p_client
	subscription.query_id = p_query_id
	subscription.queries = p_queries

	subscription.applied.connect(subscription._on_applied)
	subscription.end.connect(subscription._on_end)
	return subscription


## Creates a pre-failed subscription handle for an immediate client-side error.
static func fail(error: Error) -> SpacetimeDBSubscription:
	var subscription: SpacetimeDBSubscription = SpacetimeDBSubscription.new()
	subscription.error = error
	subscription._state = State.ENDED
	return subscription


## Awaits until the subscription is applied or [param timeout_sec] elapses.[br]
## Returns [constant OK] on success, [constant ERR_TIMEOUT] on timeout, or
## [constant ERR_DOES_NOT_EXIST] if the subscription ended before applying.
func wait_for_applied(timeout_sec: float = 5) -> Error:
	if _state == State.ACTIVE:
		return OK
	if _state == State.ENDED:
		return ERR_DOES_NOT_EXIST

	_apply_timer = _client.get_tree().create_timer(timeout_sec)
	_apply_timer.timeout.connect(_on_applied_timeout)

	var is_timeout: bool = await _applied_or_timeout
	# Client may have been freed during the await (C5 / H8).
	if not is_instance_valid(_client):
		return ERR_DOES_NOT_EXIST
	_apply_timer = null
	if is_timeout:
		return ERR_TIMEOUT
	if _state == State.ENDED:
		return ERR_DOES_NOT_EXIST
	return OK


## Awaits until the subscription ends or [param timeout_sec] elapses.
func wait_for_end(timeout_sec: float = 5) -> Error:
	if _state == State.ENDED:
		return OK

	_end_timer = _client.get_tree().create_timer(timeout_sec)
	_end_timer.timeout.connect(_on_ended_timeout)

	var is_timeout: bool = await _ended_or_timeout
	# Client may have been freed during the await (C5 / H8).
	if not is_instance_valid(_client):
		return ERR_DOES_NOT_EXIST
	_end_timer = null
	if is_timeout:
		return ERR_TIMEOUT
	return OK


## Sends an unsubscribe request to the server. Returns [constant ERR_DOES_NOT_EXIST] if already ended.
func unsubscribe() -> Error:
	if _state == State.ENDED:
		return ERR_DOES_NOT_EXIST

	return _client.unsubscribe(query_id)


## Marks a still-PENDING subscription ended without emitting [signal end] — for
## immediate send/connection failures surfaced before any caller awaits the
## handle. No-op once ACTIVE or already ENDED: a confirmed subscription must go
## through [method _on_end] so awaiters are unblocked.
func mark_ended() -> void:
	if _state != State.PENDING:
		return
	_state = State.ENDED


func _on_applied() -> void:
	# ENDED is terminal: a late/out-of-order applied (e.g. an error ended the
	# subscription, then a stray SubscribeApplied arrives) must not resurrect it.
	# Any awaiter was already unblocked by _on_end's _applied_or_timeout emit.
	if _state == State.ENDED:
		return
	_state = State.ACTIVE
	if _apply_timer:
		_apply_timer.time_left = 0
		_apply_timer = null
	_applied_or_timeout.emit(false)


func _on_end() -> void:
	_state = State.ENDED
	# Cancel apply timer and unblock wait_for_applied() if still waiting
	if _apply_timer:
		_apply_timer.time_left = 0
		_apply_timer = null
	_applied_or_timeout.emit(false)
	if _end_timer:
		_end_timer.time_left = 0
		_end_timer = null
	_ended_or_timeout.emit(false)


func _on_applied_timeout() -> void:
	_apply_timer = null
	_applied_or_timeout.emit(true)


func _on_ended_timeout() -> void:
	_end_timer = null
	_ended_or_timeout.emit(true)
