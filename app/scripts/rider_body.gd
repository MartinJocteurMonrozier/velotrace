extends Node3D
## Corps du cycliste : maillage humain skinné (MakeHuman, CC0), tenue peinte
## par le shader, casque, lunettes et chaussures. cyclist.gd calcule à chaque
## image les cibles articulaires (bassin, dos, tête, genoux, chevilles, coudes,
## mains) et appelle apply() ; le squelette s'y ajuste avec les longueurs
## réelles de ses os, pieds sur les pédales et mains sur le cintre.
## Avant vers -Z, droite du cycliste vers +X (côté 0 : gauche, 1 : droite).

## Maillage construit par tools/rider/build_rider.py.
const ASSET := "res://assets/rider/rider.bin"
const SHADER := preload("res://shaders/rider.gdshader")

## Bascule du bassin vers l'avant, en part de l'inclinaison du dos.
const PELVIS_TILT := 0.45
## Répartition de la flexion du dos sur les vertèbres (bas vers haut).
const SPINE_SPREAD := [0.12, 0.17, 0.21, 0.27, 0.23]
## Mains : centre du tube serré sous le point de contact (cocottes, bas du
## cintre) et dos de la main levé au-dessus de l'horizontale (rad).
const GRIP_DEPTH := Vector2(0.03, 0.016)
const GRIP_ROLL := Vector2(0.95, 0.35)
## Avant-bras : part de la rotation de la main prise par chaque moitié.
const FOREARM_TWIST := Vector2(0.25, 0.65)
## Épaules avancées (rad au plus) quand les bras ne suffisent pas.
const CLAVICLE_REACH := 0.35
## Part du lever du bras reprise par la clavicule (épaule moins cassée).
const CLAVICLE_FOLLOW := 0.22

## Ressources partagées par tous les cyclistes : maillage, peau, données.
static var _shared := {}
static var _materials := {}

## Cibles, remplies par cyclist.gd avant apply().
var body := Basis()
var hip_mid := Vector3.ZERO
var spine := Vector3.UP
var head := Transform3D()
var bike := Basis()
var drops := 0.0
var knees: Array[Vector3] = [Vector3.ZERO, Vector3.ZERO]
var ankles: Array[Vector3] = [Vector3.ZERO, Vector3.ZERO]
var feet: Array[Basis] = [Basis(), Basis()]
var elbows: Array[Vector3] = [Vector3.ZERO, Vector3.ZERO]
var hands: Array[Vector3] = [Vector3.ZERO, Vector3.ZERO]

var _skeleton: Skeleton3D
var _mesh: MeshInstance3D
var _rot: Array[Basis] = []
var _pos := PackedVector3Array()


## Tenue : maillot, maillot foncé, liseré.
func setup(jersey: Color, jersey_dark: Color, accent: Color) -> void:
	var d := _data()
	_skeleton = Skeleton3D.new()
	var heads: PackedVector3Array = d.heads
	var parents: PackedInt32Array = d.parents
	for i in parents.size():
		_skeleton.add_bone(d.names[i])
		if parents[i] >= 0:
			_skeleton.set_bone_parent(i, parents[i])
		var rest := heads[i] - (heads[parents[i]] if parents[i] >= 0 else Vector3.ZERO)
		_skeleton.set_bone_rest(i, Transform3D(Basis(), rest))
	_skeleton.reset_bone_poses()
	add_child(_skeleton)
	_mesh = MeshInstance3D.new()
	_mesh.mesh = d.mesh
	_mesh.skin = d.skin
	_skeleton.add_child(_mesh)
	_mesh.skeleton = NodePath("..")
	_mesh.material_override = _material(jersey, jersey_dark, accent)
	_rot.resize(parents.size())
	_rot.fill(Basis())
	_pos.resize(parents.size())


static func _material(jersey: Color, jersey_dark: Color, accent: Color) -> ShaderMaterial:
	var key := "%s/%s/%s" % [jersey.to_html(), jersey_dark.to_html(), accent.to_html()]
	if not _materials.has(key):
		var m := ShaderMaterial.new()
		m.shader = SHADER
		m.set_shader_parameter("jersey", jersey)
		m.set_shader_parameter("jersey_dark", jersey_dark)
		m.set_shader_parameter("accent", accent)
		_materials[key] = m
	return _materials[key]


