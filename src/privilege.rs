//! 操作级内联提权（M12；SPEC §5 M12、AGENTS §5.7）。
//!
//! 文件操作因 EACCES 失败时，`sudo -A` + `rvifm --askpass` 辅助模式
//! 原地重试**当前单个操作**：主 TUI 收集密码 → 写入 FIFO（0600）→
//! `sudo -A <op>` 认证时调用 `rvifm --askpass <fifo>` 读出密码打印到
//! stdout → sudo 认证成功后以 root 执行该操作。
//!
//! 硬约束（AGENTS §5.7）：
//! - 提权必须是操作级，禁止将 rvifm 主进程提升为 root；
//! - 不启动 root rvifm 实例（askpass 是短命密码辅助子进程，非管理器实例）；
//! - 提权失败或取消后 TUI 状态必须与操作前一致；
//! - 不引入 pkexec 作为硬依赖；不使用 `sudo_prompt` crate（自建 askpass）。
//!
//! `SUDO_ASKPASS` 通过 wrapper 脚本传参（兼容所有 sudo 版本——
//! 带参数的 SUDO_ASKPASS 需要 sudo >= 1.9.4）。
//!
//! Windows（UAC 机制不同）：SIMPLIFIED 尽力而为——仅提示不支持。
//! 整体 cfg(unix) 门控。

/// askpass FIFO 文件名前缀（`/tmp` 下，0600 权限）。
pub const FIFO_PREFIX: &str = "rvifm-askpass-";

/// 生成 askpass FIFO 路径（进程 id 区分并发实例）。
#[cfg(unix)]
pub fn askpass_fifo_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("{FIFO_PREFIX}{}.fifo", std::process::id()))
}

