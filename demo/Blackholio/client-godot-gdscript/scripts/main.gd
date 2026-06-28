extends Node2D

# Upstream client's exact 10-colour palette (no near-white slot — a white blob
# both diverged from upstream and hid the white name label).
const PLAYER_COLORS: Array[Color] = [
	Color(175 / 255.0, 159 / 255.0, 49 / 255.0), # yellow
	Color(175 / 255.0, 116 / 255.0, 49 / 255.0),
	Color(112 / 255.0, 47 / 255.0, 252 / 255.0), # purple
	Color(51 / 255.0, 91 / 255.0, 252 / 255.0),
	Color(176 / 255.0, 54 / 255.0, 54 / 255.0), # red
	Color(176 / 255.0, 109 / 255.0, 54 / 255.0),
	Color(141 / 255.0, 43 / 255.0, 99 / 255.0),
	Color(2 / 255.0, 188 / 255.0, 250 / 255.0), # blue
	Color(7 / 255.0, 50 / 255.0, 251 / 255.0),
	Color(2 / 255.0, 28 / 255.0, 146 / 255.0),
]

const FOOD_COLORS: Array[Color] = [
	Color(0.2, 0.8, 0.2),
	Color(0.3, 0.9, 0.3),
	Color(0.1, 0.7, 0.3),
	Color(0.2, 0.6, 0.1),
	Color(0.4, 0.9, 0.2),
	Color(0.1, 0.8, 0.4),
]

const INPUT_RATE: float = 0.05 # 20Hz
const WORLD_SCALE: float = 1.0 # Render 1:1 with server world units (like upstream); the
# camera zoom (50/size) magnifies for visibility. Scaling the world up while using
# upstream's absolute-pixel draw constants made outlines/fonts render too thin.
const BASE_VISIBLE_RADIUS: float = 50.0 # upstream camera base; visible world radius at zoom 1
const CAMERA_FOLLOW_SPEED: float = 8.0 # match upstream camera follow lerp

var entity_nodes: Dictionary[int, Node2D] = { }
# Entities currently playing a consume (eaten) animation. The animation owns the
# node's lifetime, so the matching entity delete must not free it a second time.
var _pending_consume: Dictionary[int, bool] = { }
var circle_to_player: Dictionary[int, int] = { }
var player_circles: Dictionary[int, Array] = { }
var player_names: Dictionary[int, String] = { }

var local_identity: PackedByteArray
var local_player_id: int = -1
var world_size: int = 1000
var input_timer: float = 0.0
var game_started: bool = false
var _player_name: String = ""
var _lock_input_active: bool = false
var _lock_input_pos: Vector2 = Vector2.ZERO
var _starfield: Node2D = null
var _status_label: Label = null
var _food_field: MultiMeshInstance2D = null
var _label_layer: CanvasLayer = null # screen-space layer for crisp circle name labels

@onready var entity_container: Node2D = $EntityContainer
@onready var camera: Camera2D = $Camera2D
@onready var username_screen: Control = $UI/UsernameScreen
@onready var death_screen: Control = $UI/DeathScreen
@onready var leaderboard: Control = $UI/Leaderboard
@onready var world_border: Node2D = $WorldBorder

const QUERIES: PackedStringArray = [
	"SELECT * FROM entity",
	"SELECT * FROM circle",
	"SELECT * FROM food",
	"SELECT * FROM player",
	"SELECT * FROM config",
	"SELECT * FROM consume_entity_event",
]


func _ready() -> void:
	var options := SpacetimeDBConnectionOptions.new()
	options.debug_mode = true
	options.compression = SpacetimeDBConnection.CompressionPreference.GZIP
	options.auto_reconnect = true
	# Persist the auth token so a restart resumes the same identity (and existing
	# player row) instead of a fresh one — enables the rejoin path below.
	options.one_time_token = false

	SpacetimeDB.Blackholio.connect_db(
		"http://127.0.0.1:3000",
		"blackholio",
		options,
	)

	SpacetimeDB.Blackholio.connected.connect(_on_connected)
	SpacetimeDB.Blackholio.disconnected.connect(_on_disconnected)
	SpacetimeDB.Blackholio.connection_error.connect(_on_connection_error)
	SpacetimeDB.Blackholio.reconnected.connect(_on_reconnected)

	death_screen.visible = false
	username_screen.visible = false

	# Top-left status readout (Mass / Circles), matching the upstream HUD.
	_status_label = Label.new()
	_status_label.position = Vector2(16, 16)
	_status_label.add_theme_font_size_override("font_size", 16)
	$UI.add_child(_status_label)

	# Screen-space layer for circle name labels (crisp at any camera zoom).
	_label_layer = CanvasLayer.new()
	add_child(_label_layer)


