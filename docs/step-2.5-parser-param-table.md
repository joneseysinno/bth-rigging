# Step 2.5: Expression parser and parameter-table editor

> **Status:** planned · **Owner:** AJ · **Implementer:** Copilot (agent mode) · **Applies to:** `bth-rigging` @ `bth-graph` branch
> **Prerequisite:** Steps 0–3 complete (`rig::param::Expr` + builder ops, `EvalRig`, `solve`, `rate`, `Scene::project_solved`).
> **Roadmap context:** Step 1 D2 and Step 2 D2-8 deferred the parser and the parameter table to this step. It lands just before the first screen that draws the graph ("screen work", Step 2 D2-1). Step 3 hand-off: *"The solve is fast enough to re-run on every edit."*

---

## 1. Goal

Let AJ **type** a parameter, like `s12 + 2*lug_gauge_G` or `8'-6"`, and see the rig re-evaluate, re-hang and re-rate.

Step 2.5 delivers:

1. **Parser**: text → the same `Expr` the builder API makes. It uses spans, and every error names a column.
2. **Printer**: `Expr` → canonical text. `parse(print(e)) == e` for every expression.
3. **Unit literals**: `6 in`, `8'-6"`, `12 kip`, `45°`, `50%`. They are type-checked and converted to base units (ft, lb, deg).
4. **Edit layer (lib, headless)**: `ParamEdit` commands, a where-used walker, a cycle guard, and `RigSession` with undo/redo. Each commit produces a fresh `EvalRig` → `SolvedRig` → `rate` snapshot.
5. **Parameter-table screen**: a rigs list on the project page and a `RigEditor` page. The page shows the table plus a live results panel (numbers only, no drawing).

### Non-goals

- **No new `Expr` variants.** No functions, no `^`. Existing persisted rigs are unaffected and `SCHEMA_VERSION` stays 1 (D2.5-1).
- **No stored expression text.** The AST is the source of truth, and text is always re-printed from it (D2.5-2).
- **No graph editing.** Node coordinates, component lengths, adjusters and placements appear read-only in "where used" (printed). Editing them belongs to the graph editor, later.
- **No drawing.** `Scene` rendering is the next item ("screen work"). This page shows numbers.
- **No change** to `layers`, the checks math, solver internals, the goldens, or the pick editor.

### Definition of done

- [ ] `parse_expr("s12 + 2*lug_gauge_G", &table)` `==` the builder's `s12 + 2.0 * g` (structural `==` on `Expr`)
- [ ] Round-trip: `parse(print(e)) == e` for 10 000 generated expressions, and for **every** `Expr` inside `duplo10()`, `two_leg_bridle()` and every template rig from the corpus
- [ ] Canonical: `print(parse(s))` is idempotent for every test input
- [ ] Every `ParseErrorKind` has a test asserting its **1-based character column**, including a multi-byte (`°`) case
- [ ] Quantity errors point at the operator that failed (`span_A + load_weight` → column of `+`)
- [ ] `RigSession::apply` on Duplo10: editing `s12` changes node positions and the governing angle, editing `load_weight` changes hook load by exactly Δ, and undo restores a `Rig` that is `==` to the original
- [ ] The where-used walker agrees with a serde-JSON scan of the rig (completeness test, §6.3)
- [ ] Edit latency on Duplo10 (apply + eval + solve + rate) is measured in release and recorded in §13, and it is under budget (§7.3)
- [ ] `RigEditor` route builds (`cargo check --features desktop`), and the manual checklist in §8.4 passes
- [ ] `cargo test --no-default-features` green; `cargo fmt --check` clean; 0 warnings; no new clippy findings
- [ ] No change to any golden (`tests/fixtures/*.json`), rig row shape, layer JSON, or the pick editor

---

## 2. Grammar

```
expr     := term  (('+' | '-') term)*            left-assoc
term     := unary (('*' | '/') unary)*           left-assoc
unary    := '-' unary | atom
atom     := literal | ident | '(' expr ')'
literal  := number unit?  |  ft_in
ft_in    := number "'" '-'? number '"'           NO whitespace inside
number   := digits ('.' digits?)? exp? | '.' digits exp?
exp      := ('e'|'E') ('+'|'-')? digits
unit     := ft | ' | in | " | lb | lbs | kip | deg | ° | %
ident    := [A-Za-z_][A-Za-z0-9_]*               case-sensitive
```

