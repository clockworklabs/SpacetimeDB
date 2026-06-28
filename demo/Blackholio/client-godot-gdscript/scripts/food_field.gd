extends MultiMeshInstance2D
## GPU-instanced food field. Every food pellet is one instance of a shared quad
## (shaders/food.gdshader draws the blob), so all food renders in a single draw
## call instead of one Node2D + _draw per pellet. Keeps the per-pellet position
## lerp, grow-in, color, and pulse (the pulse runs in the shader via TIME + seed).
##
## This is the proof for the perf path: drive many homogeneous entities through one
## batched node (DOD D8) rather than N scripted nodes. Players (few, fancy) still
## use entity_node; food (many, identical) routes here.

const MAX_FOOD: int = 8192
const HALO_EXTENT: float = 1.5 # quad scale vs food radius (room for the halo)
const LERP_DURATION: float = 0.1


## Per-pellet runtime state. POD record; the field's _process transforms it into
## MultiMesh instance writes each frame.
class FoodInst:
	extends RefCounted
	var index: int = 0
	var pos: Vector2 = Vector2.ZERO
	var from: Vector2 = Vector2.ZERO
	var target: Vector2 = Vector2.ZERO
	var lerp_time: float = 0.0
	var radius: float = 0.0 # grows in from 0
	var target_radius: float = 1.0
	var color: Color = Color.WHITE
	var anim_seed: float = 0.0


var _food: Dictionary[int, FoodInst] = { } # entity_id -> instance state
var _free: Array[int] = [] # available instance indices


func _ready() -> void:
	var mm: MultiMesh = MultiMesh.new()
	mm.transform_format = MultiMesh.TRANSFORM_2D
	mm.use_colors = true
	mm.use_custom_data = true
	var quad: QuadMesh = QuadMesh.new()
	quad.size = Vector2(2.0, 2.0) # centered coords reach [-1, 1]; the shader maps the blob
	mm.mesh = quad
	mm.instance_count = MAX_FOOD
	multimesh = mm

	var mat: ShaderMaterial = ShaderMaterial.new()
	mat.shader = preload("res://shaders/food.gdshader")
	material = mat

	z_index = -1 # food sits just under player circles (above the starfield at -1000)

	# All instances start hidden (zero scale); claimed on demand.
	_free.resize(0)
	for i: int in MAX_FOOD:
		_free.append(i)
		_hide(i)


func _process(delta: float) -> void:
	for eid: int in _food:
		var f: FoodInst = _food[eid]
		f.lerp_time = minf(f.lerp_time + delta, LERP_DURATION)
		f.pos = f.from.lerp(f.target, f.lerp_time / LERP_DURATION)
		f.radius = lerpf(f.radius, f.target_radius, delta * 8.0)
		_write(f)


func has_food(entity_id: int) -> bool:
	return _food.has(entity_id)


## Registers a new pellet. No-op if the field is full or the id already exists.
func add_food(entity_id: int, pos: Vector2, mass: int, color: Color, anim_seed: float) -> void:
	if _food.has(entity_id) or _free.is_empty():
		return
	var f: FoodInst = FoodInst.new()
	f.index = _free.pop_back()
	f.pos = pos
	f.from = pos
	f.target = pos
	f.target_radius = sqrt(float(mass))
	f.color = color
	f.anim_seed = anim_seed
	_food[entity_id] = f
	_write(f)


## Retargets an existing pellet's position/mass (smoothly lerped).
func update_food(entity_id: int, pos: Vector2, mass: int) -> void:
	var f: FoodInst = _food.get(entity_id)
	if f == null:
		return
	f.from = f.pos
	f.target = pos
	f.lerp_time = 0.0
	f.target_radius = sqrt(float(mass))


## Releases a pellet's instance back to the pool.
func remove_food(entity_id: int) -> void:
	var f: FoodInst = _food.get(entity_id)
	if f == null:
		return
	_hide(f.index)
	_free.append(f.index)
	_food.erase(entity_id)


func _write(f: FoodInst) -> void:
	var s: float = f.radius * HALO_EXTENT
	multimesh.set_instance_transform_2d(f.index, Transform2D(Vector2(s, 0.0), Vector2(0.0, s), f.pos))
	multimesh.set_instance_color(f.index, f.color)
	multimesh.set_instance_custom_data(f.index, Color(f.anim_seed, 0.0, 0.0, 0.0))


func _hide(index: int) -> void:
	multimesh.set_instance_transform_2d(index, Transform2D(Vector2.ZERO, Vector2.ZERO, Vector2.ZERO))