## Lit le fichier du maillage une fois pour toutes.
static func _data() -> Dictionary:
	if not _shared.is_empty():
		return _shared
	var f := FileAccess.open(ASSET, FileAccess.READ)
	if f == null or f.get_buffer(4).get_string_from_ascii() != "VTRB":
		push_error("Cycliste : %s illisible" % ASSET)
		return _shared
	f.get_32()
	var meta: Dictionary = JSON.parse_string(f.get_buffer(f.get_32()).get_string_from_utf8())
	var blob := f.get_buffer(f.get_length() - f.get_position()).decompress(int(meta.blob_size), FileAccess.COMPRESSION_GZIP)
	var arr := []
	arr.resize(Mesh.ARRAY_MAX)
	var spans: Dictionary = meta.arrays
	var get_bytes := func(key: String, size: int) -> PackedByteArray:
		var span: Array = spans[key]
		return blob.slice(int(span[0]), int(span[0]) + int(span[1]) * size)
	arr[Mesh.ARRAY_VERTEX] = get_bytes.call("vertex", 4).to_vector3_array()
	arr[Mesh.ARRAY_NORMAL] = get_bytes.call("normal", 4).to_vector3_array()
	arr[Mesh.ARRAY_BONES] = get_bytes.call("bones", 4).to_int32_array()
	arr[Mesh.ARRAY_WEIGHTS] = get_bytes.call("weights", 4).to_float32_array()
	arr[Mesh.ARRAY_CUSTOM0] = get_bytes.call("custom0", 4).to_float32_array()
	arr[Mesh.ARRAY_CUSTOM1] = get_bytes.call("custom1", 4).to_float32_array()
	arr[Mesh.ARRAY_CUSTOM2] = get_bytes.call("custom2", 1)
	arr[Mesh.ARRAY_INDEX] = get_bytes.call("index", 4).to_int32_array()
	var flags := (Mesh.ARRAY_CUSTOM_RGBA_FLOAT << Mesh.ARRAY_FORMAT_CUSTOM0_SHIFT) \
			| (Mesh.ARRAY_CUSTOM_RGBA_FLOAT << Mesh.ARRAY_FORMAT_CUSTOM1_SHIFT) \
			| (Mesh.ARRAY_CUSTOM_RGBA8_UNORM << Mesh.ARRAY_FORMAT_CUSTOM2_SHIFT)
	# Niveaux de détail : erreur (m) -> index simplifiés sur les mêmes sommets.
	var lods := {}
	for l: Array in meta.get("lods", []):
		lods[float(l[0])] = get_bytes.call(l[1], 4).to_int32_array()
	var mesh := ArrayMesh.new()
	mesh.add_surface_from_arrays(Mesh.PRIMITIVE_TRIANGLES, arr, [], lods, flags)
	# Le maillage posé déborde largement de sa boîte de repos.
	mesh.custom_aabb = AABB(Vector3(-1.0, -0.3, -1.5), Vector3(2.0, 2.3, 2.6))

	var names := PackedStringArray()
	var parents := PackedInt32Array()
	var heads := PackedVector3Array()
	var tails := PackedVector3Array()
	var skin := Skin.new()
	for b: Dictionary in meta.bones:
		names.append(b.name)
		parents.append(names.find(b.parent) if b.parent != null else -1)
		heads.append(_v(b.head))
		tails.append(_v(b.tail))
		skin.add_bind(names.size() - 1, Transform3D(Basis(), -heads[-1]))
	var idx := {}
	for i in names.size():
		idx[names[i]] = i
	_shared = {"mesh": mesh, "skin": skin, "names": names, "parents": parents, "heads": heads, "tails": tails, "idx": idx}
	for s in 2:
		var g: Dictionary = meta.grips["LR"[s]]
		var a := _v(g.axis)
		var dor := _v(g.dorsal)
		_shared["grip%d" % s] = Transform3D(Basis(a, dor, a.cross(dor)), _v(g.center))
	return _shared


static func _v(a: Array) -> Vector3:
	return Vector3(a[0], a[1], a[2])


