//! Rotatable 3D view of the rig with a tag on every member.
//!
//! Step: screen work (Step 2 D2-1)
//! Theory: docs/rig-model.md "Rig tree and tags".
//! Inputs: an evaluated (or hung) pose, the rig, its tree, a camera.
//! Outputs: 2D geometry in screen orientation (y down), feet, with member ids.
//! Must not depend on: UI, dioxus, store.

use indexmap::IndexMap;

use crate::checks::Status;
use crate::rig::Rig;
use crate::rig::eval::{EvalRig, ItemRef};
use crate::rig::id::MemberId;
use crate::rig::solve::MemberForce;
use crate::rig::solve::rate::Rated;
use crate::rig::tree::{MemberShape, RigTree};

use super::{
    Item, Rect, Role, Transform, bounds_of, draw_bodies_with, draw_nodes_with, farthest_pair,
    fit_rect,
};

/// Orbit camera. Yaw turns about world z; pitch raises the eye above the
/// horizon. Yaw 0, pitch 0 looks along +y with +x to the right.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub yaw_deg: f64,
    pub pitch_deg: f64,
}

impl Camera {
    pub const ISO: Self = Self {
        yaw_deg: -30.0,
        pitch_deg: 25.0,
    };
    /// x right, z up (the `ViewKind::Side` plane).
    pub const SIDE: Self = Self {
        yaw_deg: 0.0,
        pitch_deg: 0.0,
    };
    /// y right, z up (the `ViewKind::End` plane).
    pub const END: Self = Self {
        yaw_deg: 90.0,
        pitch_deg: 0.0,
    };
    /// x right, y up, looking down (the `ViewKind::Plan` plane).
    pub const PLAN: Self = Self {
        yaw_deg: 0.0,
        pitch_deg: 90.0,
    };

    /// Turn by a drag. Pitch stays within ±89° so the view never flips.
    pub fn orbit(self, d_yaw_deg: f64, d_pitch_deg: f64) -> Self {
        Self {
            yaw_deg: (self.yaw_deg + d_yaw_deg).rem_euclid(360.0),
            pitch_deg: (self.pitch_deg + d_pitch_deg).clamp(-89.0, 89.0),
        }
    }

    /// World point → (screen point with y down, depth). Larger depth is
    /// nearer the eye.
    pub fn project(&self, p: [f64; 3]) -> ([f64; 2], f64) {
        let (sy, cy) = self.yaw_deg.to_radians().sin_cos();
        let (sp, cp) = self.pitch_deg.to_radians().sin_cos();
        let right = [cy, sy, 0.0];
        let up = [-sy * sp, cy * sp, cp];
        let forward = [-sy * cp, cy * cp, -sp];
        let dot = |a: [f64; 3]| a[0] * p[0] + a[1] * p[1] + a[2] * p[2];
        ([dot(right), -dot(up)], -dot(forward))
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self::ISO
    }
}

/// Which geometry the view is drawn at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoseSource {
    /// Hung under gravity by a converged solve.
    Hung,
    /// The solver's last iterate; it did not converge.
    Unconverged,
    /// Nominal descent; the solve failed or did not run.
    Descent,
}

impl PoseSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hung => "hung pose",
            Self::Unconverged => "solver did not converge — last iterate",
            Self::Descent => "nominal pose (not solved)",
        }
    }
}

/// One member in the view.
#[derive(Debug, Clone, PartialEq)]
pub struct OrbitMember {
    pub id: MemberId,
    pub tag: String,
    pub pts: Vec<[f64; 2]>,
    /// Nearest depth of the polyline.
    pub depth: f64,
    pub role: Role,
    pub is_link: bool,
    pub tension_lbs: Option<f64>,
    /// Midpoint of the longest projected chord.
    pub label_anchor: [f64; 2],
    /// Unit normal to that chord, for nudging the tag off the line.
    pub label_normal: [f64; 2],
}

/// A tag pill placed on the canvas, in canvas units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LabelBox {
    pub id: MemberId,
    pub center: [f64; 2],
    pub anchor: [f64; 2],
}

/// Renderer-agnostic 3D view: bodies and nodes as `Item`s, members with ids.
#[derive(Debug, Clone, PartialEq)]
pub struct OrbitScene {
    pub camera: Camera,
    pub bounds: Rect,
    /// Load box edges, bars, hook; far to near.
    pub bodies: Vec<Item>,
    pub nodes: Vec<Item>,
    /// Far to near.
    pub members: Vec<OrbitMember>,
}

