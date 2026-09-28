//! Side / end / plan projection data for diagrams.
//!
//! Step: 2
//! Theory: roadmap Step 2 — view projections.
//! Inputs: evaluated 3D graph.
//! Outputs: 2D projection polylines and annotations, in feet.
//! Must not depend on: UI, dioxus, store, layers.

mod annotate;

use serde::{Deserialize, Serialize};

use super::Rig;
use super::body::BodyKind;
use super::eval::EvalRig;
use super::id::MemberId;
use super::node::NodeKind;

/// Which plane the scene is projected onto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ViewKind {
    /// x–z
    Side,
    /// y–z
    End,
    /// x–y
    Plan,
}

/// Axis-aligned bounds in world units (ft).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub min: [f64; 2],
    pub max: [f64; 2],
}

impl Rect {
    pub fn empty() -> Self {
        Self {
            min: [0.0, 0.0],
            max: [0.0, 0.0],
        }
    }

    pub fn width(&self) -> f64 {
        self.max[0] - self.min[0]
    }

    pub fn height(&self) -> f64 {
        self.max[1] - self.min[1]
    }

    pub fn contains(&self, p: [f64; 2]) -> bool {
        p[0] + 1e-12 >= self.min[0]
            && p[0] <= self.max[0] + 1e-12
            && p[1] + 1e-12 >= self.min[1]
            && p[1] <= self.max[1] + 1e-12
    }

    fn include(&mut self, p: [f64; 2]) {
        self.min[0] = self.min[0].min(p[0]);
        self.min[1] = self.min[1].min(p[1]);
        self.max[0] = self.max[0].max(p[0]);
        self.max[1] = self.max[1].max(p[1]);
    }
}

/// Shared world → canvas mapping. Scale is canvas-units per foot.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub scale: f64,
    pub offset: [f64; 2],
}

impl Transform {
    pub fn apply(&self, p: [f64; 2]) -> [f64; 2] {
        [
            (p[0] * self.scale) + self.offset[0],
            (p[1] * self.scale) + self.offset[1],
        ]
    }
}

/// Semantic role. The renderer picks color / dash; the lib does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Load,
    Bar,
    Hook,
    Member,
    SlackMember,
    ShortMember,
    Lug,
    Bow,
    Edge,
    Cg,
    Dimension,
    Warning,
    /// A member whose rating check is OVER (Step 3).
    Overloaded,
}

/// One drawable primitive. Coordinates stay in feet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Item {
    Polyline {
        pts: Vec<[f64; 2]>,
        role: Role,
        depth: f64,
        label: Option<String>,
    },
    Marker {
        at: [f64; 2],
        role: Role,
        depth: f64,
        label: Option<String>,
    },
    Dim {
        from: [f64; 2],
        to: [f64; 2],
        text: String,
        offset_ft: f64,
    },
    Arc {
        at: [f64; 2],
        from_deg: f64,
        to_deg: f64,
        radius_ft: f64,
        text: String,
    },
    Text {
        at: [f64; 2],
        text: String,
        role: Role,
    },
}

impl Item {
    fn depth(&self) -> f64 {
        match self {
            Self::Polyline { depth, .. } | Self::Marker { depth, .. } => *depth,
            Self::Dim { .. } | Self::Arc { .. } | Self::Text { .. } => f64::INFINITY,
        }
    }

    fn visit_pts(&self, mut f: impl FnMut([f64; 2])) {
        match self {
            Self::Polyline { pts, .. } => {
                for p in pts {
                    f(*p);
                }
            }
            Self::Marker { at, .. } | Self::Text { at, .. } => f(*at),
            Self::Dim { from, to, .. } => {
                f(*from);
                f(*to);
            }
            Self::Arc { at, .. } => f(*at),
        }
    }
}

/// Renderer-agnostic 2D geometry for one view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    pub kind: ViewKind,
    pub bounds: Rect,
    pub items: Vec<Item>,
}

impl Scene {
    pub fn project(eval: &EvalRig, kind: ViewKind) -> Self {
        Self::project_rig(eval, None, kind)
    }

    pub fn project_rig(eval: &EvalRig, rig: Option<&Rig>, kind: ViewKind) -> Self {
        Self::project_styled(eval, rig, kind, &|_, role| (role, None))
    }