/// 创建 0600 权限 FIFO（先删除可能残留的同名路径）。
#[cfg(unix)]
pub fn create_fifo(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::remove_file(path);
    if unsafe { libc_mkfifo(path) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

/// mkfifo(3) 薄封装（模式 0600）。
#[cfg(unix)]
unsafe fn libc_mkfifo(path: &std::path::Path) -> i32 {
    unsafe extern "C" {
        fn mkfifo(path: *const core::ffi::c_char, mode: u32) -> i32;
    }
    let c = match std::ffi::CString::new(path.as_os_str().to_string_lossy().as_bytes()) {
        Ok(c) => c,
        Err(_) => return -1,
    };
    unsafe { mkfifo(c.as_ptr(), 0o600) }
}

/// askpass 子命令入口（`rvifm --askpass <fifo>`）：
/// sudo 认证时被调用——从 FIFO 读一行密码，打印到 stdout，退出。
/// 非 Unix 平台直接报错退出（UAC 机制不同，SIMPLIFIED）。
pub fn cmd_askpass(args: Vec<String>) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let Some(fifo) = args.first() else {
            eprintln!("rvifm: --askpass 缺少 FIFO 路径参数");
            std::process::exit(2);
        };
        match read_password_from_fifo(std::path::Path::new(fifo)) {
            Ok(pw) => {
                println!("{pw}");
                Ok(())
            }
            Err(e) => {
                // 读不到密码（写端已关/超时）→ 输出空行让 sudo 认证失败。
                eprintln!("rvifm: --askpass 读密码失败: {e}");
                println!();
                std::process::exit(1);
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = args;
        eprintln!("rvifm: --askpass 仅支持 Unix（Linux/macOS）");
        std::process::exit(2);
    }
}

/// 从 FIFO 读取一行密码（askpass 读端）。
/// 打开读端会阻塞直到写端打开（sudo 调用时机由主进程保证）。
#[cfg(unix)]
pub fn read_password_from_fifo(path: &std::path::Path) -> std::io::Result<String> {
    use std::io::BufRead;
    let f = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(f);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    Ok(line.trim_end_matches(['\n', '\r']).to_owned())
}

/// 主进程端：把密码写入 FIFO（写端）。
/// 打开写端阻塞直到 askpass 打开读端；写入后关闭。
#[cfg(unix)]
pub fn write_password_to_fifo(path: &std::path::Path, password: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new().write(true).open(path)?;
    writeln!(f, "{password}")
}

/// 删除 FIFO（清理残留）。
#[cfg(unix)]
pub fn cleanup_fifo(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
}

// ── M12-3：sudo -A 提权执行 ───────────────────────────────────────────────

/// 可提权的单个写操作（TUI 内由操作失败上下文构造）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivOp {
    /// 删除（rm -rf，root 侧不走应用内暂存——撤销栈不记录提权删除）。
    Delete { path: std::path::PathBuf },
    /// 重命名。
    Rename {
        from: std::path::PathBuf,
        to: std::path::PathBuf,
    },
    /// 移动。
    Move {
        from: std::path::PathBuf,
        to: std::path::PathBuf,
    },
    /// 复制。
    Copy {
        from: std::path::PathBuf,
        to: std::path::PathBuf,
    },
    /// 新建目录。
    CreateDir { path: std::path::PathBuf },
    /// 新建文件。
    CreateFile { path: std::path::PathBuf },
}

impl PrivOp {
    /// 对应的 root 侧命令（不经 shell；`--` 防止路径被当选项）。
    #[cfg(unix)]
    pub fn sudo_command(&self) -> (&'static str, Vec<std::path::PathBuf>) {
        match self {
            PrivOp::Delete { path } => ("rm", vec!["-rf".into(), "--".into(), path.into()]),
            PrivOp::Rename { from, to } => ("mv", vec!["--".into(), from.into(), to.into()]),
            PrivOp::Move { from, to } => ("mv", vec!["--".into(), from.into(), to.into()]),
            PrivOp::Copy { from, to } => {
                ("cp", vec!["-a".into(), "--".into(), from.into(), to.into()])
            }
            PrivOp::CreateDir { path } => ("mkdir", vec!["-p".into(), path.into()]),
            PrivOp::CreateFile { path } => ("touch", vec![path.into()]),
        }
    }

    /// 用户可见描述（确认与错误提示）。
    pub fn describe(&self) -> String {
        match self {
            PrivOp::Delete { path } => format!("删除 {}", path.display()),
            PrivOp::Rename { from, to } => format!("重命名 {} → {}", from.display(), to.display()),
            PrivOp::Move { from, to } => format!("移动 {} → {}", from.display(), to.display()),
            PrivOp::Copy { from, to } => format!("复制 {} → {}", from.display(), to.display()),
            PrivOp::CreateDir { path } => format!("新建目录 {}", path.display()),
            PrivOp::CreateFile { path } => format!("新建文件 {}", path.display()),
        }
    }

    /// M39-5：Windows UAC 提权命令（ShellExecuteExW "runas" 的 lpFile +
    /// lpParameters）——cmd.exe 承载；路径双引号保护（含空格）。
    #[cfg(windows)]
    pub fn uac_command(&self) -> (&'static str, String) {
        let q = |p: &std::path::Path| format!("\"{}\"", p.display());
        match self {
            PrivOp::Delete { path } => ("cmd.exe", format!("/c rd /s /q {}", q(path))),
            PrivOp::Rename { from, to } | PrivOp::Move { from, to } => {
                ("cmd.exe", format!("/c move /y {} {}", q(from), q(to)))
            }
            PrivOp::Copy { from, to } => ("cmd.exe", format!("/c copy /y {} {}", q(from), q(to))),
            PrivOp::CreateDir { path } => ("cmd.exe", format!("/c mkdir {}", q(path))),
            PrivOp::CreateFile { path } => ("cmd.exe", format!("/c type nul > {}", q(path))),
        }
    }
}

/// 生成 askpass wrapper 脚本（0700，`/tmp`）：
/// `#!/bin/sh` + `exec "<exe>" --askpass "<fifo>"`——
/// SUDO_ASKPASS 只能是不带参数的单一路径（带参数需 sudo >= 1.9.4），
/// wrapper 全版本兼容。
#[cfg(unix)]
pub fn create_wrapper_script(
    exe: &std::path::Path,
    fifo: &std::path::Path,
) -> std::io::Result<std::path::PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path = std::env::temp_dir().join(format!("{FIFO_PREFIX}{}.sh", std::process::id()));
    let script = format!(
        "#!/bin/sh\nexec \"{}\" --askpass \"{}\"\n",
        exe.display(),
        fifo.display()
    );
    std::fs::write(&path, script)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
    Ok(path)
}