## Pose le squelette d'après les cibles.
func apply() -> void:
	var d := _shared
	var H: PackedVector3Array = d.heads
	var T: PackedVector3Array = d.tails
	var I: Dictionary = d.idx
	var right := body.x
	var spine_dir := spine.normalized()
	var up := body.y

	# Bassin basculé vers l'avant, hanches au milieu des cibles.
	var lean := up.angle_to(spine_dir)
	var r_root := Basis(right, -lean * PELVIS_TILT) * body
	var root := I.root as int
	var hip_rest := (H[I["thigh.L"]] + H[I["thigh.R"]]) * 0.5
	_put(root, r_root, hip_mid + r_root * (H[root] - hip_rest))

	# Dos : flexion répartie sur les vertèbres, réglée pour que le milieu des
	# épaules suive la direction du dos.
	var chain := [I.spine05, I.spine04, I.spine03, I.spine02, I.spine01]
	var sh_rest := (H[I["upperarm.L"]] + H[I["upperarm.R"]]) * 0.5
	var bend := 0.0
	for _iter in 4:
		var r := r_root
		var prev := root
		for k in chain.size():
			var b: int = chain[k]
			var p := _pos[prev] + _rot[prev] * (H[b] - H[prev])
			r = Basis(right, bend * SPINE_SPREAD[k]) * r
			_put(b, r, p)
			prev = b
		var top: int = chain[-1]
		var sh := _pos[top] + _rot[top] * (sh_rest - H[top])
		bend += _signed_angle(sh - _pos[chain[0]], spine_dir, right)

	# Cou et tête : la tête prend l'orientation voulue, le cou la rejoint.
	var s1 := I.spine01 as int
	var neck := I.neck as int
	var hd := I.head as int
	var neck_p := _pos[s1] + _rot[s1] * (H[neck] - H[s1])
	var head_rest_c := H[hd] + Vector3(0.0, 0.09, -0.02)
	var head_joint := head.origin - head.basis * (head_rest_c - H[hd])
	var neck_rest := _rot[s1] * (H[hd] - H[neck])
	var r_neck := _arc(neck_rest, head_joint - neck_p) * _rot[s1]
	_put(neck, r_neck, neck_p)
	_put(hd, head.basis, neck_p + r_neck * (H[hd] - H[neck]))

	for s in 2:
		var side := "LR"[s]
		var sg := -1.0 if s == 0 else 1.0
		# Jambe : hanche, genou du côté de la cible, pied à plat sur la pédale.
		var th := I["thigh." + side] as int
		var sn := I["shin." + side] as int
		var ft := I["foot." + side] as int
		var hip := _pos[root] + r_root * (H[th] - H[root])
		_limb(th, sn, ft, hip, ankles[s], knees[s], Vector3.FORWARD)
		_put(ft, feet[s], _pos[ft])

		# Main : repère de prise aligné sur le tube, dos de la main vers
		# l'extérieur et le haut.
		var grip: Transform3D = d["grip%d" % s]
		var axis := bike * (Vector3(0.0, 0.14, -1.0).normalized().lerp(Vector3.FORWARD, drops)).normalized()
		var roll := lerpf(GRIP_ROLL.x, GRIP_ROLL.y, drops)
		var dorsal := bike * Vector3(sg * cos(roll), sin(roll), 0.0)
		dorsal = (dorsal - axis * dorsal.dot(axis)).normalized()
		var want := Basis(axis, dorsal, axis.cross(dorsal))
		var r_hand := want * grip.basis.inverse()
		var center := hands[s] - bike.y * lerpf(GRIP_DEPTH.x, GRIP_DEPTH.y, drops)
		var hn := I["hand." + side] as int
		var wrist := center - r_hand * (grip.origin - H[hn])

		# Épaule : la clavicule avance si le bras ne suffit pas.
		var cl := I["clavicle." + side] as int
		var ua := I["upperarm." + side] as int
		var la1 := I["lowerarm01." + side] as int
		var la2 := I["lowerarm02." + side] as int
		var cl_p := _pos[s1] + _rot[s1] * (H[cl] - H[s1])
		var cl_dir := _rot[s1] * (H[ua] - H[cl])
		var reach := H[ua].distance_to(H[la1]) + H[la1].distance_to(H[hn])
		# L'omoplate suit en partie le bras qui se lève vers le cintre.
		var arm_rest := _rot[s1] * (H[la1] - H[ua])
		var lift := _arc(arm_rest, wrist - (cl_p + cl_dir))
		var r_cl := Basis(lift.get_rotation_quaternion().slerp(Quaternion(), 1.0 - CLAVICLE_FOLLOW)) * _rot[s1]
		cl_dir = r_cl * (H[ua] - H[cl])
		var shoulder := cl_p + cl_dir
		var gap := shoulder.distance_to(wrist) - reach * 0.985
		if gap > 0.0:
			var axis_cl := cl_dir.cross(wrist - cl_p).normalized()
			r_cl = Basis(axis_cl, minf(gap / cl_dir.length(), CLAVICLE_REACH)) * r_cl
			shoulder = cl_p + r_cl * (H[ua] - H[cl])
		_put(cl, r_cl, cl_p)
		_limb(ua, la1, hn, shoulder, wrist, elbows[s], Vector3.BACK)
		# Avant-bras vrillé progressivement vers l'orientation de la main.
		var r_fore := _rot[la1]
		var fore_rest := (H[hn] - H[la1]).normalized()
		var rel := (r_fore.inverse() * r_hand).get_rotation_quaternion()
		var twist := 2.0 * atan2(Vector3(rel.x, rel.y, rel.z).dot(fore_rest), rel.w)
		twist = wrapf(twist, -PI, PI)
		_put(la1, r_fore * Basis(fore_rest, twist * FOREARM_TWIST.x), _pos[la1])
		_put(la2, r_fore * Basis(fore_rest, twist * FOREARM_TWIST.y), _pos[la1] + r_fore * (H[la2] - H[la1]))
		_put(hn, r_hand, _pos[la1] + r_fore * (H[hn] - H[la1]))

	# Poses locales du squelette (os au repos sans rotation).
	var parents: PackedInt32Array = d.parents
	for i in parents.size():
		var p := parents[i]
		if p < 0:
			_skeleton.set_bone_pose_rotation(i, _rot[i].get_rotation_quaternion())
			_skeleton.set_bone_pose_position(i, _pos[i])
		else:
			var inv := _rot[p].inverse()
			_skeleton.set_bone_pose_rotation(i, (inv * _rot[i]).get_rotation_quaternion())
			_skeleton.set_bone_pose_position(i, inv * (_pos[i] - _pos[p]))


