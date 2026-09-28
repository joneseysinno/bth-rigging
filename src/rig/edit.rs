//! Headless rig parameter edits and reference discovery.
//!
//! Step: 2.5
//! Theory: docs/step-2.5-parser-param-table.md, §6.
//! Inputs: authored rig graph and parameter edit requests.
//! Outputs: where-used sites and, in the next work item, pure edit results.
//! Must not depend on: UI, dioxus, store.

use std::collections::HashMap;

use super::body::{BodyKind, Placement};
use super::component::ComponentKind;
use super::param::Expr;
use super::{BodyId, MemberId, NodeId, ParamId, Rig};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UseSite {
    Param {
        id: ParamId,
    },
    NodeCoord {
        node: NodeId,
        axis: char,
    },
    BodyWeight {
        body: BodyId,
    },
    BodyCg {
        body: BodyId,
        axis: char,
    },
    BodyGeometry {
        body: BodyId,
        field: &'static str,
    },
    Placement {
        body: BodyId,
    },
    Component {
        member: MemberId,
        segment: usize,
        index: usize,
        field: &'static str,
    },
    ComponentKind {
        member: MemberId,
        segment: usize,
        index: usize,
        field: &'static str,
    },
    Bearing {
        node: NodeId,
        field: &'static str,
    },
}

pub fn uses(rig: &Rig, id: ParamId) -> Vec<UseSite> {
    let mut out = Vec::new();

    for param in rig.params.values() {
        if let Some(expr) = &param.expr {
            append_expr_uses(expr, &mut |referenced| {
                if referenced == id {
                    out.push(UseSite::Param { id: param.id });
                }
            });
        }
    }

    for (node_id, node) in &rig.nodes {
        for (axis, expr) in [
            ('x', &node.local.x),
            ('y', &node.local.y),
            ('z', &node.local.z),
        ] {
            append_expr_uses(expr, &mut |referenced| {
                if referenced == id {
                    out.push(UseSite::NodeCoord {
                        node: *node_id,
                        axis,
                    });
                }
            });
        }
        match &node.kind {
            super::node::NodeKind::Bow { bow_dia, mu } => {
                append_bearing_use(bow_dia, *node_id, "bow_dia", id, &mut out);
                append_bearing_use(mu, *node_id, "mu", id, &mut out);
            }
            super::node::NodeKind::Edge { radius, mu, .. } => {
                append_bearing_use(radius, *node_id, "radius", id, &mut out);
                append_bearing_use(mu, *node_id, "mu", id, &mut out);
            }
            _ => {}
        }
    }

    for (body_id, body) in &rig.bodies {
        append_body_use(
            &body.weight,
            id,
            UseSite::BodyWeight { body: *body_id },
            &mut out,
        );
        for (axis, expr) in [('x', &body.cg.x), ('y', &body.cg.y), ('z', &body.cg.z)] {
            append_expr_uses(expr, &mut |referenced| {
                if referenced == id {
                    out.push(UseSite::BodyCg {
                        body: *body_id,
                        axis,
                    });
                }
            });
        }
        match &body.kind {
            BodyKind::SpreaderBar { span, .. } | BodyKind::LiftingBeam { span, .. } => {
                append_body_use(
                    span,
                    id,
                    UseSite::BodyGeometry {
                        body: *body_id,
                        field: "span",
                    },
                    &mut out,
                );
            }
            BodyKind::Load {
                length,
                width,
                height,
            } => {
                for (field, expr) in [("length", length), ("width", width), ("height", height)] {
                    append_body_use(
                        expr,
                        id,
                        UseSite::BodyGeometry {
                            body: *body_id,
                            field,
                        },
                        &mut out,
                    );
                }
            }
            BodyKind::Hook | BodyKind::Frame => {}
        }
        if let Some(placement) = &body.placement {
            match placement {
                Placement::Derived => {}
                Placement::Pinned { at } => append_placement_coord(at, *body_id, id, &mut out),
                Placement::Posed { at, rot } => {
                    append_placement_coord(at, *body_id, id, &mut out);
                    append_placement_rot(rot, *body_id, id, &mut out);
                }
                Placement::Level { rot } => append_placement_rot(rot, *body_id, id, &mut out),
            }
        }
    }

    for (member_id, member) in &rig.members {
        for (segment_index, segment) in member.segments.iter().enumerate() {
            for (component_index, component) in segment.components.iter().enumerate() {
                let context = |field| UseSite::Component {
                    member: *member_id,
                    segment: segment_index,
                    index: component_index,
                    field,
                };
                append_component_use(&component.length, id, context("length"), &mut out);
                append_component_use(&component.weight, id, context("weight"), &mut out);
                if let Some(stiffness) = &component.stiffness_lb {
                    append_component_use(stiffness, id, context("stiffness_lb"), &mut out);
                }
                if let Some(adjust) = &component.adjust {
                    append_component_use(&adjust.min, id, context("adjust.min"), &mut out);
                    append_component_use(&adjust.max, id, context("adjust.max"), &mut out);
                    append_component_use(&adjust.setting, id, context("adjust.setting"), &mut out);
                }
                match &component.kind {
                    ComponentKind::Strap { width_in } => append_component_kind_use(
                        width_in,
                        id,
                        *member_id,
                        segment_index,
                        component_index,
                        "width_in",
                        &mut out,
                    ),
                    ComponentKind::Chain { size_in, .. } => append_component_kind_use(
                        size_in,
                        id,
                        *member_id,
                        segment_index,
                        component_index,
                        "size_in",
                        &mut out,
                    ),
                    ComponentKind::WireRope { dia_in } => append_component_kind_use(
                        dia_in,
                        id,
                        *member_id,
                        segment_index,
                        component_index,
                        "dia_in",
                        &mut out,
                    ),
                    _ => {}
                }
            }
        }
    }

    out
}

