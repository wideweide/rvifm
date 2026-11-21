//! 事件循环：绘制目录列表与状态栏 → 阻塞等待事件 → 分发按键。
//!
//! M1-3 起支持多键序列（`g` 前缀：`gg`）；M1-5 起支持多标签
//! （`gt`/`gT`/`N gt`）与最小命令行（`:tabnew` / `:tabclose` / `:tabmove`；
//! 统一命令解析在 M5-4 接入）。

pub(crate) use ratatui::Frame;
pub(crate) use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
pub(crate) use ratatui::layout::{Constraint, Layout};
pub(crate) use ratatui::style::{Color, Modifier, Style};
pub(crate) use ratatui::text::{Line, Span};
pub(crate) use ratatui::widgets::{List, ListItem, ListState, Paragraph};

pub(crate) use std::path::{Path, PathBuf};

pub(crate) use crate::keymap::Action;
pub(crate) use crate::state::AppState;
pub(crate) use crate::tab::{Location, SearchState, Tab};
pub(crate) use crate::{config, ops};

/// 判断按键是否请求退出：`q` 或 `Ctrl-C`。
// AUDIT-5：app.rs 按职责分片（keys/exec/jobs/render/preview）——类型与
// 事件循环 run() 留在根模块（子模块可见父模块私有项，零可见性改动）；
// 移动的顶层项统一 pub(crate)（兄弟模块互访），根重导出供 tests/tests 同名引用。
mod exec;
mod jobs;
mod keys;
mod preview;
mod render;

pub(crate) use exec::*;
pub(crate) use jobs::*;
pub(crate) use keys::*;
pub(crate) use preview::*;
pub(crate) use render::*;

/// Normal 模式输入状态：待决序列 + 数字计数（`N gt` 用，M1-5）。
/// `seq` 由 Keymap 驱动解析（M10-3 接线）。
#[derive(Debug)]
pub(crate) struct InputState {
    seq: Vec<crate::keymap::Key>,
    count: Option<usize>,
    keymap: crate::keymap::Keymap,
    /// M33-2：per-mode 键位表（None = 模式未配置 → 保持内建行为不变）。
    mode_maps: std::collections::BTreeMap<&'static str, crate::keymap::Keymap>,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            seq: Vec::new(),
            count: None,
            keymap: crate::keymap::Keymap::default_map(),
            mode_maps: std::collections::BTreeMap::new(),
        }
    }
}

impl InputState {
    /// M10-5 GAP-1：以（含用户分层的）键位表构造。
    fn with_keymap(keymap: crate::keymap::Keymap) -> Self {
        Self {
            seq: Vec::new(),
            count: None,
            keymap,
            mode_maps: std::collections::BTreeMap::new(),
        }
    }

    /// M33-2：以 per-mode 键位表构造（visual/view/command；None 层 = 空
    /// ModeLayers，不建表）。normal 主表恒为用户分层合并默认。
    fn with_mode_maps(keymap: crate::keymap::Keymap, layers: &crate::config::KeymapLayers) -> Self {
        let mut s = Self::with_keymap(keymap);
        let mk = |ml: &crate::config::ModeLayers| {
            crate::keymap::Keymap::with_layers(
                ml.prepend.clone(),
                ml.append.clone(),
                ml.replace.clone(),
            )
        };
        if !layers.visual.prepend.is_empty()
            || !layers.visual.append.is_empty()
            || layers.visual.replace.is_some()
        {
            s.mode_maps.insert("visual", mk(&layers.visual));
        }
        if !layers.view.prepend.is_empty()
            || !layers.view.append.is_empty()
            || layers.view.replace.is_some()
        {
            s.mode_maps.insert("view", mk(&layers.view));
        }
        s
    }

    /// M33-2：切换到指定模式的键位表（None = 回 normal 主表）。
    /// 序列/计数清空（模式边界语义）。
    fn switch_mode_map(&mut self, mode: Option<&'static str>) {
        self.seq.clear();
        self.count = None;
        match mode {
            Some(m) => {
                if let Some(km) = self.mode_maps.get(m) {
                    self.keymap = km.clone();
                }
            }
            None => {
                let base = crate::keymap::Keymap::with_layers(
                    config::keymap_layers().prepend,
                    config::keymap_layers().append,
                    config::keymap_layers().replace,
                );
                self.keymap = base;
            }
        }
    }
}

/// Normal 模式按键的处理结果。
#[derive(Debug, PartialEq)]
pub(crate) enum Outcome {
    /// 请求进入传输队列覆盖层（M36-7 `:queue`）。
    EnterQueue { cursor: usize },
    /// 无状态变化。
    Continue,
    /// 请求退出应用。
    Quit,
    /// 请求进入命令行模式（`:`）。
    EnterCommand,
    /// 请求进入搜索输入模式（`/`，M1-7）。
    EnterSearch,
    /// 请求进入过滤输入模式（`=`，M4-2）。
    EnterFilter,
    /// 请求进入查看器搜索输入（M17 VIEW-1：查看器内 `/`）。
    EnterViewerSearch,
    /// 请求打开排序对话框（M18-1：`S`）。
    EnterSortDialog,
    /// 请求挂起 TUI 进入 shell（M18-3：`s`；vifm 既有习惯）。
    EnterShell,
    /// 请求确认删除（`dd`/`DD`，M2-2/M13-3）：携带待删完整路径；
    /// permanent = 跳过暂存的不可逆删除（DD）。
    ConfirmDelete(PathBuf, bool, Option<String>),
    /// 打开文件详情对话框（M13-5：Ctrl-G，文本已构造好）。
    FileInfo { path: PathBuf, text: String },
    /// 批量重命名（M13-6：cw——名单待 $EDITOR 编辑）。
    BatchRename { base: PathBuf, names: Vec<String> },
    /// 执行用户自定义命令（M23-11：挂起 TUI 运行后恢复）。
    RunUserCommand { cmd: String },
    /// 打开 Frecency Pane 模式（M23-6：o，数据已在 handle_key 读好）。
    OpenFrecencyPane {
        entries: Vec<crate::frecency::ScoredEntry>,
    },
    /// 打开内联重命名（M21-E4：I/cc/A——光标语义由 input/pos 预填表达）。
    EnterRename {
        path: PathBuf,
        input: String,
        pos: usize,
    },
    /// 用 $EDITOR 打开文件（M23-10：e；run 循环挂起 TUI 执行）。
    EditFile { path: PathBuf },
}

/// 跳转选择器视角（M8-12）：Unified = 合一表 + 全局统一时钟；Dir/File =
/// 仅目录/仅文件 + 独立类型时钟。`o` 用 Unified，`:j`/`:J` 用 Dir/File。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JumpKind {
    Unified,
    Dir,
    File,
    /// M16 NAV-4：近期视角——全部条目按最后访问时间倒序（含会话内合并）。
    Recent,
}

/// 输入模式（M5-4 统一模式系统前先持有最小集合）。
/// M8-12 起 JumpSelect 携带 f64 分数的条目，不再派生 Eq。
/// M30-2：命令历史排序条数上限（`:` 模式 ↑/↓ 数据源规模）。
const CMD_RANKED_LIMIT: usize = 100;

