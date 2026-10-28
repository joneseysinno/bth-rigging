# Step 2.5 close-out — instructions for Cursor

**Status going in (2026-09-30):** 2.5.1–2.5.11 are committed. HEAD does **not** build with `--features desktop`, because the last commit ("refactor: improve code formatting…") mangled `src/ui/pages/project.rs`. The working tree already has two uncommitted fixes: the `project.rs` repair and the duplicate-key crash fix in `rig_editor.rs` (rated items are keyed by index). The review findings are in `claude/step-2.5-review.md` (AJ's claude.ai project) and are summarized under each item below.

`docs/roadmap.md` marks 2.5 "✅ done". **That is premature.** Item 2.5.20 fixes it.

**Read first:** `docs/step-2.5-parser-param-table.md` (§2 grammar, §6 edit layer, §7 performance, §8.4 checklist, §11 decisions, §14 rules). Then `src/rig/session.rs`, `src/rig/solve/hang.rs` (`soft_relax` only), `src/rig/solve.rs` (`SolvedRig`), `src/rig/param.rs` (`merge_div`), `src/rig/param/syntax.rs` (`scan_number`, `scan_feet_inches`, `scan_unit_suffix`, `check_and_lower`), `src/rig/edit.rs` (`check_edited_param`, `edit_error`), `src/ui/components/param_table.rs` (`ParamRow`), and `src/ui/pages/rig_editor.rs`.

---

## Rules (same as §14, with one exception)

1. Work in the order below. **One item = one commit**, message `step 2.5.N: <what>`. Run the gate before each commit. **Never commit red.**
2. **Gate:**
   ```
   cargo fmt --check
   cargo test --no-default-features
   cargo check --features desktop
   cargo clippy --no-default-features --all-targets   # no NEW warnings vs baseline
   ```
3. Read-only stays as in §14 rule 2: `src/layers/**`, `src/checks/**` math, goldens, `tests/fixtures/*.json`, serde shapes, `SCHEMA_VERSION`, and the pick editor. **The one exception:** in 2.5.13 you may change the single stop-test constant in `hang::soft_relax`. Nothing else under `src/rig/solve/**`.
4. No new dependencies. No `mod.rs`. No `dioxus` imports outside `app*` and `ui*`.
5. **Do not run a formatter over files you didn't change.** Format-on-save and `dx fmt` broke HEAD last time. Use `cargo fmt` only. If `dx fmt` is set to run on save in Cursor, turn it off for this work.
6. Don't invent APIs. Read the real source and match it. Record any deviation in §13 of the 2.5 doc.

**Stop and ask AJ when:**
- any golden, `eval_geometry` or `solve_layers` test changes or fails
- the latency numbers in 2.5.13 miss the budget (see 2.5.14)
- the `merge_div` change in 2.5.16 breaks a fixture or template expression
- you finish 2.5.20 (hand over the §8.4 checklist)

---

## 2.5.12 Commit the working tree

The working tree already contains the `project.rs` repair and the `rig_editor.rs` key-by-index fix. Run the gate. If it passes, commit as `step 2.5.12: fix desktop build and duplicate rated-item keys`. If `cargo fmt --check` fails **only** in files the formatter commit touched, run `cargo fmt`, check the diff contains whitespace changes only, and include it in this commit.

## 2.5.13 Solver stop test and a realistic benchmark

**Finding:** `hang::soft_relax` returns only when `gmax <= 1e-9 * w`. After almost any edit it runs the full 300 iterations, and each iteration does `2·dof` finite-difference gradients. Phase B (KKT Newton, `force_rtol: 1e-10`) polishes the result afterwards anyway, so phase A only needs to get close.

**Change:** in `soft_relax`, change `1e-9 * w` to `1e-6 * w`. **That is the only solver edit.** Leave the `300` cap and `HangOptions` unchanged.

**Benchmark (`session.rs`, `duplo10_edit_latency`):**
- The current sweep pushes `s12` up to 17 ft. That's unrealistic and it doesn't converge. Sweep `s12` from `8'-0"` to `9'-3"` in 3″ steps, alternating with `load_weight` edits as now.
- Measure the time **including `apply`** (wall clock around `session.apply(..)`), not only `Snapshot.elapsed`.
- Assert that every edit's `SolvedRig.converged` is `true`.

**Verify:** every existing solve test passes unchanged. Record the release median/max in §13:
```
cargo test --release --no-default-features --lib duplo10_edit_latency -- --ignored --nocapture
```
Also run it in debug (no `--release`) and record that too. The UI is used under `dx serve` in debug.

## 2.5.14 Threading decision (gate)

Go by the 2.5.13 numbers:

- **If the release median is ≤ 50 ms and the debug max is ≤ ~250 ms:** remove the threaded path. Delete `apply_async`, `schedule_recompute`, `poll`, `pending`, `generation`, `RecomputeResult` and `Snapshot::solving`, plus their tests. In `rig_editor.rs`, delete the 16 ms poller and `poller_active`, and call `session.apply(edit)` inline. That also fixes the review finding that the page re-rendered every 16 ms. Keep `invalidate_pending` only if something still needs it (nothing should).
- **Otherwise: STOP and ask AJ.** Don't extend the threading. Report the numbers.

Commit as `step 2.5.14: recompute inline (median X ms)`.

## 2.5.15 Non-converged solves are not results

**Finding:** `recompute` in `session.rs` treats any `Ok(SolvedRig)` as good, even when `converged == false`. At `s12 = 17 ft` the headline showed a max tension of 376M lb.

**Change (in `recompute` only; solver untouched):**
- If `solved` is `Ok(s)` and `!s.converged`, treat it as a failed solve. `current_good` is `None`, so the headline falls back to last-good and `headline_is_last_good` is set exactly as it is for a solve error.
- Add a user-facing error. Use a `RigErrorKind` variant if one fits. Otherwise add `SolveNotConverged` with the message: `"solver did not converge (N iterations) — geometry may be infeasible; check lengths and spans"`. Put it where the editor already shows solve errors (`Snapshot.solved` as `Err(vec![..])` is the simplest, but keep the `SolvedRig` available if the UI wants to draw it later; if both are needed, add a field rather than lose data).
- Do not rate a non-converged solve (`rated = None`).

**Tests:** a session edit that forces non-convergence (the old `s12 = 17 ft` works) gives `headline_is_last_good == true`, no `rated`, and the error text. Then undo restores a good headline with no stale flag.

## 2.5.16 Fractional inches

**Finding:** `1/4"` evaluates to **3** (ft). The unit binds only to the `4`, and then `merge_div(None, Some(Length))` returns an untyped result instead of an error. So `s12 + 1/4"` silently adds 3 ft. `8'-6 1/2"` gives a misleading error. For rigging dimensions this is the most dangerous open bug.

**Two changes:**

**(a) Lexer: tape-measure fractions** (`scan_number` / `scan_feet_inches` in `syntax.rs`):
- `N/D` followed by a unit suffix lexes as **one** `Number` literal with value `N/D`. `N` and `D` are digit runs and there is **no whitespace around `/`**. Examples: `1/4"`, `3/8 in`, `1/2'`. Without a following unit, `/` is still division, so `12/2`, `s12/2` and `a/b` are unchanged.
- **Mixed number:** integer, **one space**, then a fraction with a unit, e.g. `6 1/2"` → 6.5 in. "number space number" is invalid today, so this can't break anything.
- **Inside ft-in:** `8'-6 1/2"` → 8 ft + 6.5 in, and `8'-0 3/4"` works too. Extend `scan_feet_inches` to accept an optional ` N/D` before the closing `"`. It stays **no whitespace** except the single space before the fraction.
- `D == 0` → `DivideByZero` pointing at the fraction.
- Update the grammar block in §2 of the 2.5 doc with the new `literal` forms.

**(b) Typing: `merge_div`** (`param.rs`):
- `(None, Some(Length | Weight | Angle))` → `QuantityMismatch` with the message `"cannot divide a plain number by a {quantity}"`. Keep `(None, Some(Ratio | Count)) → Ok(None)`.
- If any fixture or template expression now fails to type-check: **STOP and ask AJ.**

**Tests (in `syntax.rs`):**

| Input | Value (ft) | Quantity |
|---|---|---|
| `1/4"` | 0.25/12 | Length |
| `6 1/2"` | 6.5/12 | Length |
| `8'-6 1/2"` | 8 + 6.5/12 | Length |
| `8'-0 3/4"` | 8 + 0.75/12 | Length |
| `3/8 in` | 0.375/12 | Length |
| `12/2` | 6 | none |
| `s12/2` (Duplo10 table) | Div, unchanged AST | Length |
| `1/0"` | DivideByZero | — |
| `1/(4 in)` | QuantityMismatch at `/` | — |
| `s12 + 1/4"` | `Add(s12, Const(0.25/12))` | Length |

The 10,000-tree round-trip property test must still pass. The printer never emits units, so the fraction fold can't collide with printed text. If it fails, stop and report.

## 2.5.17 Value-cell drafts resync

**Finding:** in `ParamRow`, `use_signal(|| initial.clone())` runs only once per mount. After Undo, `s12` still shows `8'-6"` while `=` shows 8. A derived value typed as `s23 + 6 in` stays as typed instead of re-printing as `s23 + 0.5`. The name, tolerance and note drafts have the same problem.

**Change:** when the committed text for a field changes (value, name, −tol, +tol, note), reset that draft and clear its error. Use Dioxus 0.7's `use_effect(use_reactive!(|(initial,)| { value_draft.set(initial); value_error.set(None); }))`, or an equivalent that runs when the prop changes. **Don't write signals during render.**

**Keep:** when a commit is **refused**, the rig doesn't change and the bad text stays in the box with the red outline. This is what the resync-on-change approach does naturally.

Also: the Escape handler captures `initial` at mount. Make it read the current committed text.

**Verify manually** with `dx serve` in a **debug** build: edit s12, Undo, and check the box shows the old text; then check §8.4 step 3 re-prints.

## 2.5.18 Readable error text

**Finding:** refusals show the Debug code, e.g. "ParamCycle: parameter cycle: s23 → s12 → s23". The cause is `error.to_string()` in `edit.rs` (`check_edited_param` and friends), which formats as `{code:?}: {message}`.

**Change:** use `error.message` (the human text) when building `EditError.message`. Leave `RigError`'s `Display` alone; logs and tests can keep the code. Check `rig_editor.rs` lines that render `"{error}"` for `RigError`s in the same way, and show `.message` there too. Also replace the `{residual.kind:?}` render with a short human label.

**Test:** the existing cycle test also asserts `!error.message.contains("ParamCycle")`.

## 2.5.19 Headless flow test — §8.4 as code

New file `tests/param_editor_flow.rs`, using the public lib API only. It mirrors §8.4 so an agent can check the editor without clicking. One `#[test]` per step is fine; share a helper that builds the Duplo10 session.

1. `duplo10()` → 21 params; hook load 122,983 lb (±0.5); the rig is "Provisional" (match however the app decides that, e.g. `assumed_count > 0`; read the code).
2. `s12 = 8'-6"` → `nominal == 8.5`; governing angle changes; `delta()` is `Some` and non-zero.
3. `s12 = s23 + 6 in` → `s12` is `Derived`, and `expr.to_text()` is `"s23 + 0.5"`.
4. `s23 = s12` after step 3 → refused; message contains `s23 → s12 → s23` and not `ParamCycle`.
5. `span_B = span_A + load_weight` → refused with a parse error whose span points at `+`; the rig is unchanged (`==` the pre-edit clone).
6. Rename `s23` → `span_23` → `s12` prints `span_23 + 0.5`.
7. Delete `lug_gauge_G` → refused; `EditError.uses` is non-empty and lists node coordinates.
8. Three edits, then Undo ×3 → the rig `==` the original. Save/reload: if `store::rig` can save and load headlessly against a temp directory, use it; otherwise do a `serde_json` round-trip of `Rig` and assert equality, and note which you did.
9. Rig from pick: hook load from the template-built rig equals the `layers` hook load for the same pick (reuse whatever `solve_layers` already does if it covers this; don't duplicate).
10. Fractions: `s12 = 8'-6 1/2"` → `nominal ≈ 8 + 6.5/12`.
11. Non-convergence: `s12 = 17 ft` → last-good headline, error shown; Undo → clean.

If a step fails because the behavior is wrong (not the test), fix the lib and note it in §13. **Don't weaken the assertion.**

## 2.5.20 Docs and hand-off

- §13 of the 2.5 doc: update the as-built numbers (tests, latency release/debug, the threading decision) and delete the "threaded" deviation if 2.5.14 removed it. Add a line for each of 2.5.12–2.5.19.
- `docs/roadmap.md`: set the 2.5 row to **"🟡 awaiting AJ §8.4"** (not ✅) and replace the "Verified 2026-09-28" line with the new gate results.
- **Report only, don't fix:** list the Duplo10 rated items whose status isn't OK (24 shackles were "Over" in the review). For each, give owner, item, catalog pick, WLL and demand, so AJ can decide whether it's a catalog assignment problem or real. Put the list in §13.
- **STOP.** Hand AJ the §8.4 checklist, and add one line to it: *"Run under `dx serve` (debug build), since debug-only assertions caught the key crash."*

---

## Report back to AJ

When you stop, reply with:
1. The commits made (hash + message)
2. Gate output summary (test counts, fmt/check/clippy)
3. Latency: release median/max, debug median/max, and the threading decision
4. Anything you stopped on, and why
5. The non-OK rated items table
