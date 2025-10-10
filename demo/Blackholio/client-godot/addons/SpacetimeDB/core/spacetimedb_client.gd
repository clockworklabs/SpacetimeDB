@tool
class_name SpacetimeDBClient extends Node

# --- Configuration ---
@export var base_url: String = "http://127.0.0.1:3000"
@export var database_name: String = "quickstart-chat" # Example
@export var schema_path: String = "res://spacetime_bindings/schema"
@export var auto_connect: bool = false
@export var auto_request_token: bool = true
@export var token_save_path: String = "user://spacetimedb_token.dat" # Use a more specific name
@export var one_time_token: bool = false
@export var compression: SpacetimeDBConnection.CompressionPreference
@export var debug_mode: bool = true
@export var current_subscriptions: Dictionary[int, SpacetimeDBSubscription]
@export var use_threading: bool = true

var deserializer_worker: Thread
var _packet_queue: Array[PackedByteArray] = []
var _packet_semaphore: Semaphore
var _result_queue: Array[Resource] = []
var _result_mutex: Mutex
var _packet_mutex: Mutex
var _thread_should_exit: bool = false
var _message_limit_in_frame: int = 5

var connection_options: SpacetimeDBConnectionOptions
var pending_subscriptions: Dictionary[int, SpacetimeDBSubscription]

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
var _is_initialized := false
var _received_initial_subscription := false
var _next_query_id := 0
var _next_request_id := 0

# --- Signals ---
signal connected(identity: PackedByteArray, token: String)
signal disconnected
signal connection_error(code: int, reason: String)
signal database_initialized # Emitted after InitialSubscription is processed
signal database_update(table_update: TableUpdateData) # Emitted for each table update

# From LocalDatabase
signal row_inserted(table_name: String, row: Resource)
signal row_updated(table_name: String, old_row: Resource, new_row: Resource)
signal row_deleted(table_name: String, row: Resource)
signal row_transactions_completed(table_name: String)

signal reducer_call_response(response: Resource) # TODO: Define response resource
signal reducer_call_timeout(request_id: int) # TODO: Implement timeout logic
signal transaction_update_received(update: TransactionUpdateMessage)

func _ready():
	if auto_connect:
		initialize_and_connect()
		
func _exit_tree():
	if deserializer_worker:
		_thread_should_exit = true
		_packet_semaphore.post()
		deserializer_worker.wait_to_finish()
		deserializer_worker = null
		
func print_log(log_message: String):
	if debug_mode:
		print(log_message)
	
func initialize_and_connect():
	if _is_initialized:
		return

	print_log("SpacetimeDBClient: Initializing...")
	
	# 1. Load Schema
	var module_name: String = get_meta("module_name", "")
	var schema := SpacetimeDBSchema.new(module_name, schema_path, debug_mode)

	# 2. Initialize Parser
	_deserializer = BSATNDeserializer.new(schema, debug_mode)
	_serializer = BSATNSerializer.new()

	# 3. Initialize Local Database
	_local_db = LocalDatabase.new(schema)
	_init_db(_local_db)
	
	# Connect to LocalDatabase signals to re-emit them
	_local_db.row_inserted.connect(func(tn, r) -> void: row_inserted.emit(tn, r))
	_local_db.row_updated.connect(func(tn, p, r) -> void: row_updated.emit(tn, p, r))
	_local_db.row_deleted.connect(func(tn, r) -> void: row_deleted.emit(tn, r))
	_local_db.row_transactions_completed.connect(func(tn) -> void: row_transactions_completed.emit(tn))
	_local_db.name = "LocalDatabase"
	add_child(_local_db) # Add as child if it needs signals

	# 4. Initialize REST API Handler (optional, mainly for token)
	_rest_api = SpacetimeDBRestAPI.new(base_url, debug_mode)
	_rest_api.token_received.connect(_on_token_received)
	_rest_api.token_request_failed.connect(_on_token_request_failed)
	_rest_api.name = "RestAPI"
	add_child(_rest_api)

	# 5. Initialize Connection Handler
	_connection = SpacetimeDBConnection.new(connection_options)
	_connection.disconnected.connect(func(): disconnected.emit())
	_connection.connection_error.connect(func(c, r): connection_error.emit(c, r))
	_connection.message_received.connect(_on_websocket_message_received)
	_connection.name = "Connection"
	add_child(_connection)

	_is_initialized = true
	print_log("SpacetimeDBClient: Initialization complete.")

	# 6. Get Token and Connect
	_load_token_or_request()

