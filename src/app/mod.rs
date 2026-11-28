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
// 例外：exec pub 化（M40-2 fuzz 靶引用 normalize_lexical；模块内其余项
// 均 pub(crate)，对外实际仅暴露该函数）。
pub mod exec;
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
    /// M55-2：请求进入实时递归查找（`O`；会话已在 handle_key 中启动）。
    EnterLiveFind,
    /// 请求进入查看器搜索输入（M17 VIEW-1：查看器内 `/`）。
    EnterViewerSearch,
    /// 请求打开排序对话框（M18-1：`S`）。
    EnterSortDialog,
    /// 请求挂起 TUI 进入 shell（M18-3：`s`；vifm 既有习惯）。
    EnterShell,
    /// M50-4：进入命令模式并预填 `!`（由用户自定义键位触发）。
    EnterExternalCommandLine,
    /// 请求确认删除（`dd`/`DD`，M2-2/M13-3）：携带待删完整路径；
    /// permanent = 跳过暂存的不可逆删除（DD）。
    ConfirmDelete(PathBuf, bool, Option<String>),
    /// M45-3d：可视模式 v+d/x 已把选中项加 tag，请求进入批量删除确认覆盖层。
    ConfirmTagDelete,
    /// M48-5：粘贴目标冲突，请求进入阻塞确认弹窗。
    ConfirmPaste,
    /// M49-1：进入 frecency 右侧预览搜索输入。
    EnterFrecencyPreviewSearch,
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
    /// M55-2：`O` 实时递归查找（输入实时过滤，j/k 为输入字符，↑↓ 移动）。
    LiveFind,
    /// M69：git diff 只读 overlay（数据存于 AppState.git_diff）。
    GitDiff,
    /// M93-1：`:du` 后台扫描结果视图。
    DuView { cursor: usize },
    /// M93-2：`:dup` 查重结果视图。
    DupView { cursor: usize },
    /// M69：diff 补丁内搜索输入。
    GitDiffSearch { input: String },
    /// M70：git 写操作确认。
    GitDiffConfirm { op: crate::state::GitOp },
    /// M70：git commit 信息输入。
    GitCommitInput { input: String },
    /// M56-8：100% 模块重入时的重置确认（from_list 决定取消后回列表还是 Normal）。
    TutorResetConfirm { module_id: String, from_list: bool },
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
    /// M49-1：frecency 右侧预览搜索输入态。
    FrecencyPreviewSearch { input: String },
    /// 删除确认模式：`y` 执行，其余取消（M2-2；M13-3 起 permanent 标志
    /// 区分 dd 暂存删除与 DD 永久删除，渲染与执行分支不同）。
    Confirm {
        path: PathBuf,
        permanent: bool,
        remote: Option<String>,
        /// M46-10：文件列表浏览光标/滚动（单文件通常为 0）。
        cursor: usize,
        scroll: usize,
    },
    /// 标记批量删除确认模式（`:tag delete`，M3-3；M46-10：带列表导航状态）。
    ConfirmTagDelete { cursor: usize, scroll: usize },
    /// M48-5：粘贴同名冲突的阻塞确认（复用删除确认视觉）。
    ConfirmPaste { cursor: usize, scroll: usize },
    /// M48-10：导师列表搜索输入（按模块名过滤并跳转）。
    TutorListSearch { input: String, cursor: usize },
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
    /// 向导模式模块列表覆盖层（M42-1：`:tutor` 无参）：j/k 选择、Enter/l
    /// 进入、Esc/q 关闭。
    TutorList {
        cursor: usize,
        search: Option<String>,
    },
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
    /// M39-4：首连指纹确认（strict_host_key = ask）——y 接受（写入
    /// known_hosts 并重连）、n/Esc 拒绝（中止，TUI 状态不变）。
    RemoteFingerprint {
        name: String,
        params: crate::remote::sftp::SftpParams,
        fp: String,
    },
}

/// M92-3：外部变更刷新去抖判定（150ms 窗口）。纯函数便于测试。
pub(crate) fn fs_refresh_due(last: Option<std::time::Instant>, now: std::time::Instant) -> bool {
    last.is_none_or(|t| now.duration_since(t) >= std::time::Duration::from_millis(150))
}

/// 是否需要保留用户输入法以输入自然语言。其它模式（包括 Normal、确认框和菜单）
/// 使用英文快捷键；JumpSelect 进入其 Vim 操作态后也属于 Normal 操作。
/// M48-9：Normal 模式周期切英文的到期判定（纯函数便于测试）。
pub(crate) fn ime_periodic_due(
    now: std::time::Instant,
    next: std::time::Instant,
    editable: bool,
) -> bool {
    !editable && now >= next
}

fn ime_editable_mode(mode: &InputMode) -> bool {
    matches!(
        mode,
        InputMode::LiveFind
            | InputMode::GitDiffSearch { .. }
            | InputMode::GitCommitInput { .. }
            | InputMode::Command { .. }
            | InputMode::Search { .. }
            | InputMode::Filter { .. }
            | InputMode::HelpSearch { .. }
            | InputMode::AskPass { .. }
            | InputMode::JumpSelect { normal: false, .. }
            | InputMode::ViewerSearch { .. }
            | InputMode::ArchiveDialog { .. }
            | InputMode::Rename { .. }
            | InputMode::RemotePass { .. }
            | InputMode::FrecencyPreviewSearch { .. }
            | InputMode::TutorListSearch { .. }
    )
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
    /// M50-1：与 candidates 一一对应的右侧描述（内部命令说明 / 外部命令提示）。
    descs: Vec<String>,
    /// M58-4：当前视口首行偏移（Tab 循环时自动滚动使选中项可见）。
    offset: usize,
    /// M87-1：SEARCH（输入实时过滤）/ Normal（j/k 导航）模式。
    pub(crate) normal: bool,
    /// M87-2：数据源为 `!` 外部命令历史。
    pub(crate) external: bool,
    /// M87-4：Normal 态 dd 两键删除待确认。
    pub(crate) d_pending: bool,
}

