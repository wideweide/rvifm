//! `AppState`：tabs 列表 + active 索引 + 全局撤销栈（SPEC §4.2 / §4.3）。

use std::path::PathBuf;

use crate::config::Config;
use crate::tab::Tab;

/// M83：文本预览/高亮缓存键（路径 + mtime 秒 + 字节数）。
pub type HighlightCacheKey = (PathBuf, u64, u64);

/// M85：图片 resize+encode 结果（Err 表示编码失败，需节流重试）。
pub type ResizeResult = Result<ratatui_image::thread::ResizeResponse, ()>;

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

/// M91-1：传输任务状态机（队列条目携带进度与重试次数）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransferState {
    #[default]
    Pending,
    Running,
    Paused,
    Done,
    Failed,
    Cancelled,
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
    /// M91-1：任务状态、已传字节、重试次数与最近错误。
    pub state: TransferState,
    pub transferred: u64,
    pub retries: u32,
    pub error: Option<String>,
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
    /// M49-7：`v` 多选 `D` 永久删除确认标记（由 ConfirmTagDelete 消费）。
    pub pending_permanent_delete: bool,
    /// M86-8：粘贴冲突选项列表（同路径/异路径选项集不同）。
    pub pending_paste_same_dir: bool,
    /// M87-4：外部命令 Normal 态 dd 删除后供 u 恢复。
    pub last_removed_external: Option<String>,
    /// M19-2：远端会话表（会话名 → 协议后端；`Box<dyn RemoteFs>`）。
    pub remote_sessions: std::collections::BTreeMap<String, Box<dyn crate::remote::RemoteFs>>,
    /// M36-7：传输队列（粘贴入队；`:queue` 覆盖层消费/取消）。
    pub transfer_queue: Vec<TransferQueueEntry>,
    /// M50-3：shell 历史完整命令行（仅内存补全候选，不入库）。
    pub history_commands: Vec<String>,
    /// M54 ZKEY-3：zR 保存的会话内过滤器快照（不写配置、退出不保留）。
    pub filter_snapshot: Option<crate::state::FilterSnapshot>,
    /// M55-2：`O` 实时递归查找会话（仅内存，退出即丢弃）。
    pub live_find: Option<crate::state::LiveFindState>,
    /// M60-1：预览后台 worker 回传通道 + 代次（只接受当前代次的结果）。
    pub preview_tx: std::sync::mpsc::Sender<crate::state::PreviewJobResult>,
    pub preview_rx: std::sync::mpsc::Receiver<crate::state::PreviewJobResult>,
    pub preview_generation: u64,
    /// M93-1：`:du` 后台扫描通道与结果。
    pub du_tx: std::sync::mpsc::Sender<DuResult>,
    pub du_rx: std::sync::mpsc::Receiver<DuResult>,
    pub du_result: Option<DuResult>,
    /// M92-1：目录异步加载通道与代次（UI 线程不阻塞 read_dir/stat）。
    pub dir_load_tx: std::sync::mpsc::Sender<DirLoadResult>,
    pub dir_load_rx: std::sync::mpsc::Receiver<DirLoadResult>,
    pub dir_generation: u64,
    /// M61：当前大文件的只读 mmap 视口读取器（路径变化时重建）。
    pub viewport_text: Option<crate::viewport::ViewportText>,
    /// M69：`:diff` 只读 diff overlay 会话（仅内存，退出即丢弃）。
    pub git_diff: Option<GitDiffSession>,
    /// M71：最近一次 hunk 丢弃的原文，供 `u` 立即撤销。
    pub git_undo: Option<(PathBuf, Vec<u8>)>,
    /// M63-1：目录树预览快照缓存（按目录 mtime 失效，避免每帧重扫）。
    pub directory_preview_cache:
        std::cell::RefCell<std::collections::HashMap<(PathBuf, u64), Vec<crate::tab::FileItem>>>,
    /// M61-4/M83：同一文件跨帧复用 syntect 状态；key 含 mtime/size，内容
    /// 变化即重建，避免"高亮闪一下消失"后一直用旧语法。
    pub highlighter_cache:
        std::cell::RefCell<Option<(HighlightCacheKey, crate::highlight::Highlighter)>>,
    /// M60-3：后台解码完成的图片（路径 + DynamicImage）与在途目标。
    pub decoded_preview: Option<(PathBuf, image::DynamicImage)>,
    pub image_pending: Option<PathBuf>,
    /// M85：图片 job 独立代次——viewer job 递增不再丢弃在途图片结果。
    pub image_generation: u64,
    /// M85：当前图片的线程化协议（每图独立通道，避免 id 重置碰撞）。
    pub thread_preview: std::cell::RefCell<Option<ThreadPreview>>,
    /// M60-4：Office/归档/PDF 文本预览的后台行缓存（仅内存）。
    pub special_preview: Option<(PathBuf, Vec<String>)>,
    pub special_pending: Option<PathBuf>,
    /// M53-5/M83：文本预览行缓存（key = path+mtime+size），`Arc` 让命中
    /// 只克隆指针，避免每帧整文件 `Vec<String>` 拷贝。
    pub text_preview_cache: std::cell::RefCell<
        std::collections::HashMap<HighlightCacheKey, std::sync::Arc<Vec<String>>>,
    >,
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
    /// M79-2：git 缓存写入时刻（TTL 到期重新查询，覆盖外部终端提交）。
    pub git_cache_at: std::collections::HashMap<PathBuf, std::time::Instant>,
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

