class_name PlayerController extends Node2D

@export var username: String = ""
@export var number_of_owned_circles: int = 0:
	get():
		return owned_circles.size()
@export var is_local_player: bool = false:
	get():
		return GameManager.local_player == self

const SEND_UPDATES_PER_SEC: int = 20
const SEND_UPDATES_FREQUENCY: float = 1.0 / SEND_UPDATES_PER_SEC

var player_id: int
var last_movement_timestamp: float = 0
var lock_input_position: Vector2 = Vector2.ZERO
var owned_circles: Array[CircleController] = []

func initialize(player: BlackholioPlayer):
	player_id = player.player_id
	if player.identity.hex_encode() == GameManager.local_identity:
		GameManager.local_player = self

func on_destroy():
	for circle in owned_circles:
		circle.queue_free()
		
func on_circle_spawned(circle: CircleController):
	owned_circles.append(circle)

func on_circle_deleted(circle: CircleController):
	if !is_local_player: return
	
	for owned_circle in owned_circles:
		if owned_circle == circle:
			circle.queue_free()
	owned_circles.erase(circle)
	
	if number_of_owned_circles == 0:
		GameManager.died.emit()
		return

func total_mass() -> float:
	var mass: float = 0
	var db = SpacetimeDB.get_local_database()
	for circle in owned_circles:
		if !is_instance_valid(circle): continue
		var entity = db.get_row("entity", circle.entity_id)
		if entity:
			mass += entity.mass
	return mass
		
func center_of_mass() -> Vector2:
	if number_of_owned_circles == 0:
		return Vector2.ZERO
		
	var db = SpacetimeDB.get_local_database()
	var total_pos := Vector2.ZERO
	var total_mass := 0.0
	
	for circle in owned_circles:
		if !is_instance_valid(circle): continue
		var entity = db.get_row("entity", circle.entity_id)
		if entity:
			total_pos += circle.global_position * entity.mass
			total_mass += entity.mass
		
	return total_pos / total_mass

func _process(delta: float):
	if !GameManager.local_player or number_of_owned_circles == 0:
		return
		
	if Input.is_action_pressed("split"):
		BlackholioModule.player_split()
	
	if Input.is_action_pressed("lock_input"):
		if lock_input_position != Vector2.ZERO:
			lock_input_position = Vector2.ZERO
		else:
			lock_input_position = get_viewport().get_mouse_position()
	
	if Input.is_action_pressed("suicide"):
		BlackholioModule.suicide()
		pass
		
	if Time.get_ticks_msec() - last_movement_timestamp >= SEND_UPDATES_FREQUENCY:
		last_movement_timestamp = Time.get_ticks_msec()
		var mouse_position = get_viewport().get_mouse_position() if lock_input_position == Vector2.ZERO else lock_input_position
		var screen_size := get_viewport_rect().size
		var center_of_screen := screen_size / 2
		var direction = (mouse_position - center_of_screen) / (screen_size.y / 3)
		BlackholioModule.update_player_input(BlackholioDbVector2.create(direction.x, direction.y))

func on_gui():
	if !is_local_player or !GameManager.is_connected:
		return
		
	# TODO: update label GUI.Label(new Rect(0, 0, 100, 50), $"Total Mass: {TotalMass()}");
