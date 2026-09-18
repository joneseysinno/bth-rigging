//! Parameter, expression, tolerance, and source.
//!
//! Step: 1
//! Theory: roadmap Step 1 — parametric coordinates.
//! Inputs: authored expressions and sources.
//! Outputs: Parameter values ready for eval.
//! Must not depend on: solve, UI, dioxus.

use std::collections::{BTreeSet, HashSet};
use std::ops::{Add, Div, Mul, Neg, Sub};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::RigError;
use super::RigErrorKind;
use super::id::ParamId;

/// Insertion-ordered parameter table. UUID keys; name lookup is a scan / index
/// rebuilt on load.
pub type ParamTable = IndexMap<ParamId, Param>;

/// Base quantity of a parameter. Base units are ft, lb, deg.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Quantity {
    Length,
    Angle,
    Weight,
    Ratio,
    Count,
}

impl Quantity {
    pub fn unit_label(self) -> &'static str {
        match self {
            Self::Length => "ft",
            Self::Angle => "deg",
            Self::Weight => "lb",
            Self::Ratio => "1",
            Self::Count => "ea",
        }
    }
}

/// Where a parameter value came from. `Assumed` tags any output as provisional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ParamSource {
    Drawing,
    Catalog,
    Measured,
    Assumed,
    Derived,
}

/// Named parameter with tolerance and source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Param {
    pub id: ParamId,
    /// Unique, snake_case name such as `"s12"` or `"span_A"`.
    pub name: String,
    pub quantity: Quantity,
    /// Nominal value in the quantity's base unit.
    pub nominal: f64,
    /// Minus tolerance, ≥ 0, in the same unit.
    pub minus: f64,
    /// Plus tolerance, ≥ 0, in the same unit.
    pub plus: f64,
    pub source: ParamSource,
    pub note: String,
    /// When `source` is `Derived`, evaluated instead of `nominal`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expr: Option<Expr>,
}

impl Param {
    pub fn new(name: impl Into<String>, quantity: Quantity, nominal: f64) -> Self {
        Self {
            id: ParamId::new(),
            name: name.into(),
            quantity,
            nominal,
            minus: 0.0,
            plus: 0.0,
            source: ParamSource::Assumed,
            note: String::new(),
            expr: None,
        }
    }

    pub fn min(&self) -> f64 {
        self.nominal - self.minus
    }

    pub fn max(&self) -> f64 {
        self.nominal + self.plus
    }

    pub fn is_assumed(&self) -> bool {
        self.source == ParamSource::Assumed
    }
}

/// Arithmetic expression over parameters. Built through the builder API, not parsed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expr {
    Const(f64),
    Param(ParamId),
    Neg(Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
}

impl Expr {
    pub fn c(v: f64) -> Self {
        Self::Const(v)
    }

    pub fn p(id: ParamId) -> Self {
        Self::Param(id)
    }

    /// Parameter ids referenced anywhere in this expression, in sorted order.
    pub fn params(&self) -> BTreeSet<ParamId> {
        let mut out = BTreeSet::new();
        self.collect_params(&mut out);
        out
    }

    fn collect_params(&self, out: &mut BTreeSet<ParamId>) {
        match self {
            Self::Const(_) => {}
            Self::Param(id) => {
                out.insert(*id);
            }
            Self::Neg(a) => a.collect_params(out),
            Self::Add(a, b) | Self::Sub(a, b) | Self::Mul(a, b) | Self::Div(a, b) => {
                a.collect_params(out);
                b.collect_params(out);
            }
        }
    }

    pub fn eval(&self, table: &ParamTable) -> Result<f64, RigError> {
        Ok(self.eval_qty(table, &mut Vec::new())?.0)
    }