/// M54 ZKEY-3：zR 过滤器临时快照（隐藏开关 + 各 tab 左右过滤模式）。
#[derive(Debug, Clone)]
pub struct FilterSnapshot {
    pub show_hidden: bool,
    pub left: Vec<Option<String>>,
    pub right: Vec<Option<String>>,
}

/// M55-2/3：实时递归查找会话（仅内存；后台流式扫描 + 增量子串过滤）。
#[derive(Debug)]
pub struct LiveFindState {
    pub root: PathBuf,
    pub query: String,
    /// 已扫描到的全部文件（按到达顺序）。
    pub all: Vec<crate::search::ResultItem>,
    /// 当前 query 命中的 `all` 下标。
    pub filtered: Vec<usize>,
    pub cursor: usize,
    pub scanning: bool,
    pub truncated: bool,
    /// 退出时置位，后台线程据此停止遍历。
    pub stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub rx: std::sync::mpsc::Receiver<crate::search::LiveFindUpdate>,
}

impl LiveFindState {
    /// 启动后台扫描（复用 `:find` 的 ignore 语义），初始 query 为空 = 全部文件。
    pub fn new(root: PathBuf, show_hidden: bool) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        crate::search::spawn_live_find(tx, root.clone(), show_hidden, stop.clone());
        Self {
            root,
            query: String::new(),
            all: Vec::new(),
            filtered: Vec::new(),
            cursor: 0,
            scanning: true,
            truncated: false,
            stop,
            rx,
        }
    }

    /// 大小写不敏感子串匹配（多词 AND；不做模糊评分/正则）。
    /// 只匹配**相对扫描根的路径**，避免根目录名（如临时目录的随机名）干扰命中。
    fn matches(item: &crate::search::ResultItem, query: &str, root: &std::path::Path) -> bool {
        let terms: Vec<String> = query
            .split_whitespace()
            .map(|t| t.to_lowercase())
            .filter(|t| !t.is_empty())
            .collect();
        if terms.is_empty() {
            return true;
        }
        let rel = item.path.strip_prefix(root).unwrap_or(&item.path);
        let hay = rel.to_string_lossy().to_lowercase();
        terms.iter().all(|t| hay.contains(t))
    }

    /// query 变更时从全量重建命中下标。
    pub fn refilter(&mut self) {
        self.filtered = self
            .all
            .iter()
            .enumerate()
            .filter_map(|(i, item)| Self::matches(item, &self.query, &self.root).then_some(i))
            .collect();
        self.cursor = self.cursor.min(self.filtered.len().saturating_sub(1));
    }

    /// 每帧抽干至多 `limit` 条扫描更新；新条目只追加命中项（增量过滤）。
    pub fn drain(&mut self, limit: usize) {
        let mut got = 0;
        while got < limit {
            match self.rx.try_recv() {
                Ok(crate::search::LiveFindUpdate::Entry(item)) => {
                    let hit = Self::matches(&item, &self.query, &self.root);
                    self.all.push(item);
                    if hit {
                        self.filtered.push(self.all.len() - 1);
                    }
                    got += 1;
                }
                Ok(crate::search::LiveFindUpdate::Done { truncated }) => {
                    self.scanning = false;
                    self.truncated = truncated;
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.scanning = false;
                    break;
                }
            }
        }
    }

    /// 当前命中的条目（若列表非空）。
    pub fn selected(&self) -> Option<&crate::search::ResultItem> {
        self.filtered
            .get(self.cursor)
            .and_then(|i| self.all.get(*i))
    }
}

