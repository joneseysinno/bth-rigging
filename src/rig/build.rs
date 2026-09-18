//! Fluent builder so a rig reads like the drawing.
//!
//! Step: 1
//! Theory: roadmap Step 1 — authored graph construction without an expression parser.
//! Inputs: named parameters, bodies, nodes, member paths.
//! Outputs: a `Rig` ready to validate.
//! Must not depend on: solve, UI, dioxus, store.

use uuid::Uuid;

use crate::domain::Hitch;

use super::body::{BarRating, Body, BodyKind};
use super::component::{Adjust, Component};
use super::id::{BodyId, MemberId, NodeId, ParamId};
use super::member::{Member, Segment};
use super::node::{Axis, Node, NodeKind};
use super::param::{Coord3, Expr, Param, ParamSource, Quantity};
use super::{Rig, RigError, SCHEMA_VERSION};

/// Builds a [`Rig`] by inserting parameters, bodies, nodes and members in drawing order.
pub struct RigBuilder {
    id: Uuid,
    project_id: Uuid,
    name: String,
    params: super::param::ParamTable,
    bodies: indexmap::IndexMap<BodyId, Body>,
    nodes: indexmap::IndexMap<NodeId, Node>,
    members: indexmap::IndexMap<MemberId, Member>,
    root: Option<NodeId>,
}

impl RigBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            project_id: Uuid::nil(),
            name: name.into(),
            params: indexmap::IndexMap::new(),
            bodies: indexmap::IndexMap::new(),
            nodes: indexmap::IndexMap::new(),
            members: indexmap::IndexMap::new(),
            root: None,
        }
    }

    pub fn with_id(mut self, id: Uuid) -> Self {
        self.id = id;
        self
    }

    pub fn project_id(&mut self, id: Uuid) -> &mut Self {
        self.project_id = id;
        self
    }

    pub fn param(
        &mut self,
        name: impl Into<String>,
        quantity: Quantity,
        nominal: f64,
    ) -> ParamHandle<'_> {
        let mut p = Param::new(name, quantity, nominal);
        let id = p.id;
        p.source = ParamSource::Assumed;
        self.params.insert(id, p);
        ParamHandle { builder: self, id }
    }

    pub fn hook(&mut self, label: impl Into<String>) -> NodeId {
        let body = Body::new(label, BodyKind::Hook, 0.0);
        let node = Node::root("hook", body.id);
        let nid = node.id;
        self.bodies.insert(body.id, body);
        self.nodes.insert(nid, node);
        self.root = Some(nid);
        nid
    }

    pub fn load(
        &mut self,
        label: impl Into<String>,
        length: impl Into<Expr>,
        width: impl Into<Expr>,
        height: impl Into<Expr>,
        weight: impl Into<Expr>,
    ) -> BodyId {
        let mut body = Body::new(
            label,
            BodyKind::Load {
                length: length.into(),
                width: width.into(),
                height: height.into(),
            },
            weight,
        );
        body.cg = Coord3::new(0.0, 0.0, Expr::c(0.0));
        let id = body.id;
        self.bodies.insert(id, body);
        id
    }

    pub fn bar(
        &mut self,
        label: impl Into<String>,
        span: impl Into<Expr>,
        weight: impl Into<Expr>,
    ) -> BodyId {
        self.spreader(label, span, weight, 0)
    }

    pub fn spreader(
        &mut self,
        label: impl Into<String>,
        span: impl Into<Expr>,
        weight: impl Into<Expr>,
        wll_lbs: u32,
    ) -> BodyId {
        let body = Body::new(
            label,
            BodyKind::SpreaderBar {
                span: span.into(),
                rating: BarRating::new(wll_lbs),
            },
            weight,
        );
        let id = body.id;
        self.bodies.insert(id, body);
        id
    }

    pub fn frame(&mut self, label: impl Into<String>, weight: impl Into<Expr>) -> BodyId {
        let body = Body::new(label, BodyKind::Frame, weight);
        let id = body.id;
        self.bodies.insert(id, body);
        id
    }

    pub fn set_cg(&mut self, body: BodyId, cg: Coord3) {
        if let Some(b) = self.bodies.get_mut(&body) {
            b.cg = cg;
        }
    }

    pub fn lug(
        &mut self,
        body: BodyId,
        label: impl Into<String>,
        x: impl Into<Expr>,
        y: impl Into<Expr>,
        z: impl Into<Expr>,
    ) -> NodeId {
        let node = Node::lug(label, body, Coord3::new(x, y, z));
        let id = node.id;
        self.nodes.insert(id, node);
        id
    }

    pub fn lug_axis(
        &mut self,
        body: BodyId,
        label: impl Into<String>,
        x: impl Into<Expr>,
        y: impl Into<Expr>,
        z: impl Into<Expr>,
        plate_normal: Axis,
    ) -> NodeId {
        let mut node = Node::lug(label, body, Coord3::new(x, y, z));
        if let NodeKind::Lug {
            plate_normal: n, ..
        } = &mut node.kind
        {
            *n = plate_normal;
        }
        let id = node.id;
        self.nodes.insert(id, node);
        id
    }

    pub fn bow(
        &mut self,
        body: BodyId,
        label: impl Into<String>,
        x: impl Into<Expr>,
        y: impl Into<Expr>,
        z: impl Into<Expr>,
        bow_dia: impl Into<Expr>,
        mu: impl Into<Expr>,
    ) -> NodeId {
        let node = Node::bow(label, body, Coord3::new(x, y, z), bow_dia, mu);
        let id = node.id;
        self.nodes.insert(id, node);
        id
    }

    pub fn edge(
        &mut self,
        body: BodyId,
        label: impl Into<String>,
        x: impl Into<Expr>,
        y: impl Into<Expr>,
        z: impl Into<Expr>,
        radius: impl Into<Expr>,
        mu: impl Into<Expr>,
        softener: bool,
    ) -> NodeId {
        let node = Node {
            id: NodeId::new(),
            body: Some(body),
            local: Coord3::new(x, y, z),
            label: label.into(),
            kind: NodeKind::Edge {
                radius: radius.into(),
                mu: mu.into(),
                softener,
            },
        };
        let id = node.id;
        self.nodes.insert(id, node);
        id
    }

    pub fn free(&mut self, label: impl Into<String>) -> NodeId {
        let node = Node::free(label);
        let id = node.id;
        self.nodes.insert(id, node);
        id
    }

    pub fn member(&mut self, label: impl Into<String>) -> MemberHandle<'_> {
        MemberHandle {
            builder: self,
            member: Some(Member {
                id: MemberId::new(),
                label: label.into(),
                path: Vec::new(),
                segments: Vec::new(),
            }),
        }
    }

    /// Consume the builder without validation (for tests that poke holes).
    pub fn build_unchecked(self) -> Rig {
        let root = self.root.or_else(|| self.nodes.keys().next().copied());
        Rig {
            id: self.id,
            project_id: self.project_id,
            name: self.name,
            schema_version: SCHEMA_VERSION,
            params: self.params,
            bodies: self.bodies,
            nodes: self.nodes,
            members: self.members,
            root: root.unwrap_or_else(NodeId::nil),
        }
    }

    pub fn finish(self) -> Result<Rig, Vec<RigError>> {
        let rig = self.build_unchecked();
        rig.validate()?;
        Ok(rig)
    }
}

