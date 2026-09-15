#define_import_path freeport::frame

// Drawing at a precision the hardware does not have.
//
// A point on a planet is `direction * radius`, and at a thousand
// kilometres an f32 direction carries six hundredths of a micron of
// error, which is TWELVE CENTIMETRES on the ground: the ground quantises
// and it shifts again on every origin rebase. The way out is never to
// form the direction at all. Every vertex is an OFFSET from one anchor
// the CPU works out in f64, so the shader multiplies the radius by a
// SMALL number that is accurate rather than by a number near one that is
// not.
//
// `freeport_core::pos::unit_offset` is the reference, and
// `a_small_step_keeps_its_metres_at_a_thousand_kilometres` is the
// measurement: at a thousand kilometres the naive way is 6.5 cm out and
// this is eighteen MICROMETRES, which is three and a half thousand times
// better for four more instructions.

// `normalize(anchor + step) - anchor` for a UNIT anchor and a small step,
// in closed form so there is no cancellation anywhere in it:
// `anchor * (k - 1) + step * k` with `k = 1 / sqrt(1 + s)` and
// `s = 2 anchor.step + step.step`, and `k - 1` written as
// `-s / (root * (1 + root))` rather than as a difference of two ones.
fn unit_offset(anchor: vec3<f32>, step: vec3<f32>) -> vec3<f32> {
    let s = 2.0 * dot(anchor, step) + dot(step, step);
    let root = sqrt(max(1.0 + s, 0.0));
    if (root <= 0.0) {
        return -anchor;
    }
    let k = 1.0 / root;
    let g = -s / (root * (1.0 + root));
    return anchor * g + step * k;
}
