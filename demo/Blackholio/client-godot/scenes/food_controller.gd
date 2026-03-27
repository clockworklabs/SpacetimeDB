class_name FoodController extends EntityController

@export var color_palette: Array[Color] = [
	Color(0.988, 0.678, 1.000),
	Color(0.980, 0.573, 1.000),
	Color(0.965, 0.471, 1.000),
	Color(0.984, 0.788, 1.000),
	Color(0.976, 0.722, 1.000),
	Color(0.961, 0.647, 1.000)
]

func _draw():
	# if !player_owner: return
	draw_circle(Vector2.ZERO, actual_scale.x, color)
	
func spawn(food: BlackholioFood):
	super.spawn_entity(food.entity_id)
	color = color_palette[fmod(food.entity_id, color_palette.size())]