/// 用给定密码以 `sudo -A` 重试单个操作（M12-3）。
///
/// 时序：mkfifo → 写 wrapper → `sudo -A <op>`（SUDO_ASKPASS=wrapper）→
/// sudo fork askpass 打开 FIFO 读端 → 本进程写端打开（阻塞配对）写密码 →
/// sudo 认证并执行 → 等待退出 → 清理 FIFO 与 wrapper。
/// sudo 退出非零（认证失败/执行失败）返回 Err；不残留 FIFO。
///
/// `SUDO_ASKPASS` 路径经 wrapper 后无空格问题；`sudo_command` 不经 shell。
#[cfg(unix)]
pub fn escalate_execute(op: &PrivOp, password: &str) -> Result<(), String> {
    use std::process::Command;

    let fifo = askpass_fifo_path();
    create_fifo(&fifo).map_err(|e| format!("askpass FIFO 创建失败: {e}"))?;
    let exe = std::env::current_exe().unwrap_or_else(|_| "rvifm".into());
    let wrapper = match create_wrapper_script(&exe, &fifo) {
        Ok(w) => w,
        Err(e) => {
            cleanup_fifo(&fifo);
            return Err(format!("askpass wrapper 生成失败: {e}"));
        }
    };

    let (prog, args) = op.sudo_command();
    let mut cmd = Command::new("sudo");
    cmd.arg("-A")
        .arg(prog)
        .args(args.iter().map(|a| a.as_os_str()))
        .env("SUDO_ASKPASS", &wrapper)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            cleanup_fifo(&fifo);
            let _ = std::fs::remove_file(&wrapper);
            return Err(format!("sudo 启动失败: {e}"));
        }
    };

    // 写密码（独立线程：open 写端阻塞直到 askpass 打开读端）。
    // sudo 若未调用 askpass（免密 NOPASSWD / 直接拒绝），线程随进程退出。
    let fifo_for_writer = fifo.clone();
    let pw = password.to_owned();
    std::thread::spawn(move || {
        let _ = write_password_to_fifo(&fifo_for_writer, &pw);
    });

    let output = child.wait_with_output();
    cleanup_fifo(&fifo);
    let _ = std::fs::remove_file(&wrapper);

    match output {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            Err(format!(
                "sudo 执行失败（{}）: {}",
                out.status,
                stderr.trim()
            ))
        }
        Err(e) => Err(format!("sudo 等待失败: {e}")),
    }
}

// ── M39-5：Windows UAC 操作级提权（ShellExecuteExW "runas"） ───────────────
//
// 边界（SPEC M39-5）：EACCES → UAC 弹窗；失败/取消状态不变；**不提升主
// 进程**——提权的是 cmd.exe 承载的单个操作（uac_command），rvifm 本体
// 权限不变。用户在 UAC 弹窗取消 → ERROR_CANCELLED → 可读 Err（TUI 保持
// 操作前状态，由调用方保证）。

/// ShellExecuteExW 执行信息结构（shellapi.h 精确布局；字段名对齐 SDK）。
#[cfg(windows)]
#[repr(C)]
struct ShellExecuteInfoW {
    cb_size: u32,
    f_mask: u32,
    hwnd: isize,
    lp_verb: *const u16,
    lp_file: *const u16,
    lp_parameters: *const u16,
    lp_directory: *const u16,
    n_show: i32,
    h_inst_app: isize,
    lp_id_list: *mut core::ffi::c_void,
    lp_class: *const u16,
    hkey_class: isize,
    dw_hot_key: u32,
    /// union { hIcon: HICON; hMonitor: HANDLE } —— 指针大小。
    h_icon: isize,
    h_process: isize,
}

// FFI 声明（本模块唯一 unsafe 面；Windows Shell/Kernel API）。
#[cfg(windows)]
unsafe extern "system" {
    fn ShellExecuteExW(psei: *mut ShellExecuteInfoW) -> i32;
    fn GetLastError() -> u32;
    fn WaitForSingleObject(handle: isize, milliseconds: u32) -> u32;
    fn GetExitCodeProcess(handle: isize, exit_code: *mut u32) -> i32;
    fn CloseHandle(handle: isize) -> i32;
}