func _on_connected(identity: PackedByteArray, _token: String) -> void:
	local_identity = identity
	print("Connected! Identity: 0x%s" % identity.hex_encode())
	_subscribe_all()


func _on_disconnected() -> void:
	print("Disconnected from server")


# After auto-reconnect the SDK restores subscriptions; if we were already
# playing, re-enter so the server respawns our circle (the player row survives
# the disconnect via logged_out_player). Matches the other clients' auto-rejoin.
func _on_reconnected() -> void:
	if not _player_name.is_empty():
		SpacetimeDB.Blackholio.reducers.enter_game(_player_name)
		game_started = true


func _on_connection_error(code: int, reason: String) -> void:
	printerr("Connection error %d: %s" % [code, reason])


func _subscribe_all() -> void:
	var sub := SpacetimeDB.Blackholio.subscribe(QUERIES)
	if sub.error:
		printerr("Subscription failed")
		return
	sub.applied.connect(_on_subscription_applied)


func _on_subscription_applied() -> void:
	print("Subscription applied")
	_setup_table_callbacks()

	# Read config
	var configs := SpacetimeDB.Blackholio.db.config.iter()
	print("Configs: %d" % configs.size())
	if configs.size() > 0:
		world_size = configs[0].world_size
		print("World size: %d" % world_size)
	_draw_world_border()
	# Build the starfield once (subscription-applied can fire again on resubscribe).
	if _starfield == null:
		_starfield = preload("res://scripts/starfield.gd").new()
		_starfield.setup(world_size)
		add_child(_starfield)
	# GPU-instanced food field (one draw call for all pellets).
	if _food_field == null:
		_food_field = preload("res://scripts/food_field.gd").new()
		add_child(_food_field)

	# Load existing state
	_load_existing_data()
	print(
		"Loaded: %d entities, %d players, %d circles, %d food" % [
			SpacetimeDB.Blackholio.db.entity.count(),
			SpacetimeDB.Blackholio.db.player.count(),
			SpacetimeDB.Blackholio.db.circle.count(),
			SpacetimeDB.Blackholio.db.food.count(),
		],
	)

	# Decide the entry flow. Three cases, matching the upstream client:
	var has_circles: bool = (
			local_player_id >= 0
			and player_circles.has(local_player_id)
			and not player_circles[local_player_id].is_empty()
	)
	var known_name: String = player_names.get(local_player_id, "") if local_player_id >= 0 else ""
	if has_circles:
		# Already in-game (e.g. reconnect mid-session).
		game_started = true
		username_screen.visible = false
	elif not known_name.is_empty():
		# Persisted identity with a name but no circle (rejoin after a prior session
		# or a death while away) — re-enter silently instead of re-prompting.
		_player_name = known_name
		SpacetimeDB.Blackholio.reducers.enter_game(known_name)
		game_started = true
		username_screen.visible = false
	else:
		# New / unnamed identity — ask for a name.
		username_screen.visible = true


func _setup_table_callbacks() -> void:
	SpacetimeDB.Blackholio.db.entity.on_insert(_on_entity_insert)
	SpacetimeDB.Blackholio.db.entity.on_update(_on_entity_update)
	SpacetimeDB.Blackholio.db.entity.on_delete(_on_entity_delete)
	SpacetimeDB.Blackholio.db.circle.on_insert(_on_circle_insert)
	SpacetimeDB.Blackholio.db.circle.on_delete(_on_circle_delete)
	SpacetimeDB.Blackholio.db.food.on_insert(_on_food_insert)
	SpacetimeDB.Blackholio.db.consume_entity_event.on_insert(_on_consume_event)
	SpacetimeDB.Blackholio.db.player.on_insert(_on_player_insert)
	SpacetimeDB.Blackholio.db.player.on_update(_on_player_update)
	SpacetimeDB.Blackholio.db.player.on_delete(_on_player_delete)