/// `:` 命令执行结果（M2-6）：退出请求或完成（可携带提示）。
#[derive(Debug, PartialEq)]
pub(crate) enum ExecOutcome {
    /// 关闭最后一个 tab，应用退出。
    Quit,
    /// 打开帮助覆盖层（`:help`，M5-3；完整帮助页在 M6-2）。
    EnterHelp,
    /// M69：打开 git diff 只读 overlay。
    EnterGitDiff,
    /// M93-2：打开查重结果视图。
    EnterDupView,
    /// M93-5：后台归档解压作业（测试态仍同步执行，故仅生产构造）。
    #[cfg_attr(test, allow(dead_code))]
    SpawnExtract {
        host: std::path::PathBuf,
        dest: std::path::PathBuf,
        tab_id: usize,
    },
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
    /// 打开传输队列覆盖层（M36-7：`:queue`）。
    EnterQueue {
        cursor: usize,
    },
    /// 打开向导模式模块列表覆盖层（M42-1：`:tutor` 无参）。
    EnterTutorList,
    /// 进入导师模式会话（M42-1：`:tutor <module_id>`；exec 已校验存在）。
    EnterTutorSession {
        module_id: String,
    },
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
    /// M39-4：首连指纹确认（strict_host_key = ask，缺省）。
    RemoteFingerprint {
        name: String,
        params: crate::remote::sftp::SftpParams,
        fp: String,
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

/// M42-1：向导模式模块列表按键结果。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TutorListOutcome {
    /// 未消费（仅 j/k 移动等）。
    None,
    /// 关闭列表（q/Esc）。
    Close,
    /// 进入选中模块（携带模块下标）。
    Enter(usize),
    /// M48-10：`/` 进入列表搜索。
    Search,
    /// M48-10：`d` 重置光标模块进度。
    Reset(usize),
}

/// M42-1：导师列表按键（纯函数，run 循环与测试共用）。
pub(crate) fn tutor_list_key(
    cursor: &mut usize,
    modules_len: usize,
    code: ratatui::crossterm::event::KeyCode,
) -> TutorListOutcome {
    use ratatui::crossterm::event::KeyCode;
    match code {
        KeyCode::Esc | KeyCode::Char('q') => TutorListOutcome::Close,
        KeyCode::Char('j') | KeyCode::Down if modules_len > 0 => {
            *cursor = (*cursor + 1).min(modules_len - 1);
            TutorListOutcome::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            *cursor = cursor.saturating_sub(1);
            TutorListOutcome::None
        }
        KeyCode::Enter | KeyCode::Char('l') if *cursor < modules_len => {
            TutorListOutcome::Enter(*cursor)
        }
        KeyCode::Char('/') => TutorListOutcome::Search,
        KeyCode::Char('d') if *cursor < modules_len => TutorListOutcome::Reset(*cursor),
        _ => TutorListOutcome::None,
    }
}

/// M67-3：进入向导会话时把左右两栏定位到沙箱目录（已清理则重建）。
fn enter_tutor_sandbox(state: &mut AppState) {
    let Ok((left, right)) = crate::tutor::ensure_sandbox() else {
        return;
    };
    let cfg = state.config.clone();
    let tab = state.active_tab_mut();
    tab.location = Location::Physical(left);
    tab.cursor = 0;
    tab.scroll = 0;
    let _ = tab.refresh(cfg.clone());
    if let Ok(pane) = crate::tab::Pane::from_physical(&right, cfg) {
        tab.right = pane;
    }
    tab.focus = crate::tab::Focus::Left;
}

/// M70：执行 git 写操作并刷新 diff overlay（失败不改本地视图）。
fn run_git_op(state: &mut AppState, op: &crate::state::GitOp, msg: &mut Option<String>) {
    let Some(diff) = state.git_diff.as_ref() else {
        return;
    };
    let root = diff.root.clone();
    let result = match op {
        crate::state::GitOp::StageAll => crate::git::stage_all(&root),
        crate::state::GitOp::Amend => crate::git::amend_last(&root),
        crate::state::GitOp::DiscardFile(path) => crate::git::discard_file(&root, path),
        crate::state::GitOp::Commit(text) => crate::git::commit(&root, text),
        crate::state::GitOp::Pull => crate::git::pull(&root),
        crate::state::GitOp::Push => crate::git::push(&root),
        crate::state::GitOp::DiscardHunk(path, index, _) => {
            let Some(data) = diff.data.get(path) else {
                *msg = Some("差异数据未就绪".to_owned());
                return;
            };
            let original = std::fs::read(root.join(path)).ok();
            let dirty_warning = std::process::Command::new("git")
                .args(["status", "--porcelain"])
                .current_dir(&root)
                .output()
                .ok()
                .filter(|o| !o.stdout.is_empty())
                .map(|_| "⚠ 工作区有未提交修改，反向 apply 可能受影响；请先保存现场\n")
                .unwrap_or_default();
            let _ = dirty_warning;
            match crate::git::discard_hunk(&root, path, &data.hunks, *index) {
                Ok(out) => {
                    if let Some(bytes) = original {
                        state.git_undo = Some((root.join(path), bytes));
                    }
                    Ok(out)
                }
                Err(e) => Err(e),
            }
        }
    };
    // M79-1：git 写操作后旧状态列标记必须立即失效（无论成功与否）。
    crate::git::clear_git_cache(state);
    match result {
        Ok(out) => {
            *msg = Some(format!(
                "git: {}",
                if out.is_empty() {
                    "完成".to_owned()
                } else {
                    out
                }
            ));
            if let Ok(fresh) = crate::state::GitDiffSession::open(root) {
                state.git_diff = Some(fresh);
            }
        }
        Err(e) => {
            let err = crate::state::AppError::from_git(&e);
            let _ = err.kind; // 状态栏按 hint 给出可操作提示（M94-5）
            *msg = Some(err.render());
        }
    }
}

/// M56-8：模块是否已全部掌握（按稳定 item id 判定，进度按模块 id 查）。
pub(crate) fn tutor_module_is_complete(
    progress: &crate::tutor::TutorProgress,
    module: &crate::tutor::TutorModule,
) -> bool {
    progress.get(&module.id).is_some_and(|p| {
        !module.items.is_empty()
            && module
                .items
                .iter()
                .all(|i| p.mastered.iter().any(|id| id == &i.id))
    })
}

/// M43-1：tutor 标记键前置拦截结果。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TutorHotkey {
    /// 非标记键（放行到模式分支——透明放行语义）。
    None,
    /// Ctrl+Y/Ctrl+N——已作答推进（调用方负责即时落盘）。
    Marked,
    /// Ctrl+Q——退出会话（作答已即时落盘，调用方清理 state.tutor）。
    Exit,
    /// M48-10：模块结束后 Ctrl+N 退出会话并回到导师列表重选。
    ExitToList,
}