func _init_db(local_db: LocalDatabase) -> void:
	pass

func _load_token_or_request():
	if _token:
		# If token is already set, use it
		_on_token_received(_token)
		return
	
	if one_time_token == false:
		# Try loading saved token
		if FileAccess.file_exists(token_save_path):
			var file := FileAccess.open(token_save_path, FileAccess.READ)
			if file:
				var saved_token := file.get_as_text().strip_edges()
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
		emit_signal("connection_error", -1, "Authentication token unavailable")

func _generate_connection_id() -> String:
	var random_bytes := PackedByteArray()
	random_bytes.resize(16)
	var rng := RandomNumberGenerator.new()
	for i in 16:
		random_bytes[i] = rng.randi_range(0, 255)
	return random_bytes.hex_encode() # Return as hex string
		
func _on_token_received(received_token: String):
	print_log("SpacetimeDBClient: Token acquired.")
	self._token = received_token
	if not one_time_token:
		_save_token(received_token)
	var conn_id = _generate_connection_id()
	# Pass token to components that need it
	_connection.set_token(self._token)
	_rest_api.set_token(self._token) # REST API might also need it

	# Now attempt to connect WebSocket
	_connection.connect_to_database(base_url, database_name, conn_id)

func _on_token_request_failed(error_code: int, response_body: String):
	printerr("SpacetimeDBClient: Failed to acquire token. Cannot connect.")
	emit_signal("connection_error", error_code, "Failed to acquire authentication token")

func _save_token(token_to_save: String):
	var file := FileAccess.open(token_save_path, FileAccess.WRITE)
	if file:
		file.store_string(token_to_save)
		file.close()
	else:
		printerr("SpacetimeDBClient: Failed to save token to path: ", token_save_path)

# --- WebSocket Message Handling ---
func _physics_process(_delta: float) -> void:
	_process_results_asynchronously()
	
func _on_websocket_message_received(raw_bytes: PackedByteArray):
	if not _is_initialized: return
	if use_threading:
		_packet_mutex.lock()
		_packet_queue.append(raw_bytes)
		_packet_mutex.unlock()
		_packet_semaphore.post()
	else:
		var message = _parse_packet_and_get_resource(_decompress_and_parse(raw_bytes))
		_result_queue.append(message)
		
func _thread_loop() -> void:
	while not _thread_should_exit:
		_packet_semaphore.wait()
		if _thread_should_exit: break
		
		_packet_mutex.lock()
		
		if _packet_queue.is_empty():
			_packet_mutex.unlock()
			continue
			
		var packet_to_process: PackedByteArray = _packet_queue.pop_back()
		_packet_mutex.unlock()
		
		var message_resource: Resource = null
		var payload := _decompress_and_parse(packet_to_process)
		message_resource = _parse_packet_and_get_resource(payload)
		
		if message_resource:
			_result_mutex.lock()
			_result_queue.append(message_resource)
			_result_mutex.unlock()
			
func _process_results_asynchronously():
	if use_threading and not _result_mutex: return

	if use_threading: _result_mutex.lock()
	
	if _result_queue.is_empty():
		if use_threading: _result_mutex.unlock()
		return
	
	var processed_count = 0
	
	while not _result_queue.is_empty() and processed_count < _message_limit_in_frame:
		_handle_parsed_message(_result_queue.pop_front())
		processed_count += 1
		
	if use_threading: _result_mutex.unlock()