func _load_existing_data() -> void:
	# Load players first
	for player in SpacetimeDB.Blackholio.db.player.iter():
		_register_player(player)

	# Load entities
	for entity in SpacetimeDB.Blackholio.db.entity.iter():
		_spawn_entity_node(entity)

	# Load circles (associate with players)
	for circle in SpacetimeDB.Blackholio.db.circle.iter():
		_register_circle(circle)

	# Load food
	for food in SpacetimeDB.Blackholio.db.food.iter():
		_register_food(food.entity_id)

# --- Entity callbacks ---


func _on_entity_insert(entity: Resource) -> void:
	_spawn_entity_node(entity)


func _on_entity_update(_old: Resource, new: Resource) -> void:
	var eid: int = new.entity_id
	var pos: Vector2 = Vector2(new.position.x, new.position.y) * WORLD_SCALE
	# Food is rendered by the MultiMesh field; players/circles by their node.
	if _food_field != null and _food_field.has_food(eid):
		_food_field.update_food(eid, pos, new.mass)
		return
	var node: Node2D = entity_nodes.get(eid)
	if node and node.has_method("update_target"):
		node.update_target(pos, new.mass)


func _on_entity_delete(entity: Resource) -> void:
	var eid: int = entity.entity_id
	if _food_field != null and _food_field.has_food(eid):
		_food_field.remove_food(eid)
		return
	var node: Node2D = entity_nodes.get(eid)
	if node:
		# If a consume animation is playing, it owns the node (frees itself at the
		# end) — drop our handle without a second free. Dictionary.erase returns
		# true when the key was present.
		if not _pending_consume.erase(eid):
			node.queue_free()
		entity_nodes.erase(eid)

	# Clean up circle tracking
	if circle_to_player.has(eid):
		var pid: int = circle_to_player[eid]
		circle_to_player.erase(eid)
		_remove_circle_from_player(pid, eid)

# --- Circle callbacks ---


func _on_circle_insert(circle: Resource) -> void:
	_register_circle(circle)


func _on_circle_delete(circle: Resource) -> void:
	var eid: int = circle.entity_id
	if circle_to_player.has(eid):
		var pid: int = circle_to_player[eid]
		circle_to_player.erase(eid)
		_remove_circle_from_player(pid, eid)

	# Reset entity node to default appearance
	var node: Node2D = entity_nodes.get(eid)
	if node and node.has_method("set_circle_info"):
		node.set_circle_info(-1, "")


func _register_circle(circle: Resource) -> void:
	var eid: int = circle.entity_id
	var pid: int = circle.player_id
	circle_to_player[eid] = pid

	if not player_circles.has(pid):
		player_circles[pid] = []
	if eid not in player_circles[pid]:
		player_circles[pid].append(eid)

	# Update visual
	var node: Node2D = entity_nodes.get(eid)
	if node and node.has_method("set_circle_info"):
		var color: Color = PLAYER_COLORS[pid % PLAYER_COLORS.size()]
		var pname: String = player_names.get(pid, "")
		node.set_circle_info(pid, pname, color)


func _remove_circle_from_player(pid: int, eid: int) -> void:
	if not player_circles.has(pid):
		return
	var circles: Array = player_circles[pid]
	circles.erase(eid)
	if pid == local_player_id and circles.is_empty():
		_on_local_player_died()

# --- Food callbacks ---


func _on_food_insert(food: Resource) -> void:
	_register_food(food.entity_id)