    /// Evaluate with quantity tracking. `Const` is untyped (`None`).
    pub fn eval_qty(
        &self,
        table: &ParamTable,
        stack: &mut Vec<ParamId>,
    ) -> Result<(f64, Option<Quantity>), RigError> {
        match self {
            Self::Const(v) => {
                if !v.is_finite() {
                    return Err(RigError::new(
                        RigErrorKind::BadValue,
                        format!("non-finite constant {v}"),
                    ));
                }
                Ok((*v, None))
            }
            Self::Param(id) => eval_param(*id, table, stack),
            Self::Neg(a) => {
                let (v, q) = a.eval_qty(table, stack)?;
                Ok((-v, q))
            }
            Self::Add(a, b) => {
                let (av, aq) = a.eval_qty(table, stack)?;
                let (bv, bq) = b.eval_qty(table, stack)?;
                Ok((av + bv, merge_add(aq, bq)?))
            }
            Self::Sub(a, b) => {
                let (av, aq) = a.eval_qty(table, stack)?;
                let (bv, bq) = b.eval_qty(table, stack)?;
                Ok((av - bv, merge_add(aq, bq)?))
            }
            Self::Mul(a, b) => {
                let (av, aq) = a.eval_qty(table, stack)?;
                let (bv, bq) = b.eval_qty(table, stack)?;
                Ok((av * bv, merge_mul(aq, bq)?))
            }
            Self::Div(a, b) => {
                let (av, aq) = a.eval_qty(table, stack)?;
                let (bv, bq) = b.eval_qty(table, stack)?;
                if bv == 0.0 {
                    return Err(RigError::new(
                        RigErrorKind::BadValue,
                        "division by zero in expression",
                    ));
                }
                Ok((av / bv, merge_div(aq, bq)?))
            }
        }
    }

    /// True when any referenced parameter is `Assumed` (including derived chains).
    pub fn is_assumed(&self, table: &ParamTable) -> bool {
        self.params().iter().any(|id| {
            table
                .get(id)
                .map(|p| p.is_assumed() || p.expr.as_ref().is_some_and(|e| e.is_assumed(table)))
                .unwrap_or(false)
        })
    }
}

fn eval_param(
    id: ParamId,
    table: &ParamTable,
    stack: &mut Vec<ParamId>,
) -> Result<(f64, Option<Quantity>), RigError> {
    if stack.contains(&id) {
        let names: Vec<String> = stack
            .iter()
            .chain(std::iter::once(&id))
            .map(|p| {
                table
                    .get(p)
                    .map(|x| x.name.clone())
                    .unwrap_or_else(|| p.to_string())
            })
            .collect();
        return Err(RigError::new(
            RigErrorKind::ParamCycle,
            format!("parameter cycle: {}", names.join(" → ")),
        ));
    }
    let param = table.get(&id).ok_or_else(|| {
        RigError::new(
            RigErrorKind::UnknownParam,
            format!("unknown parameter {id}"),
        )
    })?;
    if let Some(expr) = &param.expr {
        stack.push(id);
        let (v, q) = expr.eval_qty(table, stack)?;
        stack.pop();
        let q = merge_add(q, Some(param.quantity))?;
        Ok((v, q))
    } else {
        if !param.nominal.is_finite() {
            return Err(RigError::new(
                RigErrorKind::BadValue,
                format!("parameter '{}' is not finite", param.name),
            ));
        }
        Ok((param.nominal, Some(param.quantity)))
    }
}

fn merge_add(a: Option<Quantity>, b: Option<Quantity>) -> Result<Option<Quantity>, RigError> {
    match (a, b) {
        (None, q) | (q, None) => Ok(q),
        (Some(x), Some(y)) if x == y => Ok(Some(x)),
        (Some(x), Some(y)) => Err(RigError::new(
            RigErrorKind::QuantityMismatch,
            format!("cannot add {x:?} to {y:?}"),
        )),
    }
}

fn merge_mul(a: Option<Quantity>, b: Option<Quantity>) -> Result<Option<Quantity>, RigError> {
    match (a, b) {
        (None, q) | (q, None) => Ok(q),
        (Some(Quantity::Ratio), q) | (q, Some(Quantity::Ratio)) => Ok(q),
        (Some(Quantity::Count), q) | (q, Some(Quantity::Count)) => Ok(q),
        (Some(x), Some(y)) if x == y => Err(RigError::new(
            RigErrorKind::QuantityMismatch,
            format!("cannot multiply {x:?} by {y:?}"),
        )),
        (Some(x), Some(y)) => Err(RigError::new(
            RigErrorKind::QuantityMismatch,
            format!("cannot multiply {x:?} by {y:?}"),
        )),
    }
}

fn merge_div(a: Option<Quantity>, b: Option<Quantity>) -> Result<Option<Quantity>, RigError> {
    match (a, b) {
        (q, None) => Ok(q),
        (None, Some(Quantity::Ratio) | Some(Quantity::Count)) => Ok(None),
        (None, Some(_)) => Ok(None),
        (Some(x), Some(y)) if x == y => Ok(Some(Quantity::Ratio)),
        (Some(q), Some(Quantity::Ratio) | Some(Quantity::Count)) => Ok(Some(q)),
        (Some(x), Some(y)) => Err(RigError::new(
            RigErrorKind::QuantityMismatch,
            format!("cannot divide {x:?} by {y:?}"),
        )),
    }
}