func _decompress_and_parse(raw_bytes: PackedByteArray) -> PackedByteArray:
	var compression = raw_bytes[0]
	var payload = raw_bytes.slice(1)
	match compression:
		0: pass
		1: printerr("SpacetimeDBClient (Thread) : Brotli compression not supported!")
		2: payload = DataDecompressor.decompress_packet(payload)
	return payload
	
func _parse_packet_and_get_resource(bsatn_bytes: PackedByteArray) -> Resource:
	if not _deserializer: return null
	
	var result := _deserializer.process_bytes_and_extract_messages(bsatn_bytes)
	if result.is_empty(): return null
	var message_resource: Resource = result[0]
	
	if _deserializer.has_error():
		printerr("SpacetimeDBClient: Failed to parse BSATN packet: ", _deserializer.get_last_error())
		return null
	
	return message_resource
		
func _handle_parsed_message(message_resource: Resource):
	if message_resource == null:
		printerr("SpacetimeDBClient: Parser returned null message resource.")
		return
		
	# Handle known message types
	if message_resource is InitialSubscriptionMessage:
		var initial_sub: InitialSubscriptionMessage = message_resource
		print_log("SpacetimeDBClient: Processing Initial Subscription (Query ID: %d)" % initial_sub.query_id.id)
		_local_db.apply_database_update(initial_sub.database_update)
		if not _received_initial_subscription:
			_received_initial_subscription = true
			self.database_initialized.emit()
		
	elif message_resource is SubscribeMultiAppliedMessage:
		var sub: SubscribeMultiAppliedMessage = message_resource
		print_log("SpacetimeDBClient: Processing Multi Subscription (Query ID: %d)" % sub.query_id.id)
		_local_db.apply_database_update(sub.database_update)
		if pending_subscriptions.has(sub.query_id.id):
			var subscription := pending_subscriptions[sub.query_id.id]
			current_subscriptions[sub.query_id.id] = subscription
			pending_subscriptions.erase(sub.query_id.id)
			subscription.applied.emit()
		
		if not _received_initial_subscription:
			_received_initial_subscription = true
			self.database_initialized.emit()
		
	elif message_resource is UnsubscribeMultiAppliedMessage:
		var unsub: UnsubscribeMultiAppliedMessage = message_resource
		_local_db.apply_database_update(unsub.database_update)
		print_log("Unsubscribe: " + str(current_subscriptions[unsub.query_id.id]))
		if current_subscriptions.has(unsub.query_id.id):
			var subscription := current_subscriptions[unsub.query_id.id]
			current_subscriptions.erase(unsub.query_id.id)
			subscription.end.emit()
			subscription.queue_free()
		
	elif message_resource is IdentityTokenMessage:
		var identity_token: IdentityTokenMessage = message_resource
		print_log("SpacetimeDBClient: Received Identity Token.")
		_identity = identity_token.identity
		if not _token and identity_token.token:
			_token = identity_token.token
		_connection_id = identity_token.connection_id
		self.connected.emit(_identity, _token)

	elif message_resource is TransactionUpdateMessage:
		var tx_update: TransactionUpdateMessage = message_resource
		#print_log("SpacetimeDBClient: Processing Transaction Update (Reducer: %s, Req ID: %d)" % [tx_update.reducer_call.reducer_name, tx_update.reducer_call.request_id])
		# Apply changes to local DB only if committed
		if tx_update.status.status_type == UpdateStatusData.StatusType.COMMITTED:
			if tx_update.status.committed_update: # Check if update data exists
				_local_db.apply_database_update(tx_update.status.committed_update)
			else:
				# This might happen if a transaction committed but affected 0 rows relevant to the client
				print_log("SpacetimeDBClient: Committed transaction had no relevant row updates.")
		elif tx_update.status.status_type == UpdateStatusData.StatusType.FAILED:
			printerr("SpacetimeDBClient: Reducer call failed: ", tx_update.status.failure_message)
		elif tx_update.status.status_type == UpdateStatusData.StatusType.OUT_OF_ENERGY:
			printerr("SpacetimeDBClient: Reducer call ran out of energy.")

		# Emit the full transaction update signal regardless of status
		self.transaction_update_received.emit(tx_update)

	else:
		print_log("SpacetimeDBClient: Received unhandled message resource type: " + message_resource.get_class())