#[derive(Debug, Default, PartialEq)]
pub(crate) enum InputMode {
    #[default]
    Normal,
    /// 命令行模式：正在输入 `:` 后的内容。M24-6 增历史浏览游标（模式局部）
    /// 与进入历史前保存的草稿输入（↓ 越过最新条目时恢复）；M24-7 增 Tab
    /// 补全状态（None = 无进行中的补全）。
    Command {
        input: String,
        hist_pos: Option<usize>,
        draft: String,
        comp: Option<CmdComp>,
        /// M30-2：命令历史排序快照（进入命令模式时构建；↑/↓ 数据源）。
        ranked: Vec<String>,
        /// M30-4：`'` 书签 tag 等待态。
        cmd_mark_pending: bool,
    },
    /// 搜索输入模式：正在输入 `/` 后的内容（M1-7）。`origin` 为进入搜索时
    /// 的光标位置（FIX-1：实时跳转的起点，Esc 时恢复）。
    Search { input: String, origin: usize },
    /// 过滤输入模式：正在输入 `=` 后的内容（M4-2）。
    Filter { input: String },
    /// 删除确认模式：`y` 执行，其余取消（M2-2；M13-3 起 permanent 标志
    /// 区分 dd 暂存删除与 DD 永久删除，渲染与执行分支不同）。
    Confirm {
        path: PathBuf,
        permanent: bool,
        remote: Option<String>,
    },
    /// 标记批量删除确认模式（`:tag delete`，M3-3）。
    ConfirmTagDelete,
    /// 全屏帮助视图（FEAT-4，`:help`）：独立滚动状态（与目录列表无关），
    /// j/k 滚动、/ 搜索（n/N 循环跳转）、q/Esc 关闭返回。
    HelpView {
        scroll: usize,
        search: Option<String>,
    },
    /// 帮助搜索输入（FEAT-4）：携带视图滚动与既有搜索词，Enter 提交回视图。
    HelpSearch {
        input: String,
        scroll: usize,
        search: Option<String>,
    },
    /// sudo 密码输入模式（M12-3）：写操作 EACCES 后原地重试。
    /// Enter=确认提权，Esc=干净中止（M12-4）。密码 masked 渲染。
    AskPass {
        input: String,
        op: crate::privilege::PrivOp,
    },
    /// 跳转选择器模式（M8-12）：`o` 合一 / `:j` / `:J` 分型。
    /// 数据打开时一次性读入（纯只读），按键期内存筛选。
    /// M21-E8：默认搜索态；Esc→Normal（Vim 操作），再 Esc 关闭。
    JumpSelect {
        input: String,
        cursor: usize,
        entries: Vec<crate::frecency::ScoredEntry>,
        kind: JumpKind,
        normal: bool,
        g_pending: bool,
    },
    /// 右键上下文菜单（FEAT-3）：`cursor` = 菜单内选中项；Enter 执行，
    /// Esc/点击外部关闭。
    ContextMenu { cursor: usize },
    /// 目录历史菜单（M16 NAV-2）：`cursor` = 历史条目下标（live 渲染
    /// `history[..=hist_pos]`）；Enter/l 导航、dd 移除、Esc/q 关闭。
    HistoryMenu { cursor: usize },
    /// 查看器搜索输入（M17 VIEW-1）：Enter 提交定位，Esc 取消。
    ViewerSearch { input: String },
    /// 排序对话框（M18-1）：`cursor` = 高亮字段；`desc` = 空格切换的
    /// 待定方向（Enter 应用到高亮字段）。字母键直接应用并关闭。
    SortDialog { cursor: usize, desc: bool },
    /// 文件详情对话框（M13-5，Ctrl-G）：打开时读取完成，任意键关闭。
    FileInfo { path: PathBuf, text: String },
    /// 后台作业菜单（M13-8：`:jobs`；M24-9 增选择游标与 x 取消；
    /// M28-3 增 p 暂停/恢复与 Enter 详情）。
    Jobs { cursor: usize },
    /// 作业详情覆盖层（M28-3）：显示类型/源/目标/进度/速率/时长；
    /// 任意键返回 `:jobs`。
    JobDetail { index: usize },
    /// 传输队列覆盖层（M36-7）：j/k 选择、x 取消条目、Esc 关闭。
    Queue { cursor: usize },
    /// 归档创建对话框（M25-4：`:archive` 无参）——名称/格式/校验三字段。
    ArchiveDialog {
        input: String,
        /// 当前字段：0=名称 1=格式 2=校验。
        field: usize,
        /// 格式下标（zip/tar/tar.gz）。
        format_idx: usize,
        /// 校验下标（md5/sha256/none）。
        checksum_idx: usize,
    },
    /// 暂存区菜单（M13-10：`:trash`，Enter 恢复选中项到原路径）。
    TrashMenu {
        cursor: usize,
        items: Vec<crate::ops::TrashEntry>,
    },
    /// 内联重命名（M21-E4：I/cc/A）——`pos` 为插入光标位置（字符序号），
    /// Enter 应用（入撤销栈），Esc 取消无副作用。
    Rename {
        path: PathBuf,
        input: String,
        pos: usize,
    },
    /// 远端口令输入（M22-3）：masked 渲染；Enter 连接，Esc 干净取消。
    RemotePass {
        name: String,
        params: crate::remote::sftp::SftpParams,
        input: String,
    },
}

/// 命令行单键处理结果。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum CmdInput {
    Continue,
    Submit,
    Cancel,
}

/// M24-7：命令行 Tab 补全状态——`head` 为输入中最后一个空白分隔词之前的
/// 部分，`candidates` 为焦点 pane 条目名的前缀匹配结果，`index` 为循环位置。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CmdComp {
    head: String,
    candidates: Vec<String>,
    index: usize,
}

/// `:` 命令执行结果（M2-6）：退出请求或完成（可携带提示）。
#[derive(Debug, PartialEq)]
pub(crate) enum ExecOutcome {
    /// 关闭最后一个 tab，应用退出。
    Quit,
    /// 打开帮助覆盖层（`:help`，M5-3；完整帮助页在 M6-2）。
    EnterHelp,
    /// 命令完成，可携带状态栏提示。
    Done {
        msg: Option<String>,
    },
    /// 请求确认批量删除标记文件（`:tag delete`，M3-3）。
    ConfirmTagDelete,
    /// 写操作 EACCES → 进入 sudo 密码输入（M12-3 操作级提权）。
    Escalate {
        op: crate::privilege::PrivOp,
    },
    /// 打开跳转选择器（M8-12：`:j` / `:J` 内部命令，数据已读好）。
    OpenJump {
        kind: JumpKind,
        entries: Vec<crate::frecency::ScoredEntry>,
    },
    /// 后台执行批量复制/移动（M13-8：`:tag copy|move`，run 内 spawn）。
    SpawnJob {
        op: String,
        names: Vec<String>,
        target: PathBuf,
        base: PathBuf,
    },
    /// 打开后台作业菜单（M13-8：`:jobs`）。
    ShowJobs,
    /// 打开归档创建对话框（M25-4：`:archive` 无参；input 预填光标条目名）。
    ShowArchiveDialog {
        input: String,
    },
    /// 打开暂存区菜单（M13-10：`:trash`，条目已读好）。
    ShowTrash {
        items: Vec<crate::ops::TrashEntry>,
    },
    /// 打开文件详情对话框（FEAT-3：`:file`，与 Ctrl-G 同源）。
    ShowInfo {
        path: PathBuf,
        text: String,
    },
    /// 打开目录历史菜单（M16 NAV-2：`:history`）。
    ShowHistory,
    /// 批量重命名（M21-E4：`:rename` 无参——与 cw/R 同管线）。
    /// 远端口令输入（M22-3：公钥认证失败 → 口令回退；口令仅内存传递）。
    RemotePass {
        name: String,
        params: crate::remote::sftp::SftpParams,
    },
    BatchRename {
        base: PathBuf,
        names: Vec<String>,
    },
    /// 当前 pane 执行外部命令（M18-4：`:!`；run 循环挂起 TUI 后执行）。
    RunExternal {
        cmd: String,
    },
}

/// FEAT-3：右键上下文菜单（单级、内建动作，作用于右键聚焦的条目）。
pub(crate) const CONTEXT_MENU: [&str; 6] = [
    "Open",
    "File Info (:file)",
    "Copy",
    "Move",
    "Rename",
    "Delete",
];

