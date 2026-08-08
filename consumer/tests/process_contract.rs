use std::time::Duration;

use ore_mcp_process::{run_bounded, run_truncating, ProcessError, ProcessLimits};

const EMIT: &str = env!("CARGO_BIN_EXE_emit");

fn limits(timeout: Duration) -> ProcessLimits {
    ProcessLimits::new(timeout, 1024, 1024).expect("test limits are valid")
}

#[tokio::test]
async fn external_consumer_captures_small_stdout_and_stderr() {
    let output = run_bounded(
        None,
        EMIT,
        &["17", "9", "0"],
        limits(Duration::from_secs(5)),
    )
    .await
    .expect("bounded child should succeed");

    assert!(output.success());
    assert_eq!(output.stdout, vec![b'o'; 17]);
    assert_eq!(output.stderr, vec![b'e'; 9]);
}

#[tokio::test]
async fn external_consumer_fails_fast_on_stdout_overflow() {
    let error = run_bounded(
        None,
        EMIT,
        &["2048", "0", "0"],
        limits(Duration::from_secs(5)),
    )
    .await
    .expect_err("stdout overflow must fail closed");

    assert!(matches!(error, ProcessError::StdoutTooLarge));
}

#[tokio::test]
async fn external_consumer_truncates_and_drains_both_streams() {
    let output = run_truncating(
        None,
        EMIT,
        &["2048", "3072", "0"],
        limits(Duration::from_secs(5)),
    )
    .await
    .expect("truncating capture should drain the child");

    assert!(output.success());
    assert_eq!(output.stdout.bytes, vec![b'o'; 1024]);
    assert_eq!(output.stdout.dropped_bytes, 1024);
    assert_eq!(output.stderr.bytes, vec![b'e'; 1024]);
    assert_eq!(output.stderr.dropped_bytes, 2048);
    assert!(output.stdout.was_truncated());
    assert!(output.stderr.was_truncated());
}

#[tokio::test]
async fn external_consumer_kills_a_timed_out_child() {
    let error = run_truncating(
        None,
        EMIT,
        &["0", "0", "5000"],
        limits(Duration::from_millis(75)),
    )
    .await
    .expect_err("timeout must fail closed");

    assert!(matches!(error, ProcessError::TimedOut));
}
