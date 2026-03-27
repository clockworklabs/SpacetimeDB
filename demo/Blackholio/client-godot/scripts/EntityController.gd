class_name EntityController extends Node2D

const LERP_DURATION_SEC: float = 0.1

var color: Color = Color.DEEP_PINK
var entity_id: int
var lerp_time: float
var lerp_target_position: BlackholioDbVector2
var target_scale: Vector2
var actual_scale: Vector2 = Vector2.ONE

func _process(delta: float):
	lerp_time = min(lerp_time + delta, LERP_DURATION_SEC)
	if lerp_target_position:
		global_position = lerp(global_position, Vector2(lerp_target_position.x, lerp_target_position.y), lerp_time / LERP_DURATION_SEC)
	actual_scale = lerp(actual_scale, target_scale, delta * 8)
	
func spawn_entity(input_entity_id: int):
	entity_id = input_entity_id
	var db = SpacetimeDB.get_local_database()
	var entities = db.get_all_rows("entity")
	for entity in entities:
		if entity.entity_id == input_entity_id:
			global_position = Vector2(entity.position.x, entity.position.y)
			lerp_target_position = entity.position
			global_position = Vector2(entity.position.x, entity.position.y)
			scale = Vector2.ONE
			target_scale = mass_to_scale(entity.mass)

func on_delete():
	queue_free()

func on_entity_update(entity: BlackholioEntity):
	lerp_time = 0.0
	lerp_target_position = entity.position
	target_scale = mass_to_scale(entity.mass)
	queue_redraw()

func mass_to_scale(mass: float):
	var diameter = mass_to_diameter(mass)
	return Vector2(diameter, diameter)

func mass_to_radius(mass: float): return sqrt(mass)
func mass_to_diameter(mass: float): return mass_to_radius(mass) * 2