**Precedence matches Rust's operators exactly.** Unary minus binds tighter than `*`, so `-a*b` is `Mul(Neg(a), b)`, which is what `-a * b` builds in Rust. That is why parse can equal builder (D2.5-3).

| Unit | Quantity | Factor to base |
|---|---|---|
| `ft`, `'` | Length | 1 |
| `in`, `"` | Length | 1/12 |
| `lb`, `lbs` | Weight | 1 |
| `kip` | Weight | 1000 |
| `deg`, `°` | Angle | 1 |
| `%` | Ratio | 0.01 |

- A unit may be separated from its number by spaces (`6 in`, `6in`). A unit word is recognised **only directly after a number**. Anywhere else, `in` is an identifier. It is still disallowed as a parameter name (§6.4).
- **Feet-inches gotcha:** `8'-6"` (no spaces) is one literal, 8.5 ft. `8' - 6"` (spaces) is a subtraction, 7.5 ft. Both need a test, and the UI hint text shows both.
- `ton`, `kips`, `mm`, `m`, `kg` → `UnknownUnit`. `ton` gets a specific help message: "short or metric? use kip or lb".
- **Reserved, rejected with a clear error:** `name(` (function call), `^`, `**`, `%` used as an operator, and implicit multiplication (`2G`, `2(a)`). Help text: "functions aren't supported yet". See D2.5-6 for why this is more than a parser change.

---

## 3. Types and API

Two levels: a **spanned AST** that exists only inside the parser (for columns and quantity errors), lowered to the existing `Expr`.

```rust
// src/rig/param/syntax.rs   (lexer + parser + checker + lowering)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span { pub start: usize, pub end: usize }        // byte offsets into the source

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub span: Span,
    pub column: usize,          // 1-based, counted in chars (not bytes) — `°` is 2 bytes
    pub message: String,
    pub help: Option<String>,   // "did you mean `s12`?"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseErrorKind {
    Empty, UnexpectedChar, UnexpectedToken, UnexpectedEnd, UnclosedParen, BadNumber,
    UnknownName, UnknownUnit, Unsupported,          // functions, ^, implicit multiplication
    QuantityMismatch, WrongQuantity,                // WrongQuantity: result ≠ the field's quantity
    NonFinite, DivideByZero,                        // literal / 0 only; params are an eval-time error
}

impl ParseError {
    /// Two lines: the source, then `^^^` under the span. For tooltips and test output.
    pub fn caret(&self, src: &str) -> String;
}

pub struct Parsed {
    pub expr: Expr,
    pub quantity: Option<Quantity>,  // None = untyped (bare numbers only)
    pub is_constant: bool,           // references no parameter
    pub lints: Vec<Lint>,            // soft warnings, never block
}

pub fn parse_expr(src: &str, table: &ParamTable) -> Result<Parsed, ParseError>;
pub fn parse_for(src: &str, table: &ParamTable, expected: Quantity) -> Result<Parsed, ParseError>;

// src/rig/param/print.rs
impl Expr {
    pub fn display<'a>(&'a self, table: &'a ParamTable) -> impl fmt::Display + 'a;
    pub fn to_text(&self, table: &ParamTable) -> String { self.display(table).to_string() }
}
```

Implementation notes for Copilot:

- Make `merge_add` / `merge_mul` / `merge_div` in `param.rs` `pub(super)` and **reuse** them in the checker. Do not write a second set of quantity rules. Wrap their `RigError` into a `ParseError` carrying the operator's span.
- Name lookup scans `table.values()` by `name`. Build a `HashMap<&str, (ParamId, Quantity)>` once per parse.
- `UnknownName` help: use a case-insensitive exact match first (`S12` → `s12`), then the closest name by Levenshtein distance ≤ 2. Write the ~15-line function in the module; add no crate for it.
- **First error only** (D2.5-8). The parser stops at the first error.
- `Lint` for now is one lint: `BareLengthLooksLikeInches`, raised when a bare number > 60 in a Length field is almost certainly inches. More lints can come later.

---

## 4. Round-trip contract

The contract is `parse(print(e)) == e` structurally, for every finite `e`. Three places make this subtle.

**4.1 Negative constants.** The builder makes both `Const(-3)` (from the Rust literal `-3.0`) and `Neg(Const(3))` (from `-Expr::c(3.0)`). Rules:

