## High-level SpacetimeDB client node.
##
## Orchestrates connection, authentication, BSATN (de)serialization, the local
## database mirror, subscriptions, reducer/procedure calls, and automatic
## reconnection. Generated module clients (e.g. [code]WorldModuleClient[/code])
## extend this with typed database and reducer accessors.
#@tool
class_name SpacetimeDBClient
extends Node

## Emitted after the server sends [IdentityTokenMessage] confirming the connection.
signal connected(identity: PackedByteArray, token: String)
## Emitted when the WebSocket is cleanly closed.
signal disconnected
## Emitted on connection failure or abnormal close.
signal connection_error(code: int, reason: String)
## Emitted after the first [SubscribeAppliedMessage] is processed.
signal database_initialized
## Re-emitted from [LocalDatabase] when a row is inserted.
signal row_inserted(table_name: StringName, row: Resource)
## Re-emitted from [LocalDatabase] when a row is updated.
signal row_updated(table_name: StringName, old_row: Resource, new_row: Resource)
## Re-emitted from [LocalDatabase] just before a row is deleted (still queryable).
signal row_before_delete(table_name: StringName, row: Resource)
## Re-emitted from [LocalDatabase] when a row is deleted.
signal row_deleted(table_name: StringName, row: Resource)
## Re-emitted from [LocalDatabase] after a batch of changes completes.
signal row_transactions_completed(table_name: StringName)
## Emitted for every [TransactionUpdateMessage] applied to the local database.
signal transaction_update_received(update: TransactionUpdateMessage)
## Emitted when a [ReducerResultMessage] arrives. [param tx_update] is [code]null[/code] for okEmpty/err.
signal reducer_result_received(request_id: int, tx_update: TransactionUpdateMessage)
## Emitted when a [ProcedureResultData] arrives. [param return_bytes] is empty on error.
signal procedure_result_received(request_id: int, return_bytes: PackedByteArray)
## Emitted when a [OneOffQueryResponseMessage] arrives.
signal one_off_query_received(request_id: int, tables: Array[TableUpdateData], error_message: String)
## Emitted before each reconnect attempt.
signal reconnecting(attempt: int, max_attempts: int)
## Emitted after a successful reconnect and all re-subscriptions are applied.
signal reconnected
## Emitted when all reconnect attempts are exhausted.
signal reconnect_failed
## Internal: fired when the socket drops so pending [method _wait_for_response] awaiters
## (one-off queries, the deprecated reducer/procedure wait helpers) wake immediately
## instead of blocking out their full timeout.
signal _response_wait_aborted

# --- Configuration ---
## Base URL of the SpacetimeDB server (e.g. [code]http://127.0.0.1:3000[/code]).
@export var base_url: String = "http://127.0.0.1:3000"
## Name of the database to connect to.
@export var database_name: String = "quickstart-chat"
## Path to the generated schema directory (tables, types, reducers).
@export var schema_path: String = "res://spacetime_bindings/schema"
## If [code]true[/code], calls [method initialize_and_connect] automatically in [method _ready].
@export var auto_connect: bool = false
## If [code]true[/code], automatically requests a new auth token from the REST API when none is saved.
@export var auto_request_token: bool = true
## File path where the authentication token is persisted between sessions.
@export var token_save_path: String = "user://spacetimedb_token.dat"
## If [code]true[/code], the token is not saved to disk (single-use session).
@export var one_time_token: bool = false
## If [code]false[/code], the acquired token is never written to [member token_save_path].
## Pair with [member one_time_token] = [code]true[/code] (request a fresh token each
## connection) to avoid persisting a token that is then ignored on the next connect.
@export var save_token: bool = true
## WebSocket compression preference negotiated with the server.
@export var compression: SpacetimeDBConnection.CompressionPreference
## If [code]true[/code], enables verbose logging from the client.
@export var debug_mode: bool = true
## Active subscriptions keyed by query set id.
var current_subscriptions: Dictionary[int, SpacetimeDBSubscription]
## If [code]true[/code], BSATN deserialization runs on a background thread.
@export var use_threading: bool = true

## Module name used for schema resolution (set by generated subclasses).
var module_name: String = ""
## Background thread running the BSATN deserializer (only when [member use_threading] is [code]true[/code]).
var deserializer_worker: Thread
## Connection options controlling threading, compression, and reconnection behaviour.
var connection_options: SpacetimeDBConnectionOptions
## Subscriptions waiting for a [SubscribeAppliedMessage], keyed by query set id.
var pending_subscriptions: Dictionary[int, SpacetimeDBSubscription]
var _packet_queue: Array[PackedByteArray] = []
var _packet_semaphore: Semaphore
var _result_queue: Array[SpacetimeDBServerMessage] = []
# Drain batch held across frames + a cursor into it (main thread only). A batch
# is refilled from _result_queue only once fully drained, and the cursor advances
# in place — so a multi-frame backlog is never re-sliced/re-queued (O(1)/frame
# instead of O(remaining) copy/frame). Newly parsed messages wait in
# _result_queue until the current batch finishes, preserving arrival order.
var _drain_batch: Array[SpacetimeDBServerMessage] = []
var _drain_cursor: int = 0
var _result_mutex: Mutex
var _packet_mutex: Mutex
var _thread_should_exit: bool = false
## Incremented (under _packet_mutex) on every reconnect prep. The deserializer
## worker captures it when draining and re-checks it before flushing results, so
## packets parsed under a prior session are discarded instead of being applied to
## the fresh post-reconnect database.
var _session_epoch: int = 0
# Per-frame apply drain limits: time budget (primary) + hard message ceiling
# (bounded-loop backstop). Set from SpacetimeDBConnectionOptions in connect_db.
# _frame_budget_us is a live value when auto-tuning is on (mutated by
# _auto_tune_budget), otherwise the fixed configured budget.
var _frame_budget_us: int = 4000
var _max_msgs_per_frame: int = 256
var _auto_tune_budget_enabled: bool = true
var _frame_budget_min_us: int = 1000
var _frame_budget_max_us: int = 8000
var _auto_tune_target_fps: int = 0
const _MAX_RESULT_CACHE_SIZE: int = 256
# Cache of reducer results that arrived before anyone called wait_for_reducer_response
var _reducer_result_cache: Dictionary[int, TransactionUpdateMessage] = { } # request_id -> TransactionUpdateMessage (or null)
var _pending_reducer_calls: Dictionary[int, SpacetimeDBReducerCall] = { }
var _pending_procedure_calls: Dictionary[int, SpacetimeDBProcedureCall] = { }
var _procedure_result_cache: Dictionary[int, PackedByteArray] = { }
# Per-request round-trip latency, keyed by category. Always-on diagnostics.
var _stats: SpacetimeDBStats = SpacetimeDBStats.new()
var _one_off_query_cache: Dictionary[int, Array] = { }
# --- Components ---
var _connection: SpacetimeDBConnection
var _deserializer: BSATNDeserializer
var _serializer: BSATNSerializer
var _local_db: LocalDatabase
var _rest_api: SpacetimeDBRestAPI # Optional, for token/REST calls
# --- State ---
var _connection_id: PackedByteArray
var _identity: PackedByteArray
var _token: String
var _is_initialized: bool = false
var _received_initial_subscription: bool = false
var _next_query_id: int = 0
var _next_request_id: int = 0
# --- Reconnection State ---
enum _ReconnectState { IDLE, RECONNECTING }
var _reconnect_state: _ReconnectState = _ReconnectState.IDLE
var _reconnect_attempt: int = 0
## Set when the next reconnect should skip the backoff delay (stall-induced close —
## the socket dropped from a local freeze, not a network fault). Consumed on the
## first scheduled attempt.
var _reconnect_immediate: bool = false
var _intentional_disconnect: bool = false
var _saved_subscription_queries: Array[PackedStringArray] = []
## Bumped on every new reconnect cycle (start/cancel/resubscribe). A resubscribe
## settle-callback captures the epoch live when it was armed and bails if the epoch
## has since moved on, so a superseded cycle's late `applied`/`end` can't clear the
## saved queries or spuriously emit `reconnected` on a cycle that already moved past it.
var _resubscribe_epoch: int = 0
var _reconnect_timer: SceneTreeTimer = null
var _rng: RandomNumberGenerator = RandomNumberGenerator.new()


