class_name SpacetimeDBRestAPI extends Node

# Enum to track the type of the currently pending request
enum RequestType { NONE, TOKEN, REDUCER_CALL } # Add more if needed for other REST calls

var _http_request := HTTPRequest.new()
var _base_url: String
var _token: String
# State variable to track the expected response type
var _pending_request_type := RequestType.NONE
var _debug_mode := false

signal token_received(token: String)
signal token_request_failed(error_code: int, response_body: String)
signal reducer_call_completed(result: Dictionary) # Or specific resource
signal reducer_call_failed(error_code: int, response_body: String)

func _init(base_url: String, debug_mode: bool):
    self._base_url = base_url
    self._debug_mode = debug_mode
    add_child(_http_request)
    # Connect the signal ONCE
    if not _http_request.is_connected("request_completed", Callable(self, "_on_request_completed")):
        _http_request.request_completed.connect(_on_request_completed)

func print_log(log_message: String):
    if _debug_mode:
        print(log_message)

func set_token(token: String):
    self._token = token

# --- Token Management ---

func request_new_token():
    # Prevent concurrent requests if this handler isn't designed for it
    if _pending_request_type != RequestType.NONE:
        printerr("SpacetimeDBRestAPI: Cannot request token while another request is pending (%s)." % RequestType.keys()[_pending_request_type])
        # Optionally queue or emit a busy error
        return

    print_log("SpacetimeDBRestAPI: Requesting new token...")
    var url := _base_url.path_join("/v1/identity")
    # Set state *before* making the request
    _pending_request_type = RequestType.TOKEN
    var error := _http_request.request(url, [], HTTPClient.METHOD_POST)
    if error != OK:
        printerr("SpacetimeDBRestAPI: Error initiating token request: ", error)
        # Reset state on immediate failure
        _pending_request_type = RequestType.NONE
        emit_signal("token_request_failed", error, "Failed to initiate request")

func _handle_token_response(result_code: int, response_code: int, headers: PackedStringArray, body: PackedByteArray):
    # (Logic for handling token response - remains the same as before)
    if result_code != HTTPRequest.RESULT_SUCCESS:
        printerr("SpacetimeDBRestAPI: Token request failed. Result code: ", result_code)
        emit_signal("token_request_failed", result_code, body.get_string_from_utf8())
        return

    var body_text := body.get_string_from_utf8()
    var json = JSON.parse_string(body_text)

    if response_code >= 400 or json == null:
        printerr("SpacetimeDBRestAPI: Token request failed. Response code: ", response_code)
        printerr("SpacetimeDBRestAPI: Response body: ", body_text)
        emit_signal("token_request_failed", response_code, body_text)
        return

    if json.has("token") and json.token is String and not json.token.is_empty():
        var new_token: String = json.token
        print_log("SpacetimeDBRestAPI: New token received.")
        set_token(new_token) # Store it internally as well
        emit_signal("token_received", new_token)
    else:
        printerr("SpacetimeDBRestAPI: Token not found or empty in JSON response: ", body_text)
        emit_signal("token_request_failed", response_code, "Invalid token format in response")


# --- Reducer Call (REST Example) ---

func call_reducer(database: String, reducer_name: String, args: Dictionary):
    if _pending_request_type != RequestType.NONE:
        printerr("SpacetimeDBRestAPI: Cannot call reducer while another request is pending (%s)." % RequestType.keys()[_pending_request_type])
        emit_signal("reducer_call_failed", -1, "Another request pending")
        return

    if _token.is_empty():
        printerr("SpacetimeDBRestAPI: Cannot call reducer without auth token.")
        emit_signal("reducer_call_failed", -1, "Auth token not set")
        return

    var url := _base_url.path_join("/v1/database").path_join(database).path_join("call").path_join(reducer_name)
    var headers := [
        "Authorization: Bearer " + _token,
        "Content-Type: application/json"
    ]
    var body := JSON.stringify(args)

    # Set state *before* making the request
    _pending_request_type = RequestType.REDUCER_CALL
    var error := _http_request.request(url, headers, HTTPClient.METHOD_POST, body)
    if error != OK:
        printerr("SpacetimeDBRestAPI: Error initiating reducer call request: ", error)
        # Reset state on immediate failure
        _pending_request_type = RequestType.NONE
        emit_signal("reducer_call_failed", error, "Failed to initiate request")

func _handle_reducer_response(result_code: int, response_code: int, headers: PackedStringArray, body: PackedByteArray):
    # (Logic for handling reducer response - remains the same as before)
    if result_code != HTTPRequest.RESULT_SUCCESS or response_code >= 400:
        printerr("SpacetimeDBRestAPI: Reducer call failed. Result: %d, Code: %d" % [result_code, response_code])
        printerr("SpacetimeDBRestAPI: Response body: ", body.get_string_from_utf8())
        emit_signal("reducer_call_failed", response_code, body.get_string_from_utf8())
        return

    var body_text := body.get_string_from_utf8()
    var json = JSON.parse_string(body_text)
    if json == null:
        printerr("SpacetimeDBRestAPI: Failed to parse reducer response JSON: ", body_text)
        emit_signal("reducer_call_failed", response_code, "Invalid JSON response")
        return

    emit_signal("reducer_call_completed", json)


# --- Request Completion Handler ---

func _on_request_completed(result: int, response_code: int, headers: PackedStringArray, body: PackedByteArray):
    # Capture the type of request that was pending *before* resetting state
    var request_type_that_completed := _pending_request_type
    # Reset state immediately, allowing new requests
    _pending_request_type = RequestType.NONE

    # Route the response based on the captured state
    match request_type_that_completed:
        RequestType.TOKEN:
            #print("SpacetimeDBRestAPI: Handling completed request as TOKEN") # Debug line
            _handle_token_response(result, response_code, headers, body)
        RequestType.REDUCER_CALL:
            #print("SpacetimeDBRestAPI: Handling completed request as REDUCER_CALL") # Debug line
            _handle_reducer_response(result, response_code, headers, body)
        RequestType.NONE:
            # This might happen if the request failed immediately before the state was properly set,
            # or if the signal fires unexpectedly after state reset (less likely).
            push_warning("SpacetimeDBRestAPI: Received request completion signal but no request type was pending.")
        _:
            printerr("SpacetimeDBRestAPI: Internal error - completed request type was unknown: ", request_type_that_completed)
