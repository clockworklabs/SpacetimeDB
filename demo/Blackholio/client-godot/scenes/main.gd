extends Node2D

@export var circle_row_receiver: RowReceiver
@export var entity_row_receiver: RowReceiver
@export var food_row_receiver: RowReceiver
@export var player_row_receiver: RowReceiver

@onready var background: TextureRect = $Background

var circle_scene: PackedScene = load("res://scenes/circle_controller.tscn")
var player_scene: PackedScene = load("res://scenes/player_controller.tscn")
var food_scene: PackedScene = load("res://scenes/food_controller.tscn")
var entities: Dictionary[int, EntityController] = {}
var players: Dictionary[int, PlayerController] = {}

func _ready():
	# Connect to signals BEFORE connecting to the DB
	SpacetimeDB.connected.connect(_on_spacetimedb_connected)
	SpacetimeDB.disconnected.connect(_on_spacetimedb_disconnected)
	SpacetimeDB.connection_error.connect(_on_spacetimedb_connection_error)
	SpacetimeDB.identity_received.connect(_on_spacetimedb_identity_received)
	SpacetimeDB.database_initialized.connect(_on_spacetimedb_database_initialized)
	# SpacetimeDB.transaction_update_received.connect(_on_transaction_update) # For reducer results

	var options = SpacetimeDBConnectionOptions.new()
	options.compression = SpacetimeDBConnection.CompressionPreference.NONE
	options.one_time_token = true
	options.debug_mode = false
	options.inbound_buffer_size = 1024 * 1024 * 2 # 2MB
	options.outbound_buffer_size = 1024 * 1024 * 2 # 2MB

	SpacetimeDB.connect_db(
		"http://127.0.0.1:3000", # Base HTTP URL
		"blackholio", # Module Name
		options
	)
	
	circle_row_receiver.insert.connect(_on_circle_inserted)
	entity_row_receiver.update.connect(_on_entity_updated)
	entity_row_receiver.delete.connect(_on_entity_deleted)
	food_row_receiver.insert.connect(_on_food_inserted)
	player_row_receiver.insert.connect(_on_player_inserted)
	player_row_receiver.delete.connect(_on_player_deleted)

func _on_spacetimedb_connected():
	print("Game: Connected to SpacetimeDB!")
	# Good place to subscribe to initial data
	var queries = ["SELECT * FROM player", "SELECT * FROM config", "SELECT * FROM circle", "SELECT * FROM food", "SELECT * FROM entity"]
	var req_id = SpacetimeDB.subscribe(queries)
	if req_id < 0: printerr("Subscription failed!")
	GameManager.is_connected = true
  
func _on_spacetimedb_identity_received(identity_token: IdentityTokenData):
	print("Game: My Identity: 0x%s" % identity_token.identity.hex_encode())
	GameManager.local_identity = identity_token.identity.hex_encode()

func _on_spacetimedb_database_initialized():
	print("Game: Local database cache initialized.")
	# Safe to query the local DB for initially subscribed data
	var db = SpacetimeDB.get_local_database()
	var config: BlackholioConfig = db.get_row("config", 0)
	setup_arena(config.world_size)
	var initial_players = db.get_all_rows("player")
	print("Initial players found: %d" % initial_players.size())
	for player in initial_players:
		_on_player_inserted(player)
	
	var initial_circles = db.get_all_rows("circle")
	for circle in initial_circles:
		_on_circle_inserted(circle)
		
	var initial_food = db.get_all_rows("food")
	for food in initial_food:
		_on_food_inserted(food)

func _on_spacetimedb_disconnected():
	print("Game: Disconnected.")

func _on_spacetimedb_connection_error(code, reason):
	printerr("Game: Connection Error (Code: %d): %s" % [code, reason])

# func _on_transaction_update(update: TransactionUpdateData):
	# Handle results/errors from reducer calls
	#if update.status.status_type == UpdateStatusData.StatusType.FAILED:
		#printerr("Reducer call (ReqID: %d) failed: %s" % [update.reducer_call.request_id, update.status.failure_message])
	#elif update.status.status_type == UpdateStatusData.StatusType.COMMITTED:
		#print("Reducer call (ReqID: %d) committed." % update.reducer_call.request_id)
		# Optionally inspect update.status.committed_update for DB changes
	
func setup_arena(world_size: int):
	var size = world_size / background.scale.x
	background.size = Vector2(size, size)
	$CameraController.world_size = world_size
	
func _on_circle_inserted(circle: BlackholioCircle):
	var db = SpacetimeDB.get_local_database()
	var circle_controller: CircleController = circle_scene.instantiate()
	var player = get_or_create_player_by_player_id(circle.player_id)
	circle_controller.spawn(circle, player)
	add_child(circle_controller)
	entities.set(circle.entity_id, circle_controller)

func get_or_create_player_by_player_id(player_id: int):
	var db = SpacetimeDB.get_local_database()
	var players = db.get_all_rows("player")
	for player in players:
		if player.player_id == player_id:
			return get_or_create_player(player)
	
func get_or_create_player(player: BlackholioPlayer) -> PlayerController:
	if (!players.has(player.player_id)):
		var player_controller: PlayerController = player_scene.instantiate()
		add_child(player_controller)
		player_controller.initialize(player)
		players.set(player.player_id, player_controller)

	return players.get(player.player_id);

func _on_entity_updated(_previous_entity: BlackholioEntity, new_entity: BlackholioEntity):
	if (!entities.has(new_entity.entity_id)): return
	entities.get(new_entity.entity_id).on_entity_update(new_entity)
	
func _on_entity_deleted(entity: BlackholioEntity):
	if (entities.has(entity.entity_id)):
		entities.get(entity.entity_id).on_delete()
		entities.erase(entity.entity_id)

func _on_food_inserted(food: BlackholioFood):
	var food_controller: FoodController = food_scene.instantiate()
	add_child(food_controller)
	food_controller.spawn(food)
	entities.set(food.entity_id, food_controller)

func _on_player_inserted(player: BlackholioPlayer):
	get_or_create_player(player)

func _on_player_deleted(player: BlackholioPlayer):
	if (players.has(player.player_id)):
		players.get(player.player_id).queue_free()
		players.erase(player.player_id)
