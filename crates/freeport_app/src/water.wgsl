// The sea's surface: tenebris's water.fs.glsl, transcribed, on Bevy's own
// transmission. The vertex stage lifts the sheet along the radial with the
// sine swell tenebris's water.vs.glsl carries; the fragment stage bends
// the radial normal by the gradient of tenebris's fbm ripples, hands Bevy
// the water's THICKNESS along the view ray off the depth prepass so its
// attenuation is the depth of water actually looked through, and lays the
// sky's reflection on by fresnel and foam on the crests, as the GLSL does.
// Every coordinate is PLANET LOCAL, from a centre the material is handed,
// never the render frame's, which moves with the origin.

#import bevy_pbr::{
    mesh_functions,
    forward_io::{Vertex, VertexOutput, FragmentOutput},
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
    mesh_view_bindings::{view, globals},
    view_transformations::{position_world_to_clip, depth_ndc_to_view_z},
    prepass_utils::prepass_depth,
}
#import freeport::water::{fbm3_at, swell}

struct Water {
    // The planet's centre in the render frame; w the sea's radius.
    centre: vec4<f32>,
    // x: time scale, y: the RIPPLE COORDINATE's scale, cells a metre, and
    // the whole of it: tenebris's own 0.9 inside its `fbm3` is folded in
    // here so there is one number, and `water::RIPPLE` is that number on
    // the CPU, which works the cell out in f64. Two would be two places
    // to change it and a sea that pixelated again on the next tune.
    // z: wave steepness, w: swell amplitude.
    wave: vec4<f32>,
    // rgb: the deep water's colour under the surface; w: swell frequency.
    deep: vec4<f32>,
    // rgb: the sky at the horizon; w: how much of it the sheet reflects there.
    horizon: vec4<f32>,
    // rgb: the sky at the zenith; w: the slope the normal is clamped to.
    zenith: vec4<f32>,
    // rgb: foam; w: its intensity.
    foam: vec4<f32>,
    // Foam bands: crest low and high on the ripple height, slope low and high.
    band: vec4<f32>,
    // The sky at the horizon in rgb, and how much of it is in the way per
    // metre of view distance in w.
    fog: vec4<f32>,
    // The ground fog, as `terrain.wgsl`'s: x the metres it falls off
    // over, y how many times the plain haze it is at the sea, z the
    // radius that is measured from.
    haze: vec4<f32>,
    // The sun's direction in xyz; w the NIGHT FLOOR, what a share of the
    // light reaching the water's body is worth on the night side.
    sun: vec4<f32>,
    // Absorption per metre a channel, and in w the longest path of water
    // any of it is measured over.
    absorb: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> water: Water;

// How hard the ripples are worn out by their own per pixel FOOTPRINT,
// pale-blue-dot's `detail_fade`. What it replaces is a fade over 30 m to
// 160 m of DISTANCE, which is the same thing guessed rather than
// measured: at a grazing angle a ripple thirty metres away already
// covers a pixel, and a distance fade leaves it drawn, point sampled,
// as the sparkle fresnel and foam then make of it. Nought is the
// unfiltered original.
const DETAIL_FADE: f32 = 4.0;

// Where the terminator falls on the sun's elevation over the local
// horizon, as `distant.wgsl` measures the same line on the same body.
const DUSK_TO: f32 = -0.10;
const DUSK_FROM: f32 = 0.14;

// The sheet over the chunks, whose vertices ARE a mesh: the swell raises
// each along its own radial, which is tenebris's `water.vs.glsl`.
@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    var wp = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0));
    let q = wp.xyz - water.centre.xyz;
    let radial = normalize(q);
    // The SWELL keeps the imprecise q on purpose: its wavelengths are tens
    // of metres, so six centimetres of quantisation is a thousandth of a
    // wave and nothing an eye can find. It is the RIPPLES, at two thirds
    // of a metre, that the same six centimetres wrecks.
    wp = vec4<f32>(wp.xyz + radial * swell(q, water.wave, water.deep.w, globals.time), 1.0);
    out.world_position = wp;
    out.position = position_world_to_clip(wp.xyz);
    out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex.instance_index);
    // The ripple coordinate, as an exact cell and a fraction. The cell is
    // the CHUNK's, worked out in f64 on the CPU and the same at all three
    // corners of every triangle in it; the fraction carries this vertex's
    // own offset inside the chunk, which is a few metres and exact, and
    // interpolates across the triangle because it is affine in position.