func _ready() -> void:
	_rng.randomize()
	if auto_connect:
		initialize_and_connect()


# --- WebSocket Message Handling ---
func _physics_process(_delta: float) -> void:
	_process_results_asynchronously()


func _exit_tree() -> void:
	_cancel_reconnection()
	if deserializer_worker:
		_thread_should_exit = true
		_packet_semaphore.post()
		deserializer_worker.wait_to_finish()
		deserializer_worker = null


## Prints [param log_message] to the output console when [member debug_mode] is enabled.
func print_log(log_message: String) -> void:
	if debug_mode:
		print(log_message)


## Initializes the schema, serializers, local database, REST API, and connection,[br]
## then loads or requests a token and connects to the server.[br]
## Safe to call multiple times — subsequent calls are ignored if already initialized.
func initialize_and_connect() -> void:
	if _is_initialized:
		return

	print_log("SpacetimeDBClient: Initializing...")

	# 1. Load Schema
	var schema: SpacetimeDBSchema = SpacetimeDBSchema.new(module_name, schema_path, debug_mode)

	# 2. Initialize Parser
	_deserializer = BSATNDeserializer.new(schema, debug_mode)
	_serializer = BSATNSerializer.new(debug_mode)

	# 3. Initialize Local Database
	_local_db = LocalDatabase.new(schema)
	_init_db(_local_db)

	# Re-emit LocalDatabase signals as the client's own (named methods, not lambdas —
	# the formatter mangles inline-lambda indentation; see project rule S1).
	_local_db.row_inserted.connect(_forward_row_inserted)
	_local_db.row_updated.connect(_forward_row_updated)
	_local_db.row_before_delete.connect(_forward_row_before_delete)
	_local_db.row_deleted.connect(_forward_row_deleted)
	_local_db.row_transactions_completed.connect(_forward_row_transactions_completed)
	_local_db.name = "LocalDatabase"
	add_child(_local_db) # Add as child if it needs signals

	# 4. Initialize REST API Handler (optional, mainly for token)
	_rest_api = SpacetimeDBRestAPI.new(base_url, debug_mode)
	_rest_api.token_received.connect(_on_token_received)
	_rest_api.token_request_failed.connect(_on_token_request_failed)
	_rest_api.name = "RestAPI"
	add_child(_rest_api)

	# 5. Initialize Connection Handler
	_connection = SpacetimeDBConnection.new(connection_options, database_name)
	_connection.disconnected.connect(_on_connection_disconnected)
	_connection.connection_error.connect(_on_connection_error)
	_connection.connection_stalled.connect(_on_connection_stalled)
	_connection.message_received.connect(_on_websocket_message_received)
	_connection.name = "Connection"
	add_child(_connection)

	# Ensure the deserializer thread + sync primitives exist before any WS frame
	# arrives. connect_db() already calls this, but initialize_and_connect() can be
	# invoked directly, in which case use_threading defaults true and the message
	# handler would hit a null _packet_mutex on the first frame.
	_setup_threading()

	_is_initialized = true
	print_log("SpacetimeDBClient: Initialization complete.")

	# 6. Get Token and Connect
	_load_token_or_request()


## Connects to a SpacetimeDB [param database_name] at [param host_url].[br]
## Pass a [SpacetimeDBConnectionOptions] to configure threading, compression, and reconnection.
func connect_db(host_url: String, database_name: String, options: SpacetimeDBConnectionOptions = null) -> void:
	_cancel_reconnection()
	if not options:
		options = SpacetimeDBConnectionOptions.new()
	connection_options = options
	self.base_url = host_url
	self.database_name = database_name.to_lower()
	self.compression = options.compression
	self.one_time_token = options.one_time_token
	self.save_token = options.save_token
	if not options.token.is_empty():
		self._token = options.token
	self.debug_mode = options.debug_mode
	self.use_threading = options.threading
	self._auto_tune_budget_enabled = options.auto_tune_frame_budget
	# Resolve + clamp the drain limits in one pure step (unit-tested).
	var cfg: PackedInt32Array = _resolve_drain_config(
		options.max_messages_per_frame,
		options.frame_budget_min_us,
		options.frame_budget_max_us,
		options.frame_budget_us,
		options.auto_tune_target_fps,
	)
	self._max_msgs_per_frame = cfg[0]
	self._frame_budget_min_us = cfg[1]
	self._frame_budget_max_us = cfg[2]
	self._frame_budget_us = cfg[3]
	self._auto_tune_target_fps = cfg[4]

	_setup_threading()

	if not _is_initialized:
		initialize_and_connect()
	elif not _connection.is_connected_db():
		# Already initialized, just need token and connect
		_load_token_or_request()


## Intentionally disconnects from the database. Does not trigger auto-reconnect.
func disconnect_db() -> void:
	_intentional_disconnect = true
	_cancel_reconnection()
	_token = ""
	if _connection and _connection.is_connected_db():
		_connection.disconnect_from_server()
	else:
		# Socket already closed (e.g. cancelled mid-backoff during a reconnect):
		# disconnect_from_server() would be a no-op and emit nothing, leaving callers
		# waiting on disconnected forever and the intentional flag stuck. Surface the
		# terminal signal here instead.
		_intentional_disconnect = false
		disconnected.emit()


## Returns [code]true[/code] if the WebSocket is currently open.
func is_connected_db() -> bool:
	return _connection and _connection.is_connected_db()


## Returns the raw [LocalDatabase] instance. Prefer the generated [code].Db[/code] property for typed access.
func get_local_database() -> LocalDatabase:
	return _local_db


## Returns the 32-byte identity assigned to this client by the server.
func get_local_identity() -> PackedByteArray:
	return _identity


## Returns the current authentication token, or an empty string if none acquired yet.
func get_token() -> String:
	return _token


## Returns the per-request latency [SpacetimeDBStats] (reducer / procedure / one-off /
## subscribe round-trip times). Read-only diagnostics; call [code]get_stats().summary()[/code]
## for a quick dump or [code]get_stats().get_tracker(cat)[/code] for a category's numbers.
func get_stats() -> SpacetimeDBStats:
	return _stats