macro_rules! bin_op {
    ($trait:ident, $method:ident, $ctor:ident) => {
        impl $trait for Expr {
            type Output = Expr;
            fn $method(self, rhs: Expr) -> Expr {
                Expr::$ctor(Box::new(self), Box::new(rhs))
            }
        }
        impl $trait<f64> for Expr {
            type Output = Expr;
            fn $method(self, rhs: f64) -> Expr {
                Expr::$ctor(Box::new(self), Box::new(Expr::Const(rhs)))
            }
        }
        impl $trait<ParamId> for Expr {
            type Output = Expr;
            fn $method(self, rhs: ParamId) -> Expr {
                Expr::$ctor(Box::new(self), Box::new(Expr::Param(rhs)))
            }
        }
        impl $trait for ParamId {
            type Output = Expr;
            fn $method(self, rhs: ParamId) -> Expr {
                Expr::$ctor(Box::new(Expr::Param(self)), Box::new(Expr::Param(rhs)))
            }
        }
        impl $trait<Expr> for ParamId {
            type Output = Expr;
            fn $method(self, rhs: Expr) -> Expr {
                Expr::$ctor(Box::new(Expr::Param(self)), Box::new(rhs))
            }
        }
        impl $trait<f64> for ParamId {
            type Output = Expr;
            fn $method(self, rhs: f64) -> Expr {
                Expr::$ctor(Box::new(Expr::Param(self)), Box::new(Expr::Const(rhs)))
            }
        }
    };
}

bin_op!(Add, add, Add);
bin_op!(Sub, sub, Sub);
bin_op!(Mul, mul, Mul);
bin_op!(Div, div, Div);

impl Neg for Expr {
    type Output = Expr;
    fn neg(self) -> Expr {
        Expr::Neg(Box::new(self))
    }
}

impl Neg for ParamId {
    type Output = Expr;
    fn neg(self) -> Expr {
        Expr::Neg(Box::new(Expr::Param(self)))
    }
}

impl Mul<ParamId> for f64 {
    type Output = Expr;
    fn mul(self, rhs: ParamId) -> Expr {
        Expr::Mul(Box::new(Expr::Const(self)), Box::new(Expr::Param(rhs)))
    }
}

impl Mul<Expr> for f64 {
    type Output = Expr;
    fn mul(self, rhs: Expr) -> Expr {
        Expr::Mul(Box::new(Expr::Const(self)), Box::new(rhs))
    }
}

impl Add<ParamId> for f64 {
    type Output = Expr;
    fn add(self, rhs: ParamId) -> Expr {
        Expr::Add(Box::new(Expr::Const(self)), Box::new(Expr::Param(rhs)))
    }
}

impl Add<Expr> for f64 {
    type Output = Expr;
    fn add(self, rhs: Expr) -> Expr {
        Expr::Add(Box::new(Expr::Const(self)), Box::new(rhs))
    }
}

impl From<f64> for Expr {
    fn from(v: f64) -> Self {
        Expr::Const(v)
    }
}

impl From<ParamId> for Expr {
    fn from(id: ParamId) -> Self {
        Expr::Param(id)
    }
}

/// Body-local coordinate, feet, each axis an expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Coord3 {
    pub x: Expr,
    pub y: Expr,
    pub z: Expr,
}

impl Coord3 {
    pub fn new(x: impl Into<Expr>, y: impl Into<Expr>, z: impl Into<Expr>) -> Self {
        Self {
            x: x.into(),
            y: y.into(),
            z: z.into(),
        }
    }

    pub fn origin() -> Self {
        Self::new(0.0, 0.0, 0.0)
    }

    pub fn eval(&self, table: &ParamTable) -> Result<[f64; 3], RigError> {
        Ok([
            self.x.eval(table)?,
            self.y.eval(table)?,
            self.z.eval(table)?,
        ])
    }

    pub fn params(&self) -> BTreeSet<ParamId> {
        let mut s = self.x.params();
        s.extend(self.y.params());
        s.extend(self.z.params());
        s
    }
}

/// One-at-a-time min / nominal / max of a scalar output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SweepResult {
    pub nominal: f64,
    pub min: f64,
    pub max: f64,
}

