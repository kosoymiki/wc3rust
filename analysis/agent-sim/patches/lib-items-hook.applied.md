Item/shop runtime hooks for `crates/wc3-sim/src/lib.rs`.

`lib.rs` is owner-locked by ledgers 446/443 (they edit it concurrently), so the
item/shop lane does not touch it.  The new runtime lives in
`crates/wc3-sim/src/{shops.rs,items.rs}` and is compiled into the `data` module
through `include!` in `shops_gen.rs` / `items_gen.rs`; no module declaration is
needed.  Only these three call sites are required for automatic tick/death
behaviour:

1. `Sim::step` — run the per-tick item maintenance after shop restock
   (`crates/wc3-sim/src/lib.rs`, `pub fn step`, next to `advance_shops`):

         self.advance_shops();
+        self.items_tick();

2. `Sim::kill_unit` — drop the corpse's backpack immediately for script/JASS
   kills (`crates/wc3-sim/src/lib.rs`, `pub fn kill_unit` end):

         self.clear_buffs(id);
         self.creep_drop(index);
+        self.drop_items_on_death(id);

3. Combat death — the same drop in `advance_combat`'s death block
   (`crates/wc3-sim/src/lib.rs`, `if died { ... }` next to `creep_drop`):

             self.clear_buffs(to);
             self.creep_drop(target_index);
+            self.drop_items_on_death(to);

`items_tick` itself also drops dead carriers, so hook (1) alone is sufficient
for death-by-combat; hooks (2)/(3) are the exact-tick version.  The tests in
`crates/wc3-sim/tests/items_shops.rs` drive `items_tick` directly, so the
mechanic is verified before the hook lands; the hook only schedules it.

Verification after applying:

    scripts/h2-cargo test -p wc3-sim --test items_shops --offline

# APPLIED 2026-11-16 by ledger 460 — do not re-apply.
# Wiring: lib.rs:1368 items_tick() in step; lib.rs:3240 drop_items_on_death to; lib.rs:6242 kill_unit.
# Tests: crates/wc3-sim/tests/items_hook.rs (4/4).
