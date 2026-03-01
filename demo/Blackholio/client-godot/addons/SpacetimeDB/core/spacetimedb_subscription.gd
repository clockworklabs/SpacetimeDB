class_name SpacetimeDBSubscription extends Node

var query_id: int = -1
var queries: PackedStringArray
var error: Error = OK

signal applied
signal end

signal _applied_or_timeout(timeout: bool)
signal _ended_or_timeout(timeout: bool)

var _client: SpacetimeDBClient
var _active := false
var _ended := false

var active: bool:
    get:
        return _active
var ended: bool:
    get:
        return _ended

static func create(
    p_client: SpacetimeDBClient,
    p_query_id: int,
    p_queries: PackedStringArray
) -> SpacetimeDBSubscription:
    var subscription := SpacetimeDBSubscription.new()
    subscription._client = p_client
    subscription.query_id = p_query_id
    subscription.queries = p_queries
    
    subscription.applied.connect(func():
        subscription._active = true
        subscription._ended = false
        
        subscription._applied_or_timeout.emit(false)
    )
    subscription.end.connect(func():
        subscription._active = false
        subscription._ended = true
        
        subscription._ended_or_timeout.emit(false)
    )
    return subscription

static func fail(error: Error) -> SpacetimeDBSubscription:
    var subscription := SpacetimeDBSubscription.new()
    subscription.error = error
    subscription._ended = true
    return subscription

func wait_for_applied(timeout_sec: float = 5) -> Error:
    if _active:
        return OK
    if _ended:
        return ERR_DOES_NOT_EXIST
        
    get_tree().create_timer(timeout_sec).timeout.connect(_on_applied_timeout)
    
    var is_timeout: bool = await _applied_or_timeout
    if is_timeout:
        return ERR_TIMEOUT
    return OK

func wait_for_end(timeout_sec: float = 5) -> Error:
    if _ended:
        return OK
        
    get_tree().create_timer(timeout_sec).timeout.connect(_on_ended_timeout)
    
    var is_timeout: bool = await _ended_or_timeout
    if is_timeout:
        return ERR_TIMEOUT
    return OK

func unsubscribe() -> Error:
    if _ended:
        return ERR_DOES_NOT_EXIST

    return _client.unsubscribe(query_id)

func _on_applied_timeout() -> void:
    _applied_or_timeout.emit(true)
    
func _on_ended_timeout() -> void:
    _ended_or_timeout.emit(true)
