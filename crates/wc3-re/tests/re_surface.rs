//! Compile-time contract for the RE/engine surface consumed by the base.
//!
//! These are type pins, not runtime fixtures: they fail to compile if the
//! public signatures in `BASE.md` drift. Full behaviour tests are listed in
//! `analysis/tech-lead/merge-re.md` and run only after the base freeze.

use std::path::Path;

use wc3_re::ai::{AiCensus, scan_ai_dir};
use wc3_re::engine::{Engine, EngineConfig};
use wc3_re::host::HostServices;
use wc3_re::jass::{ParseError, Program, parse};
use wc3_re::scan::ScanReport;
use wc3_re::storm_api::{StormApi, StormValue};

#[test]
fn re_layer_public_signatures_are_stable() {
    let _: fn(&Path, &Path, u64) -> wc3_re::Result<ScanReport> = ScanReport::run;
    let _: fn(EngineConfig) -> wc3_re::Result<Engine> = Engine::boot;
    let _: fn(&mut HostServices, u32, &[i64]) -> i64 = HostServices::call;
    let _: fn(&mut StormApi, u32, &[StormValue]) -> wc3_re::Result<StormValue> =
        StormApi::call;
    let _: fn(&str) -> Result<Program, ParseError> = parse;
    let _: fn(&Path, u64) -> wc3_re::Result<AiCensus> = scan_ai_dir;
}
