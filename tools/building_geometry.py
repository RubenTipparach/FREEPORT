"""Editable Blender solids, boolean cutters, and matching collision boxes."""
import math
import bpy
from mathutils import Vector

CONCRETE, PLATE, GLASS, LAMP = 1, 2, 3, 4


def collection(name):
    result = bpy.data.collections.new(name)
    bpy.context.scene.collection.children.link(result)
    return result


def material(code):
    name = {1: "Concrete", 2: "Metal", 3: "Glass", 4: "Lamp"}[code]
    result = bpy.data.materials.get(name)
    if result:
        return result
    result = bpy.data.materials.new(name)
    result.use_nodes = True
    pbr = result.node_tree.nodes.get("Principled BSDF")
    pbr.inputs["Base Color"].default_value = {
        1: (0.52, 0.55, 0.57, 1), 2: (0.15, 0.2, 0.23, 1),
        3: (0.45, 0.65, 0.7, 0.18), 4: (0.95, 0.88, 0.65, 1),
    }[code]
    pbr.inputs["Roughness"].default_value = 0.12 if code == GLASS else 0.75
    if code == GLASS:
        pbr.inputs["Alpha"].default_value = 0.18
        pbr.inputs["Transmission Weight"].default_value = 0.7
        result.surface_render_method = "DITHERED"
    if code == LAMP:
        pbr.inputs["Emission Color"].default_value = (1, 0.8, 0.5, 1)
        pbr.inputs["Emission Strength"].default_value = 8
    return result


def driver(obj, path, axis, expression, root):
    curve = obj.driver_add(path, axis)
    curve.driver.type = "SCRIPTED"
    for key in root.keys():
        if key not in expression.split() and key not in expression:
            continue
        if not isinstance(root[key], (int, float)):
            continue
        variable = curve.driver.variables.new()
        variable.name = key
        variable.targets[0].id = root
        variable.targets[0].data_path = f'["{key}"]'
    curve.driver.expression = expression


def box(name, centre, half, yaw, code, target, root, role="structure"):
    bpy.ops.mesh.primitive_cube_add(size=2)
    obj = bpy.context.object
    obj.name = name
    for old in list(obj.users_collection):
        old.objects.unlink(obj)
    target.objects.link(obj)
    obj.parent = root
    for axis in range(3):
        for path, values in (("location", centre), ("scale", half)):
            value = values[axis]
            if isinstance(value, str):
                driver(obj, path, axis, value, root)
            else:
                getattr(obj, path)[axis] = value
    obj.rotation_euler.z = yaw
    obj["material_id"] = code
    obj["role"] = role
    obj.data.materials.append(material(code))
    return obj


def boolean(obj, cutters, name):
    mod = obj.modifiers.new(name, "BOOLEAN")
    mod.operation = "DIFFERENCE"
    mod.solver = "EXACT"
    mod.operand_type = "COLLECTION"
    mod.collection = cutters


def collision(centre, half, yaw=0, code=CONCRETE):
    return {"centre": list(centre), "half": list(half), "yaw": yaw, "material": code}


def partition_wall(run, height, thickness, openings, point, yaw):
    """Partition by the very same rectangular cutters used by the booleans."""
    xs = sorted(set([-run / 2, run / 2] + [v for x, z, w, h in openings for v in (x-w/2, x+w/2)]))
    zs = sorted(set([0, height] + [v for x, z, w, h in openings for v in (max(0, z-h/2), min(height, z+h/2))]))
    solids = []
    for x0, x1 in zip(xs, xs[1:]):
        for z0, z1 in zip(zs, zs[1:]):
            x, z = (x0+x1)/2, (z0+z1)/2
            if x1-x0 < 1e-6 or z1-z0 < 1e-6:
                continue
            if any(abs(x-u) < w/2 and abs(z-v) < h/2 for u, v, w, h in openings):
                continue
            solids.append(collision(point(x, z), ((x1-x0)/2, thickness/2, (z1-z0)/2), yaw))
    return solids


def wall(name, run, reach, yaw, storeys, door, dims, target, root):
    height, thickness = storeys*dims["storey"], dims["wall"]
    tangent = Vector((math.cos(yaw), math.sin(yaw), 0))
    outward = Vector((tangent.y, -tangent.x, 0))
    def point(x, z, depth=0):
        return outward*(reach+depth) + tangent*x + Vector((0, 0, z))
    wall_obj = box(name, point(0, height/2), (run/2, thickness/2, height/2), yaw, CONCRETE, target, root)
    windows = collection(name + "_window_cutters")
    doors = collection(name + "_door_cutters")
    openings, window_rects = [], []
    count = max(1, int(run / dims["window_pitch"]))
    for floor in range(storeys):
        for i in range(count):
            x = ((i+0.5)/count-0.5)*run
            w, h = dims["window_width"], dims["window_height"]
            if w + 0.2 >= run:
                w = run - 0.3
            if floor == 0 and door and abs(x) < (dims["door_width"]+w)/2 + 0.1:
                continue
            z = floor*dims["storey"]+dims["sill"]+h/2
            rect = (x, z, w, h)
            openings.append(rect)
            window_rects.append(rect)
            box(f"{name}_window_{floor}_{i}", point(x, z), (w/2, thickness, h/2), yaw, CONCRETE, windows, root, "cutter")
            frame = dims["frame"]
            for side in [-1, 1]:
                box("Window jamb", point(x+side*(w-frame)/2, z), (frame/2, thickness/2+0.025, h/2), yaw, PLATE, target, root, "frame")
                box("Window sill", point(x, z+side*(h-frame)/2), ((w-2*frame)/2, thickness/2+0.025, frame/2), yaw, PLATE, target, root, "frame")
            box("Glazing", point(x, z), (w/2-frame, 0.008, h/2-frame), yaw, GLASS, target, root, "glass")
    if door:
        w, h = dims["door_width"], dims["door_height"]
        openings.append((0, h/2, w, h))
        box("Door cutter", point(0, h/2-0.05), (w/2, thickness, h/2+0.05), yaw, CONCRETE, doors, root, "cutter")
        boolean(wall_obj, doors, "Door opening (editable)")
    if window_rects:
        boolean(wall_obj, windows, "Window openings (editable)")
    bevel = wall_obj.modifiers.new("Edge bevel (LOD0)", "BEVEL")
    bevel.width, bevel.segments = dims["bevel"], 1
    bevel.affect = "EDGES"
    # Keep cutters editable in the .blend, but never include them in a bake.
    for group in (windows, doors):
        for obj in group.objects:
            obj.hide_render = True
            obj.hide_set(True)
    solids = partition_wall(run, height, thickness, openings, point, yaw)
    # Glazing is a physical pane in the opening, not an opaque wall behind it.
    for x, z, w, h in window_rects:
        solids.append(collision(point(x, z), (w/2, 0.008, h/2), yaw, GLASS))
    return solids, (wall_obj, point, window_rects, door)