/// Evaluate `f` at nominal, then at each parameter's min and max, one at a time.
pub fn sweep<F>(table: &ParamTable, mut f: F) -> Result<SweepResult, RigError>
where
    F: FnMut(&ParamTable) -> Result<f64, RigError>,
{
    let nominal = f(table)?;
    let mut min = nominal;
    let mut max = nominal;
    let ids: Vec<ParamId> = table.keys().copied().collect();
    for id in ids {
        let param = table.get(&id).expect("id from keys");
        for bound in [param.min(), param.max()] {
            if (bound - param.nominal).abs() < 1e-15 {
                continue;
            }
            let mut trial = table.clone();
            if let Some(p) = trial.get_mut(&id) {
                p.nominal = bound;
            }
            let v = f(&trial)?;
            min = min.min(v);
            max = max.max(v);
        }
    }
    Ok(SweepResult { nominal, min, max })
}

/// Parameters that are referenced but missing from the table.
pub fn unknown_params(expr: &Expr, table: &ParamTable) -> Vec<ParamId> {
    expr.params()
        .into_iter()
        .filter(|id| !table.contains_key(id))
        .collect()
}

/// Walk derived-parameter expressions and report a cycle if one exists.
pub fn check_param_cycles(table: &ParamTable) -> Result<(), RigError> {
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for id in table.keys().copied() {
        visit_param(id, table, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn visit_param(
    id: ParamId,
    table: &ParamTable,
    visiting: &mut HashSet<ParamId>,
    visited: &mut HashSet<ParamId>,
) -> Result<(), RigError> {
    if visited.contains(&id) {
        return Ok(());
    }
    if !visiting.insert(id) {
        return Err(RigError::new(
            RigErrorKind::ParamCycle,
            format!("parameter cycle involving {id}"),
        ));
    }
    if let Some(param) = table.get(&id)
        && let Some(expr) = &param.expr
    {
        for dep in expr.params() {
            if !table.contains_key(&dep) {
                return Err(RigError::new(
                    RigErrorKind::UnknownParam,
                    format!("unknown parameter {dep} from '{}'", param.name),
                ));
            }
            visit_param(dep, table, visiting, visited)?;
        }
    }
    visiting.remove(&id);
    visited.insert(id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(params: &[Param]) -> ParamTable {
        params.iter().cloned().map(|p| (p.id, p)).collect()
    }

    #[test]
    fn eval_const_and_param() {
        let s12 = Param::new("s12", Quantity::Length, 8.0);
        let t = table(&[s12.clone()]);
        assert!((Expr::c(3.0).eval(&t).unwrap() - 3.0).abs() < 1e-12);
        assert!((Expr::p(s12.id).eval(&t).unwrap() - 8.0).abs() < 1e-12);
        let e = s12.id + s12.id;
        assert!((e.eval(&t).unwrap() - 16.0).abs() < 1e-12);
    }

    #[test]
    fn unknown_param_is_an_error() {
        let t = ParamTable::new();
        let err = Expr::p(ParamId::new()).eval(&t).unwrap_err();
        assert_eq!(err.code, RigErrorKind::UnknownParam);
    }

    #[test]
    fn derived_cycle_is_detected() {
        let mut a = Param::new("a", Quantity::Length, 1.0);
        let mut b = Param::new("b", Quantity::Length, 1.0);
        a.source = ParamSource::Derived;
        b.source = ParamSource::Derived;
        a.expr = Some(Expr::p(b.id));
        b.expr = Some(Expr::p(a.id));
        let t = table(&[a.clone(), b]);
        let err = Expr::p(a.id).eval(&t).unwrap_err();
        assert_eq!(err.code, RigErrorKind::ParamCycle);
        assert!(check_param_cycles(&t).is_err());
    }

    #[test]
    fn quantity_mismatch_on_add() {
        let len = Param::new("span", Quantity::Length, 10.0);
        let w = Param::new("wt", Quantity::Weight, 100.0);
        let t = table(&[len.clone(), w.clone()]);
        let err = (len.id + w.id).eval(&t).unwrap_err();
        assert_eq!(err.code, RigErrorKind::QuantityMismatch);
    }

    #[test]
    fn sweep_min_nominal_max() {
        let mut w = Param::new("load_weight", Quantity::Weight, 100.0);
        w.minus = 10.0;
        w.plus = 20.0;
        let id = w.id;
        let t = table(&[w]);
        let s = sweep(&t, |tab| Expr::p(id).eval(tab)).unwrap();
        assert!((s.nominal - 100.0).abs() < 1e-12);
        assert!((s.min - 90.0).abs() < 1e-12);
        assert!((s.max - 120.0).abs() < 1e-12);
    }

    #[test]
    fn params_collects_ids() {
        let a = ParamId::new();
        let b = ParamId::new();
        let e = (Expr::p(a) + 2.0) * Expr::p(b);
        let ids = e.params();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&a) && ids.contains(&b));
    }
}
