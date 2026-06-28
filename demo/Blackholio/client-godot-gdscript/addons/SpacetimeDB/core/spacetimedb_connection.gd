## Low-level WebSocket transport for SpacetimeDB.
##
## Manages the WebSocket lifecycle (connect, poll, send, close) and emits raw
## packet data via [signal message_received]. [SpacetimeDBClient] owns an
## instance and wires its signals for higher-level message handling.
#@tool
class_name SpacetimeDBConnection
extends Node

## Emitted when the WebSocket handshake completes.
signal connected
## Emitted on a clean close (normal code).
signal disconnected
## Emitted on abnormal close or connection failure.
signal connection_error(code: int, reason: String)
## Emitted when an abnormal close (code -1) lands right after a main-thread stall
## long enough for the engine heartbeat to have falsely declared the socket dead.
## The close is real (the engine already closed the peer), but its cause is local,
## so the client can reconnect immediately instead of treating it as a network drop.
signal connection_stalled(code: int)
## Emitted for each raw BSATN packet received from the server.
signal message_received(data: PackedByteArray)
## Emitted after every send/receive with cumulative totals.
signal total_messages(sent: int, received: int)
## Emitted after every send/receive with cumulative byte totals.
signal total_bytes(sent: int, received: int)

## Payload compression modes for the WebSocket connection.
enum CompressionPreference {
	## No compression.
	NONE = 0,
	## Brotli compression (decoded via Godot's built-in Brotli decoder).
	BROTLI = 1,
	## Gzip compression.
	GZIP = 2,
}

## The BSATN sub-protocol sent during the WebSocket handshake (SpacetimeDB 2.2.0+).
## A single WebSocket frame may carry several consecutive BSATN messages; the receive
## path already drains concatenated frames, and one-message-per-frame sends remain
## valid, so this is the only protocol the client advertises. The legacy v2
## sub-protocol (servers below 2.2.0) is no longer offered — see the 2.0 changelog.
const BSATN_PROTOCOL_V3 = "v3.bsatn.spacetimedb"

var version: String = "v1"
## Sub-protocol the server selected during the handshake (e.g. "v3.bsatn.spacetimedb").
var negotiated_protocol: String = ""
var preferred_compression: CompressionPreference = CompressionPreference.NONE # Default to None
var _websocket: WebSocketPeer = WebSocketPeer.new()
var _target_url: String
var _token: String
var _is_connected: bool = false
var _connection_requested: bool = false
var _options: SpacetimeDBConnectionOptions
var _db_name: String
var _debug_mode: bool = false
var _total_bytes_sent: int = 0
var _second_bytes_sent: int = 0
var _total_bytes_received: int = 0
var _second_bytes_received: int = 0
var _total_messages_sent: int = 0
var _second_messages_sent: int = 0
var _total_messages_received: int = 0
var _second_messages_received: int = 0

## How many polls a stall keeps the abnormal-close guard armed. The engine may
## close on the same poll the stall is observed or the next, so cover both.
const STALL_GUARD_POLLS: int = 2
## Wall-clock ms of the previous poll (Time.get_ticks_msec). 0 = no prior poll.
var _last_poll_ms: int = 0
## Poll gap (ms) at or above which a stall could have tripped the engine heartbeat.
## Set from heartbeat_interval at connect; 0 disables stall detection (heartbeat off).
var _stall_threshold_ms: int = 0
## Polls remaining in the post-stall guard window; >0 means a stall was just seen.
var _post_stall_polls: int = 0


