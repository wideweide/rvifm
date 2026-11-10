//! 冒烟测试：只跑 `--version` / `--help`，不进入交互模式（AGENTS.md §7、ACCEPTANCE §1）。
//! CI 无 TTY，此测试不依赖终端。

use std::process::Command;

fn run_flag(flag: &str) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_rvifm"))
        .arg(flag)
        .output()
        .expect("failed to run rvifm");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn version_flag_prints_version_and_exits_zero() {
    let (code, stdout, stderr) = run_flag("--version");
    assert_eq!(code, 0);
    assert!(stdout.contains("rvifm"), "stdout: {stdout}");
    assert!(
        stdout.contains(env!("CARGO_PKG_VERSION")),
        "stdout: {stdout}"
    );
    assert!(stderr.is_empty(), "stderr: {stderr}");
}

#[test]
fn help_flag_prints_usage_and_exits_zero() {
    let (code, stdout, _) = run_flag("--help");
    assert_eq!(code, 0);
    assert!(stdout.contains("--help"), "stdout: {stdout}");
    assert!(stdout.contains("--version"), "stdout: {stdout}");
}

#[test]
fn unknown_flag_exits_nonzero_without_panic() {
    let (code, _, stderr) = run_flag("--no-such-flag");
    assert_ne!(code, 0);
    assert!(stderr.contains("未知参数"), "stderr: {stderr}");
}