#ifdef VERTEX_COLORS
    out.color = vertex.color;
#endif
#ifdef VERTEX_UVS_A
    let inside = vertex.position * water.wave.y;
#ifdef VERTEX_UVS_B
    out.uv = vertex.uv + inside.xy;
    out.uv_b = vec2<f32>(vertex.uv_b.x + inside.z, 0.0);
#else
    out.uv = vertex.uv + inside.xy;
#endif
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
    return out;
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    // The vertex COLOUR is not a colour on this surface: it carries the
    // ripple cell, which is a whole number over a million. Bevy's standard
    // material multiplies its base colour by the vertex colour, so handing
    // it straight through drew the sea at a million times white, which is
    // what the first render of this was: a blank sheet from the shore to
    // the horizon. It is a channel this shader owns, so it is masked out
    // before the standard material ever sees it.
    var plain = in;
#ifdef VERTEX_COLORS
    plain.color = vec4<f32>(1.0, 1.0, 1.0, 1.0);
#endif
    let q = in.world_position.xyz - water.centre.xyz;
    let radial = normalize(q);
    let t = globals.time * water.wave.x;
    // The ripple coordinate off the vertex: a cell the CPU worked out in
    // f64 and a fraction, never `q * scale` formed here out of a planet's
    // own radius held in one float.
    var cell = vec3<f32>(0.0);
    var frac = q * water.wave.y;
#ifdef VERTEX_COLORS
    cell = in.color.xyz;
#ifdef VERTEX_UVS_B
    frac = vec3<f32>(in.uv, in.uv_b.x);
#endif
#endif
    // The ripples are worn out by their own per pixel FOOTPRINT rather
    // than over a distance somebody picked. Once a ripple falls under a
    // pixel its point sampled height and gradient are NOISE, and fresnel
    // and foam turn that noise into white sparkle across the whole far
    // sea; what a fade is for is stopping at exactly that size, which is
    // a function of the angle and the field of view and not of the range.
    // Measured on the WORLD position, which is continuous across a chunk
    // seam where the ripple cell is a step.
    let foot = length(fwidth(in.world_position.xyz)) * water.wave.y;
    let detail = 1.0 / (1.0 + foot * DETAIL_FADE);
    let h = fbm3_at(cell, frac, t) * detail;
    let e = 0.08;
    let at = fbm3_at(cell, frac, t);
    var grad = vec3<f32>(
        fbm3_at(cell, frac + vec3<f32>(e, 0.0, 0.0), t) - at,
        fbm3_at(cell, frac + vec3<f32>(0.0, e, 0.0), t) - at,
        fbm3_at(cell, frac + vec3<f32>(0.0, 0.0, e), t) - at,
    ) * 12.5 * detail;
    grad = grad - radial * dot(grad, radial);
    let steep = length(grad);
    var bent = grad;
    if (steep > water.zenith.w) {
        bent = grad * (water.zenith.w / steep);
    }
    let n = normalize(radial - bent * water.wave.z);
    let away = length(in.world_position.xyz - view.world_position);

    // Which side of the terminator this piece of sea is on, and what the
    // light reaching the water's own BODY is worth there. A floor rather
    // than nought, which is this project's ambient lesson again, and it
    // is never applied to the REFLECTION: that carries the sky's own
    // level already, and dimmed twice the sea went black at the horizon,
    // where a mirror should be closest to the sky it mirrors.
    let sun = normalize(water.sun.xyz);
    let daylight = smoothstep(DUSK_TO, DUSK_FROM, dot(radial, sun));
    let lit = mix(water.sun.w, 1.0, daylight);
    let absorb = max(water.absorb.rgb, vec3<f32>(0.0));
    // The deep colour and the foam sit in the frame's own exposed range
    // beside the standard material's output; the sky colours are in
    // CANDELA and take the camera's exposure, which is what `fog` already
    // is and why the two can be mixed.
    let night = water.fog.rgb;

    var out: FragmentOutput;
    if (!is_front) {
        // Seen from under the sheet: the deep colour ATTENUATED by the
        // eye's OWN depth of water, so a dive darkens the way the seabed
        // under it already does. Left at the bare deep colour it was one
        // blue at half a metre and at eight, and a lit blue room under a
        // dark sky. The sky comes through where the view rises steeply
        // enough to leave through Snell's window.
        let eye_q = view.world_position - water.centre.xyz;
        let eye_deep = max(water.centre.w - length(eye_q), 0.0);
        let murk = water.deep.rgb * exp(-absorb * eye_deep) * lit;
        let up = clamp(dot(-radial, normalize(in.world_position.xyz - view.world_position)), 0.0, 1.0);
        let window = smoothstep(0.55, 0.75, up);
        let above = mix(night, water.horizon.rgb, daylight) * view.exposure * 0.6;
        out.color = vec4<f32>(mix(murk, above, window), 1.0);
        return out;
    }

    // How much water the view ray crosses before the ground behind, off
    // the depth prepass: the sheet opts out of that pass itself, so what
    // it reads is the sea floor rather than its own depth. CAPPED at the
    // longest path any of this is measured over, so a ray that never
    // meets a floor at all is the sheet's own colour rather than a
    // multiply by nothing.