## Subscribes to one or more SQL [param queries]. Returns a [SpacetimeDBSubscription] handle.
func subscribe(queries: PackedStringArray) -> SpacetimeDBSubscription:
	if not is_connected_db():
		push_warning("SpacetimeDBClient: Cannot subscribe, not connected.")
		return SpacetimeDBSubscription.fail(ERR_CONNECTION_ERROR)

	# 1. Generate a request ID
	var request_id: int = _next_request_id
	_next_request_id += 1
	var query_id: int = _next_query_id
	_next_query_id += 1
	# 2. Create the correct payload Resource
	var payload_data: SubscribeMessage = SubscribeMessage.new(request_id, query_id, queries)

	# 3. Serialize the complete ClientMessage using the universal function
	var message_bytes: PackedByteArray = _serializer.serialize_client_message(
		SpacetimeDBClientMessage.SUBSCRIBE,
		payload_data,
	)

	if _serializer.has_error():
		printerr("SpacetimeDBClient: Failed to serialize Subscribe message: %s" % _serializer.get_last_error())
		return SpacetimeDBSubscription.fail(ERR_PARSE_ERROR)

	# 4. Create subscription handle
	var subscription: SpacetimeDBSubscription = SpacetimeDBSubscription.create(self, query_id, queries)

	# 5. Send the binary message via WebSocket
	if _connection and _connection.is_websocket_active():
		var err: Error = _connection.send_bytes(message_bytes)
		if err != OK:
			printerr("SpacetimeDBClient: Error sending Subscribe BSATN message: %s" % error_string(err))
			subscription.error = err
			subscription.mark_ended()
		else:
			print_log("SpacetimeDBClient: Subscribe request sent successfully (BSATN), Query ID: %d" % query_id)
			pending_subscriptions.set(query_id, subscription)
			_stats.record_send(request_id, SpacetimeDBStats.Category.SUBSCRIBE)

		return subscription

	printerr("SpacetimeDBClient: Internal error - WebSocket peer not available in connection.")
	subscription.error = ERR_CONNECTION_ERROR
	subscription.mark_ended()
	return subscription


## Unsubscribes from the query set identified by [param query_id].[br]
## Returns [constant OK] on success, or an [enum Error] code on failure.
func unsubscribe(query_id: int) -> Error:
	if not is_connected_db():
		push_warning("SpacetimeDBClient: Cannot unsubscribe, not connected.")
		return ERR_CONNECTION_ERROR

	var request_id: int = _next_request_id
	_next_request_id += 1
	# 1. Create the correct payload Resource. SendDroppedRows makes the server echo
	#    the rows being removed so LocalDatabase can decrement the refcount and evict
	#    only rows no longer held by any other active subscription.
	var payload_data: UnsubscribeMessage = UnsubscribeMessage.new(
		request_id,
		query_id,
		UnsubscribeMessage.UnsubscribeFlags.SendDroppedRows,
	)

	# 2. Serialize the complete ClientMessage using the universal function
	var message_bytes: PackedByteArray = _serializer.serialize_client_message(
		SpacetimeDBClientMessage.UNSUBSCRIBE,
		payload_data,
	)

	if _serializer.has_error():
		printerr("SpacetimeDBClient: Failed to serialize Unsubscribe message: %s" % _serializer.get_last_error())
		return ERR_PARSE_ERROR

	# 3. Send the binary message via WebSocket
	if _connection and _connection.is_websocket_active():
		var err: Error = _connection.send_bytes(message_bytes)
		if err != OK:
			printerr("SpacetimeDBClient: Error sending Unsubscribe BSATN message: %s" % error_string(err))
			return err

		print_log("SpacetimeDBClient: Unsubscribe request sent successfully (BSATN), Query ID: %d" % query_id)
		return OK

	printerr("SpacetimeDBClient: Internal error - WebSocket peer not available in connection.")
	return ERR_CONNECTION_ERROR


## Calls a reducer named [param reducer_name] with the given [param args] and BSATN [param types].[br]
## [param ret_bsatn_type] (optional) lets the returned handle BSATN-decode the reducer's ok return
## value via [method SpacetimeDBReducerCall.decode]; empty for reducers that return nothing.[br]
## Returns a [SpacetimeDBReducerCall] handle that resolves when the server responds.
func call_reducer(reducer_name: String, args: Array = [], types: Array = [], ret_bsatn_type: StringName = &"") -> SpacetimeDBReducerCall:
	if not is_connected_db():
		push_warning("SpacetimeDBClient: Cannot call reducer '%s', not connected." % reducer_name)
		return SpacetimeDBReducerCall.fail(ERR_CONNECTION_ERROR)

	var args_bytes: PackedByteArray = _serializer._serialize_arguments(args, types)

	if _serializer.has_error():
		printerr("Failed to serialize args for %s: %s" % [reducer_name, _serializer.get_last_error()])
		return SpacetimeDBReducerCall.fail(ERR_PARSE_ERROR)

	var request_id: int = _next_request_id
	_next_request_id += 1

	var call_data: CallReducerMessage = CallReducerMessage.new(reducer_name, args_bytes, request_id, 0)
	var message_bytes: PackedByteArray = _serializer.serialize_client_message(
		SpacetimeDBClientMessage.CALL_REDUCER,
		call_data,
	)

	if _serializer.has_error():
		printerr("SpacetimeDBClient: Failed to serialize CallReducer message: %s" % _serializer.get_last_error())
		return SpacetimeDBReducerCall.fail(ERR_PARSE_ERROR)

	if _connection and _connection.is_websocket_active():
		var err: Error = _connection.send_bytes(message_bytes)
		if err != OK:
			printerr("SpacetimeDBClient: Error sending CallReducer message: ", err)
			return SpacetimeDBReducerCall.fail(err)

		var handle: SpacetimeDBReducerCall = SpacetimeDBReducerCall.create(self, request_id, ret_bsatn_type)
		_pending_reducer_calls[request_id] = handle
		_stats.record_send(request_id, SpacetimeDBStats.Category.REDUCER)
		return handle

	printerr("SpacetimeDBClient: Internal error - WebSocket peer not available in connection.")
	return SpacetimeDBReducerCall.fail(ERR_CONNECTION_ERROR)


## Calls a stored procedure named [param procedure_name] with the given [param args] and BSATN [param types].[br]
## [param return_bsatn_type] is used by the handle to deserialize the return value.[br]
## Returns a [SpacetimeDBProcedureCall] handle that resolves when the server responds.
func call_procedure(procedure_name: String, args: Array = [], types: Array = [], return_bsatn_type: StringName = &"") -> SpacetimeDBProcedureCall:
	if not is_connected_db():
		push_warning("SpacetimeDBClient: Cannot call procedure, not connected.")
		return SpacetimeDBProcedureCall.fail(ERR_CONNECTION_ERROR)

	var args_bytes: PackedByteArray = _serializer._serialize_arguments(args, types)
	if _serializer.has_error():
		printerr("Failed to serialize args for %s: %s" % [procedure_name, _serializer.get_last_error()])
		return SpacetimeDBProcedureCall.fail(ERR_PARSE_ERROR)

	var request_id: int = _next_request_id
	_next_request_id += 1

	var call_data: CallProcedureMessage = CallProcedureMessage.new(procedure_name, args_bytes, request_id, 0)
	var message_bytes: PackedByteArray = _serializer.serialize_client_message(
		SpacetimeDBClientMessage.CALL_PROCEDURE,
		call_data,
	)

	if _serializer.has_error():
		printerr("SpacetimeDBClient: Failed to serialize CallProcedure message: %s" % _serializer.get_last_error())
		return SpacetimeDBProcedureCall.fail(ERR_PARSE_ERROR)

	if _connection and _connection.is_websocket_active():
		var err: Error = _connection.send_bytes(message_bytes)
		if err != OK:
			printerr("SpacetimeDBClient: Error sending CallProcedure message: ", err)
			return SpacetimeDBProcedureCall.fail(err)

		var handle: SpacetimeDBProcedureCall = SpacetimeDBProcedureCall.create(self, request_id, return_bsatn_type)
		_pending_procedure_calls[request_id] = handle
		_stats.record_send(request_id, SpacetimeDBStats.Category.PROCEDURE)
		return handle

	printerr("SpacetimeDBClient: Internal error - WebSocket peer not available in connection.")
	return SpacetimeDBProcedureCall.fail(ERR_CONNECTION_ERROR)


