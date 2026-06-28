extends Node2D
## Parallax-free starfield + nebula background. Ports the upstream Blackholio
## client's StarfieldBackground: a deterministic field of twinkling stars and three
## translucent nebula blobs, sized to the world. Drawn in world space behind
## everything (z_index -1000) so the camera pans over it.

const WORLD_SCALE: float = 1.0 # match main.gd so star coords line up with the arena
const BG_COLOR: Color = Color(0.006, 0.009, 0.024)
const STAR_SEED: int = 0xB1AC40E10

var _extent: float = 1000.0
var _time: float = 0.0
# Each star: {pos: Vector2, radius: float, phase: float, twinkle: float, color: Color}
var _stars: Array[Dictionary] = []


## Builds the field for a [param world_size] arena (in unscaled world units).
func setup(world_size: int) -> void:
	_extent = float(world_size) * WORLD_SCALE
	z_index = -1000
	_generate(world_size)
	queue_redraw()


func _process(delta: float) -> void:
	_time += delta
	queue_redraw()


func _draw() -> void:
	draw_rect(Rect2(Vector2.ZERO, Vector2(_extent, _extent)), BG_COLOR)
	draw_circle(Vector2(_extent * 0.22, _extent * 0.68), _extent * 0.18, Color(0.15, 0.32, 0.58, 0.08))
	draw_circle(Vector2(_extent * 0.76, _extent * 0.24), _extent * 0.22, Color(0.45, 0.16, 0.52, 0.07))
	draw_circle(Vector2(_extent * 0.54, _extent * 0.55), _extent * 0.26, Color(0.0, 0.62, 0.72, 0.045))

	for star: Dictionary in _stars:
		var pulse: float = 0.68 + 0.22 * sin(_time * star["twinkle"] + star["phase"])
		var c: Color = star["color"]
		draw_circle(star["pos"], star["radius"] * (0.9 + pulse * 0.1), Color(c.r, c.g, c.b, c.a * pulse))


func _generate(world_size: int) -> void:
	var rng: RandomNumberGenerator = RandomNumberGenerator.new()
	rng.seed = STAR_SEED
	var count: int = roundi(float(world_size) * 0.55)
	_stars.clear()
	for i: int in count:
		var warmth: float = rng.randf_range(0.0, 1.0)
		_stars.append(
			{
				"pos": Vector2(rng.randf_range(0.0, _extent), rng.randf_range(0.0, _extent)),
				"radius": rng.randf_range(0.35, 1.15) * WORLD_SCALE,
				"phase": rng.randf_range(0.0, TAU),
				"twinkle": rng.randf_range(0.7, 1.9),
				"color": Color(
					lerpf(0.50, 0.78, warmth),
					lerpf(0.56, 0.78, warmth),
					lerpf(0.76, 0.94, warmth),
					rng.randf_range(0.16, 0.42),
				),
			},
		)