/// M39-5：UAC 提权执行（Windows 专用）。
///
/// SAFETY：ShellExecuteExW/WaitForSingleObject/CloseHandle/GetLastError
/// 为 Windows Shell/Kernel API 标准调用——操作级提权的唯一受支持途径
/// （AGENTS §7 允许的"明确必要 unsafe"）；结构体为 SDK 精确布局、全部
/// 字符串为 NUL 结尾 UTF-16、句柄在等待后关闭，无跨调用悬垂。
#[cfg(windows)]
pub fn uac_execute(op: &PrivOp) -> Result<(), String> {
    const SEE_MASK_NOCLOSEPROCESS: u32 = 0x0000_0040;
    const SW_HIDE: i32 = 0;
    const WAIT_OBJECT_0: u32 = 0;
    const ERROR_CANCELLED: u32 = 1223;
    const INVALID_HANDLE: isize = 0;

    let (file, params) = op.uac_command();
    let to_wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let verb = to_wide("runas");
    let file_w = to_wide(file);
    let params_w = to_wide(&params);

    let mut sei = ShellExecuteInfoW {
        cb_size: std::mem::size_of::<ShellExecuteInfoW>() as u32,
        f_mask: SEE_MASK_NOCLOSEPROCESS,
        hwnd: 0,
        lp_verb: verb.as_ptr(),
        lp_file: file_w.as_ptr(),
        lp_parameters: params_w.as_ptr(),
        lp_directory: std::ptr::null(),
        n_show: SW_HIDE,
        h_inst_app: 0,
        lp_id_list: std::ptr::null_mut(),
        lp_class: std::ptr::null(),
        hkey_class: 0,
        dw_hot_key: 0,
        h_icon: 0,
        h_process: INVALID_HANDLE,
    };

    // SAFETY：见函数级注释；sei 为栈上输出结构，调用后仅读取 h_process。
    let ok = unsafe { ShellExecuteExW(&mut sei) };
    if ok == 0 {
        // SAFETY： GetLastError 线程局部，读取紧随失败调用之后。
        let err = unsafe { GetLastError() };
        return Err(if err == ERROR_CANCELLED {
            "用户取消 UAC 提权（状态不变）".to_owned()
        } else {
            format!("UAC 提权启动失败（错误码 {err}）")
        });
    }
    if sei.h_process == INVALID_HANDLE {
        return Err("UAC 提权未返回进程句柄".to_owned());
    }
    // SAFETY：等待提权进程退出后关闭句柄（SEE_MASK_NOCLOSEPROCESS 契约）。
    let wait = unsafe { WaitForSingleObject(sei.h_process, 0xFFFF_FFFF) };
    let mut code: u32 = 0;
    let got = unsafe { GetExitCodeProcess(sei.h_process, &mut code) };
    unsafe {
        CloseHandle(sei.h_process);
    }
    if wait != WAIT_OBJECT_0 || got == 0 {
        return Err("UAC 提权进程等待失败".to_owned());
    }
    if code != 0 {
        return Err(format!("提权命令退出码 {code}"));
    }
    Ok(())
}

