extends Node2D

const LERP_DURATION: float = 0.1 # 100ms interpolation
const VISUAL_SCALE: float = 1.0 # Render radius 1:1 with mass (like upstream); camera zoom handles visibility
const DESPAWN_DURATION: float = 0.2 # consume animation length

var lerp_start_pos: Vector2
var lerp_target_pos: Vector2
var lerp_time: float = LERP_DURATION

var current_radius: float = 1.0
var target_radius: float = 1.0
var current_mass: int = 1

var circle_color: Color = Color.WHITE
var player_id: int = -1
var player_name: String = ""
var animation_seed: float = 0.0 # desyncs the pulse/wave between entities
var label_layer: CanvasLayer = null # screen-space layer for the name label (injected)
var _label: Label = null

var is_despawning: bool = false
var _despawn_consumer: Node2D = null # eater to fly into; null/freed → shrink in place
var _despawn_time: float = 0.0
var _despawn_from: Vector2 = Vector2.ZERO
var _despawn_from_radius: float = 0.0
var _despawn_target: Vector2 = Vector2.ZERO # last-known consumer pos (survives its free)


func _ready() -> void:
	lerp_start_pos = position
	lerp_target_pos = position


func _process(delta: float) -> void:
	if is_despawning:
		_process_despawn(delta)
		return

	# Position interpolation
	lerp_time = minf(lerp_time + delta, LERP_DURATION)
	position = lerp_start_pos.lerp(lerp_target_pos, lerp_time / LERP_DURATION)

	# Radius interpolation
	if not is_equal_approx(current_radius, target_radius):
		current_radius = lerpf(current_radius, target_radius, delta * 8.0)

	_update_label()
	# Redraw every frame so the pulse/wave animates (cheap per node).
	queue_redraw()


func _exit_tree() -> void:
	if is_instance_valid(_label):
		_label.queue_free()
	_label = null


func _draw() -> void:
	if current_radius <= 0.01:
		return
	# Players/circles only — food now renders via the MultiMesh food field.
	_draw_player()


# Layered player blob: translucent pulsing halo, dark rim, color disk, specular
# highlight, and a wavy animated outline. Ports the upstream Circle2D.DrawPlayerCircle.
func _draw_player() -> void:
	var t: float = Time.get_ticks_msec() / 1000.0
	var pulse: float = 0.5 + 0.5 * sin(t * 2.2 + animation_seed)
	# Halo breathes in both size and alpha so the pulse reads clearly.
	draw_circle(Vector2.ZERO, current_radius * (1.12 + pulse * 0.16), _with_alpha(circle_color, 0.06 + pulse * 0.16))
	draw_circle(Vector2.ZERO, current_radius, _shade(circle_color, 0.58))
	draw_circle(Vector2.ZERO, current_radius * 0.82, circle_color)
	draw_circle(
		Vector2(-current_radius * 0.22, -current_radius * 0.24),
		current_radius * 0.34,
		_with_alpha(_shade(circle_color, 1.42), 0.72),
	)

	var outline: PackedVector2Array = PackedVector2Array()
	outline.resize(73)
	for i: int in 73:
		var angle: float = TAU * i / 72.0
		var wave: float = sin(angle * 7.0 + t * 3.0 + animation_seed) * 0.035
		outline[i] = Vector2.from_angle(angle) * current_radius * (1.015 + wave)
	draw_polyline(outline, _with_alpha(_shade(circle_color, 1.55), 0.88), clampf(current_radius * 0.085, 1.5, 5.0), true)


func _shade(c: Color, m: float) -> Color:
	return Color(clampf(c.r * m, 0.0, 1.0), clampf(c.g * m, 0.0, 1.0), clampf(c.b * m, 0.0, 1.0), c.a)


func _with_alpha(c: Color, a: float) -> Color:
	return Color(c.r, c.g, c.b, a)


func update_target(new_pos: Vector2, new_mass: int) -> void:
	lerp_start_pos = position
	lerp_target_pos = new_pos
	lerp_time = 0.0

	if new_mass != current_mass:
		current_mass = new_mass
		target_radius = sqrt(float(new_mass)) * VISUAL_SCALE
		queue_redraw()


func set_mass(mass: int) -> void:
	current_mass = mass
	target_radius = sqrt(float(mass)) * VISUAL_SCALE
	# Start at 0 so the circle grows in via the _process radius lerp (matches the
	# upstream client, which seeds radius 0 on spawn). Spawn-only — live mass changes
	# go through update_target and keep their smooth resize.
	current_radius = 0.0
	queue_redraw()


func set_circle_info(pid: int, pname: String, color: Color = Color.WHITE) -> void:
	player_id = pid
	player_name = pname
	circle_color = color
	if player_name.is_empty():
		if is_instance_valid(_label):
			_label.queue_free()
		_label = null
	else:
		_ensure_label()
	queue_redraw()


# Name labels live on a screen-space CanvasLayer (injected as label_layer) so they
# render at a constant, crisp font size instead of being magnified by the camera
# zoom (which made the in-node draw_string label pixelated).
func _ensure_label() -> void:
	if label_layer == null or player_name.is_empty():
		return
	if not is_instance_valid(_label):
		_label = Label.new()
		_label.add_theme_font_size_override("font_size", 14)
		_label.add_theme_color_override("font_color", Color.WHITE)
		_label.add_theme_color_override("font_outline_color", Color(0.0, 0.0, 0.0, 0.85))
		_label.add_theme_constant_override("outline_size", 4)
		_label.z_index = 100
		label_layer.add_child(_label)
	_label.text = player_name


func _update_label() -> void:
	if not is_instance_valid(_label):
		return
	var x: Transform2D = get_global_transform_with_canvas()
	var screen_scale: float = x.get_scale().y
	var w: float = ThemeDB.fallback_font.get_string_size(player_name, HORIZONTAL_ALIGNMENT_LEFT, -1, 14).x
	# Sit clear below the circle + its halo (screen radius ~ current_radius * zoom).
	_label.position = x.origin + Vector2(-w * 0.5, current_radius * screen_scale * 1.3 + 8.0)


## Starts the consume animation: fly into [param consumer] while shrinking to
## nothing, then free. [param consumer] may be null (consumer not spawned locally)
## — then it shrinks in place. Driven per-frame in [method _process_despawn] so it
## chases a moving consumer rather than aiming at a stale position.
func despawn_into(consumer: Node2D) -> void:
	if is_despawning:
		return
	is_despawning = true
	_despawn_consumer = consumer
	_despawn_time = 0.0
	_despawn_from = position
	_despawn_from_radius = current_radius
	_despawn_target = consumer.position if is_instance_valid(consumer) else position
	z_index += 10 # render over the consumer during the fly-in
	if is_instance_valid(_label):
		_label.hide() # no name tag while being eaten


func _process_despawn(delta: float) -> void:
	_despawn_time = minf(_despawn_time + delta, DESPAWN_DURATION)
	var t: float = _despawn_time / DESPAWN_DURATION

	# Re-read the consumer each frame so we chase it if it's still moving; cache the
	# last-known position so a consumer freed mid-animation doesn't strand us.
	if is_instance_valid(_despawn_consumer):
		_despawn_target = _despawn_consumer.position
	position = _despawn_from.lerp(_despawn_target, t)
	current_radius = lerpf(_despawn_from_radius, 0.0, t)
	queue_redraw()

	if _despawn_time >= DESPAWN_DURATION:
		queue_free()