/// Fluent handle that sets tolerance and source, then yields the [`ParamId`].
pub struct ParamHandle<'a> {
    builder: &'a mut RigBuilder,
    id: ParamId,
}

impl ParamHandle<'_> {
    pub fn tol(self, minus: f64, plus: f64) -> Self {
        if let Some(p) = self.builder.params.get_mut(&self.id) {
            p.minus = minus.max(0.0);
            p.plus = plus.max(0.0);
        }
        self
    }

    pub fn note(self, note: impl Into<String>) -> Self {
        if let Some(p) = self.builder.params.get_mut(&self.id) {
            p.note = note.into();
        }
        self
    }

    pub fn drawing(self) -> ParamId {
        self.set_source(ParamSource::Drawing)
    }

    pub fn catalog(self) -> ParamId {
        self.set_source(ParamSource::Catalog)
    }

    pub fn measured(self) -> ParamId {
        self.set_source(ParamSource::Measured)
    }

    pub fn assumed(self) -> ParamId {
        self.set_source(ParamSource::Assumed)
    }

    pub fn derived(self, expr: Expr) -> ParamId {
        if let Some(p) = self.builder.params.get_mut(&self.id) {
            p.source = ParamSource::Derived;
            p.expr = Some(expr);
        }
        self.id
    }

    pub fn id(self) -> ParamId {
        self.id
    }

    fn set_source(self, source: ParamSource) -> ParamId {
        if let Some(p) = self.builder.params.get_mut(&self.id) {
            p.source = source;
        }
        self.id
    }
}