| AST | Prints as | Parses back as |
|---|---|---|
| `Const(-3)` | `-3` | parser folds unary minus on a numeric literal → `Const(-3)` ✓ |
| `Neg(Const(3))` | `-(3)` | parens block the fold → `Neg(Const(3))` ✓ |
| `Neg(Param p)` | `-p` | `Neg(Param p)` ✓ |
| `Mul(Const(-2), p)` | `-2*p` | `-2` folds first (unary binds tighter) → ✓ |

The fold applies when the operand of a unary minus is a numeric literal, with or without a unit, and with or without a space (`- 2`). `-(2)` is never folded.

**4.2 Parentheses.** The printer emits the minimum parentheses:

- Child precedence < parent precedence → parens.
- The **right** child of `Sub` / `Div` at equal precedence → parens (`a - (b - c)`, `a/(b*c)`).
- The left child at equal precedence → no parens (left-assoc).
- The operand of `Neg` gets parens when it is `Add`/`Sub`/`Mul`/`Div`/`Const`: `-(a + b)`, `-(a*b)`, `-(3)`.

Note that `-(a*b)` needs parens because `-a*b` parses as `Mul(Neg a, b)`.

**4.3 Numbers.** Rust's `{}` for `f64` prints the shortest string that round-trips exactly (`0.30000000000000004`, `8`, `0.08333333333333333`). Use it. The exception is `|x| < 1e-6` or `|x| ≥ 1e15` (excluding 0), which use `{:e}`. The grammar accepts exponents, so both round-trip. `-0.0` prints `-0` and parses to `-0.0` (`==` holds either way). Non-finite constants print `nan` / `inf`, which the parser rejects as `NonFinite`. The generator excludes them.

**4.4 Spacing (canonical).** `a + b`, `a - b`, `a*b`, `a/b`, `-a`. This matches the roadmap example `s12 + 2*G`.

**4.5 Missing ids.** A `Param(id)` not in the table prints as `?<first 8 hex of uuid>`. That text fails to parse (`UnexpectedChar`), so a dangling reference is never silently re-bound to a different name.

**What the round-trip does NOT preserve:** literal units and constant arithmetic spelling. `s12 + 6 in` is stored as `Add(s12, Const(0.5))` and re-prints as `s12 + 0.5`. The table shows the evaluated value in ft-in beside it (§8.2), so the loss is visible and harmless (D2.5-5).

---

## 5. Quantity checking at parse time

The checker walks the spanned AST with the same rules `Expr::eval_qty` uses:

- A unit literal is typed (`6 in` → Length).
- A bare number is untyped.
- A parameter carries its `quantity`. A derived parameter's quantity is its declared `quantity`; its expression is **not** walked. Walking it would repeat the eval-time cycle guard.
- A failure reports the **operator's** span: `span_A + load_weight` → `QuantityMismatch` at column 8, "cannot add Length to Weight".
- `parse_for(.., expected)` then requires `quantity ∈ {None, Some(expected)}`, or else `WrongQuantity` over the whole span. A Length ÷ Length expression is Ratio, which is legal in a Ratio field and wrong in a Length field.

Lowering drops spans and units. The stored `Expr` is identical to the builder's.

---

## 6. Edit layer (lib, headless)

All editing logic lives in the lib so it is tested without Dioxus. The UI only renders and forwards text.

### 6.1 Commands

```rust
// src/rig/edit.rs
pub enum ParamEdit {
    Add       { name: String, quantity: Quantity, value: String },
    Rename    { id: ParamId, name: String },
    SetValue  { id: ParamId, text: String },
    SetTol    { id: ParamId, minus: String, plus: String },   // literals with units, ≥ 0; "" = 0
    SetSource { id: ParamId, source: ParamSource },
    SetNote   { id: ParamId, note: String },
    Delete    { id: ParamId },
}

pub enum ParamField { Name, Value, Minus, Plus, Source, Note, Row }

pub struct EditError {
    pub field: ParamField,
    pub parse: Option<ParseError>,   // present for text fields → the UI shows the caret
    pub message: String,
    pub uses: Vec<UseSite>,          // populated for a refused Delete
}

/// Pure: returns a new Rig; the input is untouched.
pub fn apply(rig: &Rig, edit: &ParamEdit) -> Result<Rig, EditError>;
```

**`SetValue` semantics** (the one cell that matters):

| Parsed text | Result |
|---|---|
| constant (no params), e.g. `8'-6"`, `2*12` | `nominal = eval`, `expr = None`. Source unchanged, **except** a `Derived` row becomes `Assumed` (D2.5-10) |
| references params | `expr = Some(e)`, `source = Derived`, `nominal = eval` (a cached display value) |