/// M60-1/3：后台预览结果（代次不匹配即丢弃，快速导航不渲染中间预览）。
#[derive(Debug)]
pub enum PreviewJobKind {
    Viewer,
    Image,
    Special,
}

#[derive(Debug)]
pub struct PreviewJobResult {
    pub generation: u64,
    pub kind: PreviewJobKind,
    pub payload: crate::preview::ViewerPayload,
    pub image: Option<image::DynamicImage>,
}

/// M93-1：磁盘占用后台扫描结果。
pub struct DuResult {
    pub base: PathBuf,
    pub entries: Vec<(String, u64)>,
    pub total: u64,
    pub scanned: usize,
}

/// M92-1：异步目录加载结果（generation 丢弃过期结果）。
pub struct DirLoadResult {
    pub generation: u64,
    pub tab_id: usize,
    pub right: bool,
    pub path: PathBuf,
    pub entries: std::io::Result<Vec<crate::tab::FileItem>>,
}

/// M85：单张图片的线程化协议状态。每图独立请求/响应通道：切换图片时旧
/// `ThreadProtocol` 连同发送端被丢弃，在途的旧 id 请求无法串进新图（H3）。
pub struct ThreadPreview {
    pub path: PathBuf,
    pub rect: ratatui::layout::Rect,
    pub proto: ratatui_image::thread::ThreadProtocol,
    pub rx: std::sync::mpsc::Receiver<ResizeResult>,
    /// 编码失败后的节流重试时刻：一次失败不能永久空白。
    pub retry_after: Option<std::time::Instant>,
}

/// M70：git 写操作确认载荷（仅 overlay 内使用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitOp {
    StageAll,
    Amend,
    DiscardFile(String),
    Commit(String),
    Pull,
    Push,
    /// M71：实验性块级丢弃（路径、hunk 下标、预览文本）。
    DiscardHunk(String, usize, String),
}

/// M69：diff overlay 的面板焦点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffFocus {
    Files,
    Patch,
}

/// M69：diff overlay 会话（只读 git 命令 + 异步解析结果）。
#[derive(Debug)]
pub struct GitDiffSession {
    pub root: PathBuf,
    pub files: Vec<(String, char, char)>,
    pub cursor: usize,
    pub focus: DiffFocus,
    pub data: std::collections::HashMap<String, crate::gitdiff::DiffFileData>,
    pub side_by_side: bool,
    /// M82：`|` 同时切到全屏左右对照（文件列表让位）。
    pub fullscreen: bool,
    pub full: bool,
    pub wrap: bool,
    pub patch_scroll: usize,
    pub hunk: usize,
    pub hscroll: usize,
    pub query: Option<String>,
    pub generation: u64,
    pub tx: std::sync::mpsc::Sender<(u64, String, std::io::Result<crate::gitdiff::DiffFileData>)>,
    pub rx: std::sync::mpsc::Receiver<(u64, String, std::io::Result<crate::gitdiff::DiffFileData>)>,
    pub pending: std::collections::HashSet<String>,
    pub last_error: Option<String>,
}