## Executes a single SQL query without creating a subscription.[br]
## Returns an [Array] of [TableUpdateData] with the result rows, or an empty array on error/timeout.[br]
## Use [signal one_off_query_received] for non-blocking access.
func query_sql(query: String, timeout_seconds: float = 10.0) -> Array[TableUpdateData]:
	if not is_connected_db():
		push_warning("SpacetimeDBClient: Cannot run one-off query, not connected.")
		return []

	var request_id: int = _next_request_id
	_next_request_id += 1

	var payload: OneOffQueryMessage = OneOffQueryMessage.new(request_id, query)
	var message_bytes: PackedByteArray = _serializer.serialize_client_message(
		SpacetimeDBClientMessage.ONEOFF_QUERY,
		payload,
	)

	if _serializer.has_error():
		printerr("SpacetimeDBClient: Failed to serialize OneOffQuery message: %s" % _serializer.get_last_error())
		return []

	if not (_connection and _connection.is_websocket_active()):
		printerr("SpacetimeDBClient: Internal error - WebSocket peer not available in connection.")
		return []

	var err: Error = _connection.send_bytes(message_bytes)
	if err != OK:
		printerr("SpacetimeDBClient: Error sending OneOffQuery message: %s" % error_string(err))
		return []

	print_log("SpacetimeDBClient: OneOffQuery sent (request_id=%d): %s" % [request_id, query])
	_stats.record_send(request_id, SpacetimeDBStats.Category.ONE_OFF)

	# Wait for response
	var result: Variant = await _wait_for_response(request_id, _one_off_query_cache, one_off_query_received, timeout_seconds)
	if result == null:
		return []
	return result as Array[TableUpdateData]


## Awaits the reducer result for [param request_id_to_match], returning the [TransactionUpdateMessage] or [code]null[/code] on timeout.
## [br][b]Warning:[/b] a [code]null[/code] return is ambiguous — it can mean a timeout, an [code]okEmpty[/code] outcome, or a server-side error.
## Callers that need the actual outcome should use the [SpacetimeDBReducerCall] returned by generated reducer wrappers and call [code]SpacetimeDBReducerCall.wait_for_response[/code] instead.
func wait_for_reducer_response(request_id_to_match: int, timeout_seconds: float = 10.0) -> TransactionUpdateMessage:
	if request_id_to_match < 0:
		return null
	return await _wait_for_response(request_id_to_match, _reducer_result_cache, reducer_result_received, timeout_seconds)


## Awaits the procedure result for [param request_id_to_match], returning the BSATN [PackedByteArray] or empty on timeout.
func wait_for_procedure_response(request_id_to_match: int, timeout_seconds: float = 10.0) -> PackedByteArray:
	if request_id_to_match < 0:
		return PackedByteArray()
	var result: Variant = await _wait_for_response(request_id_to_match, _procedure_result_cache, procedure_result_received, timeout_seconds)
	return result if result != null else PackedByteArray()


func _wait_for_response(request_id: int, cache: Dictionary, sig: Signal, timeout_seconds: float) -> Variant:
	if cache.has(request_id):
		var cached: Variant = cache[request_id]
		cache.erase(request_id)
		print_log("SpacetimeDBClient: Cache hit for Req ID: %d" % request_id)
		return cached
	var timer: SceneTreeTimer = get_tree().create_timer(timeout_seconds)
	var result_container: Array = [null]
	# done lives in a container because GDScript lambdas capture local primitives
	# by value (godot#69014); a bare `var done` would never reflect the mutation.
	var done_ref: Array[bool] = [false]
	var connection: Callable = func(rid: int, data: Variant) -> void:
		if rid == request_id and not done_ref[0]:
			done_ref[0] = true
			result_container[0] = data
			cache.erase(rid)
			timer.time_left = 0
	# Wakes the wait early when the connection drops; leaves result null so the caller
	# gets the same empty/null it would on timeout, but without the full delay.
	var abort: Callable = func() -> void:
		if not done_ref[0]:
			done_ref[0] = true
			timer.time_left = 0
	sig.connect(connection)
	_response_wait_aborted.connect(abort)
	await timer.timeout
	# The client may have been freed during the wait (disconnect + queue_free).
	# Touching self's signals/dicts after that crashes (engine bug #72629).
	if not is_instance_valid(self):
		return null
	sig.disconnect(connection)
	_response_wait_aborted.disconnect(abort)
	if result_container[0] == null:
		if not done_ref[0]:
			printerr("SpacetimeDBClient: Timeout waiting for response for Req ID: %d" % request_id)
		return null
	print_log("SpacetimeDBClient: Received matching response for Req ID: %d" % request_id)
	return result_container[0]


func _init_db(local_db: LocalDatabase) -> void:
	pass


# --- LocalDatabase signal forwarders (re-emit as the client's own signals) ---
func _forward_row_inserted(tn: StringName, r: _ModuleTableType) -> void:
	row_inserted.emit(tn, r)


func _forward_row_updated(tn: StringName, p: _ModuleTableType, r: _ModuleTableType) -> void:
	row_updated.emit(tn, p, r)


func _forward_row_before_delete(tn: StringName, r: _ModuleTableType) -> void:
	row_before_delete.emit(tn, r)


func _forward_row_deleted(tn: StringName, r: _ModuleTableType) -> void:
	row_deleted.emit(tn, r)


func _forward_row_transactions_completed(tn: StringName) -> void:
	row_transactions_completed.emit(tn)


func _load_token_or_request() -> void:
	if _token:
		# If token is already set, use it
		_on_token_received(_token)
		return

	if one_time_token == false:
		# Try loading saved token
		if FileAccess.file_exists(token_save_path):
			var file: FileAccess = FileAccess.open(token_save_path, FileAccess.READ)
			if file:
				var saved_token: String = file.get_as_text().strip_edges()
				file.close()
				if not saved_token.is_empty():
					print_log("SpacetimeDBClient: Using saved token.")
					_on_token_received(saved_token) # Directly use the saved token
					return

	# If no valid saved token, request a new one if auto-request is enabled
	if auto_request_token:
		print_log("SpacetimeDBClient: No valid saved token found, requesting new one.")
		_rest_api.request_new_token()
	else:
		printerr("SpacetimeDBClient: No token available and auto_request_token is false.")
		connection_error.emit(-1, "Authentication token unavailable")


func _generate_connection_id() -> String:
	var random_bytes: PackedByteArray = []
	random_bytes.resize(16)
	for i: int in 16:
		random_bytes[i] = _rng.randi_range(0, 255)
	return random_bytes.hex_encode() # Return as hex string


func _on_token_received(received_token: String) -> void:
	print_log("SpacetimeDBClient: Token acquired.")
	self._token = received_token
	if save_token:
		_save_token(received_token)
	var conn_id: String = _generate_connection_id()
	# Pass token to components that need it
	_connection.set_token(self._token)
	_rest_api.set_token(self._token) # REST API might also need it

	# Now attempt to connect WebSocket
	_connection.connect_to_database(base_url, database_name, conn_id)


func _on_token_request_failed(error_code: int, response_body: String) -> void:
	printerr("SpacetimeDBClient: Failed to acquire token. Cannot connect.")
	connection_error.emit(error_code, "Failed to acquire authentication token")


