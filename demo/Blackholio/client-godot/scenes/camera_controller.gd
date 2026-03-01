extends Camera2D

@export var menu_camera: Camera2D
@export var menu: Control
@export var target_zoom: Vector2 = Vector2(5.0, 5.0)
@export var target_position: Vector2 = Vector2.ZERO

@export var default_position: Vector2 = Vector2.ZERO
@export var default_zoom: float = 4.5
@export var mass_multiplier: float = 0.0005
@export var circle_multiplier: float = 0.001

var world_size: int = 1000:
	set(new_value):
		world_size = new_value
		arena_center_transform = Vector2(world_size / 4, world_size / 4)
		menu_camera.position = default_position
		menu.position = default_position
var arena_center_transform := Vector2(world_size / 4, world_size / 4)

func _process(delta: float):
	zoom = lerp(zoom, target_zoom, delta * 2)
	offset = lerp(offset, target_position, delta * 2)

	var local_player = GameManager.local_player
	if (local_player == null || !GameManager.is_connected):
		# Set the camera to be in middle of the arena if we are not connected or 
		# there is no local player
		target_zoom = Vector2(1.0, 1.0)
		target_position = Vector2.ZERO
		return
		
	ImGui.Begin("Camera")
	ImGui.Text("Zoom: %s" % target_zoom)
	ImGui.Text("Mass: %s" % GameManager.local_player.total_mass())
	ImGui.Text("Circles: %s" % GameManager.local_player.number_of_owned_circles)
	ImGui.End()

	var center_of_mass = local_player.center_of_mass()
	if (center_of_mass):
		# Set the camera to be the center of mass of the local player
		# if the local player has one
		target_position = Vector2(center_of_mass.x, center_of_mass.y)
		target_zoom = Vector2.ONE * calculate_camera_zoom(local_player)
	else:
		target_zoom = Vector2(default_zoom, default_zoom)
		target_position = Vector2.ZERO
		menu_camera.zoom = Vector2.ONE * 2
		menu_camera.offset = offset
		menu_camera.position = position
		menu.position = Vector2.ZERO
		menu.scale = Vector2(0.5, 0.5)

func calculate_camera_zoom(local_player: PlayerController):
	var final_zoom = default_zoom - local_player.total_mass() * mass_multiplier
	final_zoom -= local_player.number_of_owned_circles * circle_multiplier
	return final_zoom
