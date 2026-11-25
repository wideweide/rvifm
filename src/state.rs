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

/// M28-3：作业进度计数与采样（线程共享；detail 覆盖层读取）。
/// `record` 节流采样（500ms）；`rate_bytes_per_sec` 取最近 5 秒窗口的
/// 滑动平均速率。锁中毒降级：unwrap_or_else 取守卫数据（AUDIT-8 口径）。
#[derive(Debug, Default)]
pub struct JobProgress {
    pub processed: std::sync::atomic::AtomicU64,
    pub bytes: std::sync::atomic::AtomicU64,
    samples: std::sync::Mutex<Vec<(std::time::Instant, u64, u64)>>,
}

impl JobProgress {
    /// M36-1：空进度实例（同步复制路径的 no-op 控制句柄；函数形态规避
    /// interior-mutable const lint——每次调用产生独立零值实例）。
    pub(crate) fn empty() -> JobProgress {
        JobProgress::default()
    }

    pub(crate) fn record(&self) {
        use std::sync::atomic::Ordering;
        let now = std::time::Instant::now();
        if let Ok(mut v) = self.samples.lock() {
            let due = v
                .last()
                .map(|(t, _, _)| now.duration_since(*t).as_millis() >= 500)
                .unwrap_or(true);
            if due {
                v.push((
                    now,
                    self.processed.load(Ordering::Relaxed),
                    self.bytes.load(Ordering::Relaxed),
                ));
                let len = v.len();
                if len > 64 {
                    v.drain(..len - 64);
                }
            }
        }
    }

    /// 滑动窗口速率（字节/秒）——最近 5 秒窗口首尾样本差；样本不足 → None。
    pub fn rate_bytes_per_sec(&self) -> Option<f64> {
        let v = self.samples.lock().unwrap_or_else(|e| e.into_inner());
        let cutoff = std::time::Instant::now() - std::time::Duration::from_secs(5);
        let (t0, _, b0) = *v.iter().find(|(t, _, _)| *t >= cutoff)?;
        let (t1, _, b1) = *v.last()?;
        let dt = t1.duration_since(t0).as_secs_f64();
        if dt <= 0.0 {
            return None;
        }
        Some((b1.saturating_sub(b0)) as f64 / dt)
    }

    pub(crate) fn processed(&self) -> u64 {
        self.processed.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub(crate) fn bytes(&self) -> u64 {
        self.bytes.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// 后台作业记录（M13-8）：`:jobs` 菜单展示。M24-9 增协作式取消标志——
/// `cancel_requested` 由覆盖层 `x` 置位，后台线程在文件边界检查后终止。
/// M28-3 增暂停标志（文件边界等待）、进度计数/采样（详情覆盖层）、
/// 启动时刻与来源目录（详情显示）。
#[derive(Debug, Clone)]
pub struct Job {
    pub id: usize,
    pub kind: String,
    pub src_count: usize,
    pub src: String,
    pub target: String,
    pub status: JobStatus,
    pub cancel_requested: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub pause: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub progress: std::sync::Arc<JobProgress>,
    pub started: std::time::Instant,
}

/// M28-3：作业执行控制束——取消/暂停标志与进度计数（降低
/// execute_job_body 参数量；生命周期同调用方）。
pub struct JobControls<'a> {
    pub cancel: &'a std::sync::atomic::AtomicBool,
    pub pause: &'a std::sync::atomic::AtomicBool,
    pub progress: &'a JobProgress,
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
    /// M45-7b：归档虚拟目录内 yy——`host` 为归档文件，`YankBuffer.paths`
    /// 为归档内成员路径（非物理路径）；粘贴时按成员解压到焦点 pane。
    Archive {
        host: PathBuf,
    },
}

/// M36-7：传输队列条目（粘贴入队；顺序消费）。
#[derive(Debug, Clone)]
pub struct TransferQueueEntry {
    pub direction: crate::app::TransferDirection,
    /// 上传 = 本地源路径；下载 = 远端源路径。
    pub src: PathBuf,
    /// 目标（远端路径 / 本地路径）。
    pub dest: PathBuf,
    pub tab_id: usize,
    /// 远端会话名（下载用；上传空）。
    pub remote: String,
}

/// M32-1/M36：no-op 控制标志（同步粘贴路径的取消/暂停句柄——恒 false）。
pub static NO_CANCEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub static NO_PAUSE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// M36-5：冲突策略（`[transfer] conflict_policy`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConflictPolicy {
    /// 逐项询问（首期：覆盖确认对话框）。
    #[default]
    Ask,
    /// 直接覆盖。
    Overwrite,
    /// 跳过冲突条目。
    Skip,
    /// 自动改名（`name (1).ext` 起递增找空闲）。
    RenameAuto,
    /// 保留较新（源 mtime 较新 → 覆盖；否则跳过）。
    KeepNewer,
}

impl ConflictPolicy {
    /// TOML 字符串解析（未知值回退 Ask 并由调用方提示）。
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "ask" => Self::Ask,
            "overwrite" => Self::Overwrite,
            "skip" => Self::Skip,
            "rename" => Self::RenameAuto,
            "newer" => Self::KeepNewer,
            _ => return None,
        })
    }
}