func _save_token(token_to_save: String) -> void:
	var dir_path: String = token_save_path.get_base_dir()
	if not dir_path.is_empty() and not DirAccess.dir_exists_absolute(dir_path):
		var err: Error = DirAccess.make_dir_recursive_absolute(dir_path)
		if err != OK:
			printerr("SpacetimeDBClient: Failed to create directory for token: ", dir_path)
			return
	var file: FileAccess = FileAccess.open(token_save_path, FileAccess.WRITE)
	if file:
		file.store_string(token_to_save)
		file.close()
	else:
		printerr("SpacetimeDBClient: Failed to save token to path: ", token_save_path)


## Allocates the deserializer thread + sync primitives when threading is enabled.
## Idempotent: safe to call from both connect_db() and initialize_and_connect().
func _setup_threading() -> void:
	if deserializer_worker != null:
		return
	if OS.has_feature("web") and use_threading:
		push_error("Threads are not supported on Web. Threading has been disabled.")
		use_threading = false
	if not use_threading:
		return
	_packet_mutex = Mutex.new()
	_packet_semaphore = Semaphore.new()
	_result_mutex = Mutex.new()
	deserializer_worker = Thread.new()
	deserializer_worker.start(_thread_loop)


func _on_websocket_message_received(raw_bytes: PackedByteArray) -> void:
	if not _is_initialized:
		return
	if use_threading:
		_packet_mutex.lock()
		_packet_queue.append(raw_bytes)
		_packet_mutex.unlock()
		_packet_semaphore.post()
	else:
		_result_queue.append_array(_parse_packet_and_get_resource(_decompress_and_parse(raw_bytes)))


func _thread_loop() -> void:
	while not _thread_should_exit:
		_packet_semaphore.wait()
		if _thread_should_exit:
			break

		# Drain all pending packets in one lock acquisition
		_packet_mutex.lock()
		if _packet_queue.is_empty():
			_packet_mutex.unlock()
			continue
		var local_packets: Array[PackedByteArray] = []
		local_packets.assign(_packet_queue)
		_packet_queue.clear()
		var batch_epoch: int = _session_epoch
		_packet_mutex.unlock()

		# Parse all packets without holding any lock
		var local_results: Array[SpacetimeDBServerMessage] = []
		for packet: PackedByteArray in local_packets:
			var payload: PackedByteArray = _decompress_and_parse(packet)
			local_results.append_array(_parse_packet_and_get_resource(payload))

		# Flush parsed results in one lock acquisition — but only if a reconnect
		# hasn't bumped the session epoch while we were parsing, otherwise these
		# results belong to a dead session and must not touch the fresh database.
		if not local_results.is_empty():
			_packet_mutex.lock()
			var still_current: bool = batch_epoch == _session_epoch
			_packet_mutex.unlock()
			if still_current:
				_result_mutex.lock()
				_result_queue.append_array(local_results)
				_result_mutex.unlock()
			else:
				print_log("SpacetimeDBClient: discarded %d stale results from a prior session." % local_results.size())


func _process_results_asynchronously() -> void:
	if use_threading and not _result_mutex:
		return

	# Refill the held batch only when the previous one is fully drained. While a
	# batch is in flight (cursor < size) no lock is taken at all — newly parsed
	# messages stay in _result_queue and are picked up in arrival order once the
	# batch finishes, so a multi-frame backlog drains via an advancing cursor,
	# never re-sliced (O(1)/frame vs O(remaining) copy/frame).
	if _drain_cursor >= _drain_batch.size():
		if use_threading:
			_result_mutex.lock()
		if _result_queue.is_empty():
			if use_threading:
				_result_mutex.unlock()
			return
		# COW handoff under the lock the parser appends under: _drain_batch takes
		# the queued messages, _result_queue is reset for the parser to fill anew.
		_drain_batch = _result_queue
		_result_queue = []
		if use_threading:
			_result_mutex.unlock()
		_drain_cursor = 0

	var remaining: int = _drain_batch.size() - _drain_cursor

	# Adapt the budget from this frame's backlog + current fps before draining.
	_auto_tune_budget(remaining)

	# Drain under a per-frame time budget, bounded by a hard message ceiling.
	# Stop rule is pure (_should_stop_drain) and checked with elapsed measured
	# AFTER each handle, so at least one message always makes progress even if a
	# single message exceeds the whole budget.
	var start_us: int = Time.get_ticks_usec()
	var processed: int = 0
	while not _should_stop_drain(
		processed,
		remaining,
		_max_msgs_per_frame,
		Time.get_ticks_usec() - start_us,
		_frame_budget_us,
	):
		_handle_parsed_message(_drain_batch[_drain_cursor])
		_drain_cursor += 1
		processed += 1

	# Release the batch once fully drained so its memory frees and the next frame
	# refills from the queue. Partially-drained batches persist with their cursor.
	if _drain_cursor >= _drain_batch.size():
		_drain_batch = []
		_drain_cursor = 0


## AIMD feedback loop for [member _frame_budget_us]. Drain runs on the main
## thread, so an oversized budget steals frame time and drops fps. While a
## backlog exists and fps is healthy, additively grow the budget; the moment
## fps dips below target, multiplicatively back off. Clamped to the configured
## min/max. [param pending] is this frame's pre-drain backlog size.
func _auto_tune_budget(pending: int) -> void:
	if not _auto_tune_budget_enabled:
		return
	var target_fps: int = _auto_tune_target_fps
	if target_fps <= 0:
		target_fps = Engine.physics_ticks_per_second
	var fps: float = Engine.get_frames_per_second()
	_frame_budget_us = _compute_tuned_budget(
		_frame_budget_us,
		fps,
		target_fps,
		pending,
		_frame_budget_min_us,
		_frame_budget_max_us,
	)


## Pure AIMD step — returns the next budget for the given state, so the controller
## is unit-testable without engine fps. [param fps]<=0 (cold start) or
## [param target_fps]<=0 → unchanged. fps below 95% of target → ×0.8 (back off);
## fps at/above 99% of target with pending work → +500us (ramp). Result clamped
## to [param min_us]/[param max_us]. The 95–99% gap is intentional hysteresis.
static func _compute_tuned_budget(
		current: int,
		fps: float,
		target_fps: int,
		pending: int,
		min_us: int,
		max_us: int,
) -> int:
	if target_fps <= 0 or fps <= 0.0:
		return current
	if fps < target_fps * 0.95:
		return maxi(min_us, int(current * 0.8))
	if pending > 0 and fps >= target_fps * 0.99:
		return mini(max_us, current + 500)
	return current


## Pure resolve+clamp of the per-frame drain limits from raw option values, so
## the clamping is unit-testable without a live connection. Returns a
## [PackedInt32Array] [code][max_msgs, min_us, max_us, budget_us, target_fps][/code].
## [param max_msgs] clamped to [1, 8192] — floor 1 keeps the loop progressing,
## ceiling keeps the hard ceiling a real bounded-loop backstop even if
## misconfigured huge. [param min_us] floored at 100: a 0 budget makes the drain
## time-check true after the first message, capping drain at 1/frame and starving
## the backlog (reviewer MEDIUM). [param max_us] floored at the resolved min so the
## clamp range is never inverted. [param budget_us] clamped into the resolved
## [min, max]. [param target_fps] floored at 0 (0 = use physics tick rate).
static func _resolve_drain_config(
		max_msgs: int,
		min_us: int,
		max_us: int,
		budget_us: int,
		target_fps: int,
) -> PackedInt32Array:
	var r_min: int = maxi(100, min_us)
	var r_max: int = maxi(r_min, max_us)
	var r_budget: int = clampi(maxi(0, budget_us), r_min, r_max)
	return PackedInt32Array(
		[
			clampi(max_msgs, 1, 8192),
			r_min,
			r_max,
			r_budget,
			maxi(0, target_fps),
		],
	)