#[cfg(unix)]
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn fifo_roundtrip_password() {
        let path = askpass_fifo_path();
        create_fifo(&path).expect("mkfifo");

        // 模拟 askpass 读端与主进程写端的协作（写端持克隆，原 path 供清理）。
        let writer_path = path.clone();
        let writer = std::thread::spawn(move || {
            // 等待读端打开（write_password_to_fifo 内部阻塞直到配对）。
            write_password_to_fifo(&writer_path, "secret-pw").expect("write fifo");
        });
        let pw = read_password_from_fifo(&path).expect("read fifo");
        writer.join().unwrap();
        assert_eq!(pw, "secret-pw");

        cleanup_fifo(&path);
        assert!(!path.exists());
    }

    #[test]
    fn askpass_fifo_path_is_unique_per_pid() {
        let a = askpass_fifo_path();
        let b = askpass_fifo_path();
        assert_eq!(a, b, "同进程内路径稳定");
        assert!(a.starts_with(std::env::temp_dir()));
        assert!(a.to_string_lossy().contains(FIFO_PREFIX));
    }

    #[test]
    fn privop_sudo_command_construction() {
        // M12-3：root 侧命令构造（`--` 防路径被当选项；不经 shell）。
        let (prog, args) = PrivOp::Delete {
            path: "/tmp/x".into(),
        }
        .sudo_command();
        assert_eq!(prog, "rm");
        assert_eq!(args[0].to_string_lossy(), "-rf");
        assert_eq!(args[2].to_string_lossy(), "/tmp/x");

        let (prog, args) = PrivOp::Copy {
            from: "/tmp/a b".into(),
            to: "/tmp/c".into(),
        }
        .sudo_command();
        assert_eq!(prog, "cp");
        assert_eq!(args[0].to_string_lossy(), "-a");
        // 路径含空格不经 shell，无需引号。
        assert_eq!(args[2].to_string_lossy(), "/tmp/a b");

        let (prog, args) = PrivOp::CreateDir {
            path: "/tmp/d".into(),
        }
        .sudo_command();
        assert_eq!(prog, "mkdir");
        assert_eq!(args[0].to_string_lossy(), "-p");
    }

    #[test]
    fn wrapper_script_contains_exe_and_fifo() {
        let wrapper = create_wrapper_script(
            Path::new("/usr/local/bin/rvifm"),
            Path::new("/tmp/rvifm-askpass-42.fifo"),
        )
        .unwrap();
        let content = std::fs::read_to_string(&wrapper).unwrap();
        assert!(content.starts_with("#!/bin/sh\n"));
        assert!(content.contains("/usr/local/bin/rvifm"));
        assert!(content.contains("--askpass"));
        assert!(content.contains("/tmp/rvifm-askpass-42.fifo"));
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&wrapper).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700, "wrapper 必须 0700");
        std::fs::remove_file(&wrapper).unwrap();
    }

    /// CI ubuntu runner（runner 用户 NOPASSWD sudo）下真实提权 touch。
    /// 无免密 sudo 的环境（本地）自动跳过。
    #[test]

    fn escalate_execute_touch_creates_file_when_sudo_available() {
        // 前置探测：sudo 可用且免密（-n）→ 执行；否则跳过。
        let probe = std::process::Command::new("sudo")
            .arg("-n")
            .arg("true")
            .status();
        match probe {
            Ok(s) if s.success() => {}
            _ => {
                eprintln!("跳过：无免密 sudo 环境");
                return;
            }
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let target = tmp.path().join("escalated.txt");
        escalate_execute(
            &PrivOp::CreateFile {
                path: target.clone(),
            },
            "unused-password",
        )
        .expect("sudo touch via escalate");
        assert!(target.exists(), "提权创建的文件应存在");
    }
}

/// M39-5：Windows UAC 命令构造（纯函数；提权执行本身为手工清单——
/// UAC 弹窗无法自动化，失败/取消状态不变由 Escalate 消费分支保证）。
#[cfg(all(test, windows))]
mod tests_uac {
    use super::*;

    #[test]
    fn m395_uac_command_mapping() {
        let (f, p) = PrivOp::Delete {
            path: r"C:\Program Data\a b\file.txt".into(),
        }
        .uac_command();
        assert_eq!(f, "cmd.exe");
        assert!(p.starts_with("/c rd /s /q \""), "路径引号保护: {p}");
        assert!(p.ends_with("file.txt\""));
        let (_, p) = PrivOp::Rename {
            from: r"C:\a".into(),
            to: r"C:\b".into(),
        }
        .uac_command();
        assert!(p.starts_with("/c move /y"), "p: {p}");
        let (_, p) = PrivOp::Copy {
            from: r"C:\a".into(),
            to: r"C:\b".into(),
        }
        .uac_command();
        assert!(p.starts_with("/c copy /y"), "p: {p}");
        let (_, p) = PrivOp::CreateDir {
            path: r"C:\nd".into(),
        }
        .uac_command();
        assert!(p.starts_with("/c mkdir"), "p: {p}");
        let (_, p) = PrivOp::CreateFile {
            path: r"C:\nf".into(),
        }
        .uac_command();
        assert!(p.starts_with("/c type nul >"), "p: {p}");
    }
}
