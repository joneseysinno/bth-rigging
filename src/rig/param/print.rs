//! Canonical expression printer.
//!
//! Step: 2.5
//! Theory: docs/step-2.5-parser-param-table.md, §4.
//! Inputs: expression AST and parameter table.
//! Outputs: stable expression text that reparses to the same AST.
//! Must not depend on: UI, dioxus, store, solver internals.

use std::fmt;

use super::{Expr, ParamTable};

impl Expr {
    pub fn display<'a>(&'a self, table: &'a ParamTable) -> impl fmt::Display + 'a {
        ExprDisplay { expr: self, table }
    }

    pub fn to_text(&self, table: &ParamTable) -> String {
        self.display(table).to_string()
    }
}

struct ExprDisplay<'a> {
    expr: &'a Expr,
    table: &'a ParamTable,
}

impl fmt::Display for ExprDisplay<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&print_expr(self.expr, self.table))
    }
}

fn print_expr(expr: &Expr, table: &ParamTable) -> String {
    match expr {
        Expr::Const(value) => format_number(*value),
        Expr::Param(id) => table
            .get(id)
            .map(|param| param.name.clone())
            .unwrap_or_else(|| format!("?{}", &id.0.simple().to_string()[..8])),
        Expr::Neg(child) => match child.as_ref() {
            Expr::Const(_)
            | Expr::Add(_, _)
            | Expr::Sub(_, _)
            | Expr::Mul(_, _)
            | Expr::Div(_, _) => {
                format!("-({})", print_expr(child, table))
            }
            _ => format!("-{}", print_expr(child, table)),
        },
        Expr::Add(left, right) => print_binary(left, right, "+", 1, table),
        Expr::Sub(left, right) => print_binary(left, right, "-", 1, table),
        Expr::Mul(left, right) => print_binary(left, right, "*", 2, table),
        Expr::Div(left, right) => print_binary(left, right, "/", 2, table),
    }
}

fn print_binary(
    left: &Expr,
    right: &Expr,
    operator: &str,
    precedence: u8,
    table: &ParamTable,
) -> String {
    let left = print_child(left, precedence, false, table);
    let right = print_child(right, precedence, true, table);
    let spaced_operator = matches!(operator, "+" | "-");
    if spaced_operator {
        format!("{left} {operator} {right}")
    } else {
        format!("{left}{operator}{right}")
    }
}

fn print_child(expr: &Expr, parent_precedence: u8, is_right: bool, table: &ParamTable) -> String {
    let rendered = print_expr(expr, table);
    let child_precedence = precedence(expr);
    if child_precedence < parent_precedence || (is_right && child_precedence == parent_precedence) {
        format!("({rendered})")
    } else {
        rendered
    }
}

fn precedence(expr: &Expr) -> u8 {
    match expr {
        Expr::Add(_, _) | Expr::Sub(_, _) => 1,
        Expr::Mul(_, _) | Expr::Div(_, _) => 2,
        Expr::Neg(_) => 3,
        Expr::Const(_) | Expr::Param(_) => 4,
    }
}