## Pure stop rule for the per-frame drain loop, so the bounded-loop + at-least-one
## progress guarantees are unit-testable without wall-clock time. Stop when the
## batch is exhausted, the hard message ceiling is reached, or the time budget is
## spent — but never before the first message ([param processed] == 0 always
## proceeds), so a single message costlier than the whole budget still makes
## progress. Checked with [param elapsed_us] measured AFTER each handle.
static func _should_stop_drain(
		processed: int,
		batch_size: int,
		max_msgs: int,
		elapsed_us: int,
		budget_us: int,
) -> bool:
	if processed >= batch_size:
		return true
	if processed == 0:
		return false
	if processed >= max_msgs:
		return true
	return elapsed_us >= budget_us


func _decompress_and_parse(raw_bytes: PackedByteArray) -> PackedByteArray:
	if raw_bytes.size() < 2:
		printerr("SpacetimeDBClient: Received packet too small (%d bytes), ignoring." % raw_bytes.size())
		return PackedByteArray()
	var compression: int = raw_bytes.get(0)
	var payload: PackedByteArray = raw_bytes.slice(1)
	if compression == 0:
		pass
	elif compression == 1:
		payload = DataDecompressor.decompress_brotli(payload)
		if payload.is_empty():
			printerr("SpacetimeDBClient: Brotli decompression failed, dropping frame.")
			return PackedByteArray()
	elif compression == 2:
		payload = DataDecompressor.decompress_packet(payload)
	else:
		printerr("SpacetimeDBClient: Unknown compression tag %d, dropping frame." % compression)
		return PackedByteArray()
	return payload


func _parse_packet_and_get_resource(bsatn_bytes: PackedByteArray) -> Array[SpacetimeDBServerMessage]:
	if not _deserializer:
		return []

	var result: Array[SpacetimeDBServerMessage] = _deserializer.process_bytes_and_extract_messages(bsatn_bytes)

	if _deserializer.has_error():
		printerr("SpacetimeDBClient: Failed to parse BSATN packet: ", _deserializer.get_last_error())
		return []

	return result


func _handle_parsed_message(message: SpacetimeDBServerMessage) -> void:
	if message == null:
		printerr("SpacetimeDBClient: Parser returned null message.")
		return

	# Handle known message types. Arms ordered hottest-first: TransactionUpdate +
	# ReducerResult are the steady-state firehose, so they win the `is`-chain
	# without walking past one-shot setup arms (IdentityToken fires once per
	# session). Arms are type-disjoint — order is behavior-neutral, perf-only.

	if message is TransactionUpdateMessage:
		_handle_transaction_update(message)

	elif message is ReducerResultMessage:
		var rid: int = message.request_id
		var outcome: ReducerOutcomeEnum = message.reducer_result
		var tx_update: TransactionUpdateMessage = null
		var handle: SpacetimeDBReducerCall = _pending_reducer_calls.get(rid)
		# Only stamp the handle if it's still PENDING (avoids overwriting a TIMEOUT verdict)
		var can_stamp: bool = handle and handle.outcome == SpacetimeDBReducerCall.Outcome.PENDING
		var _outcome_value: int = outcome.value
		if _outcome_value == ReducerOutcomeEnum.Options.ok:
			tx_update = outcome.get_ok()
			if tx_update != null:
				_handle_transaction_update(tx_update)
			if can_stamp:
				handle.outcome = SpacetimeDBReducerCall.Outcome.OK
				handle.transaction_update = tx_update
				handle.ret_value = message.ret_value
		elif _outcome_value == ReducerOutcomeEnum.Options.okEmpty:
			if can_stamp:
				handle.outcome = SpacetimeDBReducerCall.Outcome.OK_EMPTY
		elif _outcome_value == ReducerOutcomeEnum.Options.err:
			var err_bytes: PackedByteArray = outcome.get_err()
			var err_msg: String = _decode_reducer_error(err_bytes)
			print_log("SpacetimeDBClient: Reducer returned error: %s" % err_msg)
			if can_stamp:
				handle.outcome = SpacetimeDBReducerCall.Outcome.ERROR
				handle.error_message = err_msg
		elif _outcome_value == ReducerOutcomeEnum.Options.internalError:
			var err_msg: String = outcome.get_internal_error()
			printerr("SpacetimeDBClient: Reducer internal error: ", err_msg)
			if can_stamp:
				handle.outcome = SpacetimeDBReducerCall.Outcome.INTERNAL_ERROR
				handle.error_message = err_msg
		else:
			push_error("SpacetimeDBClient: unknown status_tag %d" % outcome.value)
			if can_stamp:
				handle.outcome = SpacetimeDBReducerCall.Outcome.INTERNAL_ERROR
				handle.error_message = "unknown reducer outcome tag %d" % outcome.value
		_pending_reducer_calls.erase(rid)
		_stats.record_response(rid)
		_reducer_result_cache[rid] = tx_update
		_evict_oldest(_reducer_result_cache)
		reducer_result_received.emit(rid, tx_update)

	elif message is OneOffQueryResponseMessage:
		var rid: int = message.request_id
		_stats.record_response(rid)
		if message.is_error:
			printerr("SpacetimeDBClient: OneOffQuery error (request_id=%d): %s" % [rid, message.error_message])
			_one_off_query_cache[rid] = [] as Array[TableUpdateData]
			_evict_oldest(_one_off_query_cache)
			one_off_query_received.emit(rid, [] as Array[TableUpdateData], message.error_message)
		else:
			print_log("SpacetimeDBClient: OneOffQuery result (request_id=%d): %d tables" % [rid, message.tables.size()])
			_one_off_query_cache[rid] = message.tables
			_evict_oldest(_one_off_query_cache)
			one_off_query_received.emit(rid, message.tables, "")

	elif message is ProcedureResultData:
		var rid: int = message.request_id
		var handle: SpacetimeDBProcedureCall = _pending_procedure_calls.get(rid)
		var can_stamp: bool = handle and handle.outcome == SpacetimeDBProcedureCall.Outcome.PENDING
		var ret_bytes: PackedByteArray = PackedByteArray()

		var _status_tag: int = message.status_tag
		if _status_tag == 0: # Returned
			ret_bytes = message.return_bytes
			if can_stamp:
				handle.outcome = SpacetimeDBProcedureCall.Outcome.RETURNED
				handle.return_bytes = ret_bytes
		elif _status_tag == 1: # InternalError
			printerr("SpacetimeDBClient: Procedure internal error: ", message.error_message)
			if can_stamp:
				handle.outcome = SpacetimeDBProcedureCall.Outcome.INTERNAL_ERROR
				handle.error_message = message.error_message
		else:
			push_error("SpacetimeDBClient: unknown status_tag %d" % message.status_tag)
			if can_stamp:
				handle.outcome = SpacetimeDBProcedureCall.Outcome.INTERNAL_ERROR
				handle.error_message = "unknown procedure status_tag %d" % message.status_tag

		_pending_procedure_calls.erase(rid)
		_stats.record_response(rid)
		_procedure_result_cache[rid] = ret_bytes
		_evict_oldest(_procedure_result_cache)
		procedure_result_received.emit(rid, ret_bytes)

	# --- Cold arms: setup / one-shot / rare. Kept after the hot path above. ---

	elif message is SubscribeAppliedMessage:
		_stats.record_response(message.request_id)
		print_log("SpacetimeDBClient: SubscribeApplied — tables: %d, query_set_id: %d" % [message.tables.size(), message.query_set_id.id])
		for t: TableUpdateData in message.tables:
			print_log("  Table: '%s' inserts=%d deletes=%d" % [t.table_name, t.inserts.size(), t.deletes.size()])
		_local_db.apply_database_subscription_applied(message)
		if not _received_initial_subscription:
			_received_initial_subscription = true
			self.database_initialized.emit()
		var qid: int = message.query_set_id.id
		if pending_subscriptions.has(qid):
			var sub: SpacetimeDBSubscription = pending_subscriptions[qid]
			pending_subscriptions.erase(qid)
			current_subscriptions[qid] = sub
			sub.applied.emit()

	elif message is SubscriptionErrorMessage:
		printerr("SpacetimeDBClient: Subscription error: %s" % message.error_message)
		if message.has_query_id():
			var qid: int = message.query_id.id
			if pending_subscriptions.has(qid):
				var sub: SpacetimeDBSubscription = pending_subscriptions[qid]
				pending_subscriptions.erase(qid)
				sub.error_message = message.error_message
				sub.end.emit()
			elif current_subscriptions.has(qid):
				var sub: SpacetimeDBSubscription = current_subscriptions[qid]
				current_subscriptions.erase(qid)
				sub.error_message = message.error_message
				sub.end.emit()
				# Already-applied subscription: prune exactly its rows. The server sends no
				# dropped rows on an error, so LocalDatabase reconstructs them from per-query
				# membership and decrements their refcounts — rows still held by another
				# subscription survive. No disconnect/rebuild needed, regardless of auto_reconnect.
				_local_db.prune_query(qid)
				print_log("SpacetimeDBClient: SubscriptionError on applied query_id %d; pruned its rows." % qid)

	elif message is UnsubscribeAppliedMessage:
		var qid: int = message.query_id.id
		if not message.tables.is_empty():
			for table_update: TableUpdateData in message.tables:
				_local_db.apply_table_update(table_update, qid)
		_local_db.forget_query(qid)
		if current_subscriptions.has(qid):
			var sub: SpacetimeDBSubscription = current_subscriptions[qid]
			current_subscriptions.erase(qid)
			sub.end.emit()
		print_log("SpacetimeDBClient: Unsubscribe applied for query_id %d." % qid)

	elif message is IdentityTokenMessage:
		print_log("SpacetimeDBClient: Received Identity Token.")
		_identity = message.identity
		if not _token and message.token:
			_token = message.token
		_connection_id = message.connection_id
		self.connected.emit(_identity, _token)

		# Handle reconnection completion
		if _reconnect_state == _ReconnectState.RECONNECTING:
			print_log("SpacetimeDBClient: Reconnected. Re-subscribing to %d query sets." % _saved_subscription_queries.size())
			_reconnect_state = _ReconnectState.IDLE
			_reconnect_attempt = 0
			if _saved_subscription_queries.is_empty():
				reconnected.emit()
			else:
				_resubscribe_saved_queries()

	else:
		print_log("SpacetimeDBClient: Unhandled message type: " + message.get_class())


