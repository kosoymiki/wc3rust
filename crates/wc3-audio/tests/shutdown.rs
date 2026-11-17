//! Process-exit regression: the render thread used to outlive the engine and
//! `Drop` joined it without a timeout, so every heavy test binary that opened
//! the SDL backend hung at exit (rc=124 at 1800 s on `playable_loop`).
//!
//! This spawns the `audio_shutdown_probe` binary, which plays one voice and
//! drops the engine, and requires the process to exit on its own with both
//! `H2_AUDIO=null` and the real backend.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const PROBE: &str = env!("CARGO_BIN_EXE_audio_shutdown_probe");

fn run_probe(env: &[(&str, &str)], timeout: Duration) -> (String, std::process::ExitStatus, Duration) {
    let started = Instant::now();
    let mut child = Command::new(PROBE)
        .envs(env.iter().copied())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn audio_shutdown_probe");
    loop {
        if let Some(status) = child.try_wait().expect("wait probe") {
            let output = child.wait_with_output().expect("probe output");
            return (
                String::from_utf8_lossy(&output.stdout).into_owned(),
                status,
                started.elapsed(),
            );
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "audio_shutdown_probe did not exit within {timeout:?} (env {env:?}); \
                 Drop is blocking on the audio backend"
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn assert_clean_exit(env: &[(&str, &str)], timeout: Duration) {
    let (stdout, status, wall) = run_probe(env, timeout);
    assert!(
        status.success(),
        "probe exited with {status:?} (env {env:?}); stdout={stdout}"
    );
    assert!(stdout.contains("PROBE_BACKEND"), "backend marker: {stdout}");
    assert!(stdout.contains("PROBE_DROPPED"), "drop marker: {stdout}");
    assert!(stdout.contains("PROBE_EXIT"), "exit marker: {stdout}");
    let dropped_ms = stdout
        .lines()
        .find_map(|line| line.strip_prefix("PROBE_DROPPED ms="))
        .and_then(|ms| ms.parse::<u128>().ok())
        .unwrap_or_else(|| panic!("drop timing missing: {stdout}"));
    println!(
        "SHUTDOWN_PROBE env={env:?} dropped_ms={dropped_ms} wall_ms={}",
        wall.as_millis()
    );
    // `Drop` must never block longer than the bounded join + close budget.
    assert!(
        dropped_ms <= 250,
        "Drop took {dropped_ms} ms (budget 250 ms)"
    );
}

#[test]
fn audio_engine_drop_exits_process_with_null_backend() {
    assert_clean_exit(&[("H2_AUDIO", "null")], Duration::from_secs(10));
}

#[test]
fn audio_engine_drop_exits_process_with_platform_backend() {
    assert_clean_exit(&[("H2_AUDIO", "")], Duration::from_secs(20));
}