fn format_number(value: f64) -> String {
    if value.is_nan() {
        return "nan".into();
    }
    if value == f64::INFINITY {
        return "inf".into();
    }
    if value == f64::NEG_INFINITY {
        return "-inf".into();
    }
    if value != 0.0 && (value.abs() < 1e-6 || value.abs() >= 1e15) {
        format!("{value:e}")
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::domain::{Hitch, Pick, SavedSpreader, SlingLayer};
    use crate::rig::{Param, Quantity, Rig, duplo10, two_leg_bridle};

    fn table() -> ParamTable {
        ["a", "b", "c", "d", "e"]
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                let param = Param::new(name, Quantity::Ratio, index as f64 + 1.0);
                (param.id, param)
            })
            .collect()
    }

    fn parse_print(expr: &Expr, table: &ParamTable) -> Expr {
        super::super::syntax::parse_expr(&expr.to_text(table), table)
            .unwrap_or_else(|error| panic!("parse failed: {error:?}"))
            .expr
    }

    #[test]
    fn canonical_parentheses_preserve_every_binary_tree_shape() {
        let table = table();
        let mut ids = table.keys().copied();
        let a = ids.next().unwrap();
        let b = ids.next().unwrap();
        let c = ids.next().unwrap();
        let expressions = [
            Expr::Add(
                Box::new(a.into()),
                Box::new(Expr::Add(Box::new(b.into()), Box::new(c.into()))),
            ),
            Expr::Mul(
                Box::new(a.into()),
                Box::new(Expr::Mul(Box::new(b.into()), Box::new(c.into()))),
            ),
            Expr::Sub(
                Box::new(a.into()),
                Box::new(Expr::Add(Box::new(b.into()), Box::new(c.into()))),
            ),
            Expr::Div(
                Box::new(a.into()),
                Box::new(Expr::Mul(Box::new(b.into()), Box::new(c.into()))),
            ),
            Expr::Neg(Box::new(Expr::Const(3.0))),
            Expr::Neg(Box::new(Expr::Add(Box::new(a.into()), Box::new(b.into())))),
        ];
        for expr in &expressions {
            assert_eq!(
                &parse_print(expr, &table),
                expr,
                "text: {}",
                expr.to_text(&table)
            );
        }
        assert_eq!(expressions[0].to_text(&table), "a + (b + c)");
        assert_eq!(expressions[1].to_text(&table), "a*(b*c)");
        assert_eq!(expressions[4].to_text(&table), "-(3)");
    }

    #[test]
    fn canonical_numbers_round_trip_edge_values() {
        let table = table();
        for value in [
            0.0,
            -0.0,
            8.0,
            -3.0,
            1.0 / 3.0,
            0.30000000000000004,
            1e-9,
            1e18,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
        ] {
            let expr = Expr::Const(value);
            if value.is_finite() {
                assert_eq!(
                    parse_print(&expr, &table),
                    expr,
                    "text: {}",
                    expr.to_text(&table)
                );
            } else {
                let text = expr.to_text(&table);
                let error = super::super::syntax::parse_expr(&text, &table).unwrap_err();
                assert_eq!(error.kind, super::super::syntax::ParseErrorKind::NonFinite);
            }
        }
    }

    #[test]
    fn dangling_parameter_prints_a_non_rebinding_marker() {
        let id = crate::rig::ParamId::new();
        let text = Expr::Param(id).to_text(&ParamTable::new());
        assert_eq!(text, format!("?{}", &id.0.simple().to_string()[..8]));
        assert_eq!(
            super::super::syntax::parse_expr(&text, &ParamTable::new())
                .unwrap_err()
                .kind,
            super::super::syntax::ParseErrorKind::UnexpectedChar
        );
    }

    #[test]
    fn generated_expressions_round_trip_ten_thousand_times() {
        let table = table();
        let ids: Vec<_> = table.keys().copied().collect();
        let mut rng = XorShift64(0x2d35_8dcc_aa6c_78a5);
        for _ in 0..10_000 {
            let expr = generate(&mut rng, &ids, 6);
            assert_eq!(
                parse_print(&expr, &table),
                expr,
                "text: {}",
                expr.to_text(&table)
            );
            let text = expr.to_text(&table);
            let printed_again = super::super::syntax::parse_expr(&text, &table)
                .unwrap()
                .expr
                .to_text(&table);
            assert_eq!(printed_again, text, "text: {text}");
        }
    }

    #[test]
    fn all_fixture_and_template_expressions_round_trip() {
        let mut rigs = vec![duplo10(), two_leg_bridle(10_000.0, 10.0, 8.0)];
        rigs.extend(template_rigs());
        for rig in rigs {
            check_serialized_exprs(&serde_json::to_value(&rig).unwrap(), &rig);
        }
    }

    fn check_serialized_exprs(value: &Value, rig: &Rig) {
        match value {
            Value::Array(values) => {
                for value in values {
                    check_serialized_exprs(value, rig);
                }
            }
            Value::Object(fields) => {
                let is_expr = fields.len() == 1
                    && ["Const", "Param", "Neg", "Add", "Sub", "Mul", "Div"]
                        .iter()
                        .any(|variant| fields.contains_key(*variant));
                if is_expr {
                    let expr: Expr = serde_json::from_value(value.clone()).unwrap();
                    let printed = expr.to_text(&rig.params);
                    assert_eq!(
                        super::super::syntax::parse_expr(&printed, &rig.params)
                            .unwrap_or_else(|error| panic!("{printed}: {error:?}"))
                            .expr,
                        expr,
                        "rig {}, expression {printed}",
                        rig.name
                    );
                    return;
                }
                for value in fields.values() {
                    check_serialized_exprs(value, rig);
                }
            }
            _ => {}
        }
    }

    fn template_rigs() -> Vec<Rig> {
        let mut cases = Vec::new();
        for (name, count, length) in [
            ("single manual", 2, 10.0),
            ("two layer spreader", 4, 12.0),
            ("impossible geometry", 2, 10.0),
            ("database fixture", 2, 12.0),
            ("two over four", 4, 0.0),
            ("corpus shackles", 4, 12.0),
            ("legacy zero length", 2, 0.0),
        ] {
            let pick = Pick::new(uuid::Uuid::nil(), name, 10_000.0);
            let layer = SlingLayer {
                pick_id: pick.id,
                layer_index: 0,
                size: 5,
                hitch: Hitch::Vertical,
                angle_deg: 60.0,
                sling_count: count,
                sling_length_ft: length,
                pick_spacing_ft: Some(8.0),
                pick_width_ft: None,
                spreader_span_ft: None,
                apex_shackle: None,
                leg_shackle: None,
                spreader_id: None,
                spreader_wll_lbs: None,
                tare_lbs: 0.0,
            };
            cases.push(crate::rig::from_layers(&pick, &[layer], &[]));
        }
        let bar = SavedSpreader::new("Test", "Bar", 40_000, 200.0);
        let pick = Pick::new(uuid::Uuid::nil(), "with spreader", 20_000.0);
        let mut layer = SlingLayer {
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
            apex_shackle: Some("1-1/4".into()),
            leg_shackle: Some("1".into()),
            spreader_id: Some(bar.id),
            spreader_wll_lbs: None,
            tare_lbs: 25.0,
        };
        layer.pick_id = pick.id;
        let bottom = SlingLayer {
            pick_id: pick.id,
            layer_index: 1,
            size: 5,
            hitch: Hitch::Vertical,
            angle_deg: 90.0,
            sling_count: 4,
            sling_length_ft: 10.0,
            pick_spacing_ft: Some(10.0),
            pick_width_ft: None,
            spreader_span_ft: None,
            apex_shackle: None,
            leg_shackle: None,
            spreader_id: None,
            spreader_wll_lbs: None,
            tare_lbs: 0.0,
        };
        cases.push(crate::rig::from_layers(&pick, &[layer, bottom], &[bar]));
        cases
    }

    struct XorShift64(u64);

    impl XorShift64 {
        fn next(&mut self) -> u64 {
            let mut value = self.0;
            value ^= value << 13;
            value ^= value >> 7;
            value ^= value << 17;
            self.0 = value;
            value
        }
    }

    fn generate(rng: &mut XorShift64, ids: &[crate::rig::ParamId], depth: u8) -> Expr {
        if depth == 0 || rng.next() % 5 == 0 {
            return match rng.next() % 3 {
                0 => Expr::Param(ids[(rng.next() as usize) % ids.len()]),
                1 => Expr::Const(match rng.next() % 8 {
                    0 => -0.0,
                    1 => 1.0 / 3.0,
                    2 => 1e-9,
                    3 => 1e18,
                    _ => (rng.next() % 2000) as f64 / 17.0 - 50.0,
                }),
                _ => Expr::Const((rng.next() % 32) as f64),
            };
        }
        let operation = rng.next() % 5;
        let left = Box::new(generate(rng, ids, depth - 1));
        let right = Box::new(if operation == 4 {
            Expr::Const(1.0 + (rng.next() % 5) as f64)
        } else {
            generate(rng, ids, depth - 1)
        });
        match operation {
            0 => Expr::Neg(left),
            1 => Expr::Add(left, right),
            2 => Expr::Sub(left, right),
            3 => Expr::Mul(left, right),
            _ => Expr::Div(left, right),
        }
    }
}
