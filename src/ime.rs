//! Linux 输入法命令适配层（FIX-9）。
//!
//! 只调用桌面已有命令，不链接输入法库或 D-Bus crate。事件循环通过后台线程
//! 调用本模块，命令探测和 IPC 不占用 TUI 事件循环。

use crate::config::ImeConfig;

/// M51-1：IME 强制切换暂停标志（桌面输入法状态是全局的）。rvifm 打开外部
/// 终端窗口或失去焦点时置位，避免周期探测把别的程序（vim 等）的中文输入法
/// 瞬间切回英文；回到 rvifm 并收到按键/FocusGained 时清除。
static IME_SUSPENDED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn suspend_ime() {
    IME_SUSPENDED.store(true, std::sync::atomic::Ordering::Relaxed);
}

pub fn resume_ime() {
    IME_SUSPENDED.store(false, std::sync::atomic::Ordering::Relaxed);
}

pub fn ime_suspended() -> bool {
    IME_SUSPENDED.load(std::sync::atomic::Ordering::Relaxed)
}

/// M51-1：是否允许强制切英文的纯判定（便于测试隔离全局标志）。
fn ime_force_allowed(suspended: bool, enabled: bool, linux: bool) -> bool {
    !suspended && enabled && linux
}
#[cfg(target_os = "linux")]
use std::process::Command;
#[cfg(target_os = "linux")]
use std::sync::mpsc::{self, Sender};
#[cfg(target_os = "linux")]
use std::thread::JoinHandle;

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandResult {
    pub success: bool,
    pub stdout: Vec<u8>,
}

#[cfg(target_os = "linux")]
pub(crate) trait CommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandResult, ()>;
}

#[cfg(target_os = "linux")]
pub(crate) struct SystemRunner;

#[cfg(target_os = "linux")]
impl CommandRunner for SystemRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandResult, ()> {
        let output = Command::new(program).args(args).output().map_err(|_| ())?;
        Ok(CommandResult {
            success: output.status.success(),
            stdout: output.stdout,
        })
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Backend {
    Fcitx5,
    Fcitx,
    Ibus,
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, PartialEq, Eq)]
enum SavedState {
    FcitxActive,
    IbusEngine(String),
}

#[cfg(target_os = "linux")]
pub(crate) struct ImeManager<R> {
    runner: R,
    config: ImeConfig,
    backend: Option<Backend>,
    saved: Option<SavedState>,
    /// M47-6：防御性激活的节流窗口（避免每次 Esc 都探测/切换）。
    last_force: Option<std::time::Instant>,
}

#[cfg(target_os = "linux")]
impl<R: CommandRunner> ImeManager<R> {
    pub(crate) fn new(runner: R, config: ImeConfig) -> Self {
        Self {
            runner,
            config,
            backend: None,
            saved: None,
            last_force: None,
        }
    }

    /// 离开可编辑模式时切到英文；重复调用不会覆盖首次保存的状态。
    pub(crate) fn enter_normal(&mut self) {
        if !self.config.enabled || self.saved.is_some() {
            return;
        }
        if !cfg!(target_os = "linux") {
            return;
        }

        if self.try_fcitx(Backend::Fcitx5, "fcitx5-remote")
            || self.try_fcitx(Backend::Fcitx, "fcitx-remote")
        {
            return;
        }
        self.try_ibus();
    }