pub fn use_counts(rig: &Rig) -> HashMap<ParamId, usize> {
    rig.params
        .keys()
        .copied()
        .filter_map(|id| {
            let count = uses(rig, id).len();
            (count > 0).then_some((id, count))
        })
        .collect()
}

fn append_expr_uses(expr: &Expr, visit: &mut dyn FnMut(ParamId)) {
    match expr {
        Expr::Const(_) => {}
        Expr::Param(id) => visit(*id),
        Expr::Neg(child) => append_expr_uses(child, visit),
        Expr::Add(left, right)
        | Expr::Sub(left, right)
        | Expr::Mul(left, right)
        | Expr::Div(left, right) => {
            append_expr_uses(left, visit);
            append_expr_uses(right, visit);
        }
    }
}

fn append_body_use(expr: &Expr, target: ParamId, site: UseSite, out: &mut Vec<UseSite>) {
    append_expr_uses(expr, &mut |id| {
        if id == target {
            out.push(site.clone());
        }
    });
}

fn append_component_use(expr: &Expr, target: ParamId, site: UseSite, out: &mut Vec<UseSite>) {
    append_body_use(expr, target, site, out);
}

fn append_component_kind_use(
    expr: &Expr,
    target: ParamId,
    member: MemberId,
    segment: usize,
    index: usize,
    field: &'static str,
    out: &mut Vec<UseSite>,
) {
    append_expr_uses(expr, &mut |id| {
        if id == target {
            out.push(UseSite::ComponentKind {
                member,
                segment,
                index,
                field,
            });
        }
    });
}

fn append_bearing_use(
    expr: &Expr,
    node: NodeId,
    field: &'static str,
    target: ParamId,
    out: &mut Vec<UseSite>,
) {
    append_expr_uses(expr, &mut |id| {
        if id == target {
            out.push(UseSite::Bearing { node, field });
        }
    });
}

fn append_placement_coord(
    coord: &super::param::Coord3,
    body: BodyId,
    target: ParamId,
    out: &mut Vec<UseSite>,
) {
    for expr in [&coord.x, &coord.y, &coord.z] {
        append_expr_uses(expr, &mut |id| {
            if id == target {
                out.push(UseSite::Placement { body });
            }
        });
    }
}

fn append_placement_rot(
    rot: &super::body::RotExpr,
    body: BodyId,
    target: ParamId,
    out: &mut Vec<UseSite>,
) {
    for expr in [&rot.yaw, &rot.pitch, &rot.roll] {
        append_expr_uses(expr, &mut |id| {
            if id == target {
                out.push(UseSite::Placement { body });
            }
        });
    }
}

fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

pub fn validate_name(
    name: &str,
    table: &super::ParamTable,
    except: Option<ParamId>,
) -> Result<(), String> {
    if !valid_name(name) {
        return Err("name must match [A-Za-z_][A-Za-z0-9_]*".into());
    }
    if [
        "ft", "in", "lb", "lbs", "kip", "deg", "e", "E", "sqrt", "min", "max", "hypot", "abs",
        "sin", "cos", "tan", "atan2",
    ]
    .contains(&name)
    {
        return Err(format!("'{name}' is reserved"));
    }
    if table
        .values()
        .any(|param| param.name == name && Some(param.id) != except)
    {
        return Err(format!("parameter '{name}' already exists"));
    }
    Ok(())
}