#ifdef DEPTH_PREPASS
    let scene_z = depth_ndc_to_view_z(prepass_depth(in.position, 0u));
    let here_z = depth_ndc_to_view_z(in.position.z);
    let thickness = max(here_z - scene_z, 0.02);
#else
    let thickness = 2.0;
#endif
    let path = min(thickness, water.absorb.w);

    var pbr_input = pbr_input_from_standard_material(plain, is_front);
    pbr_input.N = n;
    // Bevy's own transmission attenuates the refracted ray over this, on
    // the colour `water::attenuation` DERIVED from the same absorption
    // the next line reads, so the two halves of one Beer's law cannot
    // drift: they are one number and one path.
    pbr_input.material.thickness = path;
    var color = apply_pbr_lighting(pbr_input);

    // What is left where the path runs out. Bevy's attenuation takes the
    // seabed to NOUGHT over a long path, which is a black sea; what deep
    // water actually is is its own body colour, so the share the water
    // absorbed comes back as that. Together the two are the transmitted
    // term pale-blue-dot writes as `mix(deep, scene, exp(-a * path))`,
    // and it is the term with more authority over this picture than
    // every shine knob in the shader put together: 31 levels of 255 on
    // its own shore frame, against 1 for the sun's glint.
    let through = exp(-absorb * path);
    let body = (color.rgb + water.deep.rgb * (1.0 - through)) * lit;

    // The sky, by fresnel: brighter at the horizon than the zenith, and
    // only as much of the horizon as the material allows.
    //
    // At NIGHT it is the sky the DOME is actually painting, which the
    // same march already hands this shader as the fog colour at the
    // horizon. An authored day gradient there is a lit blue sheet under
    // a black sky, brighter than the land beside it, and it needs no
    // second sky model and no authored night colour to avoid: the one
    // the atmosphere computed is already in this uniform.
    let V = pbr_input.V;
    let r = reflect(-V, n);
    let up = clamp(dot(r, radial), 0.0, 1.0);
    let day = mix(water.horizon.rgb, water.zenith.rgb, up);
    let sky = mix(night, day, daylight) * view.exposure;
    let fresnel = 0.02 + 0.98 * pow(1.0 - max(dot(V, n), 0.0), 5.0);
    let strength = fresnel * mix(water.horizon.w, 1.0, up);
    var shaded = mix(body, sky, strength);

    // Foam on the crests and on the steep, as the GLSL blends them, and
    // on the light the water's body gets rather than the sky's.
    let foam = max(
        smoothstep(water.band.x, water.band.y, h) * 0.55,
        smoothstep(water.band.z, water.band.w, steep) * 0.26,
    ) * water.foam.w;
    shaded = mix(shaded, water.foam.rgb * view.exposure * lit, foam);
    color = vec4<f32>(shaded, color.a);

    color = main_pass_post_lighting_processing(pbr_input, color);
    // The same air the ground fades into, so the sea meets the land in
    // one haze and the horizon is one line.
    let eye_up = length(view.world_position - water.centre.xyz) - water.haze.z;
    let here_up = length(q) - water.haze.z;
    let mid_up = max((eye_up + here_up) * 0.5, 0.0);
    let pooled = 1.0 + (water.haze.y - 1.0) * exp(-mid_up / max(water.haze.x, 1.0));
    let in_the_way = 1.0 - exp(-away * water.fog.w * pooled);
    out.color = vec4<f32>(
        mix(color.rgb, water.fog.rgb * view.exposure, in_the_way),
        color.a,
    );
    return out;
}