    /// M47-6：防御性切英文——从其它窗口/应用切回、Normal 模式 Esc 时调用。
    /// 与 `enter_normal` 不同：不保存/覆盖原状态（否则会破坏进入文本输入模式
    /// 后的恢复语义），仅在检测到非英文时发一次切换命令。节流避免连按 Esc
    /// 反复探测/IPC。
    pub(crate) fn force_english(&mut self) {
        // M51-1：外部窗口拥有焦点/会话期间不得干扰全局输入法状态。
        if !ime_force_allowed(
            ime_suspended(),
            self.config.enabled,
            cfg!(target_os = "linux"),
        ) {
            return;
        }
        const FORCE_THROTTLE: std::time::Duration = std::time::Duration::from_millis(250);
        let now = std::time::Instant::now();
        if self
            .last_force
            .is_some_and(|at| now.duration_since(at) < FORCE_THROTTLE)
        {
            return;
        }
        self.last_force = Some(now);

        for (backend, program) in [
            (Backend::Fcitx5, "fcitx5-remote"),
            (Backend::Fcitx, "fcitx-remote"),
        ] {
            let Ok(result) = self.runner.run(program, &[]) else {
                continue;
            };
            if !result.success {
                continue;
            }
            let Some(status @ (1 | 2)) = parse_fcitx_status(&result.stdout) else {
                continue;
            };
            self.backend = Some(backend);
            if status == 2 && self.runner.run(program, &["-c"]).is_ok_and(|r| r.success) {
                // 不写 saved：外部窗口切回时用户并未经过 rvifm 的可编辑模式。
            }
            return;
        }

        let Ok(current) = self.runner.run("ibus", &["engine"]) else {
            return;
        };
        if !current.success {
            return;
        }
        let engine = String::from_utf8_lossy(&current.stdout).trim().to_owned();
        self.backend = Some(Backend::Ibus);
        if engine.is_empty() || engine == self.config.ibus_english_engine {
            return;
        }
        let _ = self
            .runner
            .run("ibus", &["engine", &self.config.ibus_english_engine]);
    }

    /// 重新进入可编辑模式时恢复进入 Normal 前捕获的状态。
    pub(crate) fn enter_editable(&mut self) {
        let Some(saved) = self.saved.as_ref() else {
            return;
        };
        let result = match (self.backend, saved) {
            (Some(Backend::Fcitx5), SavedState::FcitxActive) => {
                self.runner.run("fcitx5-remote", &["-o"])
            }
            (Some(Backend::Fcitx), SavedState::FcitxActive) => {
                self.runner.run("fcitx-remote", &["-o"])
            }
            (Some(Backend::Ibus), SavedState::IbusEngine(engine)) => {
                self.runner.run("ibus", &["engine", engine])
            }
            _ => return,
        };
        if result.is_ok_and(|r| r.success) {
            self.saved = None;
        }
    }

    fn try_fcitx(&mut self, backend: Backend, program: &str) -> bool {
        let Ok(result) = self.runner.run(program, &[]) else {
            return false;
        };
        if !result.success {
            return false;
        }
        let Some(status @ (1 | 2)) = parse_fcitx_status(&result.stdout) else {
            // `0` 表示守护进程没有连接到当前会话，不应阻止后端回退。
            return false;
        };
        self.backend = Some(backend);
        if status == 2 && self.runner.run(program, &["-c"]).is_ok_and(|r| r.success) {
            self.saved = Some(SavedState::FcitxActive);
        }
        // 找到可用 Fcitx 后，不再向其他后端发命令；状态为 inactive 时无须切换。
        true
    }

    fn try_ibus(&mut self) {
        let Ok(current) = self.runner.run("ibus", &["engine"]) else {
            return;
        };
        if !current.success {
            return;
        }
        let engine = String::from_utf8_lossy(&current.stdout).trim().to_owned();
        if engine.is_empty() || engine == self.config.ibus_english_engine {
            self.backend = Some(Backend::Ibus);
            return;
        }
        if self
            .runner
            .run("ibus", &["engine", &self.config.ibus_english_engine])
            .is_ok_and(|r| r.success)
        {
            self.backend = Some(Backend::Ibus);
            self.saved = Some(SavedState::IbusEngine(engine));
        }
    }
}

#[cfg(target_os = "linux")]
fn parse_fcitx_status(stdout: &[u8]) -> Option<u8> {
    String::from_utf8_lossy(stdout).trim().parse().ok()
}

#[cfg(target_os = "linux")]
enum WorkerCommand {
    EnterNormal,
    EnterEditable,
    /// M47-6：防御性切英文（不覆盖已保存状态）。
    ForceEnglish,
    Shutdown,
}