# --- Public API ---

func connect_db(host_url: String, database_name: String, options: SpacetimeDBConnectionOptions = null):
	if not options:
		options = SpacetimeDBConnectionOptions.new()
	connection_options = options
	self.base_url = host_url
	self.database_name = database_name
	self.compression = options.compression
	self.one_time_token = options.one_time_token
	if options.token:
		self._token = options.token
	self.debug_mode = options.debug_mode
	self.use_threading = options.threading
	
	if OS.has_feature("web") and use_threading == true:
		push_error("Threads are not supported on Web. Threading has been disabled.")
		use_threading = false
		
	if use_threading:
		_packet_mutex = Mutex.new()
		_packet_semaphore = Semaphore.new()
		_result_mutex = Mutex.new()
		deserializer_worker = Thread.new()
		deserializer_worker.start(_thread_loop)
		
	if not _is_initialized:
		initialize_and_connect()
	elif not _connection.is_connected_db():
		# Already initialized, just need token and connect
		_load_token_or_request()

func disconnect_db():
	_token = ""
	if _connection:
		_connection.disconnect_from_server()

func is_connected_db() -> bool:
	return _connection and _connection.is_connected_db()

# The untyped local database instance, use the generated .Db property for querying
func get_local_database() -> LocalDatabase:
	return _local_db
	
func get_local_identity() -> PackedByteArray:
	return _identity
	
func subscribe(queries: PackedStringArray) -> SpacetimeDBSubscription:
	if not is_connected_db():
		printerr("SpacetimeDBClient: Cannot subscribe, not connected.")
		return SpacetimeDBSubscription.fail(ERR_CONNECTION_ERROR)

	# 1. Generate a request ID
	var query_id := _next_query_id
	_next_query_id += 1
	# 2. Create the correct payload Resource
	var payload_data := SubscribeMultiMessage.new(queries, query_id)

	# 3. Serialize the complete ClientMessage using the universal function
	var message_bytes := _serializer.serialize_client_message(
		SpacetimeDBClientMessage.SUBSCRIBE_MULTI,
		payload_data
	)

	if _serializer.has_error():
		printerr("SpacetimeDBClient: Failed to serialize SubscribeMulti message: %s" % _serializer.get_last_error())
		return SpacetimeDBSubscription.fail(ERR_PARSE_ERROR)

	# 4. Create subscription handle
	var subscription := SpacetimeDBSubscription.create(self, query_id, queries)
	
	# 5. Send the binary message via WebSocket
	if _connection and _connection._websocket:
		var err := _connection.send_bytes(message_bytes)
		if err != OK:
			printerr("SpacetimeDBClient: Error sending SubscribeMulti BSATN message: %s" % error_string(err))
			subscription.error = err
			subscription._ended = true
		else:
			print_log("SpacetimeDBClient: SubscribeMulti request sent successfully (BSATN), Query ID: %d" % query_id)
			pending_subscriptions.set(query_id, subscription)
			# Add as child for signals
			subscription.name = "Subscription"
			add_child(subscription)
			
		return subscription
   
	printerr("SpacetimeDBClient: Internal error - WebSocket peer not available in connection.")
	subscription.error = ERR_CONNECTION_ERROR
	subscription._ended = true
	return subscription

