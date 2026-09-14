//! The cube sphere: six faces, a quadtree on each, and which leaves to draw.
//!
//! A planet's surface is addressed as six square faces of a cube inflated to
//! a sphere, each face a quadtree, each node a patch of ground. The mapping
//! is tangent warped (`tan(u * PI / 4)` rather than `u`), because a plain
//! cube sphere gives a corner cell a fifth of the area of a centre cell and
//! every LOD decision would be five times too eager on one and too lazy on
//! the other; warped, the spacing along a face is equal angle by
//! construction and the corner cell is within a half of the centre one
//! (`the_warp_evens_out_the_cells` measures both: 1.42 against 5.2). tenebris addresses its
//! planets as a Goldberg polyhedron, which is right for a hex grid you must
//! index into by tile forever; a quadtree is right for terrain that is a
//! continuous field sampled at whatever detail the eye is near enough to
//! see, which is what freeport's planets are.

use glam::DVec3;
use std::f64::consts::{FRAC_PI_4, PI};

/// A cube has six faces.
pub const FACES: u8 = 6;

/// One patch of one face: `x` and `y` run 0..2^level across it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Node {
    pub face: u8,
    pub level: u8,
    pub x: u32,
    pub y: u32,
}

/// A face's frame: its outward normal and the two axes u and v run along,
/// chosen so that `u x v = n` on every face.
fn axes(face: u8) -> (DVec3, DVec3, DVec3) {
    match face {
        0 => (DVec3::X, DVec3::Y, DVec3::Z),
        1 => (DVec3::NEG_X, DVec3::Z, DVec3::Y),
        2 => (DVec3::Y, DVec3::Z, DVec3::X),
        3 => (DVec3::NEG_Y, DVec3::X, DVec3::Z),
        4 => (DVec3::Z, DVec3::X, DVec3::Y),
        _ => (DVec3::NEG_Z, DVec3::Y, DVec3::X),
    }
}

/// The unit direction a face coordinate points at. `u` and `v` run -1..1.
pub fn face_uv_to_dir(face: u8, u: f64, v: f64) -> DVec3 {
    let (n, ua, va) = axes(face);
    let s = (u * FRAC_PI_4).tan();
    let t = (v * FRAC_PI_4).tan();
    (n + ua * s + va * t).normalize()
}

/// The face a direction lands on and where on it, the inverse of
/// `face_uv_to_dir`. A zero vector lands at the centre of face 0 rather
/// than anywhere a NaN could go.
pub fn dir_to_face_uv(d: DVec3) -> (u8, f64, f64) {
    let a = d.abs();
    let face = if a.x >= a.y && a.x >= a.z {
        if d.x >= 0.0 {
            0
        } else {
            1
        }
    } else if a.y >= a.z {
        if d.y >= 0.0 {
            2
        } else {
            3
        }
    } else if d.z >= 0.0 {
        4
    } else {
        5
    };
    let (n, ua, va) = axes(face);
    let depth = d.dot(n);
    if depth <= 0.0 {
        return (0, 0.0, 0.0);
    }
    let s = d.dot(ua) / depth;
    let t = d.dot(va) / depth;
    (face, s.atan() / FRAC_PI_4, t.atan() / FRAC_PI_4)
}

impl Node {
    /// The six roots.
    pub fn roots() -> [Node; 6] {
        std::array::from_fn(|f| Node {
            face: f as u8,
            level: 0,
            x: 0,
            y: 0,
        })
    }

    /// The width of this node in face units (a face is 2 across).
    pub fn size(&self) -> f64 {
        2.0 / (1u64 << self.level) as f64
    }

    /// The u, v of this node's lower corner.
    pub fn corner_uv(&self) -> (f64, f64) {
        let s = self.size();
        (-1.0 + self.x as f64 * s, -1.0 + self.y as f64 * s)
    }

    /// The direction through the middle of this patch.
    pub fn centre_dir(&self) -> DVec3 {
        let (u, v) = self.corner_uv();
        let h = self.size() * 0.5;
        face_uv_to_dir(self.face, u + h, v + h)
    }

    /// Roughly how far across the ground this patch is, in metres, on a
    /// planet of `radius`: a face unit is a quarter turn, warped to equal
    /// angle, so a node's width is that share of `PI / 2 * radius`.
    pub fn arc(&self, radius: f64) -> f64 {
        self.size() * FRAC_PI_4 * radius
    }