/// Fluent member: `.from().through().to().segment(|s| ...)`.
pub struct MemberHandle<'a> {
    builder: &'a mut RigBuilder,
    member: Option<Member>,
}

impl MemberHandle<'_> {
    pub fn from(self, node: NodeId) -> Self {
        self.push_node(node)
    }

    pub fn through(self, node: NodeId) -> Self {
        self.push_node(node)
    }

    pub fn to(self, node: NodeId) -> Self {
        self.push_node(node)
    }

    fn push_node(mut self, node: NodeId) -> Self {
        if let Some(m) = self.member.as_mut() {
            m.path.push(node);
        }
        self
    }

    pub fn segment(mut self, f: impl FnOnce(SegmentBuilder) -> SegmentBuilder) -> Self {
        let seg = f(SegmentBuilder {
            components: Vec::new(),
        })
        .build();
        if let Some(m) = self.member.as_mut() {
            m.segments.push(seg);
        }
        self
    }

    pub fn id(&self) -> Option<MemberId> {
        self.member.as_ref().map(|m| m.id)
    }
}

impl Drop for MemberHandle<'_> {
    fn drop(&mut self) {
        if let Some(m) = self.member.take() {
            self.builder.members.insert(m.id, m);
        }
    }
}

/// Builds one segment's component list, top → bottom.
pub struct SegmentBuilder {
    components: Vec<Component>,
}

impl SegmentBuilder {
    pub fn shackle(mut self, size: &str) -> Self {
        self.components.push(Component::shackle(size));
        self
    }

    pub fn roundsling(mut self, size: u8, hitch: Hitch, length_ft: f64) -> Self {
        self.components
            .push(Component::roundsling(size, hitch, length_ft));
        self
    }

    pub fn strap(mut self, length: impl Into<Expr>) -> Self {
        let length = length.into();
        self.components
            .push(Component::strap(length, Expr::c(0.0), 4.0));
        self
    }

    pub fn strap_w(mut self, length: impl Into<Expr>, weight: impl Into<Expr>) -> Self {
        self.components.push(Component::strap(length, weight, 4.0));
        self
    }

    pub fn chain(
        mut self,
        grade: u8,
        size_in: impl Into<Expr>,
        length: impl Into<Expr>,
        weight: impl Into<Expr>,
        adjust: Adjust,
    ) -> Self {
        self.components
            .push(Component::chain(grade, size_in, length, weight, adjust));
        self
    }

    pub fn master_link(mut self, length: impl Into<Expr>, weight: impl Into<Expr>) -> Self {
        self.components.push(Component::master_link(length, weight));
        self
    }

    pub fn push(mut self, c: Component) -> Self {
        self.components.push(c);
        self
    }

    fn build(self) -> Segment {
        Segment::new(self.components)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_reads_like_a_drawing() {
        let mut b = RigBuilder::new("sample");
        let span = b
            .param("span_A", Quantity::Length, 20.0)
            .tol(0.0, 0.0)
            .drawing();
        let w_bar = b.param("bar_weight", Quantity::Weight, 200.0).drawing();
        let w_load = b.param("load_weight", Quantity::Weight, 10_000.0).drawing();
        let hook = b.hook("Hook");
        let unit = b.load("Load", 10.0, 4.0, 2.0, w_load);
        let bar = b.bar("Bar L", span, w_bar);
        let p1 = b.lug(unit, "P1", -4.0, 0.0, 2.0);
        let p3 = b.lug(unit, "P3", 4.0, 0.0, 2.0);
        let bow = b.bow(bar, "Bar L bow F", 0.0, -6.0, 0.0, 2.0, 0.0);
        let lug_a = b.lug(bar, "Bar L end A", -(Expr::from(span) / 2.0), 0.0, 0.0);
        b.member("Hook leg").from(hook).to(lug_a).segment(|s| {
            s.shackle("1-1/4")
                .roundsling(7, Hitch::Vertical, 12.0)
                .shackle("1-1/4")
        });
        b.member("Basket strap P1–P3 (front)")
            .from(p1)
            .through(bow)
            .to(p3)
            .segment(|s| s.shackle("1-1/4").strap(11.0).shackle("1-1/4"))
            .segment(|s| s.shackle("1-1/4").strap(11.0).shackle("1-1/4"));
        let rig = b.finish().expect("valid");
        assert_eq!(rig.bodies.len(), 3);
        assert_eq!(rig.members.len(), 2);
        assert!(rig.param_named("span_A").is_some());
    }
}
