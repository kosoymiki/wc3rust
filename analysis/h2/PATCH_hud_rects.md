# Patch: publish HUD touch geometry (device harness contract)

Apply in `crates/wc3-game/src/game.rs` `rebuild_layout()`, immediately after
`self.mobile_layout = Some(mobile);` (~line 1548). Fires once per layout rebuild,
never per frame.

```rust
// Automation/support contract: publish the touch geometry once per layout
// rebuild so the device evidence harness taps exact slots instead of guessing
// pixels (`GAME_HUD_RECTS` is consumed by `analysis/tech-lead/device_pass.sh`).
{
    let m = self.mobile_layout.as_ref().expect("just stored");
    let slots: String = m
        .card_slots
        .iter()
        .enumerate()
        .map(|(index, rect)| format!("{index}@{:.0},{:.0}", rect.centre().x, rect.centre().y))
        .collect::<Vec<_>>()
        .join(" ");
    crate::dev_log!(
        "GAME_HUD_RECTS scale={:.2} card={:.0},{:.0},{:.0},{:.0} tile={:.1} slots=[{}] \
         army={:.0},{:.0} minimap={:.0},{:.0},{:.0},{:.0}",
        m.scale,
        m.card.x, m.card.y, m.card.w, m.card.h,
        m.cell,
        slots,
        m.army.centre().x, m.army.centre().y,
        m.minimap.x, m.minimap.y, m.minimap.w, m.minimap.h
    );
}
```

Why: `analysis/tech-lead/device_pass.sh` needs slot centres to (a) open a build
menu for the ghost frame (#10), (b) select the army panel (#7), (c) tap the
minimap to reach the water edge (#14). Guessing pixel coordinates caused two
mis-taps today (`PLACE_TAP` at coordinates the harness never sent).
