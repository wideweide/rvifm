//! M98-3：轻量事件钩子（外部进程 + stdin JSON，无脚本执行）。
//!
//! pre 钩子在有限超时内同步等待，可取消操作；post 钩子后台执行、不阻塞 UI。

use crate::config::Config;
use std::io::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    OpenPre,
    OpenPost,
    OpenFailed,
    DeletePre,
    DeletePost,
    RenamePre,
    RenamePost,
    EnterDir,
    JobDone,
    CmdOk,
}

impl HookEvent {
    pub fn name(self) -> &'static str {
        match self {
            Self::OpenPre => "open_pre",
            Self::OpenPost => "open_post",
            Self::OpenFailed => "open_failed",
            Self::DeletePre => "delete_pre",
            Self::DeletePost => "delete_post",
            Self::RenamePre => "rename_pre",
            Self::RenamePost => "rename_post",
            Self::EnterDir => "enter_dir",
            Self::JobDone => "job_done",
            Self::CmdOk => "cmd_ok",
        }
    }

    pub fn is_pre(self) -> bool {
        matches!(self, Self::OpenPre | Self::DeletePre | Self::RenamePre)
    }
}

fn run_external(cmd: &str, payload: &str) -> std::io::Result<bool> {
    use std::process::{Command, Stdio};
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(payload.as_bytes());
    }
    let out = child.wait_with_output()?;
    Ok(out.status.success())
}

/// pre 钩子：超时（200ms）按放行处理；仅 `hook_abort = true` 时失败才取消。
pub fn fire_pre(config: &Config, event: HookEvent, payload: &str) -> bool {
    let Some(cmd) = config.hooks.get(event.name()) else {
        return true;
    };
    if !config.hook_abort {
        return true;
    }
    let cmd = cmd.clone();
    let payload = payload.to_owned();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let allowed = run_external(&cmd, &payload).unwrap_or(true);
        let _ = tx.send(allowed);
    });
    rx.recv_timeout(std::time::Duration::from_millis(200))
        .unwrap_or(true)
}

/// post 钩子：后台执行，不阻塞 UI；无回写核心状态。
pub fn fire_post(config: &Config, event: HookEvent, payload: &str) {
    let Some(cmd) = config.hooks.get(event.name()) else {
        return;
    };
    let cmd = cmd.clone();
    let payload = payload.to_owned();
    std::thread::spawn(move || {
        let _ = run_external(&cmd, &payload);
    });
}

/// JSON 转义的最小实现（路径/名称中可能有引号或反斜杠）。
pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

/// M98-4：全局异步 frecency 写入线程（调用方只投递记录，不阻塞）。
type VisitMsg = (
    crate::frecency::EntryRecord,
    Option<std::sync::mpsc::Sender<()>>,
);

#[cfg_attr(test, allow(dead_code))]
static VISIT_TX: std::sync::OnceLock<std::sync::mpsc::Sender<VisitMsg>> =
    std::sync::OnceLock::new();

#[cfg_attr(test, allow(dead_code))]
fn visit_sender() -> &'static std::sync::mpsc::Sender<VisitMsg> {
    VISIT_TX.get_or_init(|| {
        let (tx, rx): (
            std::sync::mpsc::Sender<VisitMsg>,
            std::sync::mpsc::Receiver<VisitMsg>,
        ) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for (record, ack) in rx {
                let _ = crate::frecency::flush_visits(std::slice::from_ref(&record));
                if let Some(ack) = ack {
                    let _ = ack.send(());
                }
            }
        });
        tx
    })
}

/// 异步投递一次访问记录；测试态跳过真实 redb 写入。
pub fn record_async(record: crate::frecency::EntryRecord) {
    #[cfg(test)]
    {
        let _ = record;
        return;
    }
    #[cfg(not(test))]
    let _ = visit_sender().send((record, None));
}

/// 退出前等待在途记录写完（best-effort）。
pub fn flush_records() {
    #[cfg(test)]
    return;
    #[cfg(not(test))]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        if visit_sender()
            .send((placeholder_record(), Some(tx)))
            .is_ok()
        {
            let _ = rx.recv_timeout(std::time::Duration::from_secs(2));
        }
    }
}

#[cfg(not(test))]
fn placeholder_record() -> crate::frecency::EntryRecord {
    crate::frecency::EntryRecord {
        path: String::new(),
        topic: "cmd".to_owned(),
        kind: crate::frecency::EntryKind::Open,
        ts: 0,
        clock: 0,
        keyword: String::new(),
    }
}