#[cfg(test)]
fn fixture_rigs() -> Vec<Rig> {
    use crate::domain::{Hitch, Pick, SavedSpreader, SlingLayer};
    let mut rigs = vec![super::duplo10(), super::two_leg_bridle(10_000.0, 10.0, 8.0)];
    let pick = Pick::new(uuid::Uuid::nil(), "template", 10_000.0);
    let layer = SlingLayer {
        pick_id: pick.id,
        layer_index: 0,
        size: 5,
        hitch: Hitch::Vertical,
        angle_deg: 60.0,
        sling_count: 2,
        sling_length_ft: 10.0,
        pick_spacing_ft: Some(8.0),
        pick_width_ft: Some(6.0),
        spreader_span_ft: None,
        apex_shackle: None,
        leg_shackle: None,
        spreader_id: None,
        spreader_wll_lbs: None,
        tare_lbs: 0.0,
    };
    rigs.push(super::from_layers(&pick, &[layer], &[]));
    let bar = SavedSpreader::new("Test", "Bar", 30_000, 150.0);
    let mut top = SlingLayer {
        pick_id: pick.id,
        layer_index: 0,
        size: 7,
        hitch: Hitch::Vertical,
        angle_deg: 90.0,
        sling_count: 2,
        sling_length_ft: 12.0,
        pick_spacing_ft: None,
        pick_width_ft: None,
        spreader_span_ft: Some(10.0),
        apex_shackle: None,
        leg_shackle: None,
        spreader_id: Some(bar.id),
        spreader_wll_lbs: None,
        tare_lbs: 0.0,
    };
    top.pick_id = pick.id;
    let bottom = SlingLayer {
        pick_id: pick.id,
        layer_index: 1,
        size: 5,
        hitch: Hitch::Vertical,
        angle_deg: 90.0,
        sling_count: 4,
        sling_length_ft: 9.0,
        pick_spacing_ft: Some(10.0),
        pick_width_ft: None,
        spreader_span_ft: None,
        apex_shackle: None,
        leg_shackle: None,
        spreader_id: None,
        spreader_wll_lbs: None,
        tare_lbs: 0.0,
    };
    rigs.push(super::from_layers(&pick, &[top, bottom], &[bar]));
    rigs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rig::Expr;
    use crate::rig::{Param, Quantity};
    use serde_json::Value;

    #[test]
    fn where_used_counts_every_expression_reference() {
        let rig = crate::rig::duplo10();
        let s12 = rig.param_named("s12").unwrap().id;
        let sites = uses(&rig, s12);
        assert!(!sites.is_empty());
        assert_eq!(use_counts(&rig).get(&s12).copied(), Some(sites.len()));
        assert!(
            sites
                .iter()
                .any(|site| matches!(site, UseSite::NodeCoord { .. }))
        );

        let mut changed = rig.clone();
        let body = changed.bodies.values_mut().next().unwrap();
        body.weight = Expr::Param(s12);
        assert!(
            uses(&changed, s12)
                .iter()
                .any(|site| matches!(site, UseSite::BodyWeight { .. }))
        );
    }

    #[test]
    fn where_used_matches_serde_reference_completeness_oracle() {
        for rig in fixture_rigs() {
            let mut value = serde_json::to_value(&rig).unwrap();
            if let Value::Object(root) = &mut value
                && let Some(Value::Object(params)) = root.get_mut("params")
            {
                for param in params.values_mut() {
                    if let Value::Object(fields) = param {
                        fields.remove("id");
                    }
                }
            }
            let serialized_total: usize =
                rig.params.keys().map(|id| count_uuid(&value, id.0)).sum();
            let walked_total: usize = rig.params.keys().map(|id| uses(&rig, *id).len()).sum();
            assert_eq!(walked_total, serialized_total, "rig {}", rig.name);
        }
    }

    fn count_uuid(value: &Value, id: uuid::Uuid) -> usize {
        match value {
            Value::String(value) => usize::from(value == &id.to_string()),
            Value::Array(values) => values.iter().map(|value| count_uuid(value, id)).sum(),
            Value::Object(fields) => fields.values().map(|value| count_uuid(value, id)).sum(),
            _ => 0,
        }
    }

    #[test]
    fn every_fixture_and_template_parameter_name_is_valid() {
        for rig in fixture_rigs() {
            for param in rig.params.values() {
                validate_name(&param.name, &rig.params, Some(param.id))
                    .unwrap_or_else(|message| panic!("{}: {message}", param.name));
            }
        }
    }

    #[test]
    fn names_reject_invalid_reserved_and_duplicate_values() {
        let param = Param::new("span_A", Quantity::Length, 10.0);
        let table = [(param.id, param.clone())].into_iter().collect();
        for name in ["", "2span", "span-A", "ft", "e", "E", "sqrt"] {
            assert!(validate_name(name, &table, None).is_err(), "name: {name}");
        }
        assert!(validate_name("span_A", &table, None).is_err());
        assert!(validate_name("span_A", &table, Some(param.id)).is_ok());
        assert!(validate_name("span_2", &table, None).is_ok());
    }
}