/// FEAT-3：菜单动作执行结果——run 循环据此迁移模式。
#[derive(Debug)]
pub(crate) enum ContextMenuOutcome {
    /// 动作完成（或无动作），关闭菜单。
    Close,
    /// 打开文件详情对话框。
    Info { path: PathBuf, text: String },
    /// 进入删除确认（permanent=false，走应用内暂存可撤销）。
    Delete { path: PathBuf },
    /// 打开命令行并预填（Rename → `rename `）。
    RenamePrefill { line: String },
}

/// M18-4：外部命令执行目标判定——`[ui] terminal_in_new_window=true` 且
/// 检测到 $TMUX/$STY 时走复用器新窗口（fire-and-forget），否则当前 pane
/// （调用方挂起 TUI 后以 sh -c / cmd /C 执行）。
pub(crate) enum ExternalRun {
    /// 复用器新窗口已完成（携带状态栏消息）。
    Multiplexer(String),
    /// 当前 pane 执行（携带命令串）。
    CurrentPane(String),
}

/// 复用器种类（M18-4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MuxKind {
    Tmux,
    Screen,
}

/// M18-1：排序对话框按键结果。
#[derive(Debug)]
pub(crate) enum SortDialogOutcome {
    None,
    Close,
    Apply(crate::tab::SortSpec),
}

/// M25-4：归档对话框单键结果。
/// M26-1：`Exec` 携带作业参数（op 编码格式/校验、条目集、目标、基目录）
/// ——创建在后台线程执行（run 循环 spawn），不再同步阻塞 UI。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ArchiveDialogOutcome {
    None,
    Close,
    Exec {
        op: String,
        names: Vec<String>,
        target: PathBuf,
        base: PathBuf,
    },
}

/// M26-1：登记 + spawn 后台作业（SpawnJob 臂与归档对话框共用）。
/// 返回登记异常提示（正常返回 None）。
fn spawn_job_from_plan(
    state: &mut AppState,
    job_tx: &std::sync::mpsc::Sender<crate::state::JobUpdate>,
    op: &str,
    names: &[String],
    target: &std::path::Path,
    base: &std::path::Path,
) -> Option<String> {
    // :jobs 展示作业种类取 op 首段（"archive:zip:md5" → "archive"）。
    let kind = op.split(':').next().unwrap_or(op);
    let src_dir = base.display().to_string();
    let job_id = state.add_job(kind, names.len(), &src_dir, &target.display().to_string());
    // M24-9：取消标志与线程共享（覆盖层 x 置位）。
    // M28-3：暂停标志与进度计数随作业共享（p 切换；详情覆盖层读取）。
    // AUDIT-8：去 expect——登记异常时不 spawn（可读提示），不留悬挂线程。
    let Some((cancel, pause, progress)) = state.jobs.iter().find(|j| j.id == job_id).map(|j| {
        (
            j.cancel_requested.clone(),
            j.pause.clone(),
            j.progress.clone(),
        )
    }) else {
        return Some("后台作业登记异常：未找到作业".to_owned());
    };
    let tab_id = state.active;
    spawn_job_thread(
        job_tx,
        job_id,
        &JobSpec {
            op,
            names,
            base,
            target,
            tab_id,
        },
        cancel,
        pause,
        progress,
    );
    None
}