    /// The half angle of the smallest cone about `centre_dir` that holds the
    /// whole patch, from its four corners. What the LOD rule measures the
    /// eye against is the NEAREST point of a patch and not its middle: an eye
    /// ten metres over the corner of a patch a thousand kilometres wide is
    /// five hundred kilometres from its middle, and a rule that measured
    /// that would leave the ground under the eye at the coarsest level
    /// there is, which is exactly what the first cut of `select` did.
    pub fn angular_radius(&self) -> f64 {
        let c = self.centre_dir();
        let (u0, v0) = self.corner_uv();
        let s = self.size();
        let mut worst = 1.0f64;
        for (du, dv) in [(0.0, 0.0), (s, 0.0), (0.0, s), (s, s)] {
            worst = worst.min(c.dot(face_uv_to_dir(self.face, u0 + du, v0 + dv)));
        }
        worst.clamp(-1.0, 1.0).acos()
    }

    /// How far the eye is from the nearest point of this patch on a planet
    /// of `radius`, in metres, going by angle: the eye's bearing from the
    /// planet's centre against the patch's cone, then the chord.
    pub fn distance_to(&self, eye: DVec3, radius: f64) -> f64 {
        let e = eye.length();
        let away = if e > 0.0 {
            (eye / e).dot(self.centre_dir()).clamp(-1.0, 1.0).acos()
        } else {
            0.0
        };
        let theta = (away - self.angular_radius()).max(0.0);
        (e * e + radius * radius - 2.0 * e * radius * theta.cos())
            .max(0.0)
            .sqrt()
    }

    /// The four children, in x then y order.
    pub fn children(&self) -> [Node; 4] {
        let (f, l, x, y) = (self.face, self.level + 1, self.x * 2, self.y * 2);
        [
            Node {
                face: f,
                level: l,
                x,
                y,
            },
            Node {
                face: f,
                level: l,
                x: x + 1,
                y,
            },
            Node {
                face: f,
                level: l,
                x,
                y: y + 1,
            },
            Node {
                face: f,
                level: l,
                x: x + 1,
                y: y + 1,
            },
        ]
    }

    /// Whether a direction falls inside this patch.
    pub fn contains(&self, d: DVec3) -> bool {
        let (face, u, v) = dir_to_face_uv(d);
        if face != self.face {
            return false;
        }
        let (u0, v0) = self.corner_uv();
        let s = self.size();
        u >= u0 && u < u0 + s && v >= v0 && v < v0 + s
    }
}

/// The leaves to draw for an eye at `eye` (planet frame, metres) on a planet
/// of `radius`. A node splits while it is wider than `ratio` times the
/// distance from the eye to its nearest point and is not yet at `max_level`;
/// the leaves come back
/// sorted, so the same eye always gives the same list in the same order,
/// which is what lets a frame's chunk set be compared to the last one's.
pub fn select(eye: DVec3, radius: f64, max_level: u8, ratio: f64) -> Vec<Node> {
    let mut stack: Vec<Node> = Node::roots().to_vec();
    let mut leaves = Vec::new();
    while let Some(node) = stack.pop() {
        let dist = node.distance_to(eye, radius);
        if node.level < max_level && node.arc(radius) > ratio * dist {
            stack.extend_from_slice(&node.children());
        } else {
            leaves.push(node);
        }
    }
    leaves.sort();
    leaves
}

/// How many levels a planet needs for its leaves to be about `cell` metres
/// across at the surface: the level at which a node's arc first drops under
/// `cell`.
pub fn levels_for(radius: f64, cell: f64) -> u8 {
    let mut level = 0u8;
    while (Node {
        face: 0,
        level,
        x: 0,
        y: 0,
    })
    .arc(radius)
        > cell
        && level < 30
    {
        level += 1;
    }
    level
}

/// The full turn, for anyone measuring a planet's girth off a radius.
pub fn circumference(radius: f64) -> f64 {
    2.0 * PI * radius
}

#[cfg(test)]
mod tests {
    use super::*;

    fn linear_uv_to_dir(face: u8, u: f64, v: f64) -> DVec3 {
        let (n, ua, va) = axes(face);
        (n + ua * u + va * v).normalize()
    }

    fn cell_area_ratio(map: impl Fn(f64, f64) -> DVec3, n: usize) -> f64 {
        let (mut lo, mut hi) = (f64::MAX, 0.0f64);
        let s = 2.0 / n as f64;
        for j in 0..n {
            for i in 0..n {
                let (u, v) = (-1.0 + i as f64 * s, -1.0 + j as f64 * s);
                let a = map(u, v);
                let b = map(u + s, v);
                let c = map(u, v + s);
                let area = (b - a).cross(c - a).length();
                lo = lo.min(area);
                hi = hi.max(area);
            }
        }
        hi / lo
    }