/// 将可能阻塞的输入法命令放在专用线程执行，避免停顿 TUI 事件循环。
pub(crate) struct ImeWorker {
    #[cfg(target_os = "linux")]
    tx: Option<Sender<WorkerCommand>>,
    #[cfg(target_os = "linux")]
    thread: Option<JoinHandle<()>>,
}

impl ImeWorker {
    pub(crate) fn start(config: ImeConfig) -> Self {
        #[cfg(target_os = "linux")]
        if config.enabled {
            let (tx, rx) = mpsc::channel();
            let thread = std::thread::spawn(move || {
                let mut manager = ImeManager::new(SystemRunner, config);
                while let Ok(command) = rx.recv() {
                    match command {
                        WorkerCommand::EnterNormal => manager.enter_normal(),
                        WorkerCommand::EnterEditable => manager.enter_editable(),
                        WorkerCommand::ForceEnglish => manager.force_english(),
                        WorkerCommand::Shutdown => {
                            manager.enter_editable();
                            break;
                        }
                    }
                }
            });
            let _ = tx.send(WorkerCommand::EnterNormal);
            return Self {
                tx: Some(tx),
                thread: Some(thread),
            };
        }

        #[cfg(not(target_os = "linux"))]
        let _ = config;

        Self {
            #[cfg(target_os = "linux")]
            tx: None,
            #[cfg(target_os = "linux")]
            thread: None,
        }
    }

    pub(crate) fn enter_normal(&self) {
        #[cfg(target_os = "linux")]
        if let Some(tx) = &self.tx {
            let _ = tx.send(WorkerCommand::EnterNormal);
        }
    }

    pub(crate) fn enter_editable(&self) {
        #[cfg(target_os = "linux")]
        if let Some(tx) = &self.tx {
            let _ = tx.send(WorkerCommand::EnterEditable);
        }
    }

    /// M47-6：请求防御性切英文（Normal 模式 Esc / 终端 FocusGained）。
    pub(crate) fn force_english(&self) {
        #[cfg(target_os = "linux")]
        if let Some(tx) = &self.tx {
            let _ = tx.send(WorkerCommand::ForceEnglish);
        }
    }
}