func _init(options: SpacetimeDBConnectionOptions, db_name: String) -> void:
	_options = options
	_db_name = db_name
	if options.monitor_mode:
		Performance.add_custom_monitor("spacetime/" + db_name + "_second_received_packets", get_second_received_packets)
		Performance.add_custom_monitor("spacetime/" + db_name + "_second_received_bytes", get_second_received_bytes)
		Performance.add_custom_monitor("spacetime/" + db_name + "_total_received_packets", get_received_packets)
		Performance.add_custom_monitor("spacetime/" + db_name + "_total_received_kbytes", get_received_kbytes)
		Performance.add_custom_monitor("spacetime/" + db_name + "_second_sent_packets", get_second_sent_packets)
		Performance.add_custom_monitor("spacetime/" + db_name + "_second_sent_bytes", get_second_sent_bytes)
		Performance.add_custom_monitor("spacetime/" + db_name + "_total_sent_packets", get_sent_packets)
		Performance.add_custom_monitor("spacetime/" + db_name + "_total_sent_kbytes", get_sent_kbytes)

	_websocket.inbound_buffer_size = options.inbound_buffer_size
	_websocket.outbound_buffer_size = options.outbound_buffer_size
	# Keepalive: peer pings every interval and closes (code -1) if a pong is missed,
	# surfacing a dead socket as STATE_CLOSED so the reconnect path can fire.
	_websocket.heartbeat_interval = options.heartbeat_interval_seconds
	_stall_threshold_ms = int(options.heartbeat_interval_seconds * 1000.0)
	set_compression_preference(options.compression)
	self._debug_mode = options.debug_mode
	set_physics_process(false) # Don't process until connect is called


func _physics_process(_delta: float) -> void:
	_track_stall()
	_websocket.poll()
	var state: WebSocketPeer.State = _websocket.get_ready_state()

	if state == WebSocketPeer.STATE_OPEN:
		if not _is_connected:
			negotiated_protocol = _websocket.get_selected_protocol()
			_print_log("SpacetimeDBConnection: Connection established (protocol: %s)." % negotiated_protocol)
			_is_connected = true
			_connection_requested = false
			connected.emit()

		# Process incoming packets
		var rx_before: int = _total_messages_received
		while _websocket.get_available_packet_count() > 0:
			var packet_bytes: PackedByteArray = _websocket.get_packet()
			if packet_bytes.is_empty():
				continue

			_total_bytes_received += packet_bytes.size()
			_second_bytes_received += packet_bytes.size()
			_total_messages_received += 1
			_second_messages_received += 1

			message_received.emit(packet_bytes)

		# Stat signals carry cumulative counters — last value per frame is all a
		# display consumer needs. Emit once after draining, not per packet, so a
		# burst of N packets costs one stat emit pair instead of N.
		if _total_messages_received != rx_before:
			total_messages.emit(_total_messages_sent, _total_messages_received)
			total_bytes.emit(_total_bytes_sent, _total_bytes_received)
	elif state == WebSocketPeer.STATE_CONNECTING:
		# Still trying to connect
		pass
	elif state == WebSocketPeer.STATE_CLOSING:
		# Connection is closing
		_print_log("SpacetimeDBConnection: connection closing")
		pass
	elif state == WebSocketPeer.STATE_CLOSED:
		var code: int = _websocket.get_close_code()
		var reason: String = _websocket.get_close_reason()
		if _is_connected or _connection_requested: # Only report if we were connected or trying
			if code == -1: # Abnormal closure
				if _post_stall_polls > 0: # heartbeat tripped by a local stall, not a network drop
					push_warning("SpacetimeDBConnection: abnormal close right after a main-thread stall — stall-induced, fast reconnect")
					_post_stall_polls = 0
					connection_stalled.emit(code)
				else:
					printerr("SpacetimeDBConnection: connection_error %d, abnormal closure. Reason: %s" % [code, reason])
					connection_error.emit(code, "Abnormal closure: %s" % reason)
			else:
				_print_log("SpacetimeDBConnection: Connection closed (Code: %d, Reason: %s)" % [code, reason])
				disconnected.emit() # Normal closure signal
		_is_connected = false
		_connection_requested = false
		set_physics_process(false) # Stop polling


