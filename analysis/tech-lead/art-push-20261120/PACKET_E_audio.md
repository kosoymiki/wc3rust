# THICK GOAL — Agent E: Audio layer (ledger 589)

## Mission
Close operator bugs #4/#13 (peasant sounds like orc; every unit must have its own voice): per-def voices actually resolved and played.

## Evidence snapshot
- `analysis/agent-qa/operator-bugs/REPORT.md` #4/#13 partial: "unit_ack_set/attack/death per def id (8 defs -> 8 sets)" implemented but "per-def voice capture not captured"; underruns=0.
- `crates/wc3-audio/src/events.rs` resolves ack sets; `crates/wc3-game/src/game.rs` (Agent B owns) calls play_unit_ack.

## Goal + done-criteria
1. Add a deterministic log/hook that records which def id and which ack set plays on every unit command, so per-def distinctness is verifiable without audio recording. Named test `artE_perdef_voices` with >=8 defs resolving to distinct sets.
2. Fix resolution if two defs map to the same set. Verify `cargo test -p wc3-audio --offline` rc=0 + log lines in report.
3. Report table: def id -> ack set -> sound file, with observed log evidence.

## Invariants / READ_ONLY
- OWNER: `crates/wc3-audio/**`, `analysis/agent-audio/**`.
- NEVER edit: `crates/wc3-game/**`, other crates, `Cargo.toml`, `scripts/**`. Preserve uncommitted work. No stubs.

## Verification
Named test rc=0; log evidence; checkpoint 589 each phase, close 589 with h2_ids_json. Reply H2_PERSISTED + SESSION_CLOSED with ids.
