//! `AppState`：tabs 列表 + active 索引 + 全局撤销栈（SPEC §4.2 / §4.3）。

use std::path::PathBuf;

use crate::config::Config;
use crate::tab::Tab;

/// 可逆操作记录（M2-1）。每条都携带所属 tab 与路径（ACCEPTANCE §4），
/// 撤销栈全局共享，跨 tab 撤销据此定位。
/// 各操作的构造与 `u` 执行逻辑随 M2-2～M2-5 接入，此前仅结构落地。
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum UndoableOp {
    /// 删除（可恢复）：`path` 为原路径，`staged` 为应用内暂存路径（undo 移回）。
    Delete {
        tab_id: usize,
        path: PathBuf,
        staged: PathBuf,
    },
    /// 重命名：`from` → `to`（undo 反向）。
    Rename {
        tab_id: usize,
        from: PathBuf,
        to: PathBuf,
    },
    /// 移动：`from` → `to`（undo 反向）。
    Move {
        tab_id: usize,
        from: PathBuf,
        to: PathBuf,
    },
    /// 新建目录/文件：undo 即移除 `path`。`recursive` 标记目录树整体为新建产物
    /// （copy 目录，M2-4b），undo 可整树移除；mkdir 只允许移除空目录（保护内容）。
    Create {
        tab_id: usize,
        path: PathBuf,
        recursive: bool,
    },
    /// M20-1：远端写操作（`remote` 为会话名；undo 经远端会话执行）。
    Remote {
        tab_id: usize,
        remote: String,
        kind: RemoteUndo,
    },
}

/// M20-1：远端撤销的种类（路径均为远端绝对路径）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteUndo {
    /// dd 可恢复删除：undo = trash 路径移回原位。
    Delete { path: PathBuf, trash_path: PathBuf },
    /// 新建（p 粘贴/:mkdir/:touch/:copy）：undo = 远端永久删除。
    Create { path: PathBuf, is_dir: bool },
    /// 重命名/移动：undo = 反向 rename。
    Rename { from: PathBuf, to: PathBuf },
}

#[allow(dead_code)] // 消费方：M2-2 起的撤销执行路径
impl UndoableOp {
    /// 所属 tab（跨 tab 撤销定位用）。
    pub fn tab_id(&self) -> usize {
        match self {
            UndoableOp::Delete { tab_id, .. }
            | UndoableOp::Rename { tab_id, .. }
            | UndoableOp::Move { tab_id, .. }
            | UndoableOp::Create { tab_id, .. }
            | UndoableOp::Remote { tab_id, .. } => *tab_id,
        }
    }

    /// 主路径（提示与定位用）。
    pub fn primary_path(&self) -> &PathBuf {
        match self {
            UndoableOp::Delete { path, .. } | UndoableOp::Create { path, .. } => path,
            UndoableOp::Rename { from, .. } | UndoableOp::Move { from, .. } => from,
            UndoableOp::Remote { kind, .. } => match kind {
                RemoteUndo::Delete { path, .. } | RemoteUndo::Create { path, .. } => path,
                RemoteUndo::Rename { from, .. } => from,
            },
        }
    }

    /// 人类可读描述（`u` 成功后的状态栏提示，M2-6）。
    pub fn describe(&self) -> String {
        match self {
            UndoableOp::Delete { path, .. } => format!("删除 {}", path.display()),
            UndoableOp::Rename { from, to, .. } => {
                format!("重命名 {} -> {}", from.display(), to.display())
            }
            UndoableOp::Move { from, to, .. } => {
                format!("移动 {} -> {}", from.display(), to.display())
            }
            UndoableOp::Create { path, .. } => format!("新建 {}", path.display()),
            UndoableOp::Remote { remote, kind, .. } => match kind {
                RemoteUndo::Delete { path, .. } => {
                    format!("远端删除 [{remote}] {}", path.display())
                }
                RemoteUndo::Create { path, .. } => {
                    format!("远端新建 [{remote}] {}", path.display())
                }
                RemoteUndo::Rename { from, to } => {
                    format!(
                        "远端重命名 [{remote}] {} -> {}",
                        from.display(),
                        to.display()
                    )
                }
            },
        }
    }
}

/// 全局撤销栈（不是 per-tab，SPEC §4.3）。LIFO：`u` 逆序弹出最近操作。
#[derive(Debug, Default)]
#[allow(dead_code)] // ops 由 M2-2 起写入、`u`（M2-2）读取
pub struct UndoStack {
    ops: Vec<UndoableOp>,
}