## Decodes the BSATN payload of a reducer `err` outcome into a readable message.
## The payload is a BSATN value of the reducer's declared error type; the common case
## (Result<_, String>) is a u32-length-prefixed UTF-8 string, so strip that prefix.
## Falls back to raw UTF-8, then a hex dump, for non-string / malformed payloads.
func _decode_reducer_error(err_bytes: PackedByteArray) -> String:
	if err_bytes.is_empty():
		return ""
	if err_bytes.size() >= 4:
		var n: int = err_bytes.decode_u32(0)
		if n == err_bytes.size() - 4:
			return err_bytes.slice(4).get_string_from_utf8()
	var raw: String = err_bytes.get_string_from_utf8()
	if not raw.is_empty():
		return raw
	return "raw error bytes: " + err_bytes.hex_encode()


func _handle_transaction_update(update_sets: TransactionUpdateMessage) -> void:
	for dataset: DatabaseUpdateData in update_sets.query_sets:
		_local_db.apply_database_update(dataset)
		if not _received_initial_subscription:
			_received_initial_subscription = true
			self.database_initialized.emit()
	# Emit the full transaction update signal regardless of status
	self.transaction_update_received.emit(update_sets)

# --- Reconnection ---


func _on_connection_disconnected() -> void:
	_response_wait_aborted.emit()
	if _intentional_disconnect:
		_intentional_disconnect = false
		disconnected.emit()
		return

	if connection_options and connection_options.auto_reconnect:
		print_log("SpacetimeDBClient: Unintentional disconnect, starting auto-reconnect.")
		_start_reconnection()
	else:
		disconnected.emit()


func _on_connection_error(code: int, reason: String) -> void:
	_response_wait_aborted.emit()
	if _intentional_disconnect:
		_intentional_disconnect = false
		connection_error.emit(code, reason)
		return

	if _reconnect_state == _ReconnectState.RECONNECTING:
		print_log("SpacetimeDBClient: Reconnect attempt %d failed: %s (code %d)" % [_reconnect_attempt, reason, code])
		_schedule_next_reconnect_attempt()
	elif connection_options and connection_options.auto_reconnect:
		print_log("SpacetimeDBClient: Connection error, starting auto-reconnect. Reason: %s" % reason)
		connection_error.emit(code, reason)
		_start_reconnection()
	else:
		connection_error.emit(code, reason)


## Handles a stall-induced abnormal close. The socket really did close, but the
## cause was a local main-thread freeze (the engine heartbeat missed a pong while
## the thread was stalled), not a network fault — so reconnect immediately without
## the escalating backoff a genuine drop would warrant.
func _on_connection_stalled(code: int) -> void:
	_response_wait_aborted.emit()
	if _intentional_disconnect:
		_intentional_disconnect = false
		return
	if _reconnect_state == _ReconnectState.RECONNECTING:
		_reconnect_immediate = true # a stall during reconnect keeps the fast path
		_schedule_next_reconnect_attempt()
	elif connection_options and connection_options.auto_reconnect:
		print_log("SpacetimeDBClient: stall-induced close (code %d) — fast reconnect, no backoff." % code)
		_start_reconnection(true)
	else:
		connection_error.emit(code, "Abnormal closure (stall)")


func _start_reconnection(immediate: bool = false) -> void:
	if _reconnect_state == _ReconnectState.RECONNECTING:
		return

	_reconnect_state = _ReconnectState.RECONNECTING
	_reconnect_attempt = 0
	_reconnect_immediate = immediate
	# Supersede any in-flight resubscribe cycle so its late settles bail (see _resubscribe_epoch).
	_resubscribe_epoch += 1

	# Only rebuild the saved set when it's empty. A re-drop that lands mid-resubscribe
	# must keep the queries from the interrupted cycle — at that moment they sit in
	# pending_subscriptions (not yet applied), so rebuilding from current_subscriptions
	# alone would lose them.
	if _saved_subscription_queries.is_empty():
		for sub_id: int in current_subscriptions:
			var sub: SpacetimeDBSubscription = current_subscriptions[sub_id]
			if not sub.queries.is_empty():
				_saved_subscription_queries.append(sub.queries.duplicate())
		for sub_id: int in pending_subscriptions:
			var sub: SpacetimeDBSubscription = pending_subscriptions[sub_id]
			if not sub.queries.is_empty():
				_saved_subscription_queries.append(sub.queries.duplicate())
	print_log("SpacetimeDBClient: Saved %d subscription query sets for re-subscription." % _saved_subscription_queries.size())

	_schedule_next_reconnect_attempt()