impl GitDiffSession {
    pub fn open(root: PathBuf) -> std::io::Result<Self> {
        let files = crate::git::changed_files(&root)?;
        let (tx, rx) = std::sync::mpsc::channel();
        Ok(Self {
            root,
            files,
            cursor: 0,
            focus: DiffFocus::Files,
            data: std::collections::HashMap::new(),
            side_by_side: false,
            fullscreen: false,
            full: false,
            wrap: true,
            patch_scroll: 0,
            hunk: 0,
            hscroll: 0,
            query: None,
            generation: 0,
            tx,
            rx,
            pending: std::collections::HashSet::new(),
            last_error: None,
        })
    }

    pub fn selected_path(&self) -> Option<&str> {
        self.files.get(self.cursor).map(|(p, _, _)| p.as_str())
    }

    /// M69-2：投递后台 diff 任务（staged = index 版本 vs HEAD）。
    pub fn request(&mut self, path: &str, staged: bool, context: u32) {
        if self.data.contains_key(path) || self.pending.contains(path) {
            return;
        }
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.pending.insert(path.to_owned());
        let root = self.root.clone();
        let target = path.to_owned();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = crate::git::diff_payload(&root, &target, staged, context);
            let _ = tx.send((generation, target, result));
        });
    }

    pub fn drain(&mut self) {
        while let Ok((generation, path, result)) = self.rx.try_recv() {
            // M72-2：过期结果也必须清 pending，否则该路径永远停在「正在读取」。
            self.pending.remove(&path);
            if generation != self.generation {
                continue;
            }
            match result {
                Ok(data) => {
                    self.data.insert(path, data);
                }
                Err(e) => self.last_error = Some(e.to_string()),
            }
        }
    }

    pub fn selected(&self) -> Option<&crate::gitdiff::DiffFileData> {
        self.selected_path().and_then(|p| self.data.get(p))
    }

    /// 当前补丁面板的渲染行与 hunk 起始行（供滚动、搜索与 J/K 共用）。
    pub fn rows(&self, width: usize) -> (Vec<String>, Vec<usize>) {
        let Some(data) = self.selected() else {
            return (vec!["（正在读取 diff…）".to_owned()], Vec::new());
        };
        if self.side_by_side {
            let (pairs, starts) = crate::gitdiff::render_side_by_side(
                data,
                self.full,
                self.wrap,
                width.saturating_sub(2),
            );
            let half = width / 2;
            let rows = pairs
                .into_iter()
                .map(|(l, r)| format!("{:<w$}│{}", l, r, w = half.saturating_sub(1)))
                .collect();
            (rows, starts)
        } else {
            crate::gitdiff::render_unified(&data.hunks, self.wrap, width)
        }
    }
}