#[allow(dead_code)] // 消费方：M2-2 起的 `u` 键处理
impl UndoStack {
    /// 记录一次已完成的操作。
    pub fn push(&mut self, op: UndoableOp) {
        self.ops.push(op);
    }

    /// 弹出最近一次操作（`u` 消费；空栈返回 None）。
    pub fn pop(&mut self) -> Option<UndoableOp> {
        self.ops.pop()
    }

    /// 栈内操作数（测试与提示用）。
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

/// 后台作业状态（M13-8）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobStatus {
    Running,
    Done {
        summary: String,
    },
    Failed {
        err: String,
    },
    /// M24-9：用户请求取消后的终态（已完成项仍入撤销栈，`u` 可逆）。
    Cancelled {
        summary: String,
    },
}

/// 后台作业记录（M13-8）：`:jobs` 菜单展示。M24-9 增协作式取消标志——
/// `cancel_requested` 由覆盖层 `x` 置位，后台线程在文件边界检查后终止。
#[derive(Debug, Clone)]
pub struct Job {
    pub id: usize,
    pub kind: String,
    pub src_count: usize,
    pub target: String,
    pub status: JobStatus,
    pub cancel_requested: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

/// 后台线程完成后的更新包（主线程收包后入撤销栈并刷新）。
pub struct JobUpdate {
    pub job_id: usize,
    pub tab_id: usize,
    pub ops: Vec<UndoableOp>,
    pub status: JobStatus,
}

/// yank 缓冲区（FIX-2，SPEC §4.8）：`yy` 放入 copy 缓冲、`dd` 放入 cut 缓冲
/// （cut 接线待后续：dd 先经暂存删除，p 需从暂存恢复到任意目录）。
/// `p`（PutCopy）/`P`（PutMove）消费缓冲区，复制还是移动由类型与键位决定。
#[derive(Debug, Clone)]
pub struct YankBuffer {
    pub paths: Vec<PathBuf>,
    pub cut: bool,
    /// M20-3：来源（Local = 本地路径；Remote(会话名) = 远端路径）。
    pub source: YankSource,
}

/// M20-3：yank 来源（跨 VFS 粘贴方向选择）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum YankSource {
    Local,
    Remote(String),
}

/// 应用状态：所有标签 + 当前标签索引 + 全局撤销栈 + 用户配置。
/// Debug 手工实现（remote_sessions 的 Box<dyn RemoteFs> 无 Debug）。
pub struct AppState {
    pub tabs: Vec<Tab>,
    pub active: usize,
    /// 写入（操作完成后记录）与读取（`u` 撤销）在 M2-2 起接入。
    #[allow(dead_code)]
    pub undo_stack: UndoStack,
    /// 用户配置（M5-1 加载；选项生效在 M5-2 接入）。
    #[allow(dead_code)]
    pub config: Config,
    /// 图片预览引擎（M11-2）：启动时探测一次协议，非 TTY 回退 halfblocks。
    pub image_engine: crate::image::ImageEngine,
    /// 后台作业列表（M13-8）：`:jobs` 菜单展示。
    pub jobs: Vec<Job>,
    /// yank 缓冲区（FIX-2）：`yy` 复制 / `P` 移动 / `p` 按缓冲类型粘贴。
    pub yank: Option<YankBuffer>,
    /// M19-2：远端会话表（会话名 → 协议后端；`Box<dyn RemoteFs>`）。
    pub remote_sessions: std::collections::BTreeMap<String, Box<dyn crate::remote::RemoteFs>>,
    /// M21-F5：外部变更监听——watcher 持有监听目标（活动 tab 的物理目录），
    /// rx 承接事件。归档/远端视图不监听（None）；监听失败静默降级。
    pub watcher: Option<(std::path::PathBuf, notify::RecommendedWatcher)>,
    pub watch_rx: Option<std::sync::mpsc::Receiver<notify::Event>>,
    /// M21-F9：l/Enter 已实时记录的访问次数（tab 序号 + 侧别 L/R + 归一化
    /// 路径）——退出批量刷写（GAP-2）据此按次数去重，同一访问不重复计分。
    pub live_recorded: std::collections::HashMap<(usize, char, String), usize>,
    /// M22-6：远端文件打开的临时副本路径——run 退出时统一清理。
    pub temp_opens: Vec<std::path::PathBuf>,
    /// M23-9：副本修改防抖后的上传请求通道（run 循环消费）。
    pub upload_tx:
        Option<std::sync::mpsc::Sender<(String, std::path::PathBuf, std::path::PathBuf)>>,
    /// M24-6：命令行历史（会话内存，仅成功执行的命令；上限 [`CMD_HISTORY_MAX`]）。
    pub cmd_history: Vec<String>,
    /// M25-5：git 状态缓存（目录 → Some(快照) 就绪 / None 在途或非仓库）。
    pub git_cache: std::collections::HashMap<PathBuf, Option<crate::git::GitDirStatus>>,
    /// M25-5：在途查询目录（防重复 spawn）。
    pub git_pending: std::collections::HashSet<PathBuf>,
    /// M25-5：git 查询回传通道（后台线程 → run 循环）。
    pub git_tx:
        Option<std::sync::mpsc::Sender<(PathBuf, std::io::Result<crate::git::GitDirStatus>)>>,
    /// AUDIT-7：block=true opener 待执行命令（open_selected 转交，
    /// run 循环取走并挂起 TUI 执行后清空）。
    pub pending_external: Option<String>,
}