/// 事件循环：进入时终端已由 `ratatui::init` 处于 raw mode + 备用屏。
pub fn run(mut terminal: ratatui::DefaultTerminal) -> std::io::Result<()> {
    // FEAT-1：启用鼠标捕获（单击聚焦/滚轮滚动）；禁用在 main 的 restore 后统一执行。
    let _ = ratatui::crossterm::execute!(std::io::stdout(), event::EnableMouseCapture);
    // M5-1：启动加载用户配置；缺失用默认值，语法错误在状态栏提示（不退出）。
    let mut msg: Option<String> = None;
    let config = match config::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            msg = Some(e);
            config::Config::default()
        }
    };
    // M16 NAV-1：加载 [quickjump] char→keyword 别名（一次）。
    config::init_quickjump();
    // AUDIT-13：[ui] theme 启动激活（内置优先、用户主题次之；失败提示
    // 不 panic——保留配置错误或回退默认 Dark）。
    if let Some(name) = &config.ui.theme
        && let Err(e) = crate::theme::activate(name)
        && msg.is_none()
    {
        msg = Some(format!("主题激活失败: {e}"));
    }
    // M23-5：加载 [frecency] per-topic 设置（λ/容量上限）。
    crate::frecency::init_settings(&config.frecency);
    // AUDIT-1：TUI 启动合并上次降级的 pending（合并只发生在 record/TUI 启动）。
    crate::frecency::merge_pending_startup();
    // AUDIT-4：TUI 启动惰性 compaction（events 超阈值 → snapshot + 截断；
    // pick 不触发——只读红线）。
    crate::frecency::lazy_compact_on_startup();
    // M10-5 GAP-1：加载 [normal] 键位分层（prepend/append/replace）。
    config::init_keymap_layers();
    let mut state = AppState::new(Tab::from_physical(
        &std::env::current_dir()?,
        config.clone(),
    )?);
    state.config = config;
    // M25-1：会话恢复（关闭/缺失/损坏 → 保持初始 CWD tab）。
    // M32-3：恢复跳过的 tab 汇入状态栏提示（远端重连失败/归档缺失）。
    let hints = crate::session::restore_session_with_hints(&mut state);
    if let Some(h) = hints.first() {
        msg = Some(h.clone());
    }
    let layers = config::keymap_layers();
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
    let mut input = InputState::with_mode_maps(keymap, &config::keymap_layers());
    let mut mode = InputMode::default();
    // AUDIT-6：多键序列可配置超时（`[keymap] seq_timeout_ms`，缺省 500）。
    // 待决 seq 的截止时刻：seq 变化时重置，空时清除；超时清空 seq
    // （不吞后续按键）。`seq_len` 变化检测用于区分"同一段待决"与"新键"。
    let seq_timeout_ms = state.config.keymap.seq_timeout_ms;
    let mut seq_deadline: Option<std::time::Instant> = None;
    let mut last_seq_len = 0usize;
    // M13-8：后台作业通道（多作业共存，run 退出时 rx 关闭）。
    let (job_tx, job_rx) = std::sync::mpsc::channel::<crate::state::JobUpdate>();
    // M23-9：副本修改防抖后的上传请求通道（监听线程回传，run 循环消费）。
    let (upload_tx, upload_rx) =
        std::sync::mpsc::channel::<(String, std::path::PathBuf, std::path::PathBuf)>();
    state.upload_tx = Some(upload_tx);
    // M25-5：git 状态查询回传通道（后台线程 → run 循环回填缓存）。
    let (git_tx, git_rx) = std::sync::mpsc::channel::<(
        std::path::PathBuf,
        std::io::Result<crate::git::GitDirStatus>,
    )>();
    state.git_tx = Some(git_tx);
    loop {
        terminal.draw(|frame| render(frame, &state, &mode, msg.as_deref()))?;
        // 列表区高度 = 终端高度 - 状态栏 1 行。
        let viewport_h = terminal.size()?.height.saturating_sub(1);
        // AUDIT-7：block=true opener 的挂起执行（open_selected 经
        // state.pending_external 转交；与 :! 同款 TUI 挂起/恢复语义）。
        if let Some(cmd) = state.pending_external.take() {
            (terminal, msg) = suspend_execute(&cmd);
        }
        // M13-8：收割后台作业更新（撤销栈入栈 + 状态刷新 + 提示）。
        while let Ok(update) = job_rx.try_recv() {
            let id = update.job_id;
            let changed = state.apply_job_update(update);
            msg = Some(if changed {
                format!("后台作业 {id} 完成（可 u 撤销）")
            } else {
                format!("后台作业 {id} 完成")
            });
        }
        // M23-9：消费副本修改上传请求（读最新内容 → RemoteFs write）。
        while let Ok((remote, temp, rpath)) = upload_rx.try_recv() {
            if let Some(backend) = state.remote_sessions.get(&remote) {
                let result = std::fs::read(&temp).and_then(|bytes| {
                    backend
                        .write(&rpath, &bytes)
                        .map_err(|e| std::io::Error::other(e.to_string()))
                });
                msg = Some(match result {
                    Ok(_) => format!("已同步上传 {}", rpath.display()),
                    Err(e) => format!("上传失败: {e}"),
                });
            }
        }
        // M21-F5：外部变更监听——先同步监听目标（导航/切 tab 后重挂或卸载），
        // 再抽干事件；创建/删除/重命名/修改触发刷新（光标跟随原条目）。
        sync_watcher(&mut state);
        let mut fs_dirty = false;
        if let Some(rx) = &state.watch_rx {
            while let Ok(ev) = rx.try_recv() {
                fs_dirty |= fs_event_relevant(&ev.kind);
            }
        }
        if fs_dirty {
            // AUDIT-8：自动刷新失败不再静默（可读提示进状态栏）。
            if let Some(e) = refresh_on_fs_change(&mut state) {
                msg = Some(e);
            }
            // M25-5：外部变更 → git 状态缓存整体失效（触发器按可见目录重查）。
            state.git_cache.clear();
            state.git_pending.clear();
        }
        // M25-5：收割 git 状态查询结果（回填缓存）+ 按需触发新查询。
        while let Ok((dir, res)) = git_rx.try_recv() {
            state.git_pending.remove(&dir);
            state.git_cache.insert(dir, res.ok());
        }
        crate::git::sync_status_trigger(&mut state);
        // AUDIT-6：待决序列超时——poll 预算按截止时刻收缩；到期清空 seq
        // （下一次按键按全新序列解析，不被旧前缀吞掉）。
        let now = std::time::Instant::now();
        seq_deadline = seq_deadline_after(
            input.seq.len(),
            last_seq_len,
            seq_deadline,
            now,
            seq_timeout_ms,
        );
        last_seq_len = input.seq.len();
        let (poll_ms, expired) = seq_poll_plan(seq_deadline, now);
        if expired {
            input.seq.clear();
            seq_deadline = None;
            last_seq_len = 0;
        }
        // M36-7：传输队列消费（每次 poll 前——头条目上传同步执行）。
        if !state.transfer_queue.is_empty()
            && state
                .transfer_queue
                .first()
                .map(|e| e.direction == crate::app::jobs::TransferDirection::Upload)
                .unwrap_or(false)
        {
            let entry = state.transfer_queue.remove(0);
            if let Some(backend) = state.remote_sessions.get(&entry.remote) {
                match upload_chunked(backend.as_ref(), &entry.src, &entry.dest) {
                    Ok(()) => {
                        state.undo_stack.push(crate::state::UndoableOp::Remote {
                            tab_id: entry.tab_id,
                            remote: entry.remote.clone(),
                            kind: crate::state::RemoteUndo::Create {
                                path: entry.dest.clone(),
                                is_dir: false,
                            },
                        });
                    }
                    Err((_, e)) => {
                        msg = Some(format!("queue: {} 失败: {e}", entry.src.display()));
                    }
                }
                refresh_tab_panes(&mut state, entry.tab_id);
            }
        }
        // M13-8：事件轮询（100ms）——后台作业完成即时感知，UI 不空转阻塞；
        // 有待决序列时预算收缩到超时余量（AUDIT-6）。
        let polled = event::poll(std::time::Duration::from_millis(poll_ms))?;
        let key_ev = if polled { event::read().ok() } else { None };
        if let Some(Event::Key(key)) = key_ev {
            match &mut mode {
                InputMode::Command {
                    input,
                    hist_pos,
                    draft,
                    comp,
                    ranked,
                    cmd_mark_pending,
                } => {
                    // M24-6：↑/↓ 浏览会话命令历史（内存 Vec，无持久化）；
                    // M24-7：Tab 补全焦点 pane 条目名（前缀循环，>10 候选
                    // 显示计数）；其余按键走原有单键处理。
                    let handled = if key.kind == KeyEventKind::Press {
                        match key.code {
                            // M30-4：' 置位 tag 等待态（字面回显）；
                            // tag 字符消费展开为书签命令；Esc 取消还原。
                            KeyCode::Char('\'') if !*cmd_mark_pending => {
                                *cmd_mark_pending = true;
                                input.push('\'');
                                Some(CmdInput::Continue)
                            }
                            KeyCode::Char(c) if *cmd_mark_pending => {
                                *cmd_mark_pending = false;
                                input.pop();
                                match crate::frecency::cmd_bookmark(&c.to_string()) {
                                    Some(cmd) => {
                                        *input = cmd;
                                        *hist_pos = None;
                                        Some(CmdInput::Continue)
                                    }
                                    None => {
                                        msg = Some(format!("无命令书签 {c}"));
                                        Some(CmdInput::Continue)
                                    }
                                }
                            }
                            KeyCode::Up => {
                                hist_prev(ranked, hist_pos, input, draft);
                                Some(CmdInput::Continue)
                            }
                            KeyCode::Down => {
                                hist_next(ranked, hist_pos, input, draft);
                                Some(CmdInput::Continue)
                            }
                            KeyCode::Tab => {
                                // M30-3：候选加权排序（frecency 分数优先、
                                // 字典序兜底；只读快照，store 缺失零成本降级）。
                                let names: Vec<String> = {
                                    let tab = state.active_tab();
                                    let base = tab.focus_base().to_path_buf();
                                    let items: Vec<crate::tab::FileItem> =
                                        if tab.focus == crate::tab::Focus::Right {
                                            tab.right.entries.clone()
                                        } else {
                                            tab.entries.clone()
                                        };
                                    let scores = crate::frecency::path_scores();
                                    completion_candidates(&base, &items, &scores)
                                };
                                let n = tab_complete(input, comp, &names);
                                if n > 10 {
                                    msg = Some(format!("补全候选 {n} 个（Tab 循环切换）"));
                                }
                                Some(CmdInput::Continue)
                            }
                            _ => None,
                        }
                    } else {
                        None
                    };
                    match handled.unwrap_or_else(|| command_input(input, key)) {
                        CmdInput::Continue => {}
                        CmdInput::Cancel => mode = InputMode::Normal,
                        CmdInput::Submit => {
                            let line = std::mem::take(input);
                            mode = InputMode::Normal;
                            let result = exec_command_impl(&mut state, line.trim());
                            // M24-6：仅记录成功执行的命令（失败不记；空命令不记）。
                            // M30-1：命令文本入 frecency cmd topic（成功才记录；
                            // IGNORE_PATTERNS 命中静默跳过；写失败静默降级）。
                            if result.is_ok() {
                                let patterns = state.config.frecency.ignore_patterns.clone();
                                state.record_cmd_history(&line);
                                crate::frecency::record_command_line_now(&line, &patterns);
                            }
                            match result {
                                Ok(ExecOutcome::Quit) => {
                                    flush_session_visits(&state);
                                    crate::session::save_session(&state);
                                    cleanup_temp_opens(&mut state);
                                    // M27-3：退出清理 PDF 视觉预览临时 PNG。
                                    crate::pdf::cleanup_visual_temps();
                                    return Ok(());
                                }
                                Ok(ExecOutcome::EnterHelp) => {
                                    mode = InputMode::HelpView {
                                        scroll: 0,
                                        search: None,
                                    };
                                }
                                Ok(ExecOutcome::Done { msg: m }) => msg = m,
                                Ok(ExecOutcome::ConfirmTagDelete) => {
                                    mode = InputMode::ConfirmTagDelete;
                                }
                                Ok(ExecOutcome::Escalate { op }) => {
                                    // M12-3：EACCES → 原地 sudo 密码输入（操作级提权）。
                                    mode = InputMode::AskPass {
                                        input: String::new(),
                                        op,
                                    };
                                }
                                Ok(ExecOutcome::OpenJump { kind, entries }) => {
                                    // M8-12：:j/:J → 分型跳转选择器。
                                    mode = InputMode::JumpSelect {
                                        input: String::new(),
                                        cursor: 0,
                                        entries,
                                        kind,
                                        normal: false,
                                        g_pending: false,
                                    };
                                }
                                Ok(ExecOutcome::SpawnJob {
                                    op,
                                    names,
                                    target,
                                    base,
                                }) => {
                                    // M13-8/M26-1：登记作业并 spawn 后台线程
                                    // （与归档对话框共用 helper）。
                                    if let Some(e) = spawn_job_from_plan(
                                        &mut state, &job_tx, &op, &names, &target, &base,
                                    ) {
                                        msg = Some(e);
                                    }
                                }
                                Ok(ExecOutcome::ShowJobs) => {
                                    mode = InputMode::Jobs { cursor: 0 };
                                }
                                Ok(ExecOutcome::ShowArchiveDialog { input }) => {
                                    // M25-4：格式缺省 zip；校验下标取 [archive] checksum。
                                    let checksum_idx =
                                        match ops::Checksum::parse(&state.config.archive.checksum)
                                            .unwrap_or(ops::Checksum::Md5)
                                        {
                                            ops::Checksum::Sha256 => 1,
                                            ops::Checksum::None => 2,
                                            _ => 0,
                                        };
                                    mode = InputMode::ArchiveDialog {
                                        input,
                                        field: 0,
                                        format_idx: 0,
                                        checksum_idx,
                                    };
                                }
                                Ok(ExecOutcome::ShowTrash { items }) => {
                                    mode = InputMode::TrashMenu { cursor: 0, items };
                                }
                                Ok(ExecOutcome::ShowInfo { path, text }) => {
                                    // FEAT-3：`:file` → 详情对话框（任意键关闭）。
                                    mode = InputMode::FileInfo { path, text };
                                }
                                Ok(ExecOutcome::ShowHistory) => {
                                    // M16 NAV-2：:history → 历史菜单（游标落在当前视图）。
                                    let cur = state.active_tab().hist_pos;
                                    mode = InputMode::HistoryMenu { cursor: cur };
                                }
                                Ok(ExecOutcome::BatchRename { base, names }) => {
                                    // M21-E4：:rename 无参 → 与 cw/R 同管线。
                                    ratatui::restore();
                                    let result = batch_rename_flow(&mut state, &base, &names);
                                    terminal = ratatui::init();
                                    let _ = ratatui::crossterm::execute!(
                                        std::io::stdout(),
                                        event::EnableMouseCapture
                                    );
                                    msg = match result {
                                        Ok(m) => Some(m),
                                        Err(e) => Some(e),
                                    };
                                }
                                Ok(ExecOutcome::RemotePass { name, params }) => {
                                    // M22-3：公钥认证失败 → 口令输入模式（不落盘）。
                                    mode = InputMode::RemotePass {
                                        name,
                                        params,
                                        input: String::new(),
                                    };
                                }
                                Ok(ExecOutcome::RunExternal { cmd }) => {
                                    // M18-4：:! 当前 pane 路径——挂起 TUI 执行后恢复
                                    // （AUDIT-7 起与 block=true opener 共用 suspend_execute）。
                                    (terminal, msg) = suspend_execute(&cmd);
                                }
                                Err(m) => msg = Some(m),
                            }
                        }
                    }
                }
                InputMode::Search { input, origin } => match command_input(input, key) {
                    // FIX-1：输入变化即实时跳转到第一个匹配项（incsearch）。
                    CmdInput::Continue => live_search_jump(&mut state, *origin, input, &mut msg),
                    CmdInput::Cancel => {
                        // M21-F3：Esc 清理——连同上次提交的搜索一并清除
                        // （状态栏与列表高亮不残留），再次 `/` 干净进入；
                        // 光标回到进入搜索前的位置。
                        search_cancel(&mut state, *origin);
                        msg = None;
                        mode = InputMode::Normal;
                    }
                    CmdInput::Submit => {
                        let line = std::mem::take(input);
                        mode = InputMode::Normal;
                        submit_search(&mut state, &line, &mut msg);
                    }
                },
                InputMode::Filter { input } => match command_input(input, key) {
                    CmdInput::Continue => {
                        // M21-E7：实时过滤——输入即生效（含隐式 *模式* 语义）。
                        let cfg = state.config.clone();
                        state.active_tab_mut().apply_filter(input, cfg.clone());
                    }
                    CmdInput::Cancel => {
                        // M21-E7：Esc 清除过滤，恢复完整列表。
                        let cfg = state.config.clone();
                        state.active_tab_mut().apply_filter("", cfg.clone());
                        msg = None;
                        mode = InputMode::Normal;
                    }
                    CmdInput::Submit => {
                        let line = std::mem::take(input);
                        mode = InputMode::Normal;
                        let cfg = state.config.clone();
                        state.active_tab_mut().apply_filter(&line, cfg);
                        msg = None;
                    }
                },
                InputMode::Rename { path, input, pos } => {
                    // M21-E4：内联重命名——插入/退格/删除/左右移动/Enter 应用
                    // /Esc 取消。字符编辑经 Vec<char> 保证多字节边界安全。
                    if key.kind == KeyEventKind::Press {
                        let mut chars: Vec<char> = input.chars().collect();
                        match key.code {
                            KeyCode::Enter => {
                                let path = std::mem::take(path);
                                let new_name: String = chars.into_iter().collect();
                                *pos = 0;
                                mode = InputMode::Normal;
                                msg = rename_apply(&mut state, &path, &new_name);
                            }
                            KeyCode::Esc => {
                                mode = InputMode::Normal;
                                msg = None;
                            }
                            KeyCode::Backspace if *pos > 0 => {
                                chars.remove(*pos - 1);
                                *pos -= 1;
                                *input = chars.into_iter().collect();
                            }
                            KeyCode::Delete if *pos < chars.len() => {
                                chars.remove(*pos);
                                *input = chars.into_iter().collect();
                            }
                            KeyCode::Left => *pos = pos.saturating_sub(1),
                            KeyCode::Right => *pos = (*pos + 1).min(chars.len()),
                            KeyCode::Home => *pos = 0,
                            KeyCode::End => *pos = chars.len(),
                            KeyCode::Char(c)
                                if key.modifiers.is_empty()
                                    || key.modifiers == KeyModifiers::SHIFT =>
                            {
                                chars.insert(*pos, c);
                                *pos += 1;
                                *input = chars.into_iter().collect();
                            }
                            _ => {}
                        }
                    }
                }
                InputMode::RemotePass {
                    name,
                    params,
                    input,
                } => {
                    // M22-3：远端口令输入——Enter 连接（失败留在模式内重试，
                    // 清空口令），Esc 干净取消（TUI 状态不变，SPEC M22-3）。
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Enter => {
                                let name = name.clone();
                                let params = params.clone();
                                let password = std::mem::take(input);
                                match remote_pass_apply(&mut state, &name, &params, &password) {
                                    Ok(m) => {
                                        mode = InputMode::Normal;
                                        msg = Some(m);
                                    }
                                    Err(e) => {
                                        msg = Some(e);
                                        mode = InputMode::RemotePass {
                                            name,
                                            params,
                                            input: String::new(),
                                        };
                                    }
                                }
                            }
                            KeyCode::Esc => {
                                mode = InputMode::Normal;
                                msg = None;
                            }
                            KeyCode::Backspace => {
                                input.pop();
                            }
                            KeyCode::Char(c) => input.push(c),
                            _ => {}
                        }
                    }
                }
                InputMode::Confirm {
                    path,
                    permanent,
                    remote,
                } => {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            // y / Enter 确认删除；n / Esc / 其他 = 取消（无副作用）。
                            KeyCode::Char('y') | KeyCode::Enter => {
                                let path = std::mem::take(path);
                                let permanent = *permanent;
                                let remote = remote.clone();
                                mode = InputMode::Normal;
                                if let Some(op) =
                                    execute_delete(&mut state, &mut msg, path, permanent, remote)
                                {
                                    // M12-3：EACCES → 原地进入 sudo 密码输入。
                                    mode = InputMode::AskPass {
                                        input: String::new(),
                                        op,
                                    };
                                }
                            }
                            _ => mode = InputMode::Normal,
                        }
                    }
                }
                InputMode::HelpView { scroll, search } => {
                    // FEAT-4：全屏帮助——j/k 滚动、/ 搜索、n/N 跳转、q/Esc 返回。
                    // 状态仅模式局部，关闭后目录列表光标/滚动不变。
                    if key.kind == KeyEventKind::Press {
                        let lines = help_lines();
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => mode = InputMode::Normal,
                            KeyCode::Char('j') | KeyCode::Down => {
                                *scroll = (*scroll + 1).min(lines.len().saturating_sub(1));
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                *scroll = scroll.saturating_sub(1);
                            }
                            KeyCode::Char('/') => {
                                mode = InputMode::HelpSearch {
                                    input: String::new(),
                                    scroll: *scroll,
                                    search: search.take(),
                                };
                            }
                            KeyCode::Char('n') => {
                                if let Some(q) = search.clone()
                                    && let Some(idx) =
                                        help_find_match(&lines, &q, *scroll + 1, true)
                                {
                                    *scroll = idx;
                                }
                            }
                            KeyCode::Char('N') => {
                                if let Some(q) = search.clone()
                                    && let Some(idx) = help_find_match(
                                        &lines,
                                        &q,
                                        *scroll + lines.len().saturating_sub(1),
                                        false,
                                    )
                                {
                                    *scroll = idx;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                InputMode::HelpSearch {
                    input,
                    scroll,
                    search,
                } => {
                    // FEAT-4：帮助页搜索输入——Enter 提交定位，Esc 取消回视图。
                    match command_input(input, key) {
                        CmdInput::Continue => {}
                        CmdInput::Cancel => {
                            mode = InputMode::HelpView {
                                scroll: *scroll,
                                search: search.clone(),
                            };
                        }
                        CmdInput::Submit => {
                            let query = std::mem::take(input);
                            let lines = help_lines();
                            let next = if query.trim().is_empty() {
                                None
                            } else {
                                help_find_match(&lines, query.trim(), *scroll, true).or(Some(0))
                            };
                            mode = InputMode::HelpView {
                                scroll: next.unwrap_or(*scroll),
                                search: next.map(|_| query.trim().to_owned()),
                            };
                        }
                    }
                }
                InputMode::AskPass { input, op } => {
                    // M12-3/M12-4：sudo 密码输入。Enter=提权重试；Esc=干净中止
                    // （不触碰任何状态——取消后 TUI 与操作前一致）。
                    if key.kind == KeyEventKind::Press {
                        match askpass_key(input, key.code) {
                            AskPassResult::Continue => {}
                            AskPassResult::Cancel => {
                                // M12-4：取消干净中止——丢弃密码与操作，状态不变。
                                input.clear();
                                mode = InputMode::Normal;
                                msg = Some("已取消提权".to_owned());
                            }
                            AskPassResult::Retry { password } => {
                                let op = op.clone();
                                mode = InputMode::Normal;
                                #[cfg(unix)]
                                match crate::privilege::escalate_execute(&op, password.trim()) {
                                    Ok(()) => {
                                        msg = Some(format!("提权完成: {}", op.describe()));
                                        let _tid = state.active;
                                        refresh_tab_panes(&mut state, _tid);
                                    }
                                    Err(e) => {
                                        msg = Some(format!("提权失败: {e}"));
                                    }
                                }
                                #[cfg(not(unix))]
                                {
                                    let _ = (op, password);
                                    msg = Some("提权仅支持 Unix（Linux/macOS）".to_owned());
                                }
                            }
                        }
                    }
                }
                InputMode::ConfirmTagDelete => {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            // y / Enter 确认批量删除；n / Esc / 其他 = 取消（无副作用）。
                            KeyCode::Char('y') | KeyCode::Enter => {
                                mode = InputMode::Normal;
                                execute_tag_delete(&mut state, &mut msg);
                            }
                            _ => mode = InputMode::Normal,
                        }
                    }
                }
                InputMode::Normal => {
                    match handle_key(&mut state, &mut input, &mut msg, key, viewport_h) {
                        Outcome::Continue => {}
                        Outcome::Quit => {
                            flush_session_visits(&state);
                            crate::session::save_session(&state);
                            cleanup_temp_opens(&mut state);
                            // M27-3：退出清理 PDF 视觉预览临时 PNG。
                            crate::pdf::cleanup_visual_temps();
                            return Ok(());
                        }
                        Outcome::EnterCommand => {
                            mode = InputMode::Command {
                                input: String::new(),
                                hist_pos: None,
                                draft: String::new(),
                                comp: None,
                                ranked: crate::frecency::cmd_history_ranked(
                                    &state.cmd_history,
                                    CMD_RANKED_LIMIT,
                                ),
                                cmd_mark_pending: false,
                            };
                        }
                        Outcome::EnterSearch => {
                            // M34-3：origin 取焦点 pane 光标（右 pane Esc 恢复正确）。
                            let tab = state.active_tab();
                            let origin = match tab.focus {
                                crate::tab::Focus::Right => tab.right.cursor,
                                crate::tab::Focus::Left => tab.cursor,
                            };
                            mode = InputMode::Search {
                                input: String::new(),
                                origin,
                            };
                        }
                        Outcome::EnterFilter => {
                            mode = InputMode::Filter {
                                input: String::new(),
                            };
                        }
                        Outcome::EnterViewerSearch => {
                            mode = InputMode::ViewerSearch {
                                input: String::new(),
                            };
                        }
                        Outcome::EnterQueue { cursor } => {
                            mode = InputMode::Queue { cursor };
                        }
                        Outcome::EnterSortDialog => {
                            // 高亮字段初始 = 当前排序字段（否则 name）。
                            let cur = state
                                .active_tab()
                                .sort
                                .clone()
                                .and_then(|v| v.first().map(|s| s.field))
                                .unwrap_or(crate::tab::SortField::Name);
                            let cursor = SORT_FIELDS.iter().position(|f| f.3 == cur).unwrap_or(0);
                            let desc = state
                                .active_tab()
                                .sort
                                .clone()
                                .and_then(|v| v.first().map(|s| s.desc))
                                .unwrap_or(false);
                            mode = InputMode::SortDialog { cursor, desc };
                        }
                        Outcome::ConfirmDelete(path, permanent, remote) => {
                            mode = InputMode::Confirm {
                                path,
                                permanent,
                                remote,
                            };
                        }
                        Outcome::FileInfo { path, text } => {
                            mode = InputMode::FileInfo { path, text };
                        }
                        Outcome::RunUserCommand { cmd } => {
                            // M23-11：挂起 TUI，外部命令运行后恢复。
                            ratatui::restore();
                            let result = std::process::Command::new("sh")
                                .arg("-c")
                                .arg(&cmd)
                                .status();
                            terminal = ratatui::init();
                            let _ = ratatui::crossterm::execute!(
                                std::io::stdout(),
                                event::EnableMouseCapture
                            );
                            msg = match result {
                                Ok(st) if st.success() => Some(format!("命令完成: {cmd}")),
                                Ok(st) => Some(format!("命令失败: {cmd}（退出码 {}）", st)),
                                Err(e) => Some(format!("命令执行失败: {e}")),
                            };
                        }
                        Outcome::OpenFrecencyPane { entries } => {
                            // M23-6：o → 焦点 pane 进入 Frecency 内嵌搜索。
                            let tab = state.active_tab_mut();
                            tab.pane_frecency = Some(crate::tab::FrecencyPane {
                                entries,
                                query: String::new(),
                                cursor: 0,
                                normal: false,
                                g_pending: false,
                            });
                        }
                        Outcome::EditFile { path } => {
                            // M23-10：e——挂起 TUI，$EDITOR 打开文件，恢复后反馈。
                            ratatui::restore();
                            let editor = editor_name();
                            let result = std::process::Command::new(&editor)
                                .arg(&path)
                                .status()
                                .map_err(|e| e.to_string())
                                .and_then(|st| {
                                    if st.success() {
                                        Ok(())
                                    } else {
                                        Err(format!("编辑器 {editor} 非零退出（{st}）"))
                                    }
                                });
                            terminal = ratatui::init();
                            let _ = ratatui::crossterm::execute!(
                                std::io::stdout(),
                                event::EnableMouseCapture
                            );
                            msg = Some(match result {
                                Ok(()) => format!("编辑完成: {}", path.display()),
                                Err(e) => format!("编辑失败: {e}"),
                            });
                        }
                        Outcome::EnterShell => {
                            // M18-3/M18-4：shell 入口——按 [ui] 配置分流复用器
                            // 新窗口（不挂起）/ 当前 pane（挂起 TUI；state 全程
                            // 未动，光标/滚动与 raw mode/alt screen 恢复）。
                            let ui = state.config.ui.clone();
                            let shell =
                                std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned());
                            match run_external_target(&ui, &shell) {
                                ExternalRun::Multiplexer(m) => msg = Some(m),
                                ExternalRun::CurrentPane(cmd) => {
                                    ratatui::restore();
                                    let result = std::process::Command::new("sh")
                                        .arg("-c")
                                        .arg(&cmd)
                                        .status();
                                    terminal = ratatui::init();
                                    // FEAT-1：init 不含鼠标捕获，重新启用。
                                    let _ = ratatui::crossterm::execute!(
                                        std::io::stdout(),
                                        event::EnableMouseCapture
                                    );
                                    msg = run_external_current_result(result);
                                }
                            }
                        }
                        Outcome::BatchRename { base, names } => {
                            // M13-6：TUI 挂起 → $EDITOR 编辑名单 → 应用 → 恢复。
                            ratatui::restore();
                            let result = batch_rename_flow(&mut state, &base, &names);
                            terminal = ratatui::init();
                            // FEAT-1：init 不含鼠标捕获，重新启用。
                            let _ = ratatui::crossterm::execute!(
                                std::io::stdout(),
                                event::EnableMouseCapture
                            );
                            msg = match result {
                                Ok(m) => Some(m),
                                Err(e) => Some(e),
                            };
                        }
                        Outcome::EnterRename { path, input, pos } => {
                            // M21-E4：I/cc/A → 内联重命名模式。
                            mode = InputMode::Rename { path, input, pos };
                        }
                    }
                }
                InputMode::FileInfo { .. } => {
                    // M13-5：详情对话框——任意键关闭（同帮助覆盖层）。
                    mode = InputMode::Normal;
                }
                InputMode::Jobs { cursor } => {
                    // M13-8 → M24-9：j/k 选择、x 取消；M28-3：p 暂停/恢复、
                    // Enter 详情、Esc/q 关闭。
                    if key.kind == KeyEventKind::Press {
                        match jobs_menu_key(&mut state, cursor, key.code, &mut msg) {
                            JobsMenuOutcome::None => {}
                            JobsMenuOutcome::Close => mode = InputMode::Normal,
                            JobsMenuOutcome::OpenDetail => {
                                mode = InputMode::JobDetail { index: *cursor };
                            }
                        }
                    }
                }
                InputMode::JobDetail { index } => {
                    // M28-3：详情覆盖层——任意键返回 :jobs（Esc/q/Enter 语义一致）。
                    if key.kind == KeyEventKind::Press {
                        let _ = index;
                        mode = InputMode::Jobs { cursor: *index };
                    }
                }
                InputMode::Queue { cursor } => {
                    // M36-7：队列覆盖层——j/k 选择、x 取消条目、Esc 关闭。
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') => mode = InputMode::Normal,
                            KeyCode::Char('j') | KeyCode::Down => {
                                if !state.transfer_queue.is_empty() {
                                    *cursor = (*cursor + 1).min(state.transfer_queue.len() - 1);
                                }
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                *cursor = cursor.saturating_sub(1);
                            }
                            KeyCode::Char('x') if *cursor < state.transfer_queue.len() => {
                                state.transfer_queue.remove(*cursor);
                                if *cursor >= state.transfer_queue.len()
                                    && !state.transfer_queue.is_empty()
                                {
                                    *cursor = state.transfer_queue.len() - 1;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                InputMode::ArchiveDialog {
                    input,
                    field,
                    format_idx,
                    checksum_idx,
                } => {
                    // M25-4：Tab/↑↓ 切字段、←→/空格 换值、名称字段直接输入、
                    // Enter 执行、Esc 取消。
                    if key.kind == KeyEventKind::Press {
                        match handle_archive_dialog_key(
                            &mut state,
                            input,
                            field,
                            format_idx,
                            checksum_idx,
                            key.code,
                            &mut msg,
                        ) {
                            ArchiveDialogOutcome::None => {}
                            ArchiveDialogOutcome::Close => mode = InputMode::Normal,
                            ArchiveDialogOutcome::Exec {
                                op,
                                names,
                                target,
                                base,
                            } => {
                                // M26-1：对话框确认 → 后台线程创建归档
                                // （完成回包入撤销栈，:jobs 可见可取消）。
                                mode = InputMode::Normal;
                                if let Some(e) = spawn_job_from_plan(
                                    &mut state, &job_tx, &op, &names, &target, &base,
                                ) {
                                    msg = Some(e);
                                }
                            }
                        }
                    }
                }
                InputMode::SortDialog { cursor, desc } => {
                    // M18-1：排序对话框——字母直接应用；空格换向；回车应用。
                    if key.kind == KeyEventKind::Press {
                        match handle_sort_dialog_key(&mut state, key, cursor, desc, &mut msg) {
                            SortDialogOutcome::None => {}
                            SortDialogOutcome::Close => mode = InputMode::Normal,
                            SortDialogOutcome::Apply(spec) => {
                                let cfg = state.config.clone();
                                state.active_tab_mut().apply_sort(spec, cfg);
                                mode = InputMode::Normal;
                                msg = Some(format!(
                                    "已排序: {} {}",
                                    spec.field.label(),
                                    if spec.desc { "desc" } else { "asc" }
                                ));
                            }
                        }
                    }
                }
                InputMode::ViewerSearch { input } => {
                    // M17 VIEW-1：查看器搜索输入——Enter 提交定位，Esc 取消。
                    match command_input(input, key) {
                        CmdInput::Continue => {}
                        CmdInput::Cancel => mode = InputMode::Normal,
                        CmdInput::Submit => {
                            let query = std::mem::take(input);
                            mode = InputMode::Normal;
                            state.active_tab_mut().viewer_search_commit(query.trim());
                        }
                    }
                }
                InputMode::HistoryMenu { cursor } => {
                    // M16 NAV-2：历史菜单——j/k 移动、Enter/l 导航、dd 移除。
                    if key.kind == KeyEventKind::Press {
                        let max_idx = state.active_tab().hist_pos;
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') => mode = InputMode::Normal,
                            KeyCode::Down | KeyCode::Char('j') => {
                                *cursor = (*cursor + 1).min(max_idx);
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                *cursor = cursor.saturating_sub(1);
                            }
                            KeyCode::Enter | KeyCode::Char('l') => {
                                let idx = *cursor;
                                mode = InputMode::Normal;
                                history_menu_nav(&mut state, idx, &mut msg);
                            }
                            KeyCode::Char('d') => {
                                let idx = *cursor;
                                if state.active_tab_mut().history_remove(idx) {
                                    *cursor = (*cursor).min(state.active_tab().hist_pos);
                                } else {
                                    msg = Some("当前视图不可移除".to_owned());
                                }
                            }
                            _ => {}
                        }
                    }
                }
                InputMode::ContextMenu { cursor } => {
                    // FEAT-3：右键菜单——j/k 移动、Enter 执行、Esc/q 关闭。
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') => mode = InputMode::Normal,
                            KeyCode::Down | KeyCode::Char('j') => {
                                *cursor = (*cursor + 1).min(CONTEXT_MENU.len() - 1);
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                *cursor = cursor.saturating_sub(1);
                            }
                            KeyCode::Enter | KeyCode::Char('l') => {
                                let action = *cursor;
                                mode = InputMode::Normal;
                                match run_context_menu_action(&mut state, action, &mut msg) {
                                    ContextMenuOutcome::Close => {}
                                    ContextMenuOutcome::Info { path, text } => {
                                        mode = InputMode::FileInfo { path, text };
                                    }
                                    ContextMenuOutcome::Delete { path } => {
                                        mode = InputMode::Confirm {
                                            path,
                                            permanent: false,
                                            remote: None,
                                        };
                                    }
                                    ContextMenuOutcome::RenamePrefill { line } => {
                                        mode = InputMode::Command {
                                            input: line,
                                            hist_pos: None,
                                            draft: String::new(),
                                            comp: None,
                                            ranked: crate::frecency::cmd_history_ranked(
                                                &state.cmd_history,
                                                CMD_RANKED_LIMIT,
                                            ),
                                            cmd_mark_pending: false,
                                        };
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                InputMode::TrashMenu { cursor, items } => {
                    // M13-10：暂存区菜单——j/k 移动、Enter 恢复、Esc 关闭。
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Esc => mode = InputMode::Normal,
                            KeyCode::Down | KeyCode::Char('j') => {
                                *cursor = cursor.saturating_add(1);
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                *cursor = cursor.saturating_sub(1);
                            }
                            KeyCode::Enter => {
                                let entry = items.get(*cursor).cloned();
                                let snapshot = items.clone();
                                let cur = *cursor;
                                mode = InputMode::Normal;
                                match entry {
                                    Some(e) => match crate::ops::restore_from_trash(&e) {
                                        Ok(orig) => {
                                            msg = Some(format!("已恢复: {}", orig.display()));
                                            let cfg = state.config.clone();
                                            if let Some(tab) = state.tabs.get_mut(state.active) {
                                                let _ = tab.refresh(cfg);
                                            }
                                            let rest: Vec<crate::ops::TrashEntry> = snapshot
                                                .into_iter()
                                                .filter(|it| it.staged != e.staged)
                                                .collect();
                                            let new_cursor = rest
                                                .len()
                                                .saturating_sub(1)
                                                .min(cur.saturating_sub(1));
                                            mode = InputMode::TrashMenu {
                                                cursor: new_cursor,
                                                items: rest,
                                            };
                                        }
                                        Err(err) => msg = Some(format!("恢复失败: {err}")),
                                    },
                                    None => msg = Some("暂存区: 无选定条目".to_owned()),
                                }
                            }
                            _ => {}
                        }
                    }
                }
                InputMode::JumpSelect {
                    input,
                    cursor,
                    entries,
                    kind: _,
                    normal,
                    g_pending,
                } => {
                    // M8-12：选择器按键——输入筛选、j/k 移动、Enter 选定、Esc 取消。
                    // M21-E8：双段（搜索/Normal）+ Vim 操作（j/k/gg/G/l），经
                    // jump_key 可测抽取；Select/Close 由 run 循环消费。
                    let visible_len = filter_jump(entries, input).len();
                    match jump_key(normal, g_pending, input, cursor, visible_len, &key) {
                        JumpKey::Continue => {}
                        JumpKey::Close => {
                            mode = InputMode::Normal;
                            msg = None;
                        }
                        JumpKey::Select => {
                            let query = std::mem::take(input);
                            let visible = filter_jump(entries, &query);
                            let picked = visible.get(*cursor).cloned();
                            mode = InputMode::Normal;
                            match picked {
                                Some(entry) => match jump_to(&mut state, &entry) {
                                    Ok(m) => msg = Some(m),
                                    Err(e) => msg = Some(e),
                                },
                                None => msg = Some("跳转: 无选定条目".to_owned()),
                            }
                        }
                    }
                }
            } // match &mut mode
        } else if let Some(Event::Mouse(mouse)) = key_ev {
            // FEAT-1：鼠标事件（单击聚焦/再击打开/滚轮滚动）。
            // FEAT-3：右键聚焦条目并弹出上下文菜单；菜单内点击执行、外部关闭。
            let size = terminal.size()?;
            let area = ratatui::layout::Rect::new(0, 0, size.width, size.height);
            if let InputMode::ContextMenu { cursor: _ } = &mut mode {
                let popup = context_menu_rect(area);
                let inside_item = mouse.kind == MouseEventKind::Down(MouseButton::Left)
                    && mouse.row > popup.y
                    && mouse.row < popup.y + popup.height - 1
                    && mouse.column > popup.x
                    && mouse.column < popup.x + popup.width;
                if inside_item {
                    let action = mouse.row - popup.y - 1;
                    mode = InputMode::Normal;
                    match run_context_menu_action(&mut state, action as usize, &mut msg) {
                        ContextMenuOutcome::Close => {}
                        ContextMenuOutcome::Info { path, text } => {
                            mode = InputMode::FileInfo { path, text };
                        }
                        ContextMenuOutcome::Delete { path } => {
                            mode = InputMode::Confirm {
                                path,
                                permanent: false,
                                remote: None,
                            };
                        }
                        ContextMenuOutcome::RenamePrefill { line } => {
                            mode = InputMode::Command {
                                input: line,
                                hist_pos: None,
                                draft: String::new(),
                                comp: None,
                                ranked: crate::frecency::cmd_history_ranked(
                                    &state.cmd_history,
                                    CMD_RANKED_LIMIT,
                                ),
                                cmd_mark_pending: false,
                            };
                        }
                    }
                } else if mouse.kind == MouseEventKind::Down(MouseButton::Left)
                    || mouse.kind == MouseEventKind::Down(MouseButton::Right)
                {
                    // 点击菜单外 → 关闭（无副作用）。
                    mode = InputMode::Normal;
                }
            } else if mouse.kind == MouseEventKind::Down(MouseButton::Right)
                && matches!(mode, InputMode::Normal)
            {
                // 右键条目行：聚焦该项并打开菜单（左 pane；`..` 行无菜单）。
                if let Some(idx) = hit_entry_index(&state, &mouse, area) {
                    let tab = state.active_tab_mut();
                    tab.on_parent_row = false;
                    tab.cursor = idx;
                    mode = InputMode::ContextMenu { cursor: 0 };
                }
            } else {
                handle_mouse(&mut state, &mode, &mouse, &mut msg, area);
            }
        } // if event::poll && let Event::Key
    } // loop
} // fn run

#[cfg(test)]
mod tests;