func _put(i: int, r: Basis, p: Vector3) -> void:
	_rot[i] = r.orthonormalized()
	_pos[i] = p


## Chaîne à deux os (cuisse et jambe, bras et avant-bras) de `root` vers
## `target`, l'articulation du milieu du côté de `pole`. Le repère de chaque os
## suit l'axe de flexion : pas de vrille parasite. `rest_pole` donne le côté
## de flexion dans la pose de repos (genou vers l'avant, coude vers l'arrière).
func _limb(a: int, b: int, c: int, root: Vector3, target: Vector3, pole: Vector3, rest_pole: Vector3) -> void:
	var H: PackedVector3Array = _shared.heads
	var l1 := H[a].distance_to(H[b])
	var l2 := H[b].distance_to(H[c])
	var d := target - root
	var dist := clampf(d.length(), 0.05, l1 + l2 - 0.0005)
	var dir := d.normalized()
	var pv := pole - root
	pv = (pv - dir * pv.dot(dir)).normalized()
	var cos_a := (l1 * l1 + dist * dist - l2 * l2) / (2.0 * l1 * dist)
	var ang := acos(clampf(cos_a, -1.0, 1.0))
	var mid := root + dir * (l1 * cos(ang)) + pv * (l1 * sin(ang))
	var end := mid + (target - mid).normalized() * l2
	var hinge := dir.cross(pv).normalized()
	# Même construction dans la pose de repos.
	var d0 := H[c] - H[a]
	var p0 := (H[b] - H[a]) - d0.normalized() * (H[b] - H[a]).dot(d0.normalized())
	if p0.length() < 0.01:
		p0 = rest_pole
	p0 = (p0 - d0.normalized() * p0.dot(d0.normalized())).normalized()
	var hinge0 := d0.normalized().cross(p0).normalized()
	_put(a, _frame(hinge, mid - root) * _frame(hinge0, H[b] - H[a]).inverse(), root)
	_put(b, _frame(hinge, end - mid) * _frame(hinge0, H[c] - H[b]).inverse(), mid)
	_pos[c] = end


static func _frame(hinge: Vector3, seg: Vector3) -> Basis:
	var y := seg.normalized()
	var x := (hinge - y * hinge.dot(y)).normalized()
	return Basis(x, y, x.cross(y))


## Rotation la plus courte qui amène `from` sur `to`.
static func _arc(from: Vector3, to: Vector3) -> Basis:
	var a := from.normalized()
	var b := to.normalized()
	var axis := a.cross(b)
	if axis.length() < 1e-6:
		return Basis()
	return Basis(axis.normalized(), a.angle_to(b))


static func _signed_angle(a: Vector3, b: Vector3, axis: Vector3) -> float:
	var pa := (a - axis * a.dot(axis)).normalized()
	var pb := (b - axis * b.dot(axis)).normalized()
	return atan2(axis.dot(pa.cross(pb)), pa.dot(pb))