    #[test]
    fn uv_round_trips_through_a_direction() {
        for face in 0..FACES {
            for j in 0..9 {
                for i in 0..9 {
                    let (u, v) = (-0.96 + 0.24 * i as f64, -0.96 + 0.24 * j as f64);
                    let d = face_uv_to_dir(face, u, v);
                    assert!((d.length() - 1.0).abs() < 1e-12);
                    let (f2, u2, v2) = dir_to_face_uv(d);
                    assert_eq!(f2, face, "face {face} at {u},{v} came back as {f2}");
                    assert!((u2 - u).abs() < 1e-9 && (v2 - v).abs() < 1e-9);
                }
            }
        }
    }

    #[test]
    fn the_warp_evens_out_the_cells() {
        let warped = cell_area_ratio(|u, v| face_uv_to_dir(0, u, v), 16);
        let linear = cell_area_ratio(|u, v| linear_uv_to_dir(0, u, v), 16);
        assert!(
            warped < 1.5,
            "warped corner to centre area ratio is {warped}"
        );
        assert!(
            linear > 4.0,
            "a plain cube sphere is {linear}, the warp is {warped}"
        );
    }

    #[test]
    fn far_away_the_whole_planet_is_six_leaves() {
        let leaves = select(DVec3::new(0.0, 0.0, 5.0e7), 1.0e6, 12, 1.0);
        assert_eq!(leaves.len(), 6);
        assert!(leaves.iter().all(|n| n.level == 0));
    }

    #[test]
    fn the_leaves_partition_the_sphere() {
        let radius = 6.0e5;
        let eye = DVec3::new(radius + 3.0, 0.0, 0.0);
        let leaves = select(eye, radius, 14, 1.5);
        for face in 0..FACES {
            let covered: f64 = leaves
                .iter()
                .filter(|n| n.face == face)
                .map(|n| n.size() * n.size())
                .sum();
            assert!((covered - 4.0).abs() < 1e-9, "face {face} covers {covered}");
        }
        let mut sorted = leaves.clone();
        sorted.dedup();
        assert_eq!(sorted.len(), leaves.len(), "a leaf is listed twice");
    }

    #[test]
    fn detail_is_where_the_eye_is() {
        let radius = 1.0e6;
        let eye = DVec3::new(radius + 10.0, 0.0, 0.0);
        let leaves = select(eye, radius, 18, 1.5);
        let under = leaves
            .iter()
            .find(|n| n.contains(DVec3::X))
            .expect("a leaf under the eye");
        let behind = leaves
            .iter()
            .find(|n| n.contains(DVec3::NEG_X))
            .expect("a leaf on the far side");
        // Ten metres up at a ratio of one and a half, the patch under the eye
        // is the first one narrower than fifteen metres, and its parent was
        // not.
        assert!(
            under.arc(radius) <= 15.0,
            "under the eye: {under:?}, {} m",
            under.arc(radius)
        );
        let parent = Node {
            face: 0,
            level: under.level - 1,
            x: under.x / 2,
            y: under.y / 2,
        };
        assert!(parent.arc(radius) > 15.0);
        assert!(behind.level <= 2, "far side: {behind:?}");
        assert!(leaves.len() < 1000, "{} leaves", leaves.len());
    }

    #[test]
    fn levels_are_counted_off_the_cell_size() {
        let level = levels_for(1.0e6, 32.0);
        let node = Node {
            face: 0,
            level,
            x: 0,
            y: 0,
        };
        assert!(node.arc(1.0e6) <= 32.0);
        let coarser = Node {
            face: 0,
            level: level - 1,
            x: 0,
            y: 0,
        };
        assert!(coarser.arc(1.0e6) > 32.0);
        assert!((circumference(1.0e6) - 6.283185307e6).abs() < 1.0);
    }

    #[test]
    fn a_patch_is_as_far_as_its_nearest_point() {
        let radius = 1.0e6;
        let root = Node {
            face: 0,
            level: 0,
            x: 0,
            y: 0,
        };
        assert!(
            (root.angular_radius() - 0.9553).abs() < 1e-3,
            "{}",
            root.angular_radius()
        );
        let over = DVec3::new(radius + 10.0, 0.0, 0.0);
        assert!((root.distance_to(over, radius) - 10.0).abs() < 1e-6);
        let behind = Node {
            face: 1,
            level: 0,
            x: 0,
            y: 0,
        };
        assert!(behind.distance_to(over, radius) > radius);
        let leaf = Node {
            face: 0,
            level: 10,
            x: 512,
            y: 512,
        };
        assert!((leaf.distance_to(over, radius) - 10.0).abs() < 1e-6);
    }

    #[test]
    fn a_zero_direction_lands_somewhere_finite() {
        let (f, u, v) = dir_to_face_uv(DVec3::ZERO);
        assert_eq!((f, u, v), (0, 0.0, 0.0));
    }
}
