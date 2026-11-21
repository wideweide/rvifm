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
fn rv_alias_binary_prints_version_and_exits_zero() {
    // MISC-1：`rv` 短别名与 `rvifm` 同源（同一 main.rs）——冒烟只验
    // `--version` 形态一致，不进入交互模式（AGENTS §7）。
    let output = Command::new(env!("CARGO_BIN_EXE_rv"))
        .arg("--version")
        .output()
        .expect("rv 二进制可启动");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("rvifm"), "版本输出: {stdout}");
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
    writeln!(w, " hunter2").unwrap();
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
fn askpass_without_arg_exits_two() {
    let out = Command::new(env!("CARGO_BIN_EXE_rvifm"))
        .arg("--askpass")
        .output()
        .expect("run askpass");
    assert_eq!(out.status.code(), Some(2));
}

// ── AUDIT-15：pick CLI 契约（--topic 对称评分 + 数据目录缺失降级） ──────────
// SPEC §4.6 示例形态：`selected=$(rvifm pick --topic dir < <(fd -t d))`。

use std::io::Write as _;
use std::process::Stdio;

/// 把 frecency 数据目录重定向到独立临时目录（LOCALAPPDATA / XDG_DATA_HOME
/// 仅影响子进程，测试间互不干扰、不触宿主真实数据）。
fn pick_command(data_root: &std::path::Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_rvifm"));
    if cfg!(windows) {
        c.env("LOCALAPPDATA", data_root);
    } else {
        c.env("XDG_DATA_HOME", data_root);
    }
    c
}

fn run_pick(data_root: &std::path::Path, args: &[&str], stdin: &str) -> (i32, String) {
    let mut child = pick_command(data_root)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn rvifm pick");
    child
        .stdin
        .as_mut()
        .expect("stdin piped")
        .write_all(stdin.as_bytes())
        .expect("write candidates");
    let out = child.wait_with_output().expect("wait pick");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn pick_scores_by_topic_and_degrades_without_data_dir() {
    // 验收②：数据目录缺失 → 空评分降级，filter 语义照常输出候选、退出 0。
    let fresh = tempfile::tempdir().expect("tempdir");
    let (code, stdout) = run_pick(fresh.path(), &["pick", "--topic", "dir"], "/some/dir\n");
    assert_eq!(code, 0, "stdout: {stdout}");
    assert!(stdout.contains("/some/dir"), "stdout: {stdout}");

    // 播种：dir 事件 /data/proj + file 事件 /data/proj/main.rs。
    let seeded = tempfile::tempdir().expect("tempdir");
    for (topic, path) in [("dir", "/data/proj"), ("file", "/data/proj/main.rs")] {
        let st = pick_command(seeded.path())
            .args(["record", "--topic", topic, path])
            .status()
            .expect("run record");
        assert!(st.success(), "record {topic} {path}");
    }
    let stdin = "/data/proj/main.rs\n/data/proj\n";
    // 验收①③ dir 视角：目录条目排前；main.rs 无 dir 分数（0 分兜底排后）。
    let (code, stdout) = run_pick(seeded.path(), &["pick", "--topic", "dir"], stdin);
    assert_eq!(code, 0, "stdout: {stdout}");
    let proj = stdout.find("/data/proj\n").expect("dir hit");
    let main = stdout.find("/data/proj/main.rs").expect("file candidate");
    assert!(proj < main, "dir 视角目录排前: {stdout}");
    // 验收③ file 视角：文件条目排前，目录不得以 dir 分数兜底。
    let (code, stdout) = run_pick(seeded.path(), &["pick", "--topic", "file"], stdin);
    assert_eq!(code, 0, "stdout: {stdout}");
    let main = stdout.find("/data/proj/main.rs").expect("file hit");
    let proj = stdout.find("/data/proj\n").expect("dir candidate");
    assert!(main < proj, "file 视角文件排前: {stdout}");
}

// ── AUDIT-12：CLI 子命令分派补测（maintenance 臂） ──────────────────────────

#[test]
fn maintenance_dispatch_compact_and_missing_subcommand() {
    let data = tempfile::tempdir().expect("tempdir");
    let mut cmd = pick_command(data.path());
    cmd.args(["maintenance", "compact"]);
    let out = cmd.output().expect("run maintenance compact");
    // 空库压实 = no-op，退出 0。
    assert_eq!(out.status.code(), Some(0));
    // 缺子命令 → 退出 2 + 可读提示。
    let data2 = tempfile::tempdir().expect("tempdir");
    let mut cmd2 = pick_command(data2.path());
    cmd2.arg("maintenance");
    let out2 = cmd2.output().expect("run maintenance");
    assert_eq!(out2.status.code(), Some(2), "缺子命令退出 2");
    let stderr = String::from_utf8_lossy(&out2.stderr);
    assert!(stderr.contains("compact"), "stderr: {stderr}");
}

// ── M38-3：`rvifm init <shell>` 四 shell 初始化脚本（只输出不写 rc） ────────

fn run_init(shell: &str) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_rvifm"))
        .arg("init")
        .arg(shell)
        .output()
        .expect("failed to run rvifm init");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn m38_3_init_four_shells_emit_record_and_pick_hooks() {
    // 四 shell 均输出非空脚本且接入 record（cd 钩子）与 pick（快捷键）。
    for shell in ["bash", "zsh", "fish", "powershell"] {
        let (code, stdout, stderr) = run_init(shell);
        assert_eq!(code, 0, "{shell} init 退出码");
        assert!(stderr.is_empty(), "{shell} init stderr: {stderr}");
        assert!(!stdout.trim().is_empty(), "{shell} 脚本非空");
        assert!(stdout.contains("record"), "{shell} 脚本缺 record 接入");
        assert!(stdout.contains("pick"), "{shell} 脚本缺 pick 调用");
    }
}

#[test]
fn m38_3_init_unknown_shell_fails() {
    let (code, _, stderr) = run_init("tcsh");
    assert_ne!(code, 0);
    assert!(stderr.contains("未知 shell"), "stderr: {stderr}");
}

#[cfg(unix)]
#[test]
fn m38_3_init_bash_script_passes_syntax_check() {
    // bash -n 语法检查（CI ubuntu 覆盖；Windows 不编译 cfg(unix) 测试——
    // M12-2 教训，bash 由 CI ubuntu job 兜底）。
    let output = Command::new(env!("CARGO_BIN_EXE_rvifm"))
        .arg("init")
        .arg("bash")
        .output()
        .expect("init bash");
    assert!(output.status.success());
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("init.sh");
    std::fs::write(&path, &output.stdout).expect("write script");
    let check = Command::new("bash")
        .arg("-n")
        .arg(&path)
        .output()
        .expect("bash -n");
    assert!(
        check.status.success(),
        "bash -n 失败: {}",
        String::from_utf8_lossy(&check.stderr)
    );
}
