"""Run: blender --background --python tools/bake_buildings.py -- [--only house_1]

JSON recipes are the parametric source. Each .blend retains its wall solids,
editable window/door cutters, exact booleans, and bevel modifiers. The bake
evaluates that stack into static GLB and a compact JSON mesh/collision bundle
for the existing town batching renderer. Blender is never needed at runtime.
"""
import argparse
import hashlib
import json
from pathlib import Path
import sys

import bpy
from mathutils import Vector
from mathutils.bvhtree import BVHTree

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools"))
from building_geometry import build


def validate(checks, dims):
    """Ray tests through evaluated boolean holes, and positive solid volumes."""
    import bmesh
    deps = bpy.context.evaluated_depsgraph_get()
    for obj, point, windows, has_door in checks:
        evaluated = obj.evaluated_get(deps)
        mesh = evaluated.to_mesh()
        bm = bmesh.new()
        bm.from_mesh(mesh)
        assert all(e.is_manifold for e in bm.edges), f"Nonmanifold boolean on {obj.name}"
        assert bm.calc_volume(signed=True) > 0, f"Inward winding on {obj.name}"
        bm.free()
        vertices = [evaluated.matrix_world @ v.co for v in mesh.vertices]
        tree = BVHTree.FromPolygons(vertices, [list(p.vertices) for p in mesh.polygons])
        def hit(x, z):
            a, b = point(x, z, 2), point(x, z, -2)
            return tree.ray_cast(a, (b-a).normalized(), 4)[0]
        for x, z, _, h in windows:
            assert hit(x, z) is None, f"Window is filled on {obj.name}"
            assert hit(x, z-h/2-0.15) is not None, f"Missing wall below window on {obj.name}"
        if has_door:
            assert hit(0, dims["door_height"]/2) is None, "Door is blocked"
        evaluated.to_mesh_clear()


def bake(target, lod):
    for obj in target.objects:
        for mod in obj.modifiers:
            mod.show_viewport = not (mod.type == "BEVEL" and lod > 0 or mod.name.startswith("Window") and lod == 2)
            mod.show_render = mod.show_viewport
    bpy.context.view_layer.update()
    deps = bpy.context.evaluated_depsgraph_get()
    result = {"positions": [], "normals": [], "indices": [], "materials": []}
    cache, copies = {}, []
    for obj in list(target.objects):
        if obj.type != "MESH" or obj.get("role") == "cutter":
            continue
        if lod > 0 and obj.get("role") == "detail":
            continue
        if lod == 2 and obj.get("role") in ("frame", "glass"):
            continue
        evaluated = obj.evaluated_get(deps)
        mesh = bpy.data.meshes.new_from_object(evaluated, depsgraph=deps)
        # Exact booleans and bevels can leave collinear corners. Triangulate
        # the evaluated copy and drop zero-area faces before either export.
        import bmesh
        bm = bmesh.new()
        bm.from_mesh(mesh)
        bmesh.ops.triangulate(bm, faces=list(bm.faces))
        zero = [f for f in bm.faces if f.calc_area() < 1e-12]
        if zero:
            bmesh.ops.delete(bm, geom=zero, context="FACES_ONLY")
        bm.to_mesh(mesh)
        bm.free()
        mesh.calc_loop_triangles()
        transform = evaluated.matrix_world
        normal_transform = transform.to_3x3().inverted().transposed()
        code = obj["material_id"]
        for triangle in mesh.loop_triangles:
            normal = tuple(round(v, 6) for v in (normal_transform @ triangle.normal).normalized())
            for vertex in triangle.vertices:
                p = tuple(round(v, 6) for v in transform @ mesh.vertices[vertex].co)
                key = (p, normal)
                if key not in cache:
                    cache[key] = len(result["positions"])
                    result["positions"].append(p)
                    result["normals"].append(normal)
                result["indices"].append(cache[key])
            result["materials"].append(code)
        copy = bpy.data.objects.new(obj.name + " baked", mesh)
        bpy.context.scene.collection.objects.link(copy)
        copy.matrix_world = transform
        copies.append(copy)
    return result, copies


def export_glb(path, objects):
    bpy.ops.object.select_all(action="DESELECT")
    for obj in objects:
        obj.select_set(True)
    bpy.ops.export_scene.gltf(filepath=str(path), export_format="GLB", use_selection=True,
                             export_animations=False, export_apply=True, export_yup=True)
    for obj in objects:
        mesh = obj.data
        bpy.data.objects.remove(obj, do_unlink=True)
        bpy.data.meshes.remove(mesh)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--only")
    parser.add_argument("--config", type=Path, default=ROOT / "assets/config/buildings.json")
    parser.add_argument("--output", type=Path, default=ROOT / "assets/models/buildings")
    args = parser.parse_args(sys.argv[sys.argv.index("--")+1:] if "--" in sys.argv else [])
    source = args.config.read_bytes()
    config = json.loads(source)
    assert config["schema"] == 1
    dims = config["dimensions"]
    assert dims["width"] > dims["door_width"] + 2*dims["wall"]
    assert dims["depth"] > 2*dims["wall"] > 0
    assert dims["sill"] + dims["window_height"] < dims["storey"]
    args.output.mkdir(parents=True, exist_ok=True)
    manifest = {"schema": 1, "coordinates": "x east, y north, z up; metres", "source_sha256": hashlib.sha256(source).hexdigest(), "models": []}
    for recipe in config["recipes"]:
        for storeys in recipe["storeys"]:
            key = f'{recipe["kind"]}_{storeys}'
            manifest["models"].append({"kind": recipe["kind"], "storeys": storeys, "file": key + ".json"})
            if args.only and key != args.only:
                continue
            name, target, root, solids, lamps, checks = build(recipe, storeys, dims)
            validate(checks, dims)
            # Save the source BEFORE evaluating or changing modifiers for LOD.
            bpy.ops.wm.save_as_mainfile(filepath=str(args.output / (name + ".blend")), compress=True)
            data = {"schema": 1, "kind": recipe["kind"], "storeys": storeys,
                    "width": dims["width"], "depth": dims["depth"], "solids": solids, "lamps": lamps, "lods": []}
            for lod in range(3):
                mesh, objects = bake(target, lod)
                data["lods"].append(mesh)
                export_glb(args.output / f"{name}_lod{lod}.glb", objects)
            (args.output / (name + ".json")).write_text(json.dumps(data, separators=(",", ":")))
            print(f"BAKED {name}: {[len(m['indices'])//3 for m in data['lods']]} triangles; boolean/winding tests passed", flush=True)
    if not args.only:
        (args.output / "manifest.json").write_text(json.dumps(manifest, indent=2)+"\n")


if __name__ == "__main__":
    main()