    /// Project a solved rig (Step 3): the hung pose, each member labelled with
    /// its tension, slack members as `SlackMember`, and any member with an
    /// OVER check as `Overloaded`.
    pub fn project_solved(
        solved: &crate::rig::solve::SolvedRig,
        rig: &Rig,
        kind: ViewKind,
        rated: &[crate::rig::solve::rate::Rated],
    ) -> Self {
        use crate::checks::Status;
        use crate::rig::eval::ItemRef;
        let style = |id: MemberId, role: Role| -> (Role, Option<String>) {
            let Some(mf) = solved.members.get(&id) else {
                return (role, None);
            };
            let over = rated.iter().any(|r| {
                matches!(r.at, ItemRef::Member { id: m, .. } if m == id)
                    && r.check.status == Status::Over
            });
            let role = if over {
                Role::Overloaded
            } else if !mf.taut && !mf.is_link {
                Role::SlackMember
            } else if role == Role::ShortMember {
                role
            } else {
                Role::Member
            };
            let label = (!mf.is_link).then(|| format!("{:.0} lb", mf.tension_lbs));
            (role, label)
        };
        Self::project_styled(&solved.eval, Some(rig), kind, &style)
    }

    fn project_styled(
        eval: &EvalRig,
        rig: Option<&Rig>,
        kind: ViewKind,
        style: &dyn Fn(MemberId, Role) -> (Role, Option<String>),
    ) -> Self {
        let mut items = Vec::new();
        if let Some(rig) = rig {
            draw_bodies(eval, rig, kind, &mut items);
            draw_nodes(eval, rig, kind, &mut items);
        } else {
            draw_nodes_only(eval, kind, &mut items);
        }
        draw_members(eval, kind, &mut items, style);
        items.sort_by(|a, b| {
            a.depth()
                .partial_cmp(&b.depth())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if let Some(rig) = rig {
            annotate::annotate(&mut items, eval, rig, kind);
        }
        let bounds = bounds_of(&items);
        Self {
            kind,
            bounds,
            items,
        }
    }

    pub fn fit(&self, w: f64, h: f64, margin: f64) -> Transform {
        let bw = self.bounds.width().max(1e-9);
        let bh = self.bounds.height().max(1e-9);
        let inner_w = (w - 2.0 * margin).max(1e-9);
        let inner_h = (h - 2.0 * margin).max(1e-9);
        let scale = (inner_w / bw).min(inner_h / bh);
        let cx = (self.bounds.min[0] + self.bounds.max[0]) * 0.5;
        let cy = (self.bounds.min[1] + self.bounds.max[1]) * 0.5;
        Transform {
            scale,
            offset: [w * 0.5 - cx * scale, h * 0.5 - cy * scale],
        }
    }

    pub fn has_nan(&self) -> bool {
        let mut bad = false;
        if !self.bounds.min.iter().all(|c| c.is_finite())
            || !self.bounds.max.iter().all(|c| c.is_finite())
        {
            return true;
        }
        for item in &self.items {
            item.visit_pts(|p| {
                if !p[0].is_finite() || !p[1].is_finite() {
                    bad = true;
                }
            });
        }
        bad
    }

    pub fn count_role(&self, role: Role) -> usize {
        self.items
            .iter()
            .filter(|i| match i {
                Item::Polyline { role: r, .. }
                | Item::Marker { role: r, .. }
                | Item::Text { role: r, .. } => *r == role,
                _ => false,
            })
            .count()
    }
}

pub(crate) fn project_pt(p: [f64; 3], kind: ViewKind) -> ([f64; 2], f64) {
    match kind {
        ViewKind::Side => ([p[0], p[2]], p[1]),
        ViewKind::End => ([p[1], p[2]], p[0]),
        ViewKind::Plan => ([p[0], p[1]], p[2]),
    }
}

fn draw_members(
    eval: &EvalRig,
    kind: ViewKind,
    items: &mut Vec<Item>,
    style: &dyn Fn(MemberId, Role) -> (Role, Option<String>),
) {
    for (id, m) in &eval.members {
        if m.points.len() < 2 {
            continue;
        }
        let pts: Vec<[f64; 2]> = m.points.iter().map(|p| project_pt(*p, kind).0).collect();
        let depth = m
            .points
            .iter()
            .map(|p| project_pt(*p, kind).1)
            .fold(0.0_f64, f64::min);
        let short = m.short_ft.iter().any(|s| *s > 1e-9);
        let slack = m.slack_ft.iter().any(|s| *s > 1e-6);
        let role = if short {
            Role::ShortMember
        } else if slack {
            Role::SlackMember
        } else {
            Role::Member
        };
        let (role, label) = style(*id, role);
        items.push(Item::Polyline {
            pts,
            role,
            depth,
            label,
        });
    }
}

fn draw_nodes(eval: &EvalRig, rig: &Rig, kind: ViewKind, items: &mut Vec<Item>) {
    for (id, p) in &eval.nodes {
        let Some(node) = rig.nodes.get(id) else {
            continue;
        };
        let (at, depth) = project_pt(*p, kind);
        let role = match node.kind {
            NodeKind::Root => Role::Hook,
            NodeKind::Bow { .. } => Role::Bow,
            NodeKind::Edge { .. } => Role::Edge,
            NodeKind::Lug { .. } => Role::Lug,
            NodeKind::Free => Role::Lug,
        };
        items.push(Item::Marker {
            at,
            role,
            depth,
            label: Some(node.label.clone()),
        });
    }
}

fn draw_nodes_only(eval: &EvalRig, kind: ViewKind, items: &mut Vec<Item>) {
    for p in eval.nodes.values() {
        let (at, depth) = project_pt(*p, kind);
        items.push(Item::Marker {
            at,
            role: Role::Lug,
            depth,
            label: None,
        });
    }
}

fn draw_bodies(eval: &EvalRig, rig: &Rig, kind: ViewKind, items: &mut Vec<Item>) {
    for (id, body) in &rig.bodies {
        let Some(be) = eval.bodies.get(id) else {
            continue;
        };
        match &body.kind {
            BodyKind::Load {
                length,
                width,
                height,
            } => {
                let Ok(l) = length.eval(&rig.params) else {
                    continue;
                };
                let Ok(w) = width.eval(&rig.params) else {
                    continue;
                };
                let Ok(h) = height.eval(&rig.params) else {
                    continue;
                };
                let corners = [
                    [-l / 2.0, -w / 2.0, 0.0],
                    [l / 2.0, -w / 2.0, 0.0],
                    [l / 2.0, w / 2.0, 0.0],
                    [-l / 2.0, w / 2.0, 0.0],
                    [-l / 2.0, -w / 2.0, h],
                    [l / 2.0, -w / 2.0, h],
                    [l / 2.0, w / 2.0, h],
                    [-l / 2.0, w / 2.0, h],
                ];
                let world: Vec<[f64; 3]> = corners.iter().map(|c| be.frame.transform(*c)).collect();
                let edges = [
                    (0, 1),
                    (1, 2),
                    (2, 3),
                    (3, 0),
                    (4, 5),
                    (5, 6),
                    (6, 7),
                    (7, 4),
                    (0, 4),
                    (1, 5),
                    (2, 6),
                    (3, 7),
                ];
                for (a, b) in edges {
                    let pa = project_pt(world[a], kind);
                    let pb = project_pt(world[b], kind);
                    items.push(Item::Polyline {
                        pts: vec![pa.0, pb.0],
                        role: Role::Load,
                        depth: pa.1.min(pb.1),
                        label: None,
                    });
                }
            }
            BodyKind::SpreaderBar { .. } | BodyKind::LiftingBeam { .. } => {
                let mut pts: Vec<[f64; 3]> = rig
                    .nodes
                    .values()
                    .filter(|n| n.body == Some(*id))
                    .filter_map(|n| eval.nodes.get(&n.id).copied())
                    .collect();
                if pts.len() < 2 {
                    continue;
                }
                pts.sort_by(|a, b| {
                    let da = a[0] * a[0] + a[1] * a[1];
                    let db = b[0] * b[0] + b[1] * b[1];
                    // span ends: extreme along the longest plan axis
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                });
                // Pick the two furthest-apart nodes.
                let mut best = (0usize, 1usize, 0.0_f64);
                for i in 0..pts.len() {
                    for j in (i + 1)..pts.len() {
                        let d = dist3(pts[i], pts[j]);
                        if d > best.2 {
                            best = (i, j, d);
                        }
                    }
                }
                let a = project_pt(pts[best.0], kind);
                let b = project_pt(pts[best.1], kind);
                items.push(Item::Polyline {
                    pts: vec![a.0, b.0],
                    role: Role::Bar,
                    depth: a.1.min(b.1),
                    label: Some(body.label.clone()),
                });
                // Depth tick at mid-span.
                let mid = [
                    (pts[best.0][0] + pts[best.1][0]) * 0.5,
                    (pts[best.0][1] + pts[best.1][1]) * 0.5,
                    (pts[best.0][2] + pts[best.1][2]) * 0.5,
                ];
                let tick_dir = match kind {
                    ViewKind::Plan => [0.0, 0.0, 0.4],
                    _ => [0.0, 0.4, 0.0],
                };
                let t0 = project_pt(
                    [
                        mid[0] - tick_dir[0],
                        mid[1] - tick_dir[1],
                        mid[2] - tick_dir[2],
                    ],
                    kind,
                );
                let t1 = project_pt(
                    [
                        mid[0] + tick_dir[0],
                        mid[1] + tick_dir[1],
                        mid[2] + tick_dir[2],
                    ],
                    kind,
                );
                items.push(Item::Polyline {
                    pts: vec![t0.0, t1.0],
                    role: Role::Bar,
                    depth: t0.1.min(t1.1),
                    label: None,
                });
            }
            BodyKind::Hook => {
                let (at, depth) = project_pt(be.frame.origin, kind);
                items.push(Item::Marker {
                    at,
                    role: Role::Hook,
                    depth,
                    label: Some(body.label.clone()),
                });
            }
            BodyKind::Frame => {
                let (at, depth) = project_pt(be.frame.origin, kind);
                items.push(Item::Marker {
                    at,
                    role: Role::Bar,
                    depth,
                    label: Some(body.label.clone()),
                });
            }
        }
        if kind == ViewKind::Plan {
            let (cg, depth) = project_pt(be.cg_world, kind);
            items.push(Item::Marker {
                at: cg,
                role: Role::Cg,
                depth,
                label: Some(format!("{} CG", body.label)),
            });
            if let Some(s) = be.support_centroid {
                let (sc, d2) = project_pt(s, kind);
                items.push(Item::Marker {
                    at: sc,
                    role: Role::Cg,
                    depth: d2,
                    label: Some(format!("{} support", body.label)),
                });
            }
        }
    }
}

fn bounds_of(items: &[Item]) -> Rect {
    let mut r = Rect {
        min: [f64::INFINITY, f64::INFINITY],
        max: [f64::NEG_INFINITY, f64::NEG_INFINITY],
    };
    let mut any = false;
    for item in items {
        item.visit_pts(|p| {
            if p[0].is_finite() && p[1].is_finite() {
                if !any {
                    r.min = p;
                    r.max = p;
                    any = true;
                } else {
                    r.include(p);
                }
            }
        });
    }
    if !any {
        return Rect::empty();
    }
    // Pad a hair so degenerate (vertical-only) scenes still have area.
    if (r.max[0] - r.min[0]).abs() < 1e-9 {
        r.min[0] -= 0.5;
        r.max[0] += 0.5;
    }
    if (r.max[1] - r.min[1]).abs() < 1e-9 {
        r.min[1] -= 0.5;
        r.max[1] += 0.5;
    }
    r
}

fn dist3(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rig::eval::EvalRig;
    use crate::rig::{duplo10, two_leg_bridle};

    #[test]
    fn two_leg_scenes_are_finite_and_contain_points() {
        let rig = two_leg_bridle(1_000.0, 12.0, 12.0);
        let ev = EvalRig::evaluate(&rig).unwrap();
        for kind in [ViewKind::Side, ViewKind::End, ViewKind::Plan] {
            let s = Scene::project_rig(&ev, Some(&rig), kind);
            assert!(!s.has_nan(), "{kind:?}");
            assert!(s.bounds.min[0].is_finite() && s.bounds.max[0].is_finite());
            assert!(s.count_role(Role::Member) + s.count_role(Role::SlackMember) >= 1);
            assert!(s.count_role(Role::Load) >= 1);
            assert!(s.count_role(Role::Hook) >= 1);
            let n_pts = s.items.iter().fold(0usize, |n, i| {
                let mut c = 0;
                i.visit_pts(|_| c += 1);
                n + c
            });
            assert!(n_pts > 0);
            for item in &s.items {
                item.visit_pts(|p| {
                    assert!(s.bounds.contains(p), "{kind:?} {p:?} not in {:?}", s.bounds);
                });
            }
            let t = s.fit(400.0, 300.0, 20.0);
            assert!(t.scale.is_finite() && t.scale > 0.0);
        }
    }

    #[test]
    fn duplo10_plan_shows_cg() {
        let rig = duplo10();
        let ev = EvalRig::evaluate(&rig).unwrap();
        let s = Scene::project_rig(&ev, Some(&rig), ViewKind::Plan);
        assert!(!s.has_nan());
        assert!(s.count_role(Role::Cg) >= 1);
        assert!(s.count_role(Role::Bar) >= 1);
    }
}