## Updates the post-stall guard window. A poll gap at or beyond the heartbeat
## window means the main thread was frozen long enough for the engine to falsely
## close the socket on a missed pong; arm the guard so the next abnormal close is
## classified as stall-induced rather than a network drop.
func _track_stall() -> void:
	var now_ms: int = Time.get_ticks_msec()
	var gap_ms: int = (now_ms - _last_poll_ms) if _last_poll_ms > 0 else 0
	_last_poll_ms = now_ms
	if is_stall_gap(gap_ms, _stall_threshold_ms):
		_post_stall_polls = STALL_GUARD_POLLS
	elif _post_stall_polls > 0:
		_post_stall_polls -= 1


## True when the wall-clock gap between two polls is large enough that the engine
## heartbeat could have falsely declared the socket dead. threshold_ms == 0
## (heartbeat disabled) → never a stall.
static func is_stall_gap(gap_ms: int, threshold_ms: int) -> bool:
	return threshold_ms > 0 and gap_ms >= threshold_ms


func _notification(what: int) -> void:
	if what == NOTIFICATION_PREDELETE:
		if _options and _options.monitor_mode:
			for suffix: String in [
				"_second_received_packets",
				"_second_received_bytes",
				"_total_received_packets",
				"_total_received_kbytes",
				"_second_sent_packets",
				"_second_sent_bytes",
				"_total_sent_packets",
				"_total_sent_kbytes",
			]:
				Performance.remove_custom_monitor("spacetime/" + _db_name + suffix)
	elif what == NOTIFICATION_CRASH or what == NOTIFICATION_WM_CLOSE_REQUEST:
		if is_websocket_active():
			get_tree().auto_accept_quit = false
			_handle_game_closing()


func get_second_sent_bytes() -> int:
	var amount: int = _second_bytes_sent
	_second_bytes_sent = 0
	return amount


func get_second_received_bytes() -> int:
	var amount: int = _second_bytes_received
	_second_bytes_received = 0
	return amount


func get_second_sent_packets() -> int:
	var amount: int = _second_messages_sent
	_second_messages_sent = 0
	return amount


func get_second_received_packets() -> int:
	var amount: int = _second_messages_received
	_second_messages_received = 0
	return amount


func get_sent_kbytes() -> float:
	return _total_bytes_sent / 1000.0


func get_received_kbytes() -> float:
	return _total_bytes_received / 1000.0


func get_sent_packets() -> int:
	return _total_messages_sent


func get_received_packets() -> int:
	return _total_messages_received


func set_token(token: String) -> void:
	self._token = token


func set_compression_preference(preference: CompressionPreference) -> void:
	self.preferred_compression = preference


## Sends [param bytes] over the WebSocket. Returns [constant OK] on success.
func send_bytes(bytes: PackedByteArray) -> Error:
	var err: Error = _websocket.send(bytes)
	if err == OK:
		_second_bytes_sent += bytes.size()
		_total_bytes_sent += bytes.size()
		_second_messages_sent += 1
		_total_messages_sent += 1
		total_messages.emit(_total_messages_sent, _total_messages_received)
		total_bytes.emit(_total_bytes_sent, _total_bytes_received)
	return err