Both cases then run a **cycle guard**: build the trial table, run `check_param_cycles`, and evaluate the edited param. On `ParamCycle`, refuse with the name path from `eval_param` (`a → b → a`).

**`SetTol`:** refused on a `Derived` row ("tolerance comes from its inputs") (D2.5-11). Values must be ≥ 0 and of the param's quantity. The text `±x` is accepted as shorthand and fills both.

**`SetSource(Derived)`** without an expression is refused ("type an expression to derive this value"). Switching **away** from `Derived` keeps the last evaluated value as `nominal` and drops `expr`.

### 6.2 Where used

```rust
pub enum UseSite {
    Param   { id: ParamId },
    NodeCoord { node: NodeId, axis: char },
    BodyWeight { body: BodyId }, BodyCg { body: BodyId, axis: char }, Placement { body: BodyId },
    Component { member: MemberId, segment: usize, index: usize, field: &'static str }, // length, weight, stiffness_lb, adjust.min/max/setting
    Bearing { node: NodeId, field: &'static str },
}
pub fn uses(rig: &Rig, id: ParamId) -> Vec<UseSite>;
pub fn use_counts(rig: &Rig) -> HashMap<ParamId, usize>;
```

**Copilot: before you write this, find every `Expr`, `Coord3` and `RotExpr` field in `src/rig/`.** Run `grep -rn "Expr\b\|Coord3\|RotExpr" src/rig/*.rs` and include `bearing.rs`. Do not work from this list alone.

### 6.3 Completeness test (so a new field cannot be missed)

Serialize the rig to `serde_json::Value`. Count every string equal to a param's UUID, skipping the params table itself (its keys and each `Param.id`). Assert that count equals the total from `uses` summed over all params. Run it on Duplo10 and on every template rig. When someone later adds an `Expr` field and forgets the walker, this test fails.

### 6.4 Names

`Rename` / `Add` names must match `[A-Za-z_][A-Za-z0-9_]*` and be unique. They must not be a unit word (`ft in lb lbs kip deg`), `e`/`E` alone (it collides with exponent lexing, which needs a test), or a reserved function name (`sqrt min max hypot abs sin cos tan atan2`). Existing fixture names (`span_A`, `lug_gauge_G`, …) already comply, and a test asserts that for every fixture and template rig.

Because expressions store `ParamId`, **a rename needs no rewriting**. The next print shows the new name everywhere. A test covers this.

`Delete` is refused while `uses` is non-empty. The error lists the sites.

### 6.5 Session

```rust
// src/rig/session.rs
pub struct Snapshot {
    pub validation: ValidationReport,
    pub eval: Result<EvalRig, Vec<RigError>>,
    pub solved: Option<Result<SolvedRig, Vec<RigError>>>,   // None if eval failed
    pub rated: Option<Vec<Rated>>,
    pub headline: Headline,          // hook load, governing angle, max tension, max utilization, worst Status, assumed count
    pub elapsed: Duration,
}

pub struct RigSession { /* rig, undo: Vec<Rig>, redo: Vec<Rig>, snapshot, last_good: Option<Headline>, dirty: bool */ }

impl RigSession {
    pub fn new(rig: Rig) -> Self;                            // computes the first snapshot
    pub fn apply(&mut self, edit: ParamEdit) -> Result<(), EditError>;  // push undo, recompute
    pub fn undo(&mut self) -> bool;  pub fn redo(&mut self) -> bool;
    pub fn rig(&self) -> &Rig;  pub fn snapshot(&self) -> &Snapshot;
    pub fn delta(&self) -> Option<HeadlineDelta>;             // vs the previous good headline
    pub fn mark_saved(&mut self);  pub fn is_dirty(&self) -> bool;
}
```

- A failed `apply` changes nothing. The rig, undo stack and snapshot are untouched.
- An edit that makes eval or solve fail is still **accepted**, because the rig is valid data. The snapshot carries the errors and `last_good` keeps the previous headline so the UI can grey it out.
- **`HeadlineDelta`** is the change in hook load, governing angle, max tension and max utilization since the previous good snapshot. It answers "what did that edit just do?" directly, one parameter at a time.

---

## 7. Performance

### 7.1 When things run

| Event | Work |
|---|---|
| Keystroke (`oninput`) | `parse_for` only: microseconds, drives the red outline and caret |
| Commit (Enter / blur → `onchange`) | `apply` → validate → eval → solve → rate |

### 7.2 Measure first