def roof(kind, width, depth, height, dims, target, root):
    if kind == "flat":
        for side in [-1, 1]:
            box("Parapet", (0, side*(depth/2-0.1), height+0.55), (width/2, 0.1, 0.3), 0, PLATE, target, root, "detail")
            box("Parapet", (side*(width/2-0.1), 0, height+0.55), (0.1, depth/2-0.2, 0.3), 0, PLATE, target, root, "detail")
        return
    if kind == "gable":
        section = [(-depth/2-0.2, height+0.25), (0, height+depth*0.35), (depth/2+0.2, height+0.25)]
        axis, reach = 1, width/2+0.2
    else:
        section = [(-width/2*math.cos(math.pi*k/12), height+0.25+width*0.275*math.sin(math.pi*k/12)) for k in range(13)]
        axis, reach = 0, depth/2
    # A closed solid roof shell, with thickness and end caps, not one-sided quads.
    outer = section
    inner = [(x, z-dims["slab"]) for x, z in reversed(section)]
    loop = outer + inner
    verts = []
    for end in [-reach, reach]:
        verts.extend([(end, x, z) if axis == 1 else (x, end, z) for x, z in loop])
    n = len(loop)
    faces = [tuple(range(n-1, -1, -1)), tuple(range(n, 2*n))]
    faces += [(i, (i+1)%n, (i+1)%n+n, i+n) for i in range(n)]
    mesh = bpy.data.meshes.new("Roof shell")
    mesh.from_pydata(verts, [], faces)
    mesh.update()
    import bmesh
    bm = bmesh.new()
    bm.from_mesh(mesh)
    bmesh.ops.recalc_face_normals(bm, faces=list(bm.faces))
    bm.to_mesh(mesh)
    bm.free()
    obj = bpy.data.objects.new("Roof shell", mesh)
    target.objects.link(obj)
    obj.parent = root
    obj["material_id"], obj["role"] = PLATE, "structure"
    mesh.materials.append(material(PLATE))


def build(recipe, storeys, dims):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    name = f'{recipe["kind"]}_{storeys}'
    target = collection(name)
    root = bpy.data.objects.new(name + " Parameters", None)
    target.objects.link(root)
    for key, value in dims.items():
        root[key] = value
    root["height"] = storeys*dims["storey"]
    root["storeys"] = storeys
    root["recipe"] = recipe["kind"]
    root["regenerate"] = "Edit assets/config/buildings.json and rerun tools/bake_buildings.py; cutters/modifiers remain editable here."
    w, d, h, t = dims["width"], dims["depth"], root["height"], dims["wall"]
    solids, checks = [], []
    for z in [dims["slab"]/2, h+dims["slab"]/2]:
        box("Floor" if z < h else "Ceiling", (0, 0, z), ("width / 2", "depth / 2", "slab / 2"), 0, CONCRETE, target, root)
        solids.append(collision((0, 0, z), (w/2, d/2, dims["slab"]/2)))
    if recipe.get("sides"):
        n = recipe["sides"]
        reach = min(w, d)/2*math.cos(math.pi/n)-t/2
        run = 2*(reach+t/2)*math.tan(math.pi/n)
        walls = [(run, reach, k*2*math.pi/n, k == 0) for k in range(n)]
    else:
        walls = [(w, d/2-t/2, 0, True), (w, d/2-t/2, math.pi, False),
                 (d-2*t, w/2-t/2, math.pi/2, False), (d-2*t, w/2-t/2, -math.pi/2, False)]
    for i, (run, reach, yaw, door) in enumerate(walls):
        colliders, check = wall(f"Wall_{i}", run, reach, yaw, storeys, door, dims, target, root)
        solids.extend(colliders)
        checks.append(check)
    roof(recipe["roof"], w, d, h, dims, target, root)
    lamps = [[0, -d/2-0.2, dims["door_height"]+0.5], [0, 0, h-0.5]]
    for p in lamps:
        box("Lamp", p, (0.18, 0.18, 0.12), 0, LAMP, target, root, "detail")
    bpy.context.view_layer.update()
    return name, target, root, solids, lamps, checks
