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

#![allow(dead_code)]

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
        fn mkfifo(path: *const i8, mode: u32) -> i32;
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

#[cfg(unix)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifo_roundtrip_password() {
        let path = askpass_fifo_path();
        create_fifo(&path).expect("mkfifo");

        // 模拟 askpass 读端与主进程写端的协作。
        let writer = std::thread::spawn(move || {
            // 等待读端打开（write_password_to_fifo 内部阻塞直到配对）。
            write_password_to_fifo(&path, "secret-pw").expect("write fifo");
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
}