func _schedule_next_reconnect_attempt() -> void:
	var max_attempts: int = connection_options.max_reconnect_attempts

	if max_attempts > 0 and _reconnect_attempt >= max_attempts:
		print_log("SpacetimeDBClient: All %d reconnect attempts exhausted." % max_attempts)
		_reconnect_state = _ReconnectState.IDLE
		_reconnect_attempt = 0
		_saved_subscription_queries.clear()
		reconnect_failed.emit()
		disconnected.emit()
		return

	_reconnect_attempt += 1
	var backoff: float = 0.0 if _reconnect_immediate else _calculate_backoff(_reconnect_attempt)
	_reconnect_immediate = false # one-shot: only the first stall-induced attempt skips backoff
	var max_str: String = str(max_attempts) if max_attempts > 0 else "inf"
	print_log("SpacetimeDBClient: Reconnect attempt %d/%s in %.2f seconds." % [_reconnect_attempt, max_str, backoff])

	reconnecting.emit(_reconnect_attempt, max_attempts)

	var tree: SceneTree = get_tree()
	if not tree:
		printerr("SpacetimeDBClient: Cannot schedule reconnect — not in scene tree.")
		_reconnect_state = _ReconnectState.IDLE
		reconnect_failed.emit()
		disconnected.emit()
		return

	_reconnect_timer = tree.create_timer(backoff)
	if _reconnect_timer:
		_reconnect_timer.timeout.connect(_attempt_reconnect, CONNECT_ONE_SHOT)
	else:
		printerr("SpacetimeDBClient: Failed to create reconnect timer.")
		_reconnect_state = _ReconnectState.IDLE
		reconnect_failed.emit()
		disconnected.emit()


func _calculate_backoff(attempt: int) -> float:
	var base_delay: float = connection_options.reconnect_initial_delay * pow(
		connection_options.reconnect_backoff_multiplier,
		attempt - 1,
	)
	base_delay = minf(base_delay, connection_options.reconnect_max_delay)

	var jitter_range: float = base_delay * connection_options.reconnect_jitter_fraction
	var jitter_offset: float = randf() * jitter_range
	return maxf(0.0, base_delay - jitter_offset)


func _attempt_reconnect() -> void:
	_reconnect_timer = null

	if _reconnect_state != _ReconnectState.RECONNECTING:
		return

	if not _connection or _token.is_empty():
		printerr("SpacetimeDBClient: Cannot reconnect — missing connection or token.")
		_reconnect_state = _ReconnectState.IDLE
		reconnect_failed.emit()
		disconnected.emit()
		return

	_prepare_for_reconnect()

	var conn_id: String = _generate_connection_id()
	_connection.set_token(_token)

	print_log("SpacetimeDBClient: Attempting reconnect (attempt %d)." % _reconnect_attempt)
	_connection.connect_to_database(base_url, database_name, conn_id)


func _prepare_for_reconnect() -> void:
	if _local_db:
		_local_db.clear_all_tables()

	_reducer_result_cache.clear()
	for call_id: int in _pending_reducer_calls:
		var handle: SpacetimeDBReducerCall = _pending_reducer_calls[call_id]
		if handle.outcome == SpacetimeDBReducerCall.Outcome.PENDING:
			handle.outcome = SpacetimeDBReducerCall.Outcome.DISCONNECTED
			handle.error_message = "Connection lost during reducer call"
	_pending_reducer_calls.clear()

	# Cleared so a post-reconnect request_id (counter resets to 0 below) can't read a
	# stale pre-disconnect one-off result out of the cache.
	_one_off_query_cache.clear()

	_procedure_result_cache.clear()
	for call_id: int in _pending_procedure_calls:
		var handle: SpacetimeDBProcedureCall = _pending_procedure_calls[call_id]
		if handle.outcome == SpacetimeDBProcedureCall.Outcome.PENDING:
			handle.outcome = SpacetimeDBProcedureCall.Outcome.DISCONNECTED
			handle.error_message = "Connection lost during procedure call"
	_pending_procedure_calls.clear()

	for sub: SpacetimeDBSubscription in pending_subscriptions.values():
		sub.end.emit()
	for sub: SpacetimeDBSubscription in current_subscriptions.values():
		sub.end.emit()
	pending_subscriptions.clear()
	current_subscriptions.clear()

	_received_initial_subscription = false
	_next_query_id = 0
	_next_request_id = 0

	if use_threading and _packet_mutex:
		_packet_mutex.lock()
		# Bump the epoch under the same lock the worker drains under, so any batch
		# it has already pulled will fail its post-parse epoch check and be dropped.
		_session_epoch += 1
		_packet_queue.clear()
		_packet_mutex.unlock()

		_result_mutex.lock()
		_result_queue.clear()
		_result_mutex.unlock()

	# Drop any in-flight batch from the old session so its messages aren't applied
	# to the fresh post-reconnect database (main-thread-only state).
	_drain_batch = []
	_drain_cursor = 0


func _cancel_reconnection() -> void:
	if _reconnect_state == _ReconnectState.IDLE:
		return

	print_log("SpacetimeDBClient: Cancelling reconnection.")
	_reconnect_state = _ReconnectState.IDLE
	_reconnect_attempt = 0
	_reconnect_immediate = false
	_resubscribe_epoch += 1 # supersede any in-flight resubscribe settles
	_saved_subscription_queries.clear()

	if _reconnect_timer and _reconnect_timer.time_left > 0:
		if _reconnect_timer.timeout.is_connected(_attempt_reconnect):
			_reconnect_timer.timeout.disconnect(_attempt_reconnect)
	_reconnect_timer = null


func _resubscribe_saved_queries() -> void:
	_resubscribe_epoch += 1
	var epoch: int = _resubscribe_epoch
	# Snapshot so a re-entrant _start_reconnection rebuilding _saved_subscription_queries
	# (on a drop mid-resubscribe) can't disturb this loop (mutation-during-iteration).
	var query_sets: Array[PackedStringArray] = _saved_subscription_queries.duplicate()
	var total_sets: int = query_sets.size()
	var applied_count: Array[int] = [0]

	if total_sets == 0:
		_finish_resubscribe(epoch)
		return

	for queries: PackedStringArray in query_sets:
		var sub: SpacetimeDBSubscription = subscribe(queries)
		if sub.error != OK:
			printerr("SpacetimeDBClient: Failed to re-subscribe during reconnection: %s" % error_string(sub.error))
			applied_count[0] += 1
			if applied_count[0] >= total_sets:
				_finish_resubscribe(epoch)
			continue

		var settled: Array[bool] = [false]
		var on_settled: Callable = func() -> void:
			# Bail if this sub already settled, or a newer reconnect cycle superseded us.
			if settled[0] or epoch != _resubscribe_epoch:
				return
			settled[0] = true
			applied_count[0] += 1
			print_log("SpacetimeDBClient: Re-subscription settled (%d/%d)." % [applied_count[0], total_sets])
			if applied_count[0] >= total_sets:
				_finish_resubscribe(epoch)
		sub.applied.connect(on_settled, CONNECT_ONE_SHOT)
		sub.end.connect(on_settled, CONNECT_ONE_SHOT)


## Completes a resubscribe cycle: clears the saved set and emits [signal reconnected],
## but only if [param epoch] is still current — a superseded cycle does nothing.
func _finish_resubscribe(epoch: int) -> void:
	if epoch != _resubscribe_epoch:
		return
	_saved_subscription_queries.clear()
	reconnected.emit()


func _evict_oldest(cache: Dictionary) -> void:
	while cache.size() > _MAX_RESULT_CACHE_SIZE and not cache.is_empty():
		# Grab the first (oldest-inserted) key via iteration — no keys() array alloc.
		for oldest_key: Variant in cache:
			cache.erase(oldest_key)
			break