impl OrbitScene {
    /// Project `eval` through `camera`. With `forces` (a solve at this pose),
    /// slack follows the solve; members with an OVER check on `rated` are
    /// `Overloaded`.
    pub fn project(
        eval: &EvalRig,
        rig: &Rig,
        tree: &RigTree,
        camera: Camera,
        forces: Option<&IndexMap<MemberId, MemberForce>>,
        rated: &[Rated],
    ) -> Self {
        let proj = |p: [f64; 3]| camera.project(p);
        let mut bodies = Vec::new();
        draw_bodies_with(eval, rig, &proj, principal_axis, None, false, &mut bodies);
        sort_far_to_near(&mut bodies);
        let mut nodes = Vec::new();
        draw_nodes_with(eval, rig, &proj, &mut nodes);
        sort_far_to_near(&mut nodes);

        let mut members = Vec::new();
        for (id, m) in &eval.members {
            if m.points.len() < 2 {
                continue;
            }
            let projected: Vec<([f64; 2], f64)> = m.points.iter().map(|p| proj(*p)).collect();
            let pts: Vec<[f64; 2]> = projected.iter().map(|(p, _)| *p).collect();
            let depth = projected
                .iter()
                .map(|(_, d)| *d)
                .fold(f64::NEG_INFINITY, f64::max);
            let force = forces.and_then(|f| f.get(id));
            let over = rated.iter().any(|r| {
                matches!(r.at, ItemRef::Member { id: m, .. } if m == *id)
                    && r.check.status == Status::Over
            });
            let is_link = force.is_some_and(|f| f.is_link)
                || tree.get(*id).is_some_and(|t| t.shape == MemberShape::Link);
            let short = m.short_ft.iter().any(|s| *s > 1e-9);
            let slack = match force {
                Some(f) => !f.taut && !f.is_link,
                None => m.slack_ft.iter().any(|s| *s > 1e-6),
            };
            let role = if over {
                Role::Overloaded
            } else if short {
                Role::ShortMember
            } else if slack {
                Role::SlackMember
            } else {
                Role::Member
            };
            let (label_anchor, label_normal) = anchor_of(&pts);
            members.push(OrbitMember {
                id: *id,
                tag: tree.tag_of(*id).unwrap_or("?").to_owned(),
                pts,
                depth,
                role,
                is_link,
                tension_lbs: force.filter(|f| !f.is_link).map(|f| f.tension_lbs),
                label_anchor,
                label_normal,
            });
        }
        members.sort_by(|a, b| a.depth.total_cmp(&b.depth));

        let mut all: Vec<Item> = bodies.clone();
        all.extend(nodes.iter().cloned());
        all.extend(members.iter().map(|m| Item::Polyline {
            pts: m.pts.clone(),
            role: m.role,
            depth: m.depth,
            label: None,
        }));
        Self {
            camera,
            bounds: bounds_of(&all),
            bodies,
            nodes,
            members,
        }
    }

    pub fn fit(&self, w: f64, h: f64, margin: f64) -> Transform {
        fit_rect(&self.bounds, w, h, margin)
    }

    pub fn has_nan(&self) -> bool {
        let finite = |p: &[f64; 2]| p[0].is_finite() && p[1].is_finite();
        !finite(&self.bounds.min)
            || !finite(&self.bounds.max)
            || self
                .members
                .iter()
                .any(|m| !m.pts.iter().all(finite) || !finite(&m.label_anchor))
    }