# Routes a food entity to the GPU-instanced field: drops the generic node the
# entity insert spawned (we don't know an entity is food until its food row lands)
# and hands its position/mass/color to the MultiMesh field.
func _register_food(entity_id: int) -> void:
	var e: Resource = SpacetimeDB.Blackholio.db.entity.entity_id.find(entity_id)
	if e == null:
		return
	var node: Node2D = entity_nodes.get(entity_id)
	if node:
		node.queue_free()
		entity_nodes.erase(entity_id)
	if _food_field == null:
		return
	var pos: Vector2 = Vector2(e.position.x, e.position.y) * WORLD_SCALE
	var color: Color = FOOD_COLORS[entity_id % FOOD_COLORS.size()]
	_food_field.add_food(entity_id, pos, e.mass, color, float(entity_id) * 0.73)

# --- Consume callbacks ---


# An entity was eaten: animate it shrinking into its consumer instead of popping.
# The matching entity delete arrives moments later; _pending_consume tells it the
# animation already owns the node. The consumer may be unsubscribed/off-screen —
# fall back to despawning in place.
func _on_consume_event(ev: Resource) -> void:
	var consumed_id: int = ev.consumed_entity_id
	var consumed_node: Node2D = entity_nodes.get(consumed_id)
	if consumed_node == null:
		return
	# Pass the consumer node (may be null) so the animation chases it live.
	var consumer_node: Node2D = entity_nodes.get(ev.consumer_entity_id)
	_pending_consume[consumed_id] = true
	consumed_node.despawn_into(consumer_node)

# --- Player callbacks ---


func _on_player_insert(player: Resource) -> void:
	_register_player(player)


# enter_game updates the existing player row (name set after connect created it),
# so without this the leaderboard would keep the empty name from the insert.
func _on_player_update(_old: Resource, new: Resource) -> void:
	_register_player(new)


func _on_player_delete(player: Resource) -> void:
	var pid: int = player.player_id
	player_names.erase(pid)
	player_circles.erase(pid)


func _register_player(player: Resource) -> void:
	var pid: int = player.player_id
	player_names[pid] = player.name

	if player.identity == local_identity:
		local_player_id = pid

# --- Entity spawning ---


func _spawn_entity_node(entity: Resource) -> void:
	if entity_nodes.has(entity.entity_id):
		return
	var node: Node2D = preload("res://scripts/entity_node.gd").new()
	node.position = Vector2(entity.position.x, entity.position.y) * WORLD_SCALE
	node.animation_seed = float(entity.entity_id) * 0.73 # desync pulse/wave per entity
	node.label_layer = _label_layer # screen-space name label target
	node.set_mass(entity.mass)
	entity_container.add_child(node)
	entity_nodes[entity.entity_id] = node

# --- Input ---


func _process(delta: float) -> void:
	_update_status()
	if not game_started:
		return

	input_timer += delta
	if input_timer >= INPUT_RATE:
		input_timer = 0.0
		_send_input()

	_update_camera(delta)
	leaderboard.update_leaderboard(self)


func _update_status() -> void:
	var mass: int = 0
	var count: int = 0
	if local_player_id >= 0 and player_circles.has(local_player_id):
		var circles: Array = player_circles[local_player_id]
		count = circles.size()
		for eid: int in circles:
			var e: Resource = SpacetimeDB.Blackholio.db.entity.entity_id.find(eid)
			if e:
				mass += e.mass
	_status_label.text = "Mass: %d\nCircles: %d" % [mass, count]


func _unhandled_input(event: InputEvent) -> void:
	if not game_started or not SpacetimeDB.Blackholio.is_connected_db():
		return

	if event.is_action_pressed("split"):
		SpacetimeDB.Blackholio.reducers.player_split()
	elif event.is_action_pressed("suicide"):
		SpacetimeDB.Blackholio.reducers.suicide()
	elif event.is_action_pressed("lock_input"):
		# Toggle: freeze the movement direction at the current mouse position
		# (matches the other clients' Q lock-toggle).
		if _lock_input_active:
			_lock_input_active = false
		else:
			_lock_input_active = true
			_lock_input_pos = get_viewport().get_mouse_position()