/// M24-6：命令历史上限（FIFO 淘汰最旧条目；无持久化）。
pub const CMD_HISTORY_MAX: usize = 100;

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("tabs", &self.tabs)
            .field("active", &self.active)
            .field("remotes", &self.remote_sessions.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl AppState {
    /// 以一个初始标签启动。
    pub fn new(initial: Tab) -> Self {
        Self {
            tabs: vec![initial],
            active: 0,
            undo_stack: UndoStack::default(),
            config: Config::default(),
            image_engine: crate::image::ImageEngine::detect(),
            jobs: Vec::new(),
            yank: None,
            remote_sessions: std::collections::BTreeMap::new(),
            watcher: None,
            watch_rx: None,
            live_recorded: std::collections::HashMap::new(),
            temp_opens: Vec::new(),
            upload_tx: None,
            cmd_history: Vec::new(),
            git_cache: std::collections::HashMap::new(),
            git_pending: std::collections::HashSet::new(),
            git_tx: None,
            pending_external: None,
        }
    }

    /// M24-6：记录成功执行的命令行（去首尾空白；空命令不记；超上限淘汰最旧）。
    pub fn record_cmd_history(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        if self.cmd_history.len() >= CMD_HISTORY_MAX {
            self.cmd_history.remove(0);
        }
        self.cmd_history.push(line.to_owned());
    }

    /// 登记新后台作业并返回其 id（M13-8）。
    pub fn add_job(&mut self, kind: &str, src_count: usize, target: &str) -> usize {
        let id = self.jobs.len() + 1;
        self.jobs.push(Job {
            id,
            kind: kind.to_owned(),
            src_count,
            target: target.to_owned(),
            status: JobStatus::Running,
            cancel_requested: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        });
        id
    }

    /// M24-9：请求取消运行中作业（协作式——后台线程在文件边界检查标志）。
    /// 返回是否发出请求（索引越界 / 非 Running / 已请求过 → false）。
    pub fn cancel_job(&mut self, index: usize) -> bool {
        let Some(job) = self.jobs.get(index) else {
            return false;
        };
        if job.status != JobStatus::Running
            || job
                .cancel_requested
                .load(std::sync::atomic::Ordering::Relaxed)
        {
            return false;
        }
        job.cancel_requested
            .store(true, std::sync::atomic::Ordering::Relaxed);
        true
    }

    /// 应用后台作业更新包：收撤销栈 + 更新作业状态 + 刷新所在 tab。
    /// 返回是否产生了可撤销变更（供状态栏提示）。
    pub fn apply_job_update(&mut self, update: JobUpdate) -> bool {
        let changed = !update.ops.is_empty();
        for op in update.ops {
            self.undo_stack.push(op);
        }
        if let Some(job) = self.jobs.iter_mut().find(|j| j.id == update.job_id) {
            job.status = update.status;
        }
        let cfg = self.config.clone();
        if let Some(tab) = self.tabs.get_mut(update.tab_id) {
            let _ = tab.refresh(cfg);
        }
        changed
    }

    pub fn active_tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    /// 当前 tab 的可变访问（j/k、目录切换等动作使用）。
    pub fn active_tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    /// 新建标签并切换为 active（M1-5 `:tabnew`）。
    pub fn push_tab(&mut self, tab: Tab) {
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
    }

    /// 切换到下一个 tab（循环；M1-5 `gt`）。
    pub fn next_tab(&mut self) {
        self.active = (self.active + 1) % self.tabs.len();
    }

    /// 切换到上一个 tab（循环；M1-5 `gT`）。
    pub fn prev_tab(&mut self) {
        self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
    }

    /// 跳到第 n 个 tab（1 起始，越界钳制；M1-5 `N gt`）。
    pub fn goto_tab(&mut self, n: usize) {
        if self.tabs.is_empty() {
            return;
        }
        self.active = n.saturating_sub(1).min(self.tabs.len() - 1);
    }

    /// 关闭当前 tab；若关闭的是最后一个 tab 返回 `true`（调用方退出应用）。
    pub fn close_tab(&mut self) -> bool {
        self.tabs.remove(self.active);
        if self.tabs.is_empty() {
            return true;
        }
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        }
        false
    }

    /// 移动当前 tab 到 1 起始位置 `n`；`None` 表示移到末尾（M1-5 `:tabmove`）。
    pub fn move_tab(&mut self, n: Option<usize>) {
        let tab = self.tabs.remove(self.active);
        let idx = n
            .map(|n| n.saturating_sub(1).min(self.tabs.len()))
            .unwrap_or(self.tabs.len());
        self.tabs.insert(idx, tab);
        self.active = idx;
    }

    /// 以当前 tab 的落点目录新建标签（M1-5 `:tabnew`）。
    /// 目录读取失败时返回 `false`，不产生副作用。
    pub fn new_tab_here(&mut self) -> bool {
        let dir = self.active_tab().location.base_dir().to_path_buf();
        match Tab::from_physical(&dir, self.config.clone()) {
            Ok(tab) => {
                self.push_tab(tab);
                true
            }
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tab::{Location, SearchState, Tab};
    use std::collections::HashSet;
    use std::path::PathBuf;

    fn tab_at(path: &str) -> Tab {
        Tab {
            location: Location::Physical(PathBuf::from(path)),
            entries: Vec::new(),
            cursor: 0,
            scroll: 0,
            search: None,
            history: Vec::new(),
            hist_pos: 0,
            results: None,
            viewer: None,
            pane_frecency: None,
            sort: None,
            pre_remote: None,
            visual: None,
            filter: None,
            tags: HashSet::new(),
            char_search: None,
            view_mode: false,
            tree: None,
            right: crate::tab::Pane::from_physical(std::path::Path::new("."), Config::default())
                .unwrap(),
            focus: crate::tab::Focus::Left,
            on_parent_row: false,
        }
    }

    fn tab(path: &str) -> Tab {
        Tab {
            location: Location::Physical(PathBuf::from(path)),
            entries: Vec::new(),
            cursor: 0,
            scroll: 0,
            search: None,
            history: Vec::new(),
            hist_pos: 0,
            results: None,
            viewer: None,
            pane_frecency: None,
            sort: None,
            pre_remote: None,
            visual: None,
            filter: None,
            tags: HashSet::new(),
            char_search: None,
            view_mode: false,
            tree: None,
            right: crate::tab::Pane::from_physical(std::path::Path::new("."), Config::default())
                .unwrap(),
            focus: crate::tab::Focus::Left,
            on_parent_row: false,
        }
    }

    #[test]
    fn starts_with_single_active_tab() {
        let s = AppState::new(tab("C:/a"));
        assert_eq!(s.tabs.len(), 1);
        assert_eq!(s.active, 0);
        assert_eq!(
            s.active_tab().location,
            Location::Physical(PathBuf::from("C:/a"))
        );
    }

    #[test]
    fn tabs_hold_independent_state() {
        let mut s = AppState::new(tab("C:/a"));
        s.active_tab_mut().cursor = 3;
        s.active_tab_mut().scroll = 5;

        s.push_tab(tab("C:/b"));
        assert_eq!(s.active, 1);
        assert_eq!(
            s.active_tab().location,
            Location::Physical(PathBuf::from("C:/b"))
        );
        // 切换后 tab 0 的光标与滚动独立保持。
        assert_eq!(s.tabs[0].cursor, 3);
        assert_eq!(s.tabs[0].scroll, 5);
    }

    #[test]
    fn per_tab_state_independent() {
        // M1-6 验收：两个 tab 打开不同目录，切换后各自状态独立保持。
        let mut s = AppState::new(tab_at("C:/a"));
        s.push_tab(tab_at("C:/b"));
        s.active = 0; // push_tab 会切到新 tab，归位到第一个

        s.active_tab_mut().cursor = 3;
        s.active_tab_mut().scroll = 5;
        s.active_tab_mut().search = Some(SearchState {
            query: "abc".to_owned(),
        });

        // 切到 tab b：未被 a 的状态污染。
        s.next_tab();
        assert_eq!(
            s.active_tab().location,
            Location::Physical(PathBuf::from("C:/b"))
        );
        assert_eq!(s.active_tab().cursor, 0);
        assert_eq!(s.active_tab().scroll, 0);
        assert!(s.active_tab().search.is_none());
        s.active_tab_mut().cursor = 7;
        s.active_tab_mut().scroll = 9;

        // 切回 tab a：光标、滚动、搜索状态全部保持。
        s.prev_tab();
        assert_eq!(
            s.active_tab().location,
            Location::Physical(PathBuf::from("C:/a"))
        );
        assert_eq!(s.active_tab().cursor, 3);
        assert_eq!(s.active_tab().scroll, 5);
        assert_eq!(s.active_tab().search.as_ref().unwrap().query, "abc");
    }

    #[test]
    fn undo_stack_lifo_with_tab_and_path() {
        use std::path::PathBuf;

        let mut s = AppState::new(tab_at("C:/a"));
        assert!(s.undo_stack.is_empty());

        // 两次操作：tab 0 删除、tab 1 移动。
        s.undo_stack.push(UndoableOp::Delete {
            tab_id: 0,
            path: PathBuf::from("C:/a/x.txt"),
            staged: PathBuf::from("C:/trash/1-x.txt"),
        });
        s.undo_stack.push(UndoableOp::Move {
            tab_id: 1,
            from: PathBuf::from("C:/b/y.txt"),
            to: PathBuf::from("C:/b/z/y.txt"),
        });
        assert_eq!(s.undo_stack.len(), 2);

        // LIFO：先弹 tab 1 的移动，再弹 tab 0 的删除（`u` 逆序撤销）。
        let op = s.undo_stack.pop().expect("stack should not be empty");
        assert_eq!(op.tab_id(), 1);
        assert_eq!(op.primary_path(), &PathBuf::from("C:/b/y.txt"));
        assert!(matches!(op, UndoableOp::Move { .. }));

        let op = s.undo_stack.pop().expect("stack should not be empty");
        assert_eq!(op.tab_id(), 0);
        assert!(matches!(op, UndoableOp::Delete { .. }));
        assert!(s.undo_stack.is_empty());
        assert_eq!(s.undo_stack.pop(), None);
    }

    #[test]
    fn job_update_completes_and_pushes_undo_ops() {
        // AUDIT-12：后台作业回包语义——ops 入撤销栈（可 u 撤销）、状态
        // 终态化、changed 反映"有撤销记录"。
        let mut s = AppState::new(tab("C:/a"));
        let id = s.add_job("copy", 1, "C:/b");
        assert!(matches!(s.jobs[0].status, JobStatus::Running));
        let changed = s.apply_job_update(crate::state::JobUpdate {
            job_id: id,
            tab_id: 0,
            ops: vec![UndoableOp::Create {
                tab_id: 0,
                path: PathBuf::from("C:/b/x.zip"),
                recursive: false,
            }],
            status: JobStatus::Done {
                summary: "copy: 完成".to_owned(),
            },
        });
        assert!(changed, "带 ops 的回包 → changed");
        assert_eq!(s.undo_stack.len(), 1, "作业产物入撤销栈");
        assert!(matches!(&s.jobs[0].status, JobStatus::Done { .. }));
        // 取消终态（M24-9）：无 ops → 不入撤销栈、changed=false。
        let changed = s.apply_job_update(crate::state::JobUpdate {
            job_id: id,
            tab_id: 0,
            ops: Vec::new(),
            status: JobStatus::Cancelled {
                summary: "copy: 已取消".to_owned(),
            },
        });
        assert!(!changed);
        assert!(matches!(&s.jobs[0].status, JobStatus::Cancelled { .. }));
        assert_eq!(s.undo_stack.len(), 1, "取消不追加撤销记录");
    }
}