Add an `#[ignore]` test, `duplo10_edit_latency`. It applies 20 `SetValue` edits to `s12` and `load_weight` and prints the median and max `Snapshot.elapsed`. Run it with:

```
cargo test --release --no-default-features --lib duplo10_edit_latency -- --ignored --nocapture
```

Record the numbers in §13.

### 7.3 Budget

- **Median ≤ 50 ms release:** run the recompute inline in the event handler.
- **Otherwise:** keep the parse and table update inline and run the solve on a `std::thread` with a generation counter; a stale result is dropped. Show "solving…" on the panel. Do **not** add `tokio` as a direct dependency for this. **Stop and ask AJ** before building the threaded path.

---

## 8. UI

### 8.1 Routes and project page

- New route: `#[route("/project/:project_id/rig/:rig_id")] RigEditor { project_id: Uuid, rig_id: Uuid }`.
- `ProjectPage` gets a **Rigs** section built from `store.list_rigs_for_project(id)`: name and an Open link.
  - **"Rig from pick…"** is a select of the project's picks. It loads the pick, its layers and the saved spreaders the same way `PickEditor` does, calls `rig::from_layers(..)` (**read its real signature in `src/rig/template.rs`**), sets `project_id`, saves, and navigates.
  - **"Add Duplo10 demo rig"**, only under `#[cfg(debug_assertions)]`, saves `rig::duplo10()` with this project's id.

### 8.2 `RigEditor` page (`src/ui/pages/rig_editor.rs`)

Layout: header (rig name, dirty dot, **Save**, **Undo**, **Redo**), then two columns: the parameter table (left, wide) and the results panel (right).

**Parameter table** (`src/ui/components/param_table.rs`), one row per `Param` in `IndexMap` order, **keyed by `ParamId`** so focus survives re-renders:

| Column | Content |
|---|---|
| Name | text input → `Rename` |
| Unit | `quantity.unit_label()` |
| Value | text input. Literal rows show `format_num(nominal)`; derived rows show `expr.to_text()` with an **fx** tag |
| = | evaluated value. Lengths also show ft-in (`8'-6 1/4"`, to 1/16") |
| −tol / +tol | text inputs; disabled on derived rows |
| Source | select of the `ParamSource` variants; `Assumed` rows get an amber tag |
| Used | `use_counts` number; clicking it expands a row listing `uses` with each expression printed |
| Note | text input |

- The Value cell's local draft is a `Signal<String>`. `oninput` parses it and shows a red outline plus a one-line `message` / `help` under the row, with the `^` caret in a monospace tooltip. `onchange` commits. **Esc** reverts to the printed value. A commit error keeps the draft and shows the error; the rig is unchanged.
- An **"+ Add parameter"** row at the bottom has name, a quantity select and a value.
- Placeholder hint in empty value cells: `8'-6"  ·  s12 + 2*lug_gauge_G  ·  12 kip`.

**Results panel** (from `Snapshot`), numbers only:

1. Banner: **Provisional**, N assumed inputs (when any), in the style of the `(assumed)` dims in `Scene`.
2. Validation errors and warnings.
3. Headline: hook load · load / gear / rigging · governing angle · max tension · worst status (OK / WARN / OVER). Each shows a `HeadlineDelta` chip after an edit (`+1,240 lb`, `−2.1°`).
4. Members: label, tension, angle, taut/slack, status.
5. Residuals (`EvalRig.residuals`), with the kind and message.
6. Rated items that are not OK: item, demand vs capacity, message.

If eval or solve fails: show the errors, and show the last good headline greyed out with "last good".

**Save** calls `store.save_rig(session.rig())`, then `mark_saved()`. Leaving the page while dirty shows a confirm (D2.5-14).

### 8.3 CSS

Add to `assets/app.css`, reusing existing classes (`page-shell`, `page-main`, `toolbar`, `btn`, `field-label`, `status-line`) wherever they fit. New classes: `.param-table`, `.param-row--error`, `.param-row--derived`, `.tag-assumed`, `.tag-fx`, `.delta-up`, `.delta-down`, `.results-panel`. **No colors in the lib** (Step 2 rule).

### 8.4 Manual checklist (AJ runs `dx serve`)

1. On a project, "Add Duplo10 demo rig" → Open → the table lists 21 params, and the panel shows hook load 122,983 lb and "Provisional".
2. Set `s12` to `8'-6"` → the `=` column shows 8.5 / 8'-6", the governing angle changes, and a Δ chip appears.
3. Set `s12` to `s23 + 6 in` → an fx tag appears and the value re-prints as `s23 + 0.5`.
4. Set `s23` to `s12` → refused with "cycle: s23 → s12 → s23".
5. Type `span_A + load_weight` into `span_B` → the red outline points at `+`, and nothing is committed.
6. Rename `s23` to `span_23` → `s12`'s value re-prints as `span_23 + 0.5`.
7. Delete `lug_gauge_G` → refused, listing the node coordinates that use it.
8. Undo ×3 → back to the original values. Save, leave, reopen → the values persist.
9. "Rig from pick…" on an existing pick → the hook load equals the pick editor's hook load.

---

## 9. Files

| File | Contents | Rough size |
|---|---|---:|
| `src/rig/param.rs` | `pub mod syntax; pub mod print;`, `merge_*` → `pub(super)`, re-exports | +15 |
| `src/rig/param/syntax.rs` | **new**: lexer, parser, spanned AST, checker, lowering, `ParseError`, `Lint`, tests | 650 |
| `src/rig/param/print.rs` | **new**: `Display` adaptor, precedence/parens, number formatting, round-trip tests + generator | 300 |
| `src/rig/edit.rs` | **new**: `ParamEdit`, `apply`, `uses`, `use_counts`, name rules, tests | 450 |
| `src/rig/session.rs` | **new**: `RigSession`, `Snapshot`, `Headline`, `HeadlineDelta`, latency test | 300 |
| `src/rig.rs` | `pub mod edit; pub mod session;` + re-exports | +10 |
| `src/format.rs` | `format_ft_in(ft, denom = 16)` + tests | +40 |
| `src/app.rs` | `RigEditor` route | +5 |
| `src/ui/pages.rs`, `ui/pages/rig_editor.rs` | **new** page | 350 |
| `src/ui/components.rs`, `ui/components/param_table.rs` | **new** table | 350 |
| `src/ui/pages/project.rs` | Rigs section | +90 |
| `assets/app.css` | table + panel styles | +80 |
| `docs/rig-model.md`, `docs/roadmap.md`, this doc | as-built | — |

Layering: `syntax`, `print`, `edit` and `session` import only `rig`, `checks` (through `rate`), `domain` and `format`. **No `dioxus` in the lib.** No `mod.rs`: a module is `x.rs` plus an `x/` directory, as in the rest of the crate. Every new file starts with the crate's header doc comment (`Step: 2.5`, `Theory:`, `Inputs:`, `Outputs:`, `Must not depend on:`).

---

## 10. Work order

Each item is **one commit** that builds and passes the gate. **Gate** (every item):

```
cargo fmt --check
cargo test --no-default-features
cargo clippy --no-default-features --lib --tests      # no NEW findings vs. before the item
```

UI items also run `cargo check --features desktop`.

**2.5.1 Lexer.** Tokens with byte spans: numbers (with exponent), identifiers, operators, parens, unit words, `'` `"` `°` `%`, and the no-whitespace `ft_in` compound. Char-column helper. Tests: every token kind; `8'-6"` is one token while `8' - 6"` is three; `45°` byte vs char columns; `1e-3`; `.5`; `5.`; `e` as an identifier vs an exponent (`2e3` vs `2 e`).

**2.5.2 Parser → spanned AST.** Precedence climbing per §2. Negative-literal fold (§4.1). All `Unsupported` cases. Tests: the table of text → AST shape, and every structural error kind with its column.

**2.5.3 Checker + lowering.** Name resolution with help text, unit conversion, quantity rules via `merge_*` with operator spans, `parse_for`, the one lint. Tests: `s12 + 2*lug_gauge_G` **== builder** on the Duplo10 table (the headline test); `8'-6"` → `Const(8.5)`; `12 kip` → 12000; `50%` → 0.5; `span_A + load_weight` → column 8; `S12` → help "did you mean `s12`?"; `2 ton` → specific help; `(s12` → `UnclosedParen` at column 1; `span_A/s12` is legal in a Ratio field and `WrongQuantity` in a Length field.

**2.5.4 Printer + round-trip.** §4 rules. A hand-rolled deterministic generator (xorshift64, fixed seed, depth ≤ 6) over `Const` (including negatives, integers, `1/3`, `1e-9`, `1e18`, `-0.0`), `Param` (from a 5-param table), and every operator. Add no `proptest` dependency. Tests: 10 000 cases `parse(print(e)) == e`; idempotence; every `Expr` in `duplo10()`, `two_leg_bridle()` and all corpus template rigs round-trips (walk them with the `uses` visitor from 2.5.5, or a local visitor for now). **If any builder-made expression fails to round-trip, stop and report it. Do not change the builder.**

**2.5.5 Where-used + names.** `uses`, `use_counts`, name validation, and the §6.3 JSON completeness test. Tests: counts on Duplo10; every fixture name is valid; unit words are rejected.

**2.5.6 Edit commands.** `apply` for every `ParamEdit`, per §6.1. Tests: literal and derived `SetValue`; a derived row set to a constant becomes `Assumed`; the cycle is refused with the name path; rename re-prints dependants; delete in use is refused with sites; tolerance on derived is refused; negative tolerance is refused; `±0.25 in`; `apply` is pure (the input `Rig` is `==` before and after).

**2.5.7 Session + latency.** `RigSession`, `Snapshot`, `Headline`, `HeadlineDelta`, undo/redo, dirty tracking. Tests: the Duplo10 `s12` and `load_weight` edits from the DoD; a failed edit changes nothing; an edit that breaks eval keeps `last_good`; undo/redo `==`. Add the `#[ignore]` latency test and **record the numbers in §13**. If the median is > 50 ms, **stop and ask AJ** (§7.3).

**2.5.8 `format_ft_in`.** Tests: 8.5 → `8'-6"`; 0.0520833 → `0'-5/8"`; 12.99999 → `13'-0"` (rounding carry); negatives.

**2.5.9 Route + project Rigs section.** The route, the list, "Rig from pick…", and the debug demo button. `cargo check --features desktop`.

**2.5.10 Rig editor page + parameter table.** §8.2 and the CSS. `cargo check --features desktop`. Then **stop and hand AJ the §8.4 checklist.**

**2.5.11 Docs.** Tick the DoD here and add as-built notes and deviations in §13. `docs/rig-model.md` "Expressions": replace "There is no parser" with the grammar summary, the units table and the round-trip contract. `docs/roadmap.md`: row 2.5 → ✅ done, plus a "Verified" line with the test counts.

---

## 11. Decisions

| # | Question | Decision |
|---|---|---|
| **D2.5-1** | New `Expr` variants (functions, power)? | **No.** The parser lowers to today's `Expr`. No schema change, no migration, and every stored rig is valid input. |
| **D2.5-2** | Store the expression text? | **No.** The AST (holding `ParamId`s) is the source of truth, and text is printed on demand. Renames are free and stored text can never disagree with what is evaluated. |
| **D2.5-3** | Operator precedence? | **Rust's**, including unary minus above `*`, so parse output equals builder output node for node. |
| **D2.5-4** | `-3` vs `-(3)`? | The parser folds unary minus on a literal into a negative `Const`, and the printer writes `Neg(Const)` as `-(x)`. Both builder shapes round-trip. |
| **D2.5-5** | Units in literals? | **Yes; they are checked at parse time and folded to base units.** The unit spelling is not preserved (`6 in` re-prints as `0.5`). The `=` column's ft-in display makes that visible. *Open for AJ:* preserving the display unit would need a persisted field, so it is deferred. |
| **D2.5-6** | Functions (`sqrt`, `^`)? | **Deferred and reserved.** `Quantity` has no Length², so `sqrt(a^2 + b^2)` can't be typed. When functions come, add **dimension-preserving** primitives (`hypot`, `min`, `max`, `abs`) as `Expr` variants, not `^`/`sqrt`. That needs a schema bump. |
| **D2.5-7** | Implicit multiplication (`2G`)? | **No.** It collides with unit suffixes (`2in`) and saves one keystroke. |
| **D2.5-8** | Report every error, or the first? | **The first.** One text field shows one error at a time. |
| **D2.5-9** | Column units? | **1-based characters** for display, **byte spans** internally. |
| **D2.5-10** | A constant typed into a derived row? | It becomes a plain value with source **`Assumed`**. The app never silently claims `Drawing`. |
| **D2.5-11** | Tolerance on derived rows? | **Disabled.** `sweep` perturbs `nominal`, which a derived param ignores. Its spread comes from its inputs. |
| **D2.5-12** | Where does edit logic live? | **The lib** (`rig::edit`, `rig::session`), headless-tested. The UI only renders and forwards text. |
| **D2.5-13** | Recompute when? | Parse on every keystroke; eval, solve and rate on commit. Threading only if §7.3 says so. |
| **D2.5-14** | Autosave or explicit Save? | **Explicit Save**, plus in-memory undo/redo and a dirty guard. Trying values is the point of the screen, so a what-if should not overwrite the rig. *AJ to confirm.* |
| **D2.5-15** | Draw the rig on this page? | **No.** It is the next item ("screen work"): `Scene::project_solved` → SVG. *AJ to confirm.* |
| **D2.5-16** | Edit node coordinates / component lengths here? | **No.** Read-only printed text in "Used". Editing them belongs to the graph editor. |

---

## 12. Risks

| Risk | Mitigation |
|---|---|
| A builder shape that can't round-trip (negative literals, `-0.0`, huge/tiny floats) | §4 rules; generator covers these; **every fixture `Expr`** is round-tripped; stop-and-report rule in 2.5.4 |
| Feet-inches `'-` read as subtraction (or the reverse) | One-token rule with no whitespace; tests for both spellings; hint text in the UI |
| Column off by one on `°` / `'` / `"` | Char columns computed from byte spans in one helper; multi-byte test |
| Where-used misses a field added later → deleting a param leaves a dangling ref | JSON completeness test (§6.3); `validate` still catches `DanglingRef` as a backstop |
| Solve on commit freezes the window | Measured in 2.5.7 against a budget; threaded path pre-designed; stop-and-ask gate |
| Table re-render steals focus mid-typing | Rows keyed by `ParamId`; drafts are local signals; commit on `onchange` only |
| Copilot "fixes" a failing golden or oracle | Hard rule: goldens and `layers` are read-only this step (§14) |

---

## 13. As-built notes

*(Copilot fills this in: deviations, latency numbers, test counts.)*

| Item | Value |
|---|---|
| Duplo10 edit latency, release (median / max) | 170.171 ms / 262.467 ms (20 edits; over the 50 ms budget) |
| Tests after 2.5 (lib / eval_geometry / golden / solve_layers) | 168 passed + 1 ignored / 10 / 3 / 8 |
| Deviations from this plan | The measured inline recompute exceeds §7.3's budget, so recompute uses `std::thread` with generation-guarded results. `Snapshot::solving` exposes the pending state; the editor panel renders it in 2.5.10. |

---

## 14. Instructions for Copilot

**Read first:** this doc; `docs/rig-model.md`; `src/rig/param.rs`; `src/rig/build.rs`; `src/rig/fixtures.rs`; `src/rig/template.rs`; `src/rig/solve.rs`; `src/rig/solve/rate.rs`; `src/ui/pages/project.rs`; `src/ui/pages/pick_editor.rs` (for the UI and store patterns).

**Rules:**

1. Work in the §10 order. One item = one commit. Run the gate before each commit. **Never commit red.**
2. **Read-only this step:** `src/layers/**`, `src/checks/**` math, `src/rig/solve/**` (except calling it), `tests/fixtures/*.json`, every `Serialize`/`Deserialize` type's shape, `SCHEMA_VERSION`, the pick editor.
3. **No new dependencies**, runtime or dev. Levenshtein, the RNG and ft-in formatting are hand-written.
4. No `mod.rs`. No `dioxus` or `rfd` import anywhere under `src/` except `app*` and `ui*`.
5. Don't invent signatures. When this doc sketches an API that already exists (`from_layers`, `EvalRig` accessors, `rate`, store calls), **read the source and use the real one**, and note the difference in §13.
6. Commit message format: `step 2.5.N: <what>`, followed by the attribution lines your environment requires.

**Stop and ask AJ when:**

- a fixture or builder expression fails to round-trip (2.5.4)
- median edit latency is > 50 ms release (2.5.7)
- any golden, `eval_geometry`, or `solve_layers` test changes or fails
- an existing param name in a fixture or template breaks §6.4
- 2.5.10 is built (hand over the §8.4 checklist)

---

## 15. Hand-off

**To screen work (next).** `RigEditor` already holds a `Snapshot` with a `SolvedRig`. The drawing is `Scene::project_solved(..)` → `fit()` → SVG, placed beside the table. The results panel shrinks to a legend.

**To Step 4 (crane).** The crane config gets its own parameters (boom length, radius, counterweight). They reuse this parser and table unchanged, because they are `Param`s.

**To Step 7 (tolerances).** The table already holds −tol / +tol. A per-row "sweep" button (min / max of hook load or governing tension over that one param) is a small addition on top of `param::sweep` + `RigSession`, and it is the seed of a tornado chart ranking which dimension matters most.