func unsubscribe(query_id: int) -> Error:
	if not is_connected_db():
		printerr("SpacetimeDBClient: Cannot unsubscribe, not connected.")
		return ERR_CONNECTION_ERROR
	
	# 1. Create the correct payload Resource
	var payload_data := UnsubscribeMultiMessage.new(query_id)
	
	# 2. Serialize the complete ClientMessage using the universal function
	var message_bytes := _serializer.serialize_client_message(
		SpacetimeDBClientMessage.UNSUBSCRIBE_MULTI,
		payload_data
	)

	if _serializer.has_error():
		printerr("SpacetimeDBClient: Failed to serialize SubscribeMulti message: %s" % _serializer.get_last_error())
		return ERR_PARSE_ERROR

	# 3. Send the binary message via WebSocket
	if _connection and _connection._websocket:
		var err := _connection.send_bytes(message_bytes)
		if err != OK:
			printerr("SpacetimeDBClient: Error sending SubscribeMulti BSATN message: %s" % error_string(err))
			return err

		print_log("SpacetimeDBClient: UnsubscribeMulti request sent successfully (BSATN), Query ID: %d" % query_id)
		return OK

	printerr("SpacetimeDBClient: Internal error - WebSocket peer not available in connection.")
	return ERR_CONNECTION_ERROR
	
func call_reducer(reducer_name: String, args: Array = [], types: Array = []) -> SpacetimeDBReducerCall:
	if not is_connected_db():
		printerr("SpacetimeDBClient: Cannot call reducer, not connected.")
		return SpacetimeDBReducerCall.fail(ERR_CONNECTION_ERROR)
	
	var args_bytes := _serializer._serialize_arguments(args, types)

	if _serializer.has_error():
		printerr("Failed to serialize args for %s: %s" % [reducer_name, _serializer.get_last_error()])
		return SpacetimeDBReducerCall.fail(ERR_PARSE_ERROR)
	
	var request_id := _next_request_id
	_next_request_id += 1
	
	var call_data := CallReducerMessage.new(reducer_name, args_bytes, request_id, 0)
	var message_bytes := _serializer.serialize_client_message(
		SpacetimeDBClientMessage.CALL_REDUCER,
		call_data
		)
	
	# Access the internal _websocket peer directly (might need adjustment if _connection API changes)
	if _connection and _connection._websocket: # Basic check
		var err := _connection.send_bytes(message_bytes)
		if err != OK:
			print("SpacetimeDBClient: Error sending CallReducer JSON message: ", err)
			return SpacetimeDBReducerCall.fail(err)
		
		return SpacetimeDBReducerCall.create(self, request_id)

	print("SpacetimeDBClient: Internal error - WebSocket peer not available in connection.")
	return SpacetimeDBReducerCall.fail(ERR_CONNECTION_ERROR)

func wait_for_reducer_response(request_id_to_match: int, timeout_seconds: float = 10.0) -> TransactionUpdateMessage:
	if request_id_to_match < 0:
		return null

	var signal_result: TransactionUpdateMessage = null
	var timeout_ms: float = timeout_seconds * 1000.0
	var start_time: float = Time.get_ticks_msec()

	while Time.get_ticks_msec() - start_time < timeout_ms:
		var received_signal = await transaction_update_received
		if _check_reducer_response(received_signal, request_id_to_match):
			signal_result = received_signal
			break

	if signal_result == null:
		printerr("SpacetimeDBClient: Timeout waiting for response for Req ID: %d" % request_id_to_match)
		self.reducer_call_timeout.emit(request_id_to_match)
		return null
	else:
		var tx_update: TransactionUpdateMessage = signal_result
		print_log("SpacetimeDBClient: Received matching response for Req ID: %d" % request_id_to_match)
		self.reducer_call_response.emit(tx_update.reducer_call)
		return tx_update

func _check_reducer_response(update: TransactionUpdateMessage, request_id_to_match: int) -> bool:
	return update != null and update.reducer_call != null and update.reducer_call.request_id == request_id_to_match