impl Drop for ImeWorker {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(WorkerCommand::Shutdown);
        }
        #[cfg(target_os = "linux")]
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
#[cfg(target_os = "linux")]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    struct FakeRunner {
        results: RefCell<VecDeque<Result<CommandResult, ()>>>,
        calls: RefCell<Vec<(String, Vec<String>)>>,
    }

    impl FakeRunner {
        fn new(results: impl IntoIterator<Item = Result<CommandResult, ()>>) -> Self {
            Self {
                results: RefCell::new(results.into_iter().collect()),
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl CommandRunner for FakeRunner {
        fn run(&self, program: &str, args: &[&str]) -> Result<CommandResult, ()> {
            self.calls.borrow_mut().push((
                program.to_owned(),
                args.iter().map(|s| (*s).to_owned()).collect(),
            ));
            self.results.borrow_mut().pop_front().unwrap_or(Err(()))
        }
    }

    fn ok(stdout: &str) -> Result<CommandResult, ()> {
        Ok(CommandResult {
            success: true,
            stdout: stdout.as_bytes().to_vec(),
        })
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn fcitx_active_state_is_deactivated_and_restored_once() {
        let runner = FakeRunner::new([ok("2\n"), ok(""), ok("")]);
        let mut manager = ImeManager::new(runner, ImeConfig::default());
        manager.enter_normal();
        manager.enter_normal();
        manager.enter_editable();
        manager.enter_editable();
        let calls = manager.runner.calls.borrow();
        assert_eq!(calls[0], ("fcitx5-remote".into(), vec![]));
        assert_eq!(calls[1], ("fcitx5-remote".into(), vec!["-c".into()]));
        assert_eq!(calls[2], ("fcitx5-remote".into(), vec!["-o".into()]));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn ibus_engine_is_saved_switched_and_restored() {
        let runner = FakeRunner::new([Err(()), Err(()), ok("rime\n"), ok(""), ok("")]);
        let mut manager = ImeManager::new(runner, ImeConfig::default());
        manager.enter_normal();
        manager.enter_editable();
        let calls = manager.runner.calls.borrow();
        assert_eq!(calls[2], ("ibus".into(), vec!["engine".into()]));
        assert_eq!(
            calls[3],
            ("ibus".into(), vec!["engine".into(), "xkb:us::eng".into()])
        );
        assert_eq!(
            calls[4],
            ("ibus".into(), vec!["engine".into(), "rime".into()])
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn unavailable_fcitx_daemons_fall_back_to_ibus() {
        let runner = FakeRunner::new([ok("0\n"), Err(()), ok("rime\n"), ok(""), ok("")]);
        let mut manager = ImeManager::new(runner, ImeConfig::default());
        manager.enter_normal();
        manager.enter_editable();
        let calls = manager.runner.calls.borrow();
        assert_eq!(calls[0].0, "fcitx5-remote");
        assert_eq!(calls[1].0, "fcitx-remote");
        assert_eq!(calls[2], ("ibus".into(), vec!["engine".into()]));
        assert_eq!(calls[3].0, "ibus");
        assert_eq!(
            calls[4],
            ("ibus".into(), vec!["engine".into(), "rime".into()])
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn inactive_fcitx_state_is_not_activated_on_return_to_editable() {
        let runner = FakeRunner::new([ok("1\n")]);
        let mut manager = ImeManager::new(runner, ImeConfig::default());
        manager.enter_normal();
        manager.enter_editable();
        let calls = manager.runner.calls.borrow();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], ("fcitx5-remote".into(), vec![]));
    }

    #[test]
    fn disabled_configuration_does_not_spawn_commands() {
        let runner = FakeRunner::new([]);
        let mut manager = ImeManager::new(
            runner,
            ImeConfig {
                enabled: false,
                ..ImeConfig::default()
            },
        );
        manager.enter_normal();
        manager.force_english();
        assert!(manager.runner.calls.borrow().is_empty());
    }

    #[test]
    fn m476_force_english_does_not_overwrite_saved_state() {
        // enter_normal 保存了中文态；随后 Esc/切回触发 force_english 时不得覆盖它，
        // 回到可编辑模式仍须恢复（-o）。
        let runner = FakeRunner::new([ok("2\n"), ok(""), ok("1\n"), ok("")]);
        let mut manager = ImeManager::new(runner, ImeConfig::default());
        manager.enter_normal();
        manager.force_english();
        manager.enter_editable();
        let calls = manager.runner.calls.borrow();
        assert_eq!(calls[1], ("fcitx5-remote".into(), vec!["-c".into()]));
        assert_eq!(calls[2], ("fcitx5-remote".into(), vec![]));
        assert_eq!(calls[3], ("fcitx5-remote".into(), vec!["-o".into()]));
    }

    #[test]
    fn m511_force_english_disallowed_while_suspended() {
        assert!(!ime_force_allowed(true, true, true), "暂停时不强制");
        assert!(ime_force_allowed(false, true, true), "恢复后允许");
        assert!(!ime_force_allowed(false, false, true), "配置关闭时不强制");
        assert!(!ime_force_allowed(false, true, false), "非 Linux 不强制");
    }

    #[test]
    fn m476_force_english_switches_ibus_and_throttles() {
        let runner = FakeRunner::new([Err(()), Err(()), ok("rime\n"), ok("")]);
        let mut manager = ImeManager::new(runner, ImeConfig::default());
        manager.force_english();
        manager.force_english(); // 节流窗口内直接返回
        let calls = manager.runner.calls.borrow();
        assert_eq!(calls.len(), 4, "第二次调用被节流: {calls:?}");
        assert_eq!(
            calls[3],
            ("ibus".into(), vec!["engine".into(), "xkb:us::eng".into()])
        );
    }
}
