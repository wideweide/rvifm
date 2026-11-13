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

// ── M12-2：askpass 辅助模式全链路（sudo -A 认证时被调用的形态） ───────────
#[cfg(unix)]
#[test]
#[ignore = "unix CI diagnosis"]
fn askpass_reads_fifo_password_and_prints_stdout() {
    use std::io::Write as _;
    use std::process::Stdio;

    let fifo = std::env::temp_dir().join(format!("rvifm-test-{}.fifo", std::process::id()));
    let _ = std::fs::remove_file(&fifo);
    // mkfifo via nix-free path: use the binary's own helper through the
    // CLI? No — create via libc here to stay dependency-free.
    let cpath = std::ffi::CString::new(fifo.to_string_lossy().as_bytes()).unwrap();
    unsafe extern "C" {
        fn mkfifo(path: *const i8, mode: u32) -> i32;
    }
    unsafe { mkfifo(cpath.as_ptr(), 0o600) };

    // 启动 askpass 子进程（sudo 认证时同形态：--askpass <fifo>）。
    let child = Command::new(env!("CARGO_BIN_EXE_rvifm"))
        .arg("--askpass")
        .arg(&fifo)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn askpass");

    // 主进程端：写密码（写端打开阻塞直到 askpass 打开读端）。
    let mut w = std::fs::OpenOptions::new()
        .write(true)
        .open(&fifo)
        .expect("open fifo write end");
    write!(w, " hunter2\n").unwrap();
    drop(w);

    let out = child.wait_with_output().expect("wait askpass");
    assert_eq!(out.status.code(), Some(0), "askpass exit");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.trim(), "hunter2", "askpass stdout: {stdout}");

    // FIFO 清理。
    std::fs::remove_file(&fifo).unwrap();
}

#[cfg(unix)]
#[test]
#[ignore = "unix CI diagnosis"]
fn askpass_with_empty_fifo_fails_gracefully() {
    // FIFO 写端从未打开 → 读端 EOF → askpass 退出非零（sudo 认证失败路径）。
    let fifo = std::env::temp_dir().join(format!("rvifm-test-eof-{}.fifo", std::process::id()));
    let _ = std::fs::remove_file(&fifo);
    let cpath = std::ffi::CString::new(fifo.to_string_lossy().as_bytes()).unwrap();
    unsafe extern "C" {
        fn mkfifo(path: *const i8, mode: u32) -> i32;
    }
    unsafe { mkfifo(cpath.as_ptr(), 0o600) };
    // 预先关闭"写端"：直接 close 未打开的 fd 无意义——用 reader 在
    // 子进程内读到 EOF 的方式：open 写端后立即关闭再启动子进程会
    // 阻塞……因此这里直接删除 FIFO，让 askpass open 失败走错误路径。
    std::fs::remove_file(&fifo).unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_rvifm"))
        .arg("--askpass")
        .arg(&fifo)
        .output()
        .expect("run askpass");
    assert_eq!(out.status.code(), Some(1), "missing fifo → exit 1");
}

#[cfg(unix)]
#[test]
#[ignore = "unix CI diagnosis"]
fn askpass_without_arg_exits_two() {
    let out = Command::new(env!("CARGO_BIN_EXE_rvifm"))
        .arg("--askpass")
        .output()
        .expect("run askpass");
    assert_eq!(out.status.code(), Some(2));
}