/// M43-1：tutor 标记键（会话激活时拦截 Ctrl+Y/N/Q；其余键一律放行——
/// 练习键照常进入各模式分支，Panel 保持显示；Esc 保留清搜索/过滤语义）。
pub(crate) fn tutor_hotkey(state: &mut AppState, key: &KeyEvent) -> TutorHotkey {
    if state.tutor.is_none() || key.kind != KeyEventKind::Press {
        return TutorHotkey::None;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    // M56-7：模块完成后 Enter = 继续学习其它模块（回向导列表，等同 :tutor）。
    if !ctrl && key.code == KeyCode::Enter && state.tutor.as_ref().is_some_and(|s| s.done) {
        state.tutor = None;
        return TutorHotkey::ExitToList;
    }
    if !ctrl {
        return TutorHotkey::None;
    }
    match key.code {
        KeyCode::Char('y') => {
            if let Some(session) = &mut state.tutor {
                session.advance(true);
            }
            TutorHotkey::Marked
        }
        KeyCode::Char('n') => {
            // M48-10：模块已结束时 Ctrl+N 回到导师列表（否则仍是「未掌握」标记）。
            if state.tutor.as_ref().is_some_and(|s| s.done) {
                state.tutor = None;
                return TutorHotkey::ExitToList;
            }
            if let Some(session) = &mut state.tutor {
                session.advance(false);
            }
            TutorHotkey::Marked
        }
        KeyCode::Char('q') => TutorHotkey::Exit,
        _ => TutorHotkey::None,
    }
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

/// M56-2：首次启动（未读取到任何配置文件）进入向导模式；否则正常浏览。
/// 纯函数便于测试隔离宿主环境变量（HOME/XDG_CONFIG_HOME/APPDATA）。
fn initial_mode_for_startup(config_exists: bool) -> InputMode {
    if config_exists {
        InputMode::default()
    } else {
        InputMode::TutorList {
            cursor: 0,
            search: None,
        }
    }
}

/// 事件循环：进入时终端已由 `ratatui::init` 处于 raw mode + 备用屏。
pub fn run(terminal: ratatui::DefaultTerminal) -> std::io::Result<()> {
    // 必须在 `ensure_default_config()` 生成默认配置之前判断。
    let first_run = !crate::config::config_path().exists();
    run_with_initial_mode(terminal, initial_mode_for_startup(!first_run))
}

fn run_with_initial_mode(
    mut terminal: ratatui::DefaultTerminal,
    initial_mode: InputMode,
) -> std::io::Result<()> {
    // 启动耗时诊断：`RVIFM_STARTUP_TRACE=1 rvifm` 打印各阶段耗时（默认关闭）。
    let trace_start = std::time::Instant::now();
    macro_rules! startup_step {
        ($name:expr) => {
            if std::env::var_os("RVIFM_STARTUP_TRACE").is_some() {
                eprintln!("startup {}: {:?}", $name, trace_start.elapsed());
            }
        };
    }
    // FEAT-1：启用鼠标捕获（单击聚焦/滚轮滚动）；禁用在 main 的 restore 后统一执行。
    let _ = ratatui::crossterm::execute!(std::io::stdout(), event::EnableMouseCapture);
    // M47-6：终端焦点事件用于从其它窗口切回时防御性切英文（不支持的终端
    // 不会发该事件，Normal 模式 Esc 兜底）。
    let _ = ratatui::crossterm::execute!(std::io::stdout(), event::EnableFocusChange);
    // M46-6：首次启动生成带注释的默认 config.toml 与键位/opener 示例；
    // 已存在文件不覆盖，失败静默降级（不与下面的加载错误提示冲突）。
    config::ensure_default_config();
    startup_step!("default_config");
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
    startup_step!("merge_pending");
    // AUDIT-4：TUI 启动惰性 compaction（events 超阈值 → snapshot + 截断；
    // pick 不触发——只读红线）。
    crate::frecency::lazy_compact_on_startup();
    startup_step!("lazy_compact");
    // M10-5 GAP-1：加载 [normal] 键位分层（prepend/append/replace）。
    config::init_keymap_layers();
    // M85：配置在构造前决定协议查询，关闭时直接 halfblocks（零 2s 等待）。
    let allow_image_query = config.preview.image_protocol_query;
    let mut state = AppState::new_with_image_query(
        Tab::from_physical(&std::env::current_dir()?, config.clone())?,
        allow_image_query,
    );
    state.config = config;
    // M85：构造期已按配置关闭查询；这里再次兜底（含运行时配置重载路径）。
    if !state.config.preview.image_protocol_query {
        state.image_engine.force_halfblocks();
    }
    // M50-3：shell 历史补全候选 + 增量记账（默认开，失败静默降级）。
    if state.config.command.scan_shell_history {
        state.history_commands = crate::shell_history::read_all_commands();
        startup_step!("history_read");
        crate::shell_history::record_new_commands(true, &state.config.frecency.ignore_patterns);
        startup_step!("history_record");
    }
    // M25-1：会话恢复（关闭/缺失/损坏 → 保持初始 CWD tab）。
    // M32-3：恢复跳过的 tab 汇入状态栏提示（远端重连失败/归档缺失）。
    startup_step!("appstate_new");
    let hints = crate::session::restore_session_with_hints(&mut state);
    // M67-4：会话若停在沙箱而临时目录已被系统清理，启动时重建。
    if state.tabs.iter().any(|t| {
        matches!(&t.location, Location::Physical(p) if crate::tutor::is_sandbox_path(p))
            || matches!(&t.right.location, Location::Physical(p) if crate::tutor::is_sandbox_path(p))
    }) {
        let _ = crate::tutor::ensure_sandbox();
    }
    if let Some(h) = hints.first() {
        msg = Some(h.clone());
    }
    let layers = config::keymap_layers();
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
    let mut input = InputState::with_mode_maps(keymap, &config::keymap_layers());
    let mut mode = initial_mode;
    startup_step!("session_restore");
    let ime_worker = crate::ime::ImeWorker::start(state.config.ime.clone());
    startup_step!("ime_worker");
    // Worker 启动时即处理初始 Normal 状态；后续转换由事件循环边界驱动。
    let mut ime_was_editable = ime_editable_mode(&mode);
    // M48-9：终端不发 FocusGained 时仍周期探测切英文（1s 量级；事件循环每
    // 100ms 醒来一次，故无需额外线程）。仅在 Normal 模式生效，不覆盖已保存态。
    const IME_PERIODIC_MS: u64 = 1_000;
    let mut next_ime_force =
        std::time::Instant::now() + std::time::Duration::from_millis(IME_PERIODIC_MS);
    // AUDIT-6：多键序列可配置超时（`[keymap] seq_timeout_ms`，缺省 500）。
    // 待决 seq 的截止时刻：seq 变化时重置，空时清除；超时清空 seq
    // （不吞后续按键）。`seq_len` 变化检测用于区分"同一段待决"与"新键"。
    let seq_timeout_ms = state.config.keymap.seq_timeout_ms;
    let mut seq_deadline: Option<std::time::Instant> = None;
    // M92-3：外部变更刷新去抖（150ms 窗口内多次事件合并为一次）。
    let mut last_fs_refresh: Option<std::time::Instant> = None;
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
        // M41-4：视口偏移同步（上一帧输入处理后的光标落点落盘 Tab/Pane
        // scroll；渲染与鼠标命中测试共用同一偏移）。
        {
            let size = terminal.size()?;
            let area = ratatui::layout::Rect::new(0, 0, size.width, size.height);
            let [_, left, _, _] =
                pane_layout(area, state.config.ui.layout(), state.active_tab().view_mode);
            let mut list_h = left_list_area(state.config.ui.pane_borders, left).height as usize;
            // M45-8c：Tutor 面板遮住 pane 底部——视口高度须扣除面板，
            // `G`/j 滚动到底部时高亮仍落在可见区（不被面板挡住）。
            if state.tutor.is_some() {
                let panel_h = crate::app::render::tutor_panel_height(area.height) as usize;
                list_h = list_h.saturating_sub(panel_h);
            }
            state.active_tab_mut().sync_viewport_scroll(list_h);
            let right_focus = state.active_tab().focus == crate::tab::Focus::Right;
            let scroll = if right_focus {
                state.active_tab().right.scroll
            } else {
                state.active_tab().scroll
            };
            state.fill_visible_attrs(state.active, right_focus, scroll, list_h);
        }
        // M42：预览跟随——焦点留在原 pane 时，任何输入/鼠标/命令导致的
        // 光标或目录变化，都在绘制前同步到对侧预览（预览窗格内不跟随）。
        state.refresh_viewer_async();
        // M61：大文件映射视口读取器（mmap 是 O(1)，不读内容）。
        let virtual_threshold = state.config.preview.virtual_text_threshold;
        if let Some(path) = state.active_tab().preview_target()
            && crate::viewport::use_for(&path, virtual_threshold)
            && state.viewport_text.as_ref().map(|v| v.path()) != Some(path.as_path())
        {
            state.viewport_text = crate::viewport::ViewportText::open(&path).ok();
        }
        // M60-3：获焦图片预览后台解码（UI 线程只做协议编码）。
        let focus = state.active_tab().preview_focus
            || state
                .active_tab()
                .pane_frecency
                .as_ref()
                .is_some_and(|p| p.preview_focus);
        if focus
            && let Some(path) = state.active_tab().preview_target()
            && crate::preview::preview_kind(&path) == crate::preview::PreviewKind::Image
        {
            state.ensure_image_preview(&path);
        }
        // M60-4：Office/归档/PDF 文本的重型解析走后台（PDF 视觉页仍需 Frame，留在 UI 线程）。
        if let Some(path) = state.active_tab().preview_target() {
            let kind = crate::preview::preview_kind(&path);
            let special = matches!(
                kind,
                crate::preview::PreviewKind::Office | crate::preview::PreviewKind::Archive
            ) || (kind == crate::preview::PreviewKind::Pdf
                && state.config.preview.pdf_image_pages == 0);
            if special {
                let external = state.config.preview.archive_external_listing;
                state.ensure_special_preview(&path, external);
            }
        }
        // M53-3：归档成员预览——光标变化时解出临时只读副本（按 key 缓存）。
        {
            let tab = state.active_tab();
            if let crate::tab::Location::Archive { host, inner } = tab.focused_location() {
                let member = tab
                    .focused_entries()
                    .get(tab.focused_cursor())
                    .map(|i| i.name.clone());
                if let Some(name) = member {
                    let full = if inner.as_os_str().is_empty() {
                        name
                    } else {
                        format!("{}/{}", inner.to_string_lossy().replace('\\', "/"), name)
                    };
                    let key = format!("{}:{}", host.display(), full);
                    let stale =
                        state.active_tab().archive_preview_key.as_deref() != Some(key.as_str());
                    if stale {
                        // M53-8：目标稳定后再解临时副本（快速 j/k 不生成中间预览）。
                        let due = state
                            .active_tab()
                            .archive_preview_pending
                            .as_ref()
                            .is_some_and(|(k, since)| {
                                k == &key && crate::tab::preview_debounce_due(*since)
                            });
                        let registered = state
                            .active_tab()
                            .archive_preview_pending
                            .as_ref()
                            .map(|(k, _)| k == &key)
                            .unwrap_or(false);
                        if due {
                            let h = host.clone();
                            if let Ok(tmp) =
                                crate::app::exec::extract_member_to_temp(&mut state, &h, &full)
                            {
                                let tab = state.active_tab_mut();
                                tab.archive_preview_key = Some(key);
                                tab.archive_preview_tmp = Some(tmp);
                                tab.archive_preview_pending = None;
                            }
                        } else if !registered {
                            state.active_tab_mut().archive_preview_pending =
                                Some((key, std::time::Instant::now()));
                        }
                    } else {
                        state.active_tab_mut().archive_preview_pending = None;
                    }
                }
            } else if state.active_tab().archive_preview_key.is_some() {
                let tab = state.active_tab_mut();
                tab.archive_preview_key = None;
                tab.archive_preview_tmp = None;
            }
        }
        terminal.draw(|frame| render(frame, &state, &mode, msg.as_deref()))?;
        // 列表区高度 = 终端高度 - 状态栏 1 行。
        let viewport_h = terminal.size()?.height.saturating_sub(1);
        // AUDIT-7：block=true opener 的挂起执行（open_selected 经
        // state.pending_external 转交；与 :! 同款 TUI 挂起/恢复语义）。
        if let Some(cmd) = state.pending_external.take() {
            // M78-2：跨外部命令成对 suspend/resume，避免周期强制切英文干扰外部程序。
            crate::ime::suspend_ime();
            (terminal, msg) = suspend_execute(&cmd);
            crate::ime::resume_ime();
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
            let now = std::time::Instant::now();
            if fs_refresh_due(last_fs_refresh, now) {
                last_fs_refresh = Some(now);
                // AUDIT-8：自动刷新失败不再静默（可读提示进状态栏）。
                if let Some(e) = refresh_on_fs_change(&mut state) {
                    msg = Some(e);
                }
                // M25-5/M79-1：外部变更 → git 状态缓存整体失效。
                crate::git::clear_git_cache(&mut state);
            }
        }
        // M25-5：收割 git 状态查询结果（回填缓存）+ 按需触发新查询。
        while let Ok((dir, res)) = git_rx.try_recv() {
            state.git_pending.remove(&dir);
            state.git_cache.insert(dir.clone(), res.ok());
            state.git_cache_at.insert(dir, std::time::Instant::now());
        }
        crate::git::sync_status_trigger(&mut state);
        // M93-1：收割 `:du` 后台扫描结果并打开视图。
        if let Ok(result) = state.du_rx.try_recv() {
            state.du_result = Some(result);
            mode = InputMode::DuView { cursor: 0 };
        }
        // M92-1：收割后台目录加载结果（代次 + 路径双校验）。
        let dir_cfg = state.config.clone();
        state.drain_dir_loads(&dir_cfg);
        // M60-1：收割后台预览载荷（只接受当前代次；快速导航不渲染中间预览）。
        state.drain_preview_results();
        // M85：收割后台 resize+encode 结果（ThreadProtocol 更新）。
        state.drain_image_resizes();
        // M69-2：收割后台 diff 结果，按需为当前文件投递只读 diff 任务。
        if let Some(diff) = state.git_diff.as_mut() {
            diff.drain();
            // M72-2：打开 :diff 即加载当前光标文件的补丁（右栏无需先 Tab）。
            let target = diff.selected_path().map(str::to_owned);
            let staged = diff
                .files
                .get(diff.cursor)
                .is_some_and(|(_, x, y)| *x != ' ' && *y == ' ');
            if let Some(path) = target {
                diff.request(&path, staged, state.config.git.diff_context);
            }
        }
        // M55-2：抽干实时递归扫描结果（每帧限量，避免大量条目阻塞绘制）。
        if let Some(lf) = state.live_find.as_mut() {
            lf.drain(256);
        }
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
        let mut return_or_skip = false;
        if !state.transfer_queue.is_empty()
            && state
                .transfer_queue
                .first()
                .map(|e| e.direction == crate::app::jobs::TransferDirection::Upload)
                .unwrap_or(false)
        {
            let mut entry = state.transfer_queue.remove(0);
            if entry.state == crate::state::TransferState::Paused
                || entry.state == crate::state::TransferState::Cancelled
            {
                // 暂停/取消项留在队列里并轮到队尾，不阻塞其它条目。
                state.transfer_queue.push(entry.clone());
                state.transfer_queue.rotate_left(1);
                return_or_skip = true;
            } else {
                entry.state = crate::state::TransferState::Running;
            }
            if !return_or_skip {
                let remote_name = entry.remote.clone();
                let tab_id = entry.tab_id;
                let src = entry.src.clone();
                let dest = entry.dest.clone();
                // 后端借用限于此作用域，之后才能安全改 state/entry。
                let uploaded = state
                    .remote_sessions
                    .get(&remote_name)
                    .map(|backend| upload_chunked(backend.as_ref(), &src, &dest));
                match uploaded {
                    Some(Ok(())) => {
                        entry.state = crate::state::TransferState::Done;
                        state.undo_stack.push(crate::state::UndoableOp::Remote {
                            tab_id,
                            remote: remote_name,
                            kind: crate::state::RemoteUndo::Create {
                                path: dest,
                                is_dir: false,
                            },
                        });
                    }
                    Some(Err((_, e))) => {
                        entry.state = crate::state::TransferState::Failed;
                        entry.error = Some(e.clone());
                        msg = Some(format!("queue: {} 失败: {e}", entry.src.display()));
                    }
                    None => {
                        entry.state = crate::state::TransferState::Failed;
                        entry.error = Some("远端会话不存在".to_owned());
                    }
                }
                refresh_tab_panes(&mut state, tab_id);
            }
            if entry.state != crate::state::TransferState::Done {
                state.transfer_queue.push(entry);
            }
        }
        // M91-4：队列消费——Download 条目走流式下载（与 Upload 对称）。
        if state
            .transfer_queue
            .first()
            .map(|e| {
                e.direction == crate::app::jobs::TransferDirection::Download
                    && e.state == crate::state::TransferState::Pending
            })
            .unwrap_or(false)
        {
            let mut entry = state.transfer_queue.remove(0);
            entry.state = crate::state::TransferState::Running;
            let remote_name = entry.remote.clone();
            let tab_id = entry.tab_id;
            let src = entry.src.clone();
            let dest = entry.dest.clone();
            let downloaded = state
                .remote_sessions
                .get(&remote_name)
                .map(|backend| crate::app::exec::download_chunked(backend.as_ref(), &src, &dest));
            match downloaded {
                Some(Ok(())) => {
                    entry.state = crate::state::TransferState::Done;
                    state.undo_stack.push(crate::state::UndoableOp::Create {
                        tab_id,
                        path: dest,
                        recursive: false,
                    });
                }
                Some(Err((offset, e))) => {
                    entry.state = crate::state::TransferState::Failed;
                    entry.transferred = offset;
                    entry.error = Some(e);
                }
                None => {
                    entry.state = crate::state::TransferState::Failed;
                    entry.error = Some("远端会话不存在".to_owned());
                }
            }
            refresh_tab_panes(&mut state, tab_id);
            if entry.state != crate::state::TransferState::Done {
                state.transfer_queue.push(entry);
            }
        }
        // M13-8：事件轮询（100ms）——后台作业完成即时感知，UI 不空转阻塞；
        // 有待决序列时预算收缩到超时余量（AUDIT-6）。
        let polled = event::poll(std::time::Duration::from_millis(poll_ms))?;
        let key_ev = if polled { event::read().ok() } else { None };
        // M51-1：焦点事件优先——失焦暂停全局 IME 强制，回焦恢复。
        if matches!(key_ev, Some(Event::FocusLost)) {
            crate::ime::suspend_ime();
        }
        if matches!(key_ev, Some(Event::FocusGained)) {
            crate::ime::resume_ime();
        }
        // M78-2：按键或鼠标都视为用户已回到 rvifm（鼠标返回不再留下 suspend 残留）。
        if matches!(key_ev, Some(Event::Key(_)) | Some(Event::Mouse(_))) {
            crate::ime::resume_ime();
        }
        // M47-6：切回终端窗口且不在可编辑模式时切英文；多数终端不发送此事件，
        // 故 Normal 模式 Esc 也触发（见下方 Normal 分支）。
        if matches!(key_ev, Some(Event::FocusGained)) && !ime_editable_mode(&mode) {
            ime_worker.force_english();
            next_ime_force =
                std::time::Instant::now() + std::time::Duration::from_millis(IME_PERIODIC_MS);
        }
        // M48-9：周期兜底——Normal 模式每 1s 重探测一次输入法状态；force_english
        // 只在非英文时发切换命令且不影响 FIX-9 保存的恢复状态。
        if ime_periodic_due(
            std::time::Instant::now(),
            next_ime_force,
            ime_editable_mode(&mode),
        ) {
            ime_worker.force_english();
            next_ime_force =
                std::time::Instant::now() + std::time::Duration::from_millis(IME_PERIODIC_MS);
        }
        if let Some(Event::Key(key)) = key_ev {
            // M43-1：tutor 标记键前置拦截（透明放行——练习键不拦，标记在
            // 任意模式生效：命令输入/覆盖层激活时也可 Ctrl+Y/N 标记）。
            match tutor_hotkey(&mut state, &key) {
                TutorHotkey::None => {}
                TutorHotkey::Marked => {
                    // 作答即时落盘（幂等合并——中途退出/应用退出均不丢进度）。
                    if let Some(session) = &state.tutor
                        && !session.marks.is_empty()
                    {
                        let progress = crate::tutor::merge_session(
                            crate::tutor::load_progress(),
                            &session.module.id,
                            &session.marks,
                        );
                        // M45-8d：缓存同步刷新（列表左侧进度即时更新）。
                        state.tutor_progress = progress.clone();
                        if let Err(e) = crate::tutor::save_progress_to(
                            &crate::tutor::tutor_progress_path(),
                            &progress,
                        ) {
                            msg = Some(format!("tutor 进度保存失败: {e}"));
                        }
                    }
                    continue;
                }
                TutorHotkey::Exit => {
                    state.tutor = None; // 作答已即时落盘，退出仅关闭面板。
                    continue;
                }
                TutorHotkey::ExitToList => {
                    // M48-10：作答已落盘，回到导师列表重新选模块。
                    state.tutor_progress = crate::tutor::load_progress();
                    mode = InputMode::TutorList {
                        cursor: 0,
                        search: None,
                    };
                    continue;
                }
            }
            match &mut mode {
                InputMode::DupView { cursor } => {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') => mode = InputMode::Normal,
                            KeyCode::Char('j') | KeyCode::Down => {
                                let len = state
                                    .dup_result
                                    .as_ref()
                                    .map(|r| r.entries.len())
                                    .unwrap_or(0);
                                if len > 0 {
                                    *cursor = (*cursor + 1).min(len - 1);
                                }
                            }
                            KeyCode::Char('k') | KeyCode::Up => *cursor = cursor.saturating_sub(1),
                            _ => {}
                        }
                    }
                    continue;
                }
                InputMode::DuView { cursor } => {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') => mode = InputMode::Normal,
                            KeyCode::Char('j') | KeyCode::Down => {
                                let len = state
                                    .du_result
                                    .as_ref()
                                    .map(|r| r.entries.len())
                                    .unwrap_or(0);
                                if len > 0 {
                                    *cursor = (*cursor + 1).min(len - 1);
                                }
                            }
                            KeyCode::Char('k') | KeyCode::Up => *cursor = cursor.saturating_sub(1),
                            KeyCode::Char('d') => {
                                if let Some(result) = &state.du_result
                                    && let Some((name, _)) = result.entries.get(*cursor)
                                {
                                    let _ = name;
                                    msg = Some("DuView: 选中项删除需在目录列表中执行".to_owned());
                                }
                            }
                            _ => {}
                        }
                    }
                    continue;
                }
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
                    // M87：`:` 直达命令补全（SEARCH 默认，Esc 进 Normal，
                    // Normal Esc 退出）；↑↓/Ctrl+j/k 导航；Tab 不再是导航键；
                    // Enter 填充选中项、Ctrl+Enter 直接执行；`!` 切外部命令源。
                    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                    let normal = comp.as_ref().is_some_and(|c| c.normal);
                    let move_sel = |c: &mut Option<CmdComp>, delta: isize| {
                        if let Some(c) = c.as_mut()
                            && !c.candidates.is_empty()
                        {
                            let n = c.candidates.len() as isize;
                            c.index = (((c.index as isize + delta) % n) + n) as usize % n as usize;
                            c.offset = c.offset.min(c.index);
                        }
                    };
                    let handled = if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Up | KeyCode::Char('k') if ctrl || normal => {
                                move_sel(comp, -1);
                                Some(CmdInput::Continue)
                            }
                            KeyCode::Down | KeyCode::Char('j') if ctrl || normal => {
                                move_sel(comp, 1);
                                Some(CmdInput::Continue)
                            }
                            KeyCode::Char('g') if normal => {
                                if let Some(c) = comp.as_mut() {
                                    c.index = 0;
                                    c.offset = 0;
                                }
                                Some(CmdInput::Continue)
                            }
                            KeyCode::Char('G') if normal => {
                                if let Some(c) = comp.as_mut() {
                                    c.index = c.candidates.len().saturating_sub(1);
                                }
                                Some(CmdInput::Continue)
                            }
                            KeyCode::Char('d')
                                if normal && comp.as_ref().is_some_and(|c| c.external) =>
                            {
                                let remove = comp.as_ref().is_some_and(|c| c.d_pending);
                                if remove {
                                    if let Some(cmd) = comp
                                        .as_ref()
                                        .and_then(|c| c.candidates.get(c.index))
                                        .cloned()
                                    {
                                        let bare = cmd.trim_start_matches('!').to_owned();
                                        state
                                            .history_commands
                                            .retain(|l| l.trim_start() != bare.as_str());
                                        state.cmd_history.retain(|l| l != &bare);
                                        state.last_removed_external = Some(bare);
                                        *comp = Some(crate::app::exec::build_command_list_comp(
                                            &state, input,
                                        ));
                                        if let Some(c) = comp.as_mut() {
                                            c.normal = true;
                                        }
                                        msg = Some("已删除外部命令记录（u 恢复）".to_owned());
                                    }
                                } else if let Some(c) = comp.as_mut() {
                                    c.d_pending = true;
                                }
                                Some(CmdInput::Continue)
                            }
                            KeyCode::Char('u')
                                if normal && comp.as_ref().is_some_and(|c| c.external) =>
                            {
                                if let Some(cmd) = state.last_removed_external.take() {
                                    state.history_commands.push(cmd);
                                    *comp = Some(crate::app::exec::build_command_list_comp(
                                        &state, input,
                                    ));
                                    if let Some(c) = comp.as_mut() {
                                        c.normal = true;
                                    }
                                    msg = Some("已恢复外部命令记录".to_owned());
                                }
                                Some(CmdInput::Continue)
                            }
                            KeyCode::Esc if comp.is_some() && !normal => {
                                if let Some(c) = comp.as_mut() {
                                    c.normal = true;
                                }
                                Some(CmdInput::Continue)
                            }
                            KeyCode::Enter
                                if comp.as_ref().is_some_and(|c| !c.candidates.is_empty()) =>
                            {
                                let selected = comp
                                    .as_ref()
                                    .and_then(|c| c.candidates.get(c.index))
                                    .cloned()
                                    .unwrap_or_default();
                                *input = selected;
                                if ctrl {
                                    Some(CmdInput::Submit)
                                } else {
                                    // 填充后可继续编辑，下一次 Enter 才执行。
                                    *comp = None;
                                    Some(CmdInput::Continue)
                                }
                            }
                            KeyCode::Tab => Some(CmdInput::Continue),
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
                                    }
                                    None => msg = Some(format!("无命令书签 {c}")),
                                }
                                Some(CmdInput::Continue)
                            }
                            KeyCode::Up => {
                                hist_prev(ranked, hist_pos, input, draft);
                                Some(CmdInput::Continue)
                            }
                            KeyCode::Down => {
                                hist_next(ranked, hist_pos, input, draft);
                                Some(CmdInput::Continue)
                            }
                            KeyCode::Char(_) | KeyCode::Backspace => {
                                let result = command_input(input, key);
                                if comp.is_some() && result != CmdInput::Cancel {
                                    *comp = Some(crate::app::exec::build_command_list_comp(
                                        &state, input,
                                    ));
                                }
                                Some(result)
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
                                Ok(ExecOutcome::EnterDupView) => {
                                    mode = InputMode::DupView { cursor: 0 };
                                }
                                Ok(ExecOutcome::EnterHelp) => {
                                    mode = InputMode::HelpView {
                                        scroll: 0,
                                        search: None,
                                    };
                                }
                                Ok(ExecOutcome::Done { msg: m }) => msg = m,
                                Ok(ExecOutcome::ConfirmTagDelete) => {
                                    mode = InputMode::ConfirmTagDelete {
                                        cursor: 0,
                                        scroll: 0,
                                    };
                                }
                                Ok(ExecOutcome::Escalate { op }) => {
                                    // M12-3：EACCES → 原地 sudo 密码输入（操作级提权）。
                                    // M39-5：Windows → UAC 弹窗（ShellExecuteExW runas，
                                    // 操作级不提升主进程；取消/失败状态不变）。
                                    #[cfg(windows)]
                                    {
                                        msg = match crate::privilege::uac_execute(&op) {
                                            Ok(()) => {
                                                let _tid = state.active;
                                                refresh_tab_panes(&mut state, _tid);
                                                Some(format!("提权完成: {}", op.describe()))
                                            }
                                            Err(e) => Some(format!("提权失败: {e}")),
                                        };
                                    }
                                    #[cfg(not(windows))]
                                    {
                                        mode = InputMode::AskPass {
                                            input: String::new(),
                                            op,
                                        };
                                    }
                                }
                                Ok(ExecOutcome::OpenJump {
                                    kind: JumpKind::Dir | JumpKind::File,
                                    entries,
                                }) => {
                                    // FIX-8：:j/:J 与 o 共用焦点 pane frecency 面板。
                                    state.active_tab_mut().pane_frecency =
                                        Some(crate::tab::FrecencyPane {
                                            entries,
                                            query: String::new(),
                                            cursor: 0,
                                            normal: false,
                                            g_pending: false,
                                            preview_focus: false,
                                            preview_scroll: 0,
                                            preview_query: None,
                                            preview_match: 0,
                                            d_pending: false,
                                            y_pending: false,
                                            last_deleted: None,
                                            r_pending: false,
                                            last_keyword: None,
                                            visual: None,
                                            last_deleted_many: Vec::new(),
                                        });
                                    mode = InputMode::Normal;
                                }
                                Ok(ExecOutcome::OpenJump { kind, entries }) => {
                                    // :recent 保留其近期排序浮层语义。
                                    mode = InputMode::JumpSelect {
                                        input: String::new(),
                                        cursor: 0,
                                        entries,
                                        kind,
                                        normal: false,
                                        g_pending: false,
                                    };
                                }
                                Ok(ExecOutcome::SpawnExtract { host, dest, tab_id }) => {
                                    let op = format!("extract:{}", host.display());
                                    if let Some(e) = spawn_job_from_plan(
                                        &mut state,
                                        &job_tx,
                                        &op,
                                        &[],
                                        &dest,
                                        &host,
                                    ) {
                                        msg = Some(e);
                                    } else {
                                        let _ = tab_id;
                                        msg = Some("extract: 后台解压已入 :jobs".to_owned());
                                    }
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
                                    } else if op.starts_with("archive:") {
                                        // M86-5：两栏同目录时产物视觉上仍在"同侧"，
                                        // 显式提示目标路径，消除"生成到同侧"的误解。
                                        msg = Some(format!(
                                            "archive: 产物写入对侧 {}",
                                            target.display().to_string().replace('\\', "/")
                                        ));
                                    }
                                }
                                Ok(ExecOutcome::ShowJobs) => {
                                    mode = InputMode::Jobs { cursor: 0 };
                                }
                                Ok(ExecOutcome::EnterQueue { cursor }) => {
                                    mode = InputMode::Queue { cursor };
                                }
                                Ok(ExecOutcome::EnterGitDiff) => {
                                    // M69：只在本地 git 仓库打开；失败给可读提示并保持原状态。
                                    let base = state.active_tab().focus_base().to_path_buf();
                                    match crate::git::require_repo_root(&base)
                                        .and_then(crate::state::GitDiffSession::open)
                                    {
                                        Ok(session) => {
                                            state.git_diff = Some(session);
                                            mode = InputMode::GitDiff;
                                        }
                                        Err(e) => {
                                            msg = Some(format!("diff: {e}"));
                                        }
                                    }
                                }
                                Ok(ExecOutcome::EnterTutorList) => {
                                    // M42-1：:tutor 无参 → 模块列表覆盖层。
                                    mode = InputMode::TutorList {
                                        cursor: 0,
                                        search: None,
                                    };
                                }
                                Ok(ExecOutcome::EnterTutorSession { module_id }) => {
                                    // M42-1：进入导师会话（模块克隆持有，渲染
                                    // 每帧免重解析；exec 已校验存在，此兜底
                                    // 仅防御内嵌内容变动）。M42-2：有错题本
                                    // 记录时进入复习模式（skipped 项排前）。
                                    match crate::tutor::module_by_id(&module_id) {
                                        Some(base) => {
                                            let progress = crate::tutor::load_progress();
                                            if tutor_module_is_complete(&progress, &base) {
                                                mode = InputMode::TutorResetConfirm {
                                                    module_id: base.id.clone(),
                                                    from_list: false,
                                                };
                                            } else {
                                                // M56-6：与列表 l/Enter 同源——有记录模块只显示错题。
                                                let (module, review) =
                                                    match crate::tutor::pending_items(
                                                        &progress,
                                                        &base.id,
                                                        &base.items,
                                                    ) {
                                                        Some(items) => (
                                                            crate::tutor::TutorModule {
                                                                items,
                                                                ..base.clone()
                                                            },
                                                            true,
                                                        ),
                                                        None => (base, false),
                                                    };
                                                state.tutor = Some(crate::tutor::TutorSession {
                                                    module,
                                                    idx: 0,
                                                    marks: Vec::new(),
                                                    done: false,
                                                    review,
                                                });
                                                enter_tutor_sandbox(&mut state);
                                            }
                                        }
                                        None => {
                                            msg = Some(format!("tutor: 未知模块 {module_id}"));
                                        }
                                    }
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
                                Ok(ExecOutcome::RemoteFingerprint { name, params, fp }) => {
                                    // M39-4：ask 首连 → 指纹确认覆盖层。
                                    mode = InputMode::RemoteFingerprint { name, params, fp };
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
                InputMode::FrecencyPreviewSearch { input } => match command_input(input, key) {
                    CmdInput::Continue => {}
                    CmdInput::Cancel => {
                        mode = InputMode::Normal;
                        msg = None;
                    }
                    CmdInput::Submit => {
                        let q = std::mem::take(input);
                        if let Some(pane) = state.active_tab_mut().pane_frecency.as_mut() {
                            pane.preview_query = (!q.is_empty()).then_some(q);
                            pane.preview_match = 0;
                            pane.preview_scroll = 0;
                        }
                        mode = InputMode::Normal;
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
                InputMode::RemoteFingerprint { name, params, fp } => {
                    // M39-4：首连指纹确认——y 接受（写 known_hosts + 重连进入），
                    // n/Esc 拒绝（中止，TUI 状态不变，SPEC M39-4）。
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') => {
                                let name = name.clone();
                                let params = params.clone();
                                let fp = fp.clone();
                                match remote_confirm_and_enter(&mut state, &name, &params) {
                                    Ok(m) => {
                                        mode = InputMode::Normal;
                                        msg = Some(m);
                                    }
                                    Err(e) => {
                                        mode = InputMode::Normal;
                                        msg = Some(format!("{e}（指纹 {fp}）"));
                                    }
                                }
                            }
                            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                mode = InputMode::Normal;
                                msg = Some("remote: 已拒绝主机指纹（连接中止）".to_owned());
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
                    scroll,
                    ..
                } => {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            // M46-10：j/k（含方向键）浏览文件列表（单文件为 0）。
                            KeyCode::Char('j') | KeyCode::Down => {
                                *scroll = scroll.saturating_add(1);
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                *scroll = scroll.saturating_sub(1);
                            }
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
                InputMode::ConfirmTagDelete { cursor, scroll } => {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            // M46-10：j/k（含方向键）滚动查看待删除文件列表。
                            KeyCode::Char('j') | KeyCode::Down => {
                                *cursor = cursor.saturating_add(1);
                                if *cursor >= *scroll + crate::app::render::CONFIRM_LIST_HEIGHT {
                                    *scroll = *cursor - crate::app::render::CONFIRM_LIST_HEIGHT + 1;
                                }
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                *cursor = cursor.saturating_sub(1);
                                if *cursor < *scroll {
                                    *scroll = *cursor;
                                }
                            }
                            // y / Enter 确认批量删除；n / Esc / 其他 = 取消（无副作用）。
                            KeyCode::Char('y') | KeyCode::Enter => {
                                mode = InputMode::Normal;
                                execute_tag_delete(&mut state, &mut msg);
                            }
                            _ => {
                                mode = InputMode::Normal;
                                state.pending_permanent_delete = false;
                            }
                        }
                    }
                }
                InputMode::ConfirmPaste { cursor, scroll } => {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('j') | KeyCode::Down => {
                                let max = state.paste_conflict_options().len().saturating_sub(1);
                                *cursor = cursor.saturating_add(1).min(max);
                                if *cursor >= *scroll + crate::app::render::CONFIRM_LIST_HEIGHT {
                                    *scroll = *cursor - crate::app::render::CONFIRM_LIST_HEIGHT + 1;
                                }
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                *cursor = cursor.saturating_sub(1);
                                if *cursor < *scroll {
                                    *scroll = *cursor;
                                }
                            }
                            KeyCode::Char('y') | KeyCode::Enter => {
                                let options = state.paste_conflict_options();
                                let choice =
                                    options.get(*cursor).copied().unwrap_or("overwrite all");
                                mode = InputMode::Normal;
                                crate::app::exec::apply_paste_conflict_choice(
                                    &mut state, &mut msg, choice,
                                );
                            }
                            KeyCode::Esc => {
                                mode = InputMode::Normal;
                                cancel_paste_conflicts(&mut state, &mut msg);
                            }
                            _ => {}
                        }
                    }
                }
                InputMode::LiveFind => {
                    // M55-2/3：实时递归查找——按键由专用 handler 处理；返回 true 表示退出。
                    if crate::app::keys::live_find_key(&mut state, &mut msg, key) {
                        if let Some(lf) = state.live_find.take() {
                            lf.stop.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                        mode = InputMode::Normal;
                    }
                }
                InputMode::Normal => {
                    // M47-6：Normal 模式 Esc 防御性切英文（比依赖模式边界可靠）。
                    // 仅捕获"按键前已是 Normal"的场景，文本输入模式按 Esc 返回
                    // 仍由下方模式边界统一恢复输入法，不在此重复触发。
                    if key.code == KeyCode::Esc {
                        ime_worker.force_english();
                        next_ime_force = std::time::Instant::now()
                            + std::time::Duration::from_millis(IME_PERIODIC_MS);
                    }
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
                            let comp = crate::app::exec::build_command_list_comp(&state, "");
                            mode = InputMode::Command {
                                input: String::new(),
                                hist_pos: None,
                                draft: String::new(),
                                comp: Some(comp),
                                ranked: crate::frecency::cmd_history_ranked(
                                    &state.cmd_history,
                                    CMD_RANKED_LIMIT,
                                ),
                                cmd_mark_pending: false,
                            };
                        }
                        Outcome::EnterExternalCommandLine => {
                            // M50-4：预填 `!`，用户可直接输入外部命令（无默认键位）。
                            let comp = crate::app::exec::build_command_list_comp(&state, "!");
                            mode = InputMode::Command {
                                input: "!".to_owned(),
                                hist_pos: None,
                                draft: String::new(),
                                comp: Some(comp),
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
                        Outcome::EnterLiveFind => {
                            mode = InputMode::LiveFind;
                        }
                        Outcome::EnterViewerSearch => {
                            mode = InputMode::ViewerSearch {
                                input: String::new(),
                            };
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
                                cursor: 0,
                                scroll: 0,
                            };
                        }
                        Outcome::ConfirmTagDelete => {
                            // M45-3d：v+d/x 选中项已加 tag，进入批量删除确认。
                            mode = InputMode::ConfirmTagDelete {
                                cursor: 0,
                                scroll: 0,
                            };
                        }
                        Outcome::FileInfo { path, text } => {
                            mode = InputMode::FileInfo { path, text };
                        }
                        Outcome::EnterFrecencyPreviewSearch => {
                            mode = InputMode::FrecencyPreviewSearch {
                                input: String::new(),
                            };
                        }
                        Outcome::ConfirmPaste => {
                            // M48-5：粘贴冲突 → 阻塞确认弹窗（与删除确认同风格）。
                            mode = InputMode::ConfirmPaste {
                                cursor: 0,
                                scroll: 0,
                            };
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
                                preview_focus: false,
                                preview_scroll: 0,
                                preview_query: None,
                                preview_match: 0,
                                d_pending: false,
                                y_pending: false,
                                last_deleted: None,
                                r_pending: false,
                                last_keyword: None,
                                visual: None,
                                last_deleted_many: Vec::new(),
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
                            let run_dir = state.active_tab().focus_base().to_path_buf();
                            let shell =
                                std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned());
                            match run_external_target_with_dir(&ui, &shell, &run_dir) {
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
                            KeyCode::Char('j') | KeyCode::Down
                                if !state.transfer_queue.is_empty() =>
                            {
                                *cursor = (*cursor + 1).min(state.transfer_queue.len() - 1);
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                *cursor = cursor.saturating_sub(1);
                            }
                            KeyCode::Char('p') if *cursor < state.transfer_queue.len() => {
                                // M91-4：暂停/恢复切换。
                                let e = &mut state.transfer_queue[*cursor];
                                e.state = if e.state == crate::state::TransferState::Paused {
                                    crate::state::TransferState::Pending
                                } else {
                                    crate::state::TransferState::Paused
                                };
                            }
                            KeyCode::Char('r') if *cursor < state.transfer_queue.len() => {
                                // M91-4：失败项重试（清空错误、回到 Pending）。
                                let e = &mut state.transfer_queue[*cursor];
                                if e.state == crate::state::TransferState::Failed {
                                    e.state = crate::state::TransferState::Pending;
                                    e.retries += 1;
                                    e.error = None;
                                    e.transferred = 0;
                                }
                            }
                            KeyCode::Char('x') if *cursor < state.transfer_queue.len() => {
                                // M91-4：取消标记留在队列可见，直到用户清理。
                                state.transfer_queue[*cursor].state =
                                    crate::state::TransferState::Cancelled;
                                *cursor = cursor.saturating_sub(1);
                            }
                            _ => {}
                        }
                    }
                }
                InputMode::TutorListSearch { input, cursor } => {
                    if key.kind == KeyEventKind::Press {
                        if matches!(key.code, KeyCode::Up | KeyCode::Down) {
                            let len = crate::tutor::builtin_modules()
                                .iter()
                                .filter(|m| {
                                    let q = input.to_lowercase();
                                    m.id.to_lowercase().contains(&q)
                                        || m.name.to_lowercase().contains(&q)
                                })
                                .count();
                            *cursor = if key.code == KeyCode::Up {
                                cursor.saturating_sub(1)
                            } else {
                                (*cursor + 1).min(len.saturating_sub(1))
                            };
                        } else {
                            match command_input(input, key) {
                                CmdInput::Submit => {
                                    let q = input.trim().to_owned();
                                    mode = InputMode::TutorList {
                                        cursor: *cursor,
                                        search: (!q.is_empty()).then_some(q),
                                    };
                                }
                                CmdInput::Cancel => {
                                    mode = InputMode::TutorList {
                                        cursor: *cursor,
                                        search: None,
                                    };
                                }
                                CmdInput::Continue => {
                                    // M56-5：键入即过滤；游标随可见项收缩。
                                    let len = crate::tutor::builtin_modules()
                                        .iter()
                                        .filter(|m| {
                                            let q = input.to_lowercase();
                                            m.id.to_lowercase().contains(&q)
                                                || m.name.to_lowercase().contains(&q)
                                        })
                                        .count();
                                    *cursor = (*cursor).min(len.saturating_sub(1));
                                }
                            }
                        }
                    }
                }
                InputMode::GitDiff => {
                    // M69：只读 diff overlay 的局部键位（不触发 Normal 全局绑定）。
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    let n = input.count.take().unwrap_or(1).max(1);
                    let mut exit = false;
                    let mut undo = false;
                    let mut pending_op: Option<crate::state::GitOp> = None;
                    if let Some(diff) = state.git_diff.as_mut() {
                        match key.code {
                            KeyCode::Esc | KeyCode::Char('q') => exit = true,
                            KeyCode::Tab => {
                                diff.focus = match diff.focus {
                                    crate::state::DiffFocus::Files => {
                                        crate::state::DiffFocus::Patch
                                    }
                                    crate::state::DiffFocus::Patch => {
                                        crate::state::DiffFocus::Files
                                    }
                                };
                            }
                            KeyCode::Char('j') | KeyCode::Down
                                if diff.focus == crate::state::DiffFocus::Files =>
                            {
                                diff.cursor =
                                    (diff.cursor + n).min(diff.files.len().saturating_sub(1));
                            }
                            KeyCode::Char('k') | KeyCode::Up
                                if diff.focus == crate::state::DiffFocus::Files =>
                            {
                                diff.cursor = diff.cursor.saturating_sub(n);
                            }
                            KeyCode::Char('g') | KeyCode::Char('G')
                                if diff.focus == crate::state::DiffFocus::Files =>
                            {
                                diff.cursor = if key.code == KeyCode::Char('g') {
                                    0
                                } else {
                                    diff.files.len().saturating_sub(1)
                                };
                            }
                            KeyCode::Char('j') | KeyCode::Down => {
                                let (rows, _) = diff.rows(100);
                                diff.patch_scroll =
                                    (diff.patch_scroll + n).min(rows.len().saturating_sub(1));
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                diff.patch_scroll = diff.patch_scroll.saturating_sub(n);
                            }
                            KeyCode::Char('g') | KeyCode::Char('G') => {
                                let (rows, _) = diff.rows(100);
                                diff.patch_scroll = if key.code == KeyCode::Char('g') {
                                    0
                                } else {
                                    rows.len().saturating_sub(1)
                                };
                            }
                            KeyCode::Char('J') => {
                                let (_, starts) = diff.rows(100);
                                if !starts.is_empty() {
                                    diff.hunk = (diff.hunk + 1).min(starts.len() - 1);
                                    diff.patch_scroll = starts[diff.hunk];
                                }
                            }
                            KeyCode::Char('K') => {
                                diff.hunk = diff.hunk.saturating_sub(1);
                                let (_, starts) = diff.rows(100);
                                if let Some(row) = starts.get(diff.hunk) {
                                    diff.patch_scroll = *row;
                                }
                            }
                            KeyCode::Char('h') | KeyCode::Left => {
                                diff.hscroll = diff.hscroll.saturating_sub(2);
                            }
                            KeyCode::Char('l') | KeyCode::Right => {
                                diff.hscroll = diff.hscroll.saturating_add(2);
                            }
                            // M82：`|` 切换全屏左右对照（与 ziggity 一致）。
                            KeyCode::Char('|') => {
                                diff.side_by_side = !diff.side_by_side;
                                diff.fullscreen = diff.side_by_side;
                            }
                            KeyCode::Char('=') if diff.side_by_side => diff.full = !diff.full,
                            KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                diff.wrap = !diff.wrap;
                            }
                            KeyCode::Char('/') => {
                                mode = InputMode::GitDiffSearch {
                                    input: String::new(),
                                };
                            }
                            KeyCode::Char('n') | KeyCode::Char('N') => {
                                if let Some(q) = diff.query.clone() {
                                    let (rows, _) = diff.rows(100);
                                    let lower = q.to_lowercase();
                                    let hits: Vec<usize> = rows
                                        .iter()
                                        .enumerate()
                                        .filter(|(_, r)| r.to_lowercase().contains(&lower))
                                        .map(|(i, _)| i)
                                        .collect();
                                    if !hits.is_empty() {
                                        let cur = hits
                                            .iter()
                                            .position(|p| *p == diff.patch_scroll)
                                            .unwrap_or(0);
                                        let next = if key.code == KeyCode::Char('n') {
                                            (cur + 1) % hits.len()
                                        } else {
                                            cur.checked_sub(1).unwrap_or(hits.len() - 1)
                                        };
                                        diff.patch_scroll = hits[next];
                                    }
                                }
                            }
                            // M71：块级丢弃（先预览确认，丢弃后 u 可撤销）。
                            KeyCode::Char('D') if diff.focus == crate::state::DiffFocus::Patch => {
                                if !state.config.git.hunk_discard {
                                    msg = Some(
                                        "hunk 丢弃为实验性功能，默认关闭（[git] hunk_discard=true 开启）"
                                            .to_owned(),
                                    );
                                } else if let Some(data) = diff.selected() {
                                    let path = data.path.clone();
                                    let preview =
                                        crate::gitdiff::hunk_preview(&data.hunks, diff.hunk);
                                    mode = InputMode::GitDiffConfirm {
                                        op: crate::state::GitOp::DiscardHunk(
                                            path, diff.hunk, preview,
                                        ),
                                    };
                                }
                            }
                            KeyCode::Char('u') => undo = true,
                            // M70：lazygit 兼容写键（仅 diff overlay 内生效）。
                            KeyCode::Char('a') => {
                                mode = InputMode::GitDiffConfirm {
                                    op: crate::state::GitOp::StageAll,
                                };
                            }
                            KeyCode::Char('A') => {
                                let op = crate::state::GitOp::Amend;
                                if state.config.git.confirm_destructive {
                                    mode = InputMode::GitDiffConfirm { op };
                                } else {
                                    pending_op = Some(op);
                                }
                            }
                            KeyCode::Char('d') if diff.focus == crate::state::DiffFocus::Files => {
                                if let Some(path) = diff.selected_path().map(str::to_owned) {
                                    let op = crate::state::GitOp::DiscardFile(path);
                                    if state.config.git.confirm_destructive {
                                        mode = InputMode::GitDiffConfirm { op };
                                    } else {
                                        pending_op = Some(op);
                                    }
                                }
                            }
                            KeyCode::Char('c') => {
                                mode = InputMode::GitCommitInput {
                                    input: String::new(),
                                };
                            }
                            KeyCode::Char('p') => {
                                mode = InputMode::GitDiffConfirm {
                                    op: crate::state::GitOp::Pull,
                                };
                            }
                            KeyCode::Char('P') => {
                                mode = InputMode::GitDiffConfirm {
                                    op: crate::state::GitOp::Push,
                                };
                            }
                            _ => {}
                        }
                    }
                    if undo {
                        if let Some((path, bytes)) = state.git_undo.take() {
                            match std::fs::write(&path, bytes) {
                                Ok(()) => {
                                    msg = Some(format!("已撤销丢弃：{}", path.display()));
                                    if let Some(diff) = state.git_diff.as_mut() {
                                        diff.data.clear();
                                        diff.pending.clear();
                                        diff.generation = diff.generation.wrapping_add(1);
                                    }
                                }
                                Err(e) => msg = Some(format!("撤销失败: {e}")),
                            }
                        } else {
                            msg = Some("没有可撤销的 hunk 丢弃".to_owned());
                        }
                    }
                    if let Some(op) = pending_op {
                        run_git_op(&mut state, &op, &mut msg);
                    }
                    if exit {
                        state.git_diff = None;
                        mode = InputMode::Normal;
                    }
                }
                InputMode::GitDiffSearch {
                    input: search_input,
                } => {
                    if key.kind == KeyEventKind::Press {
                        match command_input(search_input, key) {
                            CmdInput::Submit => {
                                let q = search_input.trim().to_owned();
                                if let Some(diff) = state.git_diff.as_mut() {
                                    diff.query = (!q.is_empty()).then_some(q);
                                    diff.patch_scroll = 0;
                                }
                                mode = InputMode::GitDiff;
                            }
                            CmdInput::Cancel => mode = InputMode::GitDiff,
                            CmdInput::Continue => {
                                if let Some(diff) = state.git_diff.as_mut() {
                                    diff.query =
                                        (!search_input.is_empty()).then(|| search_input.clone());
                                }
                            }
                        }
                    }
                }
                InputMode::GitCommitInput {
                    input: commit_input,
                } => {
                    if key.kind == KeyEventKind::Press {
                        match command_input(commit_input, key) {
                            CmdInput::Submit => {
                                let text = commit_input.trim().to_owned();
                                if !text.is_empty() {
                                    mode = InputMode::GitDiffConfirm {
                                        op: crate::state::GitOp::Commit(text),
                                    };
                                } else {
                                    mode = InputMode::GitDiff;
                                }
                            }
                            CmdInput::Cancel => mode = InputMode::GitDiff,
                            CmdInput::Continue => {}
                        }
                    }
                }
                InputMode::GitDiffConfirm { op } => {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('y') | KeyCode::Enter => {
                                let operation = op.clone();
                                run_git_op(&mut state, &operation, &mut msg);
                                mode = InputMode::GitDiff;
                            }
                            _ => mode = InputMode::GitDiff,
                        }
                    }
                }
                InputMode::TutorResetConfirm {
                    module_id,
                    from_list,
                } => {
                    if key.kind == KeyEventKind::Press {
                        if matches!(
                            key.code,
                            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter
                        ) {
                            let id = module_id.clone();
                            let progress =
                                crate::tutor::reset_module(crate::tutor::load_progress(), &id);
                            state.tutor_progress = progress.clone();
                            if let Err(e) = crate::tutor::save_progress_to(
                                &crate::tutor::tutor_progress_path(),
                                &progress,
                            ) {
                                msg = Some(format!("tutor 重置保存失败: {e}"));
                            }
                            if let Some(module) = crate::tutor::module_by_id(&id) {
                                state.tutor = Some(crate::tutor::TutorSession {
                                    module,
                                    idx: 0,
                                    marks: Vec::new(),
                                    done: false,
                                    review: false,
                                });
                                enter_tutor_sandbox(&mut state);
                            }
                            mode = InputMode::Normal;
                        } else {
                            mode = if *from_list {
                                InputMode::TutorList {
                                    cursor: 0,
                                    search: None,
                                }
                            } else {
                                InputMode::Normal
                            };
                            msg = Some("已取消重置（未进入该模块）".to_owned());
                        }
                    }
                }
                InputMode::TutorList { cursor, search } => {
                    // M42-1：向导模式模块列表——j/k 选择、Enter/l 进入、
                    // Esc/q 关闭（覆盖层独立于当前目录/位置）。
                    if key.kind == KeyEventKind::Press {
                        // M48-10：键位作用于过滤后的模块列表。
                        let visible: Vec<_> = crate::tutor::builtin_modules()
                            .into_iter()
                            .filter(|m| {
                                search.as_ref().is_none_or(|q| {
                                    let q = q.to_lowercase();
                                    m.id.to_lowercase().contains(&q)
                                        || m.name.to_lowercase().contains(&q)
                                })
                            })
                            .collect();
                        if *cursor >= visible.len() && !visible.is_empty() {
                            *cursor = visible.len() - 1;
                        }
                        match tutor_list_key(cursor, visible.len(), key.code) {
                            TutorListOutcome::None => {}
                            TutorListOutcome::Close => mode = InputMode::Normal,
                            TutorListOutcome::Search => {
                                mode = InputMode::TutorListSearch {
                                    input: search.clone().unwrap_or_default(),
                                    cursor: *cursor,
                                };
                            }
                            TutorListOutcome::Reset(i) => {
                                // M48-10：d 执行对应模块 reset 并刷新进度显示。
                                let id = visible[i].id.clone();
                                let progress =
                                    crate::tutor::reset_module(crate::tutor::load_progress(), &id);
                                state.tutor_progress = progress.clone();
                                if let Err(e) = crate::tutor::save_progress_to(
                                    &crate::tutor::tutor_progress_path(),
                                    &progress,
                                ) {
                                    msg = Some(format!("tutor reset 保存失败: {e}"));
                                } else {
                                    msg = Some(format!("tutor: 已重置 {id}"));
                                }
                            }
                            TutorListOutcome::Enter(i) => {
                                // M42-2/M48-10：有记录模块只显示错题条目。
                                let base = visible[i].clone();
                                let progress = crate::tutor::load_progress();
                                if tutor_module_is_complete(&progress, &base) {
                                    // M56-8：已 100% → 先确认重置，取消则不进入。
                                    mode = InputMode::TutorResetConfirm {
                                        module_id: base.id.clone(),
                                        from_list: true,
                                    };
                                } else {
                                    let (module, review) = match crate::tutor::pending_items(
                                        &progress,
                                        &base.id,
                                        &base.items,
                                    ) {
                                        Some(items) => (
                                            crate::tutor::TutorModule {
                                                items,
                                                ..base.clone()
                                            },
                                            true,
                                        ),
                                        None => (base, false),
                                    };
                                    state.tutor = Some(crate::tutor::TutorSession {
                                        module,
                                        idx: 0,
                                        marks: Vec::new(),
                                        done: false,
                                        review,
                                    });
                                    enter_tutor_sandbox(&mut state);
                                    // M45-8a：进入会话必须退出列表覆盖层，否则
                                    // render 先匹配 TutorList 全屏渲染，底部 Panel
                                    // 永不可见（用户实测 l/Enter 后列表不消失）。
                                    mode = InputMode::Normal;
                                }
                            }
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
                                            cursor: 0,
                                            scroll: 0,
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
                                cursor: 0,
                                scroll: 0,
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
            } else if matches!(mode, InputMode::JumpSelect { .. })
                && mouse.kind == MouseEventKind::Down(MouseButton::Left)
            {
                // M45-5b：`o`/`:j`/`:J` 选择器支持鼠标点击选中（此前非
                // Normal 模式鼠标直接落到 handle_mouse 被忽略）。列表段
                // 命中项 → 跳转并关闭，与 Enter 同源；点空白/标题无副作用。
                if let Some(idx) =
                    crate::app::render::jump_click_index(area, mouse.column, mouse.row)
                    && let InputMode::JumpSelect { input, entries, .. } = &mode
                    && let Some(entry) = filter_jump(entries, input).get(idx).cloned()
                {
                    mode = InputMode::Normal;
                    match jump_to(&mut state, &entry) {
                        Ok(m) => msg = Some(m),
                        Err(e) => msg = Some(e),
                    }
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
        let ime_is_editable = ime_editable_mode(&mode);
        if ime_is_editable != ime_was_editable {
            if ime_is_editable {
                ime_worker.enter_editable();
            } else {
                ime_worker.enter_normal();
            }
            ime_was_editable = ime_is_editable;
        }
    } // loop
} // fn run

#[cfg(test)]
mod tests;
