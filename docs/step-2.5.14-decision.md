# Step 2.5.14 decision — go-ahead for Cursor

**From AJ, 2026-09-30.** This picks up where you stopped. You were right to stop at the 2.5.14 gate. The release median was 11.5 ms (passes); the debug max was ~3.85 s (fails).

Read `docs/step-2.5-closeout.md` again first. All of its rules and its gate still apply. This file replaces its **2.5.14** section only. After 2.5.14, continue with 2.5.15 to 2.5.20 as written there.

## What we learned from 2.5.13

- The tolerance change was **not** the latency fix. On the realistic sweep the numbers were already 14.0 / 207 ms before it. The old 170 ms median came from the 17 ft sweep.
- All of the max comes from one edit: `load_weight` right after `s12 = 9'-0"`. Phase A runs to its 300 cap, then phase B converges.
- Working hypothesis: `soft_forces` clamps tension with `g.max(0.0)`, so a tension-only member sitting near zero stretch puts a kink in the soft energy. The finite-difference Hessian straddles that kink, so phase A makes tiny, accepted steps and never reaches the stop test.
- Debug is about 19× slower than release. The UI is used under `dx serve`, so the debug number counts.

## Rule change for this step

Rule 3's solver exception now covers **two** edits in `hang::soft_relax`:
1. the stop-test constant (already done), and
2. the stall exit in 2.5.14c.

Nothing else changes under `src/rig/solve/**`. That means no change to the `300` cap, `HangOptions`, `soft_forces`/`soft_energy`, phase B or the damping logic.

You may also add a `[profile.dev]` section to `Cargo.toml` (2.5.14b). No other `Cargo.toml` edits.

## 2.5.14a Confirm the outlier (no commit)

Before changing anything, run the outlier edit (`s12 = 9'-0"`, then the following `load_weight` edit from the benchmark) with temporary logging in `soft_relax`. For each iteration, log: `it`, `gmax / w`, the accepted `alpha`, `mu`, and the smallest `path_length - length` over the tension members, with that member's name.

Also record, from the **current** code, the outlier edit's result: hook load, `max_tension_lbs`, total iterations, and every member tension. You will compare against these in 2.5.14c.

Record in §13:
- whether the kink hypothesis holds: a member near zero stretch, with `gmax` stuck on a plateau instead of shrinking,
- or what you see instead.

**Remove the logging before any commit.** If the log shows something that doesn't match the hypothesis (for example `gmax` still dropping steadily at iteration 300), **STOP and ask AJ** before 2.5.14c.

## 2.5.14b Faster dev builds

In `Cargo.toml`, under the existing empty `[profile]` header, add:

```toml
[profile.dev]
opt-level = 1

[profile.dev.package."*"]
opt-level = 3
```

- `debug_assertions` stays on. It's controlled separately from `opt-level`, and it's what caught the duplicate-key crash. Don't turn it off.
- `wasm-dev`, `server-dev` and `android-dev` inherit from `dev`. Leave them alone.
- The first debug build will be slower, because `typst` and `dioxus` get optimized. That's a one-time cost.
- Check that `dx serve` still starts and the editor loads.

Rerun the debug benchmark **3 times** and record each median and max in §13:

```
cargo test --no-default-features --lib duplo10_edit_latency -- --ignored --nocapture
```

Commit: `step 2.5.14b: optimize dev profile (debug median X ms / max Y ms)`.

## 2.5.14c Phase-A stall exit

In `soft_relax`, track the best `gmax` so far and the iteration where it was set. Update it when `gmax < 0.5 * best` (only a halving counts as progress). Just after the existing stop test, add:

```rust
if it - best_it >= 10 && gmax <= 1e-3 * w {
    return it;
}
```

What this means: if phase A is already close (`gmax ≤ 1e-3·w`) and hasn't halved `gmax` in 10 iterations, return and let phase B's KKT Newton polish the result. When phase A is still far off, the guard doesn't fire, and it keeps the 300 cap it has today. That preserves phase A's job of picking the right basin.

**Verify:**
- Every golden, `eval_geometry`, `solve_layers` and solve test passes **unchanged**. If any changes or fails: **STOP and ask AJ.**
- The outlier edit gives the same answer as recorded in 2.5.14a: every member tension within `1e-6 × w_total`, and hook load within 0.5 lb. Its total iterations drop well below 302. Add this as a test in `session.rs`, with the 2.5.14a values as constants and a comment saying where they came from.
- Rerun the benchmark in release once and in debug 3×. Record all of the numbers in §13.

Commit: `step 2.5.14c: stop phase A on stall (release median X / max Y ms)`.

## 2.5.14d Threading decision

Use the **worst** of the 3 debug runs.

- **If the release median is ≤ 50 ms and the debug max is ≤ 250 ms:** remove the threaded path exactly as the original 2.5.14 describes. That means `apply_async`, `schedule_recompute`, `poll`, `pending`, `generation`, `RecomputeResult`, `Snapshot::solving`, their tests, the 16 ms poller and `poller_active`; then call `session.apply(edit)` inline. Commit: `step 2.5.14d: recompute inline (release median X ms, debug max Y ms)`.
- **Otherwise: STOP and ask AJ.** Report all of the numbers. Don't extend the threading.

## Then

Continue with 2.5.15 to 2.5.20 in `docs/step-2.5-closeout.md`.

In 2.5.20, §13 must also say:
- the round-1 root cause was the benchmark sweep, not the stop test,
- the 2.5.14a finding,
- the before/after latency for 2.5.14b and 2.5.14c separately.

## Report back

In addition to the closeout's report list, include:
1. the 2.5.14a log summary (is the hypothesis confirmed or not; which member),
2. the three debug runs after 2.5.14b and after 2.5.14c,
3. the outlier edit's iterations before and after the stall exit.