## Initiates a WebSocket connection to the SpacetimeDB [param database_name]
## at [param base_url] using the given [param connection_id].
func connect_to_database(base_url: String, database_name: String, connection_id: String) -> void:
	if _is_connected:
		_print_log("SpacetimeDBConnection: Already connected.")
		return

	if _connection_requested:
		_print_log("SpacetimeDBConnection: Previous attempt still in progress, resetting.")
		if _websocket.get_ready_state() != WebSocketPeer.STATE_CLOSED:
			_websocket.close()
		_is_connected = false
		_connection_requested = false
		_websocket = WebSocketPeer.new()
		_websocket.inbound_buffer_size = _options.inbound_buffer_size
		_websocket.outbound_buffer_size = _options.outbound_buffer_size
		# Re-apply heartbeat — a fresh peer defaults to 0 (keepalive off), which would
		# silently disable stall detection on this retried connection.
		_websocket.heartbeat_interval = _options.heartbeat_interval_seconds

	if _token.is_empty():
		_print_log("SpacetimeDBConnection: Cannot connect without auth token.")
		return

	if connection_id.is_empty():
		printerr("SpacetimeDBConnection: Cannot connect without Connection ID.")
		return

	# Construct WebSocket URL base
	# Rewrite only the leading scheme — a stray "http://" elsewhere in base_url
	# (a path or query segment) must be left alone. begins_with anchors at index 0;
	# .replace() would rewrite every occurrence. https checked first — "http" is a
	# prefix of "https".
	var ws_url_base: String = base_url
	if ws_url_base.begins_with("https://"):
		ws_url_base = "wss://" + ws_url_base.substr(8)
	elif ws_url_base.begins_with("http://"):
		ws_url_base = "ws://" + ws_url_base.substr(7)
	ws_url_base = ws_url_base.path_join("/" + version + "/database").path_join(database_name).path_join("subscribe")

	# --- Add Query Parameters ---
	# Start with connection_id
	var query_params: String = "?connection_id=%s" % connection_id
	# Add compression preference
	# Convert enum value to string for the URL parameter
	var compression_str: String

	if preferred_compression == CompressionPreference.NONE:
		compression_str = "None" # Use string "None" as seen in C# enum
	elif preferred_compression == CompressionPreference.BROTLI:
		compression_str = "Brotli"
	elif preferred_compression == CompressionPreference.GZIP:
		compression_str = "Gzip"
	else:
		compression_str = "None" # Fallback

	query_params += "&compression=%s" % compression_str
	query_params += "&confirmed=%s" % ("true" if _options.confirmed_reads else "false")
	if _options.light_mode:
		query_params += "&light=true"

	if OS.get_name() == "Web":
		query_params += "&token=%s" % _token
	else:
		var auth_header: String = "Authorization: Bearer %s" % _token
		_websocket.handshake_headers = [auth_header]

	_target_url = "%s%s" % [ws_url_base, query_params]
	_print_log("SpacetimeDBConnection: Attempting to connect to: " + _target_url)

	# v3 only — servers below 2.2.0 (which speak just v2) will fail the handshake.
	_websocket.supported_protocols = [BSATN_PROTOCOL_V3]

	var err: Error = _websocket.connect_to_url(_target_url)
	if err != OK:
		printerr("SpacetimeDBConnection: Error initiating connection: ", err)
		connection_error.emit(err, "Failed to initiate connection")
	else:
		_print_log("SpacetimeDBConnection: Connection initiated.")
		_connection_requested = true
		_last_poll_ms = 0 # fresh poll clock — first poll sets the baseline, no false stall
		_post_stall_polls = 0
		set_physics_process(true)


## Closes the WebSocket connection with the given [param code] and [param reason].
func disconnect_from_server(code: int = 1000, reason: String = "Client initiated disconnect") -> void:
	if is_websocket_active():
		_print_log("SpacetimeDBConnection: Closing connection...")
		_websocket.close(code, reason)
	_is_connected = false
	_connection_requested = false


## Returns [code]true[/code] if the WebSocket is currently open.
func is_connected_db() -> bool:
	return _is_connected


## Returns [code]true[/code] if the WebSocket peer exists and is not closed.
func is_websocket_active() -> bool:
	return _websocket.get_ready_state() != WebSocketPeer.STATE_CLOSED


func _print_log(log_message: String) -> void:
	if _debug_mode:
		print(log_message)


func _handle_game_closing() -> void:
	disconnect_from_server()
	var tree: SceneTree = get_tree()
	var physics_dt: float = 1.0 / maxf(float(Engine.physics_ticks_per_second), 1.0)
	var max_wait: float = 3.0
	var elapsed: float = 0.0
	while _websocket.get_ready_state() == WebSocketPeer.STATE_CLOSING:
		_print_log("SpacetimeDBConnection: WS closing")
		await tree.physics_frame
		if not is_instance_valid(self):
			return
		elapsed = minf(elapsed + physics_dt, max_wait + 1.0)
		if elapsed >= max_wait:
			_print_log("SpacetimeDBConnection: WS close wait exceeded cap, forcing quit")
			break
	tree.auto_accept_quit = true
	tree.quit()