impl AppState {
    /// M60-1：目标稳定后投递后台预览任务（不阻塞 UI 线程）。
    pub fn refresh_viewer_async(&mut self) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let Some(target) = tab.refresh_viewer_follow() else {
            return;
        };
        self.preview_generation = self.preview_generation.wrapping_add(1);
        let generation = self.preview_generation;
        let tx = self.preview_tx.clone();
        let threshold = self.config.preview.virtual_text_threshold;
        std::thread::spawn(move || {
            let payload = crate::preview::load_viewer_payload_at(&target, threshold);
            let _ = tx.send(PreviewJobResult {
                generation,
                kind: PreviewJobKind::Viewer,
                payload,
                image: None,
            });
        });
    }

    /// M60-3/M85：图片解码投递后台；同路径已有解码或在途则跳过。
    /// M85：使用**独立代次**，viewer job 递增不会让图片结果被丢弃。
    pub fn ensure_image_preview(&mut self, path: &std::path::Path) {
        if self
            .decoded_preview
            .as_ref()
            .is_some_and(|(p, _)| p == path)
        {
            return;
        }
        if self.image_pending.as_deref() == Some(path) {
            return;
        }
        self.image_pending = Some(path.to_path_buf());
        let target = path.to_path_buf();
        self.image_generation = self.image_generation.wrapping_add(1);
        let generation = self.image_generation;
        let max_side = self.config.preview.image_max_side.clamp(1, 4096);
        let tx = self.preview_tx.clone();
        std::thread::spawn(move || {
            let image = crate::image::decode_for_preview(&target, max_side).ok();
            let _ = tx.send(PreviewJobResult {
                generation,
                kind: PreviewJobKind::Image,
                payload: crate::preview::ViewerPayload {
                    path: target,
                    lines: Vec::new(),
                    total: 0,
                    hex: false,
                },
                image,
            });
        });
    }

    /// M92-2：按可见行补齐 size/mtime/perm（懒加载属性第二阶段）。
    pub fn fill_visible_attrs(&mut self, tab_id: usize, right: bool, start: usize, count: usize) {
        let Some(tab) = self.tabs.get_mut(tab_id) else {
            return;
        };
        let base = if right {
            &tab.right.location
        } else {
            &tab.location
        };
        let crate::tab::Location::Physical(dir) = base else {
            return;
        };
        let entries = if right {
            &mut tab.right.entries
        } else {
            &mut tab.entries
        };
        let len = entries.len();
        let begin = start.min(len);
        let end = start.saturating_add(count).min(len);
        for item in entries[begin..end].iter_mut() {
            if item.mtime != 0 || !item.perm.is_empty() {
                continue;
            }
            let path = dir.join(&item.name);
            let Ok(meta) = std::fs::metadata(&path).or_else(|_| std::fs::symlink_metadata(&path))
            else {
                continue;
            };
            item.size = meta.len();
            item.perm = crate::tab::perm_string(&meta);
            item.mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
        }
    }

    /// M92-1：投递目录异步加载；UI 线程保留旧列表直到结果到达。
    pub fn request_dir_load(
        &mut self,
        tab_id: usize,
        right: bool,
        dir: &std::path::Path,
        config: Config,
    ) {
        self.dir_generation = self.dir_generation.wrapping_add(1);
        let generation = self.dir_generation;
        let tx = self.dir_load_tx.clone();
        let path = dir.to_path_buf();
        #[cfg(test)]
        {
            // 测试保持同步语义，避免异步时序让断言不稳定。
            let entries = crate::tab::Tab::load_for(
                &crate::tab::Location::Physical(path.clone()),
                config.clone(),
            );
            let _ = tx.send(DirLoadResult {
                generation,
                tab_id,
                right,
                path,
                entries,
            });
            self.drain_dir_loads(&config);
            return;
        }
        #[cfg(not(test))]
        std::thread::spawn(move || {
            let entries =
                crate::tab::Tab::load_for(&crate::tab::Location::Physical(path.clone()), config);
            let _ = tx.send(DirLoadResult {
                generation,
                tab_id,
                right,
                path,
                entries,
            });
        });
        #[cfg(not(test))]
        let _ = tx;
    }

    /// M92-1：收割目录加载结果；仅当前代次且路径仍匹配才应用。
    pub fn drain_dir_loads(&mut self, config: &Config) {
        while let Ok(result) = self.dir_load_rx.try_recv() {
            if result.generation != self.dir_generation {
                continue;
            }
            let Some(tab) = self.tabs.get_mut(result.tab_id) else {
                continue;
            };
            let (loc, entries_ref, cursor_ref, filter_ref, sort_ref) = if result.right {
                (
                    &tab.right.location,
                    &mut tab.right.entries,
                    &mut tab.right.cursor,
                    &tab.right.filter,
                    &tab.right.sort,
                )
            } else {
                (
                    &tab.location,
                    &mut tab.entries,
                    &mut tab.cursor,
                    &tab.filter,
                    &tab.sort,
                )
            };
            if loc.base_dir() != result.path.as_path() {
                continue;
            }
            let Ok(mut loaded) = result.entries else {
                continue;
            };
            if let Some(pat) = filter_ref {
                loaded.retain(|item| crate::tab::filter_matches(pat, &item.name));
            }
            if let Some(specs) = sort_ref {
                crate::tab::sort_entries_specs(&mut loaded, specs, config);
            }
            *entries_ref = loaded;
            *cursor_ref = (*cursor_ref).min(entries_ref.len().saturating_sub(1));
        }
    }

    /// M86-8：粘贴冲突选项——同路径（原地粘贴）只给 rename/skip/skip all；
    /// 跨路径追加 append the tail / overwrite / overwrite all。
    pub fn paste_conflict_options(&self) -> Vec<&'static str> {
        if self.pending_paste_same_dir {
            vec!["rename", "skip", "skip all"]
        } else {
            vec![
                "rename",
                "skip",
                "skip all",
                "append the tail",
                "overwrite",
                "overwrite all",
            ]
        }
    }

    /// M85：收割当前图片的 resize+encode 结果；失败时清空协议并节流重试。
    pub fn drain_image_resizes(&self) {
        while let Ok(mut guard) = self.thread_preview.try_borrow_mut()
            && let Some(preview_state) = guard.as_mut()
            && let Ok(result) = preview_state.rx.try_recv()
        {
            match result {
                Ok(response) => {
                    preview_state.proto.update_resized_protocol(response);
                }
                Err(()) => {
                    preview_state.proto.empty_protocol();
                    preview_state.retry_after =
                        Some(std::time::Instant::now() + std::time::Duration::from_millis(1000));
                }
            }
        }
    }

    /// M60-4：Office/归档/PDF 文本预览行后台计算（缓存未命中时）。
    pub fn ensure_special_preview(&mut self, path: &std::path::Path, external_enabled: bool) {
        if self
            .special_preview
            .as_ref()
            .is_some_and(|(p, _)| p == path)
        {
            return;
        }
        if self.special_pending.as_deref() == Some(path) {
            return;
        }
        self.special_pending = Some(path.to_path_buf());
        let target = path.to_path_buf();
        self.preview_generation = self.preview_generation.wrapping_add(1);
        let generation = self.preview_generation;
        let tx = self.preview_tx.clone();
        std::thread::spawn(move || {
            let lines = crate::preview::load_special_preview_lines(&target, external_enabled);
            let _ = tx.send(PreviewJobResult {
                generation,
                kind: PreviewJobKind::Special,
                payload: crate::preview::ViewerPayload {
                    path: target,
                    lines,
                    total: 0,
                    hex: false,
                },
                image: None,
            });
        });
    }

    /// M60-1/3/4/M85：收割后台结果。图片用独立代次（`image_generation`），
    /// viewer/特殊预览仍用 `preview_generation`；过期图片结果也会清 pending，
    /// 避免「图片解码中…」永久卡住。
    pub fn drain_preview_results(&mut self) {
        while let Ok(result) = self.preview_rx.try_recv() {
            match result.kind {
                PreviewJobKind::Image => {
                    if self.image_pending.as_deref() == Some(result.payload.path.as_path()) {
                        self.image_pending = None;
                    }
                    if result.generation != self.image_generation {
                        continue;
                    }
                    if let Some(img) = result.image {
                        self.decoded_preview = Some((result.payload.path, img));
                    }
                }
                PreviewJobKind::Special => {
                    if result.generation != self.preview_generation {
                        continue;
                    }
                    if self.special_pending.as_deref() == Some(result.payload.path.as_path()) {
                        self.special_pending = None;
                    }
                    self.special_preview = Some((result.payload.path, result.payload.lines));
                }
                PreviewJobKind::Viewer => {
                    if result.generation != self.preview_generation {
                        continue;
                    }
                    let Some(tab) = self.tabs.get_mut(self.active) else {
                        return;
                    };
                    if !tab.view_mode || tab.preview_focus {
                        continue;
                    }
                    tab.viewer = Some(crate::tab::ViewerState {
                        scroll: 0,
                        line_count: result.payload.lines.len(),
                        total_lines: result.payload.total,
                        query: None,
                        match_line: 0,
                        hex: result.payload.hex,
                        page: 0,
                        path: Some(result.payload.path),
                    });
                }
            }
        }
    }

    /// 以一个初始标签启动（默认允许图片协议查询）。
    pub fn new(initial: Tab) -> Self {
        Self::new_with_image_query(initial, true)
    }

    /// M85：允许调用方在构造前按配置关闭图片协议查询（免去 2s 等待）。
    pub fn new_with_image_query(initial: Tab, allow_image_query: bool) -> Self {
        let t0 = std::time::Instant::now();
        let image_engine = crate::image::ImageEngine::detect_with(allow_image_query);
        if std::env::var_os("RVIFM_STARTUP_TRACE").is_some() {
            eprintln!("appstate image_engine: {:?}", t0.elapsed());
        }
        let tutor_progress = crate::tutor::load_progress();
        if std::env::var_os("RVIFM_STARTUP_TRACE").is_some() {
            eprintln!("appstate tutor_progress: {:?}", t0.elapsed());
        }
        let preview_channel = std::sync::mpsc::channel();
        let (dir_tx, dir_rx) = std::sync::mpsc::channel();
        let (du_tx, du_rx) = std::sync::mpsc::channel();
        Self {
            tabs: vec![initial],
            active: 0,
            undo_stack: UndoStack::default(),
            config: Config::default(),
            image_engine,
            jobs: Vec::new(),
            yank: None,
            pending_paste_conflicts: Vec::new(),
            pending_paste_move: false,
            pending_permanent_delete: false,
            pending_paste_same_dir: false,
            last_removed_external: None,
            remote_sessions: std::collections::BTreeMap::new(),
            transfer_queue: Vec::new(),
            history_commands: Vec::new(),
            filter_snapshot: None,
            live_find: None,
            preview_tx: preview_channel.0,
            preview_rx: preview_channel.1,
            preview_generation: 0,
            dir_load_tx: dir_tx,
            dir_load_rx: dir_rx,
            dir_generation: 0,
            du_tx,
            du_rx,
            du_result: None,
            viewport_text: None,
            git_diff: None,
            git_undo: None,
            directory_preview_cache: std::cell::RefCell::new(std::collections::HashMap::new()),
            highlighter_cache: std::cell::RefCell::new(None),
            decoded_preview: None,
            image_pending: None,
            image_generation: 0,
            thread_preview: std::cell::RefCell::new(None),
            special_preview: None,
            special_pending: None,
            text_preview_cache: std::cell::RefCell::new(std::collections::HashMap::new()),
            watcher: None,
            watch_rx: None,
            live_recorded: std::collections::HashMap::new(),
            temp_opens: Vec::new(),
            upload_tx: None,
            cmd_history: Vec::new(),
            git_cache: std::collections::HashMap::new(),
            git_cache_at: std::collections::HashMap::new(),
            git_pending: std::collections::HashSet::new(),
            git_tx: None,
            pending_external: None,
            named_searches: std::collections::HashMap::new(),
            tutor: None,
            // M45-8d：导师进度缓存（启动时读一次；作答后即时刷新）——列表
            // 左侧进度展示不必每帧读盘。
            tutor_progress,
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
            archive_preview_key: None,
            archive_preview_tmp: None,
            viewer_pending: None,
            archive_preview_pending: None,
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
            archive_preview_key: None,
            archive_preview_tmp: None,
            viewer_pending: None,
            archive_preview_pending: None,
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
