class_name CircleController extends EntityController

@export var color_palette: Array[Color] = [
	Color(0.686, 0.624, 0.192),
	Color(0.686, 0.455, 0.192),
	Color(0.439, 0.184, 0.988),
	Color(0.200, 0.357, 0.988),
	Color(0.690, 0.212, 0.212),
	Color(0.690, 0.427, 0.212),
	Color(0.553, 0.169, 0.388),
	Color(0.008, 0.737, 0.980),
	Color(0.027, 0.196, 0.984),
	Color(0.008, 0.110, 0.573) 
]

var player_owner: PlayerController

func _draw():
	# if !player_owner: return
	draw_circle(Vector2.ZERO, actual_scale.x * 0.5, color.darkened(0.2), false, 2.0)
	draw_circle(Vector2.ZERO, actual_scale.x * 0.5, color)

func spawn(circle: BlackholioCircle, input_owner: PlayerController):
	spawn_entity(circle.entity_id)
	color = color_palette[fmod(circle.player_id, color_palette.size())]
	player_owner = input_owner
	get_node("Label").text = input_owner.username
	player_owner.on_circle_spawned(self)

func on_delete():
	player_owner.on_circle_deleted(self)
	queue_free()