/// M36-5：冲突消解结果。
#[derive(Debug, PartialEq)]
pub enum ConflictDecision {
    /// 继续复制/写入目标。
    Proceed,
    /// 跳过该条目。
    Skip,
    /// 用自动改名后的路径继续。
    UseName(PathBuf),
}

/// M36-5：冲突消解纯函数——`src_mtime`（源条目 mtime，秒；不可得 None）、
/// `dest_exists`（目标已存在）、`dest_mtime`（目标 mtime，秒；不可得 None）。
/// `dest` 仅 RenameAuto 用（在此生成空闲名，调用方传探名闭包-free 形态：
/// 生成 `name (N).ext` 递增直到 `free_check` 返回 false）。
pub fn resolve_conflict(
    policy: ConflictPolicy,
    src_mtime: Option<u64>,
    dest_exists: bool,
    dest_mtime: Option<u64>,
) -> ConflictDecision {
    if !dest_exists {
        return ConflictDecision::Proceed;
    }
    match policy {
        ConflictPolicy::Overwrite | ConflictPolicy::Ask => ConflictDecision::Proceed,
        ConflictPolicy::Skip => ConflictDecision::Skip,
        ConflictPolicy::KeepNewer => match (src_mtime, dest_mtime) {
            (Some(s), Some(d)) if s > d => ConflictDecision::Proceed,
            _ => ConflictDecision::Skip,
        },
        ConflictPolicy::RenameAuto => ConflictDecision::UseName(PathBuf::new()), // 占位：调用方生成
    }
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
    /// M48-5：粘贴同名冲突的阻塞确认请求（冲突目标 + 是否移动）。
    pub pending_paste_conflicts: Vec<PathBuf>,
    pub pending_paste_move: bool,
    /// M19-2：远端会话表（会话名 → 协议后端；`Box<dyn RemoteFs>`）。
    pub remote_sessions: std::collections::BTreeMap<String, Box<dyn crate::remote::RemoteFs>>,
    /// M36-7：传输队列（粘贴入队；`:queue` 覆盖层消费/取消）。
    pub transfer_queue: Vec<TransferQueueEntry>,
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
    /// M38-5：命名搜索（`:find save <名>` / `:find load <名>`；仅内存
    /// 不跨会话，SPEC SIMPLIFIED；同名覆盖）。
    pub named_searches: std::collections::HashMap<String, crate::search::SearchResults>,
    /// M43-1：导师会话（覆盖层状态——不占 InputMode，任意模式切换不丢；
    /// None = 未激活。Panel 渲染与 Ctrl+Y/N/Q 前置拦截据此判断）。
    pub tutor: Option<crate::tutor::TutorSession>,
    /// M45-8d：导师学习进度缓存（列表左侧进度展示用；作答后即时刷新）。
    pub tutor_progress: crate::tutor::TutorProgress,
    /// M46-8：归档 `w` 预览成员列表缓存（内存态，按路径+mtime+size 复用，
    /// 避免每帧重新 `open_dispatch` / 全量解压压缩 tar）。
    pub archive_preview_cache: crate::preview::ArchivePreviewCache,
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
            pending_paste_conflicts: Vec::new(),
            pending_paste_move: false,
            remote_sessions: std::collections::BTreeMap::new(),
            transfer_queue: Vec::new(),
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
            named_searches: std::collections::HashMap::new(),
            tutor: None,
            // M45-8d：导师进度缓存（启动时读一次；作答后即时刷新）——列表
            // 左侧进度展示不必每帧读盘。
            tutor_progress: crate::tutor::load_progress(),
            archive_preview_cache: std::cell::RefCell::new(std::collections::HashMap::new()),
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
    pub fn add_job(&mut self, kind: &str, src_count: usize, src: &str, target: &str) -> usize {
        let id = self.jobs.len() + 1;
        self.jobs.push(Job {
            id,
            kind: kind.to_owned(),
            src_count,
            src: src.to_owned(),
            target: target.to_owned(),
            status: JobStatus::Running,
            cancel_requested: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            pause: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            progress: std::sync::Arc::new(JobProgress::default()),
            started: std::time::Instant::now(),
        });
        id
    }

    /// M28-3：暂停/恢复运行中作业（协作式——后台线程在文件边界等待）。
    /// 返回是否生效（越界 / 非 Running / 已请求取消 → false）。
    pub fn pause_job(&mut self, index: usize, pause: bool) -> bool {
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
        job.pause.store(pause, std::sync::atomic::Ordering::Relaxed);
        true
    }

    /// M28-3：作业是否处于暂停态（Running 且标志置位）。
    pub fn job_paused(&self, index: usize) -> bool {
        self.jobs.get(index).is_some_and(|j| {
            j.status == JobStatus::Running && j.pause.load(std::sync::atomic::Ordering::Relaxed)
        })
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
            // M46-5：作业写入可能落在对侧 pane（如 :extract 到对侧），
            // 必须双侧刷新——旧实现只刷左 pane，对侧列表长期陈旧。
            let _ = tab.refresh(cfg.clone());
            let _ = tab.right.refresh(cfg);
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
            pending_rename: None,
            viewer: None,
            pane_frecency: None,
            sort: None,
            pre_remote: None,
            visual: None,
            filter: None,
            tags: HashSet::new(),
            char_search: None,
            view_mode: false,
            preview_focus: false,
            tree: None,
            right: crate::tab::Pane::from_physical(std::path::Path::new("."), Config::default())
                .unwrap(),
            focus: crate::tab::Focus::Left,
            on_parent_row: false,
            name: None,
            cursor_memory: std::collections::HashMap::new(),
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
            pending_rename: None,
            viewer: None,
            pane_frecency: None,
            sort: None,
            pre_remote: None,
            visual: None,
            filter: None,
            tags: HashSet::new(),
            char_search: None,
            view_mode: false,
            preview_focus: false,
            tree: None,
            right: crate::tab::Pane::from_physical(std::path::Path::new("."), Config::default())
                .unwrap(),
            focus: crate::tab::Focus::Left,
            on_parent_row: false,
            name: None,
            cursor_memory: std::collections::HashMap::new(),
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
        let id = s.add_job("copy", 1, "C:/a", "C:/b");
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

    #[test]
    fn m365_resolve_conflict_policies() {
        use super::*;
        // 无冲突 → Proceed。
        assert!(matches!(
            resolve_conflict(ConflictPolicy::Ask, None, false, None),
            ConflictDecision::Proceed
        ));
        // Overwrite → 恒 Proceed。
        assert!(matches!(
            resolve_conflict(ConflictPolicy::Overwrite, None, true, None),
            ConflictDecision::Proceed
        ));
        // Skip → 恒 Skip。
        assert!(matches!(
            resolve_conflict(ConflictPolicy::Skip, Some(5), true, Some(1)),
            ConflictDecision::Skip
        ));
        // KeepNewer：源新 → Proceed；源旧/不可比 → Skip。
        assert!(matches!(
            resolve_conflict(ConflictPolicy::KeepNewer, Some(10), true, Some(5)),
            ConflictDecision::Proceed
        ));
        assert!(matches!(
            resolve_conflict(ConflictPolicy::KeepNewer, Some(5), true, Some(10)),
            ConflictDecision::Skip
        ));
        assert!(matches!(
            resolve_conflict(ConflictPolicy::KeepNewer, None, true, None),
            ConflictDecision::Skip
        ));
        // RenameAuto → UseName 占位（调用方生成空闲名）。
        assert!(matches!(
            resolve_conflict(ConflictPolicy::RenameAuto, None, true, None),
            ConflictDecision::UseName(_)
        ));
    }

    #[test]
    fn m365_conflict_policy_parse() {
        use super::*;
        assert_eq!(
            ConflictPolicy::parse("overwrite"),
            Some(ConflictPolicy::Overwrite)
        );
        assert_eq!(
            ConflictPolicy::parse("newer"),
            Some(ConflictPolicy::KeepNewer)
        );
        assert_eq!(ConflictPolicy::parse("bogus"), None);
    }
}