    /// Place a `pill_w × pill_h` tag for every non-link member, in tag order.
    /// Each pill starts on its anchor and steps along the chord normal, then
    /// along the chord, until it clears every pill already placed.
    pub fn layout_labels(&self, t: &Transform, pill_w: f64, pill_h: f64) -> Vec<LabelBox> {
        const GAP: f64 = 2.0;
        const STEPS: i32 = 24;
        let mut order: Vec<&OrbitMember> = self.members.iter().filter(|m| !m.is_link).collect();
        order.sort_by_key(|m| tag_number(&m.tag));

        let overlaps = |a: [f64; 2], b: [f64; 2]| {
            (a[0] - b[0]).abs() < pill_w + GAP && (a[1] - b[1]).abs() < pill_h + GAP
        };
        let mut placed: Vec<LabelBox> = Vec::with_capacity(order.len());
        for m in order {
            let anchor = t.apply(m.label_anchor);
            let n = m.label_normal;
            let along = [-n[1], n[0]];
            let step = pill_h * 0.75;
            let mut chosen = None;
            'search: for k in 0..=STEPS {
                for dir in [n, along] {
                    for sign in [1.0, -1.0] {
                        let d = f64::from(k) * step * sign;
                        let c = [anchor[0] + dir[0] * d, anchor[1] + dir[1] * d];
                        if placed.iter().all(|p| !overlaps(p.center, c)) {
                            chosen = Some(c);
                            break 'search;
                        }
                    }
                }
            }
            placed.push(LabelBox {
                id: m.id,
                center: chosen.unwrap_or(anchor),
                anchor,
            });
        }
        placed
    }
}