func _send_input() -> void:
	if not SpacetimeDB.Blackholio.is_connected_db():
		return
	var screen_center: Vector2 = get_viewport_rect().size / 2.0
	var mouse_pos: Vector2 = _lock_input_pos if _lock_input_active else get_viewport().get_mouse_position()
	var direction: Vector2 = (mouse_pos - screen_center) / (get_viewport_rect().size.y / 3.0)

	var db_dir := BlackholioDbVector2.create(direction.x, direction.y)
	SpacetimeDB.Blackholio.reducers.update_player_input(db_dir)

# --- Camera ---


func _update_camera(delta: float) -> void:
	if local_player_id < 0 or not player_circles.has(local_player_id):
		return

	var circle_count: int = player_circles[local_player_id].size()
	if circle_count == 0:
		return

	# Calculate center of mass
	var total_mass: float = 0.0
	var weighted_pos: Vector2 = Vector2.ZERO
	for eid: int in player_circles[local_player_id]:
		var node: Node2D = entity_nodes.get(eid)
		if node:
			var entity: Resource = SpacetimeDB.Blackholio.db.entity.entity_id.find(eid)
			if entity:
				var m: float = float(entity.mass)
				weighted_pos += node.position * m
				total_mass += m

	if total_mass > 0:
		var center: Vector2 = weighted_pos / total_mass
		camera.position = camera.position.lerp(center, delta * CAMERA_FOLLOW_SPEED)

	# Zoom: mirror the upstream camera so on-screen speed matches. Upstream uses
	# raw world units with zoom = BASE_VISIBLE_RADIUS / size; our world is rendered
	# at WORLD_SCALE, so divide it out → target_zoom = (BASE_VISIBLE_RADIUS /
	# WORLD_SCALE) / size. `size` grows with mass (capped) plus a split step, so the
	# view zooms out as you grow — without this our view stayed too tight and motion
	# read far too fast at high mass.
	var size: float = 10.0 + minf(10.0, total_mass / 5.0) + (30.0 if circle_count >= 2 else 0.0)
	var target_zoom: float = (BASE_VISIBLE_RADIUS / WORLD_SCALE) / maxf(size, 1.0)
	var z: float = lerpf(camera.zoom.x, target_zoom, delta * 2.0)
	camera.zoom = Vector2(z, z)

# --- Death / Respawn ---


func _on_local_player_died() -> void:
	game_started = false
	death_screen.visible = true


func on_enter_game(player_name: String) -> void:
	var name_to_send: String = player_name.strip_edges()
	if name_to_send.is_empty():
		name_to_send = "Player"
	_player_name = name_to_send
	print("Entering game as: %s" % name_to_send)
	var enter_game := SpacetimeDB.Blackholio.reducers.enter_game(name_to_send)
	print("enter_game reducer call sent, outcome: %s" % enter_game.outcome)
	username_screen.visible = false
	game_started = true


func on_respawn() -> void:
	SpacetimeDB.Blackholio.reducers.respawn()
	death_screen.visible = false
	game_started = true

# --- World border ---


func _draw_world_border() -> void:
	var line := Line2D.new()
	var s: float = float(world_size) * WORLD_SCALE
	line.points = PackedVector2Array(
		[
			Vector2(0, 0),
			Vector2(s, 0),
			Vector2(s, s),
			Vector2(0, s),
			Vector2(0, 0),
		],
	)
	line.width = 2.0
	line.default_color = Color(0.4, 0.4, 0.4, 0.8)
	world_border.add_child(line)

# --- Leaderboard helpers ---


func get_leaderboard_data() -> Array[Dictionary]:
	var entries: Array[Dictionary] = []
	for pid: int in player_circles:
		var circles: Array = player_circles[pid]
		if circles.is_empty():
			continue
		var total_mass: int = 0
		for eid: int in circles:
			var entity: Resource = SpacetimeDB.Blackholio.db.entity.entity_id.find(eid)
			if entity:
				total_mass += entity.mass
		if total_mass > 0:
			entries.append(
				{
					"player_id": pid,
					"name": player_names.get(pid, "???"),
					"mass": total_mass,
					"is_local": pid == local_player_id,
				},
			)
	entries.sort_custom(func(a: Dictionary, b: Dictionary) -> bool: return a.mass > b.mass)
	return entries
