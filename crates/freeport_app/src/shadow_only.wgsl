// The shadow a tile past its nearest bake casts: its solid block. The
// sun's cascades draw it through Bevy's own prepass shaders, and the
// camera's main pass draws it through THIS one, which puts every corner
// of every triangle on one point, so the rasteriser makes nothing of it.
// The material has no prepass (`cull::ShadowOnly`), so the camera's
// depth prepass never sees it either.
//
// A layer the camera cannot see does not do this: Bevy 0.18's
// `queue_shadows` skips a mesh whose layers miss the CAMERA's, so a
// block on a sun-only layer cast nothing at all.

struct Vertex {
    @location(0) position: vec3<f32>,
};

@vertex
fn vertex(v: Vertex) -> @builtin(position) vec4<f32> {
    return vec4<f32>(v.position * 0.0, 1.0);
}

@fragment
fn fragment() -> @location(0) vec4<f32> {
    return vec4<f32>(0.0);
}