/// A bar's extent along the principal axis of its nodes, through their
/// centroid. Lugs set off the bar line (a 4-leg top bar) don't skew it the way
/// the farthest pair of nodes can.
fn principal_axis(pts: &mut [[f64; 3]]) -> ([f64; 3], [f64; 3]) {
    let (a, b) = farthest_pair(pts);
    let n = pts.len() as f64;
    let mut c = [0.0; 3];
    for p in pts.iter() {
        for k in 0..3 {
            c[k] += p[k] / n;
        }
    }
    let mut cov = [[0.0; 3]; 3];
    for p in pts.iter() {
        let d = [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
        for i in 0..3 {
            for j in 0..3 {
                cov[i][j] += d[i] * d[j];
            }
        }
    }
    let unit = |v: [f64; 3]| {
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        (len > 1e-12).then(|| [v[0] / len, v[1] / len, v[2] / len])
    };
    let Some(mut axis) = unit([b[0] - a[0], b[1] - a[1], b[2] - a[2]]) else {
        return (a, b);
    };
    for _ in 0..64 {
        let next =
            [0, 1, 2].map(|i| cov[i][0] * axis[0] + cov[i][1] * axis[1] + cov[i][2] * axis[2]);
        match unit(next) {
            Some(v) => axis = v,
            None => break,
        }
    }
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for p in pts.iter() {
        let t = (p[0] - c[0]) * axis[0] + (p[1] - c[1]) * axis[1] + (p[2] - c[2]) * axis[2];
        lo = lo.min(t);
        hi = hi.max(t);
    }
    let at = |t: f64| [c[0] + axis[0] * t, c[1] + axis[1] * t, c[2] + axis[2] * t];
    (at(lo), at(hi))
}

fn sort_far_to_near(items: &mut [Item]) {
    let depth = |i: &Item| match i {
        Item::Polyline { depth, .. } | Item::Marker { depth, .. } => *depth,
        _ => f64::INFINITY,
    };
    items.sort_by(|a, b| depth(a).total_cmp(&depth(b)));
}

fn anchor_of(pts: &[[f64; 2]]) -> ([f64; 2], [f64; 2]) {
    let mut best = (0usize, -1.0_f64);
    for (i, w) in pts.windows(2).enumerate() {
        let len = (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
        if len > best.1 {
            best = (i, len);
        }
    }
    let (a, b) = (pts[best.0], pts[best.0 + 1]);
    let mid = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
    if best.1 < 1e-9 {
        return (mid, [1.0, 0.0]);
    }
    let normal = [-(b[1] - a[1]) / best.1, (b[0] - a[0]) / best.1];
    (mid, normal)
}

fn tag_number(tag: &str) -> usize {
    tag.trim_start_matches('M').parse().unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rig::duplo10;
    use crate::rig::solve::{rate, solve};
    use crate::rig::views::{ViewKind, project_pt};

    #[test]
    fn presets_match_the_flat_views() {
        let rig = duplo10();
        let eval = EvalRig::evaluate(&rig).unwrap();
        for (camera, kind) in [
            (Camera::SIDE, ViewKind::Side),
            (Camera::END, ViewKind::End),
            (Camera::PLAN, ViewKind::Plan),
        ] {
            for p in eval.nodes.values() {
                let (got, _) = camera.project(*p);
                let (want, _) = project_pt(*p, kind);
                assert!(
                    (got[0] - want[0]).abs() < 1e-9,
                    "{kind:?} x {got:?} {want:?}"
                );
                assert!(
                    (got[1] + want[1]).abs() < 1e-9,
                    "{kind:?} y {got:?} {want:?}"
                );
            }
        }
    }

    #[test]
    fn depth_grows_toward_the_eye() {
        // Side view looks along +y, so the eye is at -y.
        let (_, near) = Camera::SIDE.project([0.0, -1.0, 0.0]);
        let (_, far) = Camera::SIDE.project([0.0, 1.0, 0.0]);
        assert!(near > far);
        // Plan looks down, so higher is nearer.
        let (_, high) = Camera::PLAN.project([0.0, 0.0, 5.0]);
        let (_, low) = Camera::PLAN.project([0.0, 0.0, 0.0]);
        assert!(high > low);
    }

    #[test]
    fn orbit_clamps_pitch() {
        let c = Camera::ISO.orbit(400.0, 200.0);
        assert!((c.pitch_deg - 89.0).abs() < 1e-12);
        assert!((0.0..360.0).contains(&c.yaw_deg));
    }

    #[test]
    fn duplo10_scene_is_finite_and_tags_every_member() {
        let rig = duplo10();
        let tree = RigTree::build(&rig);
        let solved = solve(&rig).unwrap();
        let rated = rate::rate(&rig, &solved).unwrap();
        for camera in [Camera::ISO, Camera::SIDE, Camera::END, Camera::PLAN] {
            let scene = OrbitScene::project(
                &solved.eval,
                &rig,
                &tree,
                camera,
                Some(&solved.members),
                &rated,
            );
            assert!(!scene.has_nan(), "{camera:?}");
            assert_eq!(scene.members.len(), 20);
            assert!(scene.members.iter().all(|m| m.tag.starts_with('M')));
            assert!(scene.members.iter().any(|m| m.role == Role::Overloaded));
            assert!(scene.bodies.iter().any(|i| matches!(
                i,
                Item::Polyline {
                    role: Role::Bar,
                    ..
                }
            )));
            assert!(scene.bodies.iter().any(|i| matches!(
                i,
                Item::Polyline {
                    role: Role::Load,
                    ..
                }
            )));
            for w in scene.members.windows(2) {
                assert!(w[0].depth <= w[1].depth);
            }
        }
    }

    #[test]
    fn duplo10_top_bar_runs_along_x_in_plan() {
        let rig = duplo10();
        let tree = RigTree::build(&rig);
        let eval = EvalRig::evaluate(&rig).unwrap();
        let scene = OrbitScene::project(&eval, &rig, &tree, Camera::PLAN, None, &[]);
        let top = scene
            .bodies
            .iter()
            .find_map(|i| match i {
                Item::Polyline {
                    pts,
                    role: Role::Bar,
                    label: Some(l),
                    ..
                } if l == "Bar top" => Some(pts.clone()),
                _ => None,
            })
            .expect("Bar top drawn");
        assert!((top[0][1] - top[1][1]).abs() < 1e-6, "{top:?}");
        assert!(
            ((top[0][0] - top[1][0]).abs() - 34.0).abs() < 1e-6,
            "{top:?}"
        );
    }

    #[test]
    fn duplo10_labels_do_not_overlap() {
        let rig = duplo10();
        let tree = RigTree::build(&rig);
        let eval = EvalRig::evaluate(&rig).unwrap();
        let (pill_w, pill_h) = (34.0, 18.0);
        for camera in [Camera::ISO, Camera::SIDE, Camera::END, Camera::PLAN] {
            let scene = OrbitScene::project(&eval, &rig, &tree, camera, None, &[]);
            let t = scene.fit(800.0, 600.0, 40.0);
            let boxes = scene.layout_labels(&t, pill_w, pill_h);
            assert_eq!(boxes.len(), 20, "{camera:?}");
            for (i, a) in boxes.iter().enumerate() {
                for b in &boxes[i + 1..] {
                    let clear = (a.center[0] - b.center[0]).abs() >= pill_w
                        || (a.center[1] - b.center[1]).abs() >= pill_h;
                    assert!(clear, "{camera:?}: {a:?} overlaps {b:?}");
                }
            }
        }
    }
}
