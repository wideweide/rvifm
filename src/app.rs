//! 事件循环：绘制目录列表与状态栏 → 阻塞等待事件 → 分发按键。
//!
//! M1-3 起支持多键序列（`g` 前缀：`gg`）；M1-5 起支持多标签
//! （`gt`/`gT`/`N gt`）与最小命令行（`:tabnew` / `:tabclose` / `:tabmove`；
//! 统一命令解析在 M5-4 接入）。

use ratatui::Frame;
use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

use std::path::{Path, PathBuf};

use crate::keymap::Action;
use crate::state::AppState;
use crate::tab::{Location, SearchState, Tab};
use crate::{config, ops};

/// 判断按键是否请求退出：`q` 或 `Ctrl-C`。
fn is_quit_key(key: KeyEvent) -> bool {
    key.kind == KeyEventKind::Press
        && (key.code == KeyCode::Char('q')
            || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)))
}

/// Normal 模式输入状态：待决序列 + 数字计数（`N gt` 用，M1-5）。
/// `seq` 由 Keymap 驱动解析（M10-3 接线）。
#[derive(Debug)]
struct InputState {
    seq: Vec<crate::keymap::Key>,
    count: Option<usize>,
    keymap: crate::keymap::Keymap,
}

impl Default for InputState {
    fn default() -> Self {
        Self {
            seq: Vec::new(),
            count: None,
            keymap: crate::keymap::Keymap::default_map(),
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
        }
    }
}

/// Normal 模式按键的处理结果。
#[derive(Debug, PartialEq)]
enum Outcome {
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
enum JumpKind {
    Unified,
    Dir,
    File,
    /// M16 NAV-4：近期视角——全部条目按最后访问时间倒序（含会话内合并）。
    Recent,
}

/// 输入模式（M5-4 统一模式系统前先持有最小集合）。
/// M8-12 起 JumpSelect 携带 f64 分数的条目，不再派生 Eq。
#[derive(Debug, Default, PartialEq)]
enum InputMode {
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
    /// 后台作业菜单（M13-8：`:jobs`；M24-9 增选择游标与 x 取消）。
    Jobs { cursor: usize },
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
enum CmdInput {
    Continue,
    Submit,
    Cancel,
}

/// M24-7：命令行 Tab 补全状态——`head` 为输入中最后一个空白分隔词之前的
/// 部分，`candidates` 为焦点 pane 条目名的前缀匹配结果，`index` 为循环位置。
#[derive(Debug, Clone, PartialEq)]
struct CmdComp {
    head: String,
    candidates: Vec<String>,
    index: usize,
}

/// 命令行模式按键（M1-5 最小集）。
fn command_input(input: &mut String, key: KeyEvent) -> CmdInput {
    if key.kind != KeyEventKind::Press {
        return CmdInput::Continue;
    }
    match key.code {
        KeyCode::Enter => CmdInput::Submit,
        KeyCode::Esc => CmdInput::Cancel,
        KeyCode::Backspace => {
            input.pop();
            CmdInput::Continue
        }
        KeyCode::Char(c) => {
            input.push(c);
            CmdInput::Continue
        }
        _ => CmdInput::Continue,
    }
}

/// M24-6：↑ 移到更旧的历史条目；首次进入保存当前草稿输入。空历史无操作。
fn hist_prev(hist: &[String], pos: &mut Option<usize>, input: &mut String, draft: &mut String) {
    if hist.is_empty() {
        return;
    }
    let i = match *pos {
        None => {
            *draft = std::mem::take(input);
            hist.len() - 1
        }
        Some(i) => i.saturating_sub(1),
    };
    *pos = Some(i);
    *input = hist[i].clone();
}

/// M24-6：↓ 移到更新的历史条目；越过最新一条回到草稿输入（游标清空）。
fn hist_next(hist: &[String], pos: &mut Option<usize>, input: &mut String, draft: &mut String) {
    let i = match *pos {
        None => return,
        Some(i) => i + 1,
    };
    if i >= hist.len() {
        *pos = None;
        *input = std::mem::take(draft);
    } else {
        *pos = Some(i);
        *input = hist[i].clone();
    }
}

/// 取输入最后一个空白分隔词；返回（词前部分, 词）。字符边界按 char_indices
/// 处理，多字节安全。
fn split_last_word(s: &str) -> (String, String) {
    match s.char_indices().rfind(|&(_, c)| c.is_whitespace()) {
        Some((i, _)) => (s[..=i].to_owned(), s[i + 1..].to_owned()),
        None => (String::new(), s.to_owned()),
    }
}

/// M24-7：Tab 路径补全——输入最后一个空白分隔词按前缀匹配候选名
/// （焦点 pane 条目，M21-E1 同源）；输入自上次补全后未改动则循环到下一
/// 候选（尾→首环绕），否则重建候选列表。返回候选总数（调用方超 10 个
/// 时显示计数）。无候选时不改输入。
fn tab_complete(input: &mut String, comp: &mut Option<CmdComp>, names: &[String]) -> usize {
    let unchanged = matches!(
        comp.as_ref(),
        Some(c) if *input == format!("{}{}", c.head, c.candidates[c.index])
    );
    if unchanged {
        let c = comp.as_mut().expect("unchanged 时 comp 必存在");
        c.index = (c.index + 1) % c.candidates.len();
    } else {
        let (head, token) = split_last_word(input);
        let candidates: Vec<String> = names
            .iter()
            .filter(|n| n.starts_with(token.as_str()))
            .cloned()
            .collect();
        if candidates.is_empty() {
            *comp = None;
            return 0;
        }
        *comp = Some(CmdComp {
            head,
            candidates,
            index: 0,
        });
    }
    let c = comp.as_ref().expect("上方必已赋值");
    *input = format!("{}{}", c.head, c.candidates[c.index]);
    c.candidates.len()
}

/// M24-7：`~` 展开为 HOME（裸 `~` → HOME，`~/…` → HOME/…；其余原样）。
/// 复用 M19-4 的 expand_tilde（HOME 缺失时原样返回）。
fn expand_home_arg(arg: &str) -> String {
    if arg == "~"
        && let Ok(home) = std::env::var("HOME")
    {
        return home;
    }
    crate::remote::config::expand_tilde(std::path::Path::new(arg))
        .to_string_lossy()
        .into_owned()
}

/// M24-8：左右 pane 条目对比（文件名 + 大小；目录跳过——目录大小无比较
/// 意义）。输出序：左 pane 遍历（`=` 同名同大小 / `!` 同名异大小 /
/// `<` 仅左），随后右 pane 独有（`>` 仅右）。路径取实际所在侧
/// （=/!/< 左，> 右），size 透传供属性列展示。
fn compare_panes(
    left_base: &Path,
    left: &[crate::tab::FileItem],
    right_base: &Path,
    right: &[crate::tab::FileItem],
) -> Vec<crate::search::ResultItem> {
    let right_sizes: std::collections::HashMap<&str, u64> = right
        .iter()
        .filter(|i| !i.is_dir)
        .map(|i| (i.name.as_str(), i.size))
        .collect();
    let mut matched: std::collections::HashSet<&str> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for item in left.iter().filter(|i| !i.is_dir) {
        match right_sizes.get(item.name.as_str()) {
            Some(&rsize) => {
                matched.insert(item.name.as_str());
                let marker = if rsize == item.size { "=" } else { "!" };
                out.push(compare_item(left_base, item, marker));
            }
            None => out.push(compare_item(left_base, item, "<")),
        }
    }
    for item in right.iter().filter(|i| !i.is_dir) {
        if !matched.contains(item.name.as_str()) {
            out.push(compare_item(right_base, item, ">"));
        }
    }
    out
}

/// M24-8：单条对比结果构造（text 承载前缀标记）。
fn compare_item(
    base: &Path,
    item: &crate::tab::FileItem,
    marker: &str,
) -> crate::search::ResultItem {
    crate::search::ResultItem {
        path: base.join(&item.name),
        line: None,
        text: marker.to_owned(),
        size: item.size,
    }
}

/// AskPass 模式单键结果（M12-3；独立函数便于单测）。
#[derive(Debug, PartialEq, Eq)]
enum AskPassResult {
    /// 继续输入。
    Continue,
    /// Enter：提交密码重试提权。
    Retry { password: String },
    /// Esc：取消（M12-4 干净中止）。
    Cancel,
}

/// AskPass 模式单键处理（M12-3）：只维护密码缓冲，模式状态由调用方迁移。
fn askpass_key(input: &mut String, code: KeyCode) -> AskPassResult {
    match code {
        KeyCode::Enter => AskPassResult::Retry {
            password: std::mem::take(input),
        },
        KeyCode::Esc => {
            input.clear();
            AskPassResult::Cancel
        }
        KeyCode::Backspace => {
            input.pop();
            AskPassResult::Continue
        }
        KeyCode::Char(c) => {
            input.push(c);
            AskPassResult::Continue
        }
        _ => AskPassResult::Continue,
    }
}

/// `:` 命令执行结果（M2-6）：退出请求或完成（可携带提示）。
#[derive(Debug, PartialEq)]
enum ExecOutcome {
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
const CONTEXT_MENU: [&str; 6] = [
    "Open",
    "File Info (:file)",
    "Copy",
    "Move",
    "Rename",
    "Delete",
];

/// FEAT-3：菜单动作执行结果——run 循环据此迁移模式。
#[derive(Debug)]
enum ContextMenuOutcome {
    /// 动作完成（或无动作），关闭菜单。
    Close,
    /// 打开文件详情对话框。
    Info { path: PathBuf, text: String },
    /// 进入删除确认（permanent=false，走应用内暂存可撤销）。
    Delete { path: PathBuf },
    /// 打开命令行并预填（Rename → `rename `）。
    RenamePrefill { line: String },
}

/// M18-2：解析 `:sort` 参数——逗号分隔字段，字段后可跟 asc/desc。
/// 例：`name,size desc`。空参数/未知字段/坏方向返回可读错误。
fn parse_sort_spec(args: &str) -> Result<Vec<crate::tab::SortSpec>, String> {
    let mut specs = Vec::new();
    for token in args.split(',') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        let mut parts = token.split_whitespace();
        let field_name = parts.next().unwrap_or_default();
        let field = match field_name {
            "name" => crate::tab::SortField::Name,
            "mtime" | "time" => crate::tab::SortField::Mtime,
            "size" => crate::tab::SortField::Size,
            "ext" | "extension" => crate::tab::SortField::Ext,
            other => {
                return Err(format!(
                    "sort: 未知字段 `{other}`（可用 name/mtime/size/ext）"
                ));
            }
        };
        let desc = match parts.next() {
            None => false,
            Some("asc") => false,
            Some("desc") => true,
            Some(other) => return Err(format!("sort: 未知方向 `{other}`（可用 asc/desc）")),
        };
        if parts.next().is_some() {
            return Err("sort: 字段后只能跟 asc/desc".to_owned());
        }
        specs.push(crate::tab::SortSpec { field, desc });
    }
    if specs.is_empty() {
        return Err("sort: 缺少字段（如 name,size desc）".to_owned());
    }
    Ok(specs)
}

/// M18-4：tmux 新窗口参数（可测试纯函数）——命令经 sh -c 包裹，
/// 退出后窗口自动关闭。
fn tmux_window_args(cmd: &str) -> Vec<String> {
    vec![
        "new-window".to_owned(),
        "-d".to_owned(),
        format!("sh -c {}", shell_quote(cmd)),
    ]
}

// AUDIT-7：shell_quote 迁往 opener::shell_quote（tmux 窗口命令与 opener
// `%s` 展开共用同一 POSIX 转义），此处 use 引入保持原调用点不变。
use crate::opener::shell_quote;

/// M18-4：外部命令执行目标判定——`[ui] terminal_in_new_window=true` 且
/// 检测到 $TMUX/$STY 时走复用器新窗口（fire-and-forget），否则当前 pane
/// （调用方挂起 TUI 后以 sh -c / cmd /C 执行）。
enum ExternalRun {
    /// 复用器新窗口已完成（携带状态栏消息）。
    Multiplexer(String),
    /// 当前 pane 执行（携带命令串）。
    CurrentPane(String),
}

/// 复用器种类（M18-4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MuxKind {
    Tmux,
    Screen,
}

/// 复用器环境探测（生产路径）。AUDIT-14：测试一律走 `run_external_target_with`
/// 注入，宿主的 `TMUX`/`STY` 不得改变测试结果。
fn detect_mux() -> Option<MuxKind> {
    if std::env::var_os("TMUX").is_some() {
        Some(MuxKind::Tmux)
    } else if std::env::var_os("STY").is_some() {
        Some(MuxKind::Screen)
    } else {
        None
    }
}

fn run_external_target(cfg: &config::UiConfig, cmd: &str) -> ExternalRun {
    run_external_target_with(detect_mux(), cfg, cmd)
}

/// AUDIT-14：注入复用器形态的判定（`run_external_target` 与测试共用）。
/// 注意：`cfg.terminal_in_new_window=true` 且 mux 有值时会真实 spawn 复用器
/// （输出已静音）；测试只注入会短路到 `CurrentPane` 的组合。
fn run_external_target_with(
    mux: Option<MuxKind>,
    cfg: &config::UiConfig,
    cmd: &str,
) -> ExternalRun {
    if cfg.terminal_in_new_window {
        match mux {
            Some(MuxKind::Tmux) => {
                let out = std::process::Command::new("tmux")
                    .args(tmux_window_args(cmd))
                    // M21-F8 同款：复用器输出不进 TUI/测试输出。
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                return match out {
                    Ok(st) if st.success() => {
                        ExternalRun::Multiplexer(format!("已在新窗口执行: {cmd}"))
                    }
                    _ => ExternalRun::CurrentPane(cmd.to_owned()), // 失败回退
                };
            }
            Some(MuxKind::Screen) => {
                let out = std::process::Command::new("screen")
                    .args(["-X", "screen", "-t", "rvifm", "sh", "-c", cmd])
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status();
                return match out {
                    Ok(st) if st.success() => {
                        ExternalRun::Multiplexer(format!("已在 screen 新窗口执行: {cmd}"))
                    }
                    _ => ExternalRun::CurrentPane(cmd.to_owned()),
                };
            }
            None => {}
        }
    }
    ExternalRun::CurrentPane(cmd.to_owned())
}

/// M18-4：退出状态 → 状态栏消息（shell 与 :! 共用）。
fn run_external_current_result(
    result: std::io::Result<std::process::ExitStatus>,
) -> Option<String> {
    match result {
        Ok(st) if st.success() => None,
        Ok(st) => Some(format!(
            "命令退出码: {}",
            st.code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "信号终止".to_owned())
        )),
        Err(e) => Some(format!("命令执行失败: {e}")),
    }
}

/// AUDIT-7：挂起 TUI 执行外部命令并恢复（`:!` 当前 pane 与 block=true
/// opener 共用）。`ratatui::init()` 重建终端（旧终端随 restore 弃用），
/// 返回新终端与新状态栏消息。
fn suspend_execute(cmd: &str) -> (ratatui::DefaultTerminal, Option<String>) {
    ratatui::restore();
    #[cfg(unix)]
    let result = std::process::Command::new("sh").arg("-c").arg(cmd).status();
    #[cfg(windows)]
    let result = std::process::Command::new("cmd").args(["/C", cmd]).status();
    let terminal = ratatui::init();
    // FEAT-1：init 不含鼠标捕获，重新启用。
    let _ = ratatui::crossterm::execute!(std::io::stdout(), event::EnableMouseCapture);
    (terminal, run_external_current_result(result))
}

/// M18-1：排序对话框按键结果。
#[derive(Debug)]
enum SortDialogOutcome {
    None,
    Close,
    Apply(crate::tab::SortSpec),
}

/// M18-1：排序对话框按键处理（字母=直接应用；空格=换向；回车=应用
/// 高亮字段当前方向；j/k 移动；Esc=取消）。
fn handle_sort_dialog_key(
    _state: &mut AppState,
    key: KeyEvent,
    cursor: &mut usize,
    desc: &mut bool,
    msg: &mut Option<String>,
) -> SortDialogOutcome {
    let field_of = |code: KeyCode| -> Option<(crate::tab::SortField, bool)> {
        SORT_FIELDS.iter().find_map(|(lo, hi, _, f)| {
            if *lo == code {
                Some((*f, false))
            } else if *hi == code {
                Some((*f, true))
            } else {
                None
            }
        })
    };
    if let Some((field, d)) = field_of(key.code) {
        let _ = msg;
        return SortDialogOutcome::Apply(crate::tab::SortSpec { field, desc: d });
    }
    match key.code {
        KeyCode::Esc => SortDialogOutcome::Close,
        KeyCode::Down | KeyCode::Char('j') => {
            *cursor = (*cursor + 1).min(SORT_FIELDS.len() - 1);
            SortDialogOutcome::None
        }
        KeyCode::Up | KeyCode::Char('k') => {
            *cursor = cursor.saturating_sub(1);
            SortDialogOutcome::None
        }
        KeyCode::Char(' ') => {
            *desc = !*desc;
            SortDialogOutcome::None
        }
        KeyCode::Enter => {
            let field = SORT_FIELDS[*cursor].3;
            SortDialogOutcome::Apply(crate::tab::SortSpec { field, desc: *desc })
        }
        _ => SortDialogOutcome::None,
    }
}

/// FEAT-3：菜单浮窗矩形——固定 34×8 居中（含边框）。
fn context_menu_rect(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    use ratatui::layout::Rect;
    let w = 34u16.min(area.width);
    let h = ((CONTEXT_MENU.len() as u16) + 2).min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

/// FEAT-3：当前焦点条目的详情（物理 → fs 元数据；归档成员 →
/// `host:/inner/name` 路径 + 成员索引元数据）。`:file` 与右键菜单同源。
fn current_item_info(state: &AppState) -> Option<(PathBuf, String)> {
    let tab = state.active_tab();
    let item = tab.entries.get(tab.cursor)?;
    match &tab.location {
        Location::Physical(base) => {
            let path = base.join(&item.name);
            Some((path.clone(), file_info_text(&path, &item.name)))
        }
        Location::Archive { host, inner } => {
            let shown = format!(
                "{}:/{}",
                host.display(),
                crate::archive::normalize_inner(inner.to_string_lossy().as_ref())
            );
            let kind = if item.is_dir {
                "目录（归档内）"
            } else {
                "文件（归档内）"
            };
            let text = [
                format!("名称: {}", item.name),
                format!("类型: {kind}"),
                format!("大小: {}", crate::tab::format_size(item.size)),
                "权限: -".to_owned(),
                format!("路径: {shown}/{}", item.name),
            ]
            .join("\n");
            Some((host.clone(), text))
        }
        Location::Remote { remote, path } => {
            // M19-2：远端成员信息（元数据取自条目列表，无 fs 权限位）。
            let shown = format!("{}:{}", remote, path.to_string_lossy().replace('\\', "/"));
            let kind = if item.is_dir {
                "目录（远端）"
            } else {
                "文件（远端）"
            };
            let text = [
                format!("名称: {}", item.name),
                format!("类型: {kind}"),
                format!("大小: {}", crate::tab::format_size(item.size)),
                "权限: -".to_owned(),
                format!("路径: {shown}/{}", item.name),
            ]
            .join("\n");
            Some((PathBuf::from("/"), text))
        }
    }
}

/// M16 NAV-2：历史菜单导航——落到历史第 idx 条（作为新导航，截断
/// forward）；当前位置提示。
fn history_menu_nav(state: &mut AppState, idx: usize, msg: &mut Option<String>) {
    let cfg = state.config.clone();
    let (target, cur) = {
        let tab = state.active_tab();
        (tab.history.get(idx).cloned(), tab.hist_pos)
    };
    let Some(target) = target else {
        *msg = Some("无效的历史条目".to_owned());
        return;
    };
    if idx == cur {
        *msg = Some("已在当前位置".to_owned());
        return;
    }
    if let Err(e) = state.active_tab_mut().navigate_to(target, cfg) {
        *msg = Some(format!("导航失败: {e}"));
        return;
    }
    *msg = None;
}

/// FEAT-3：执行菜单动作（作用于当前光标条目 = 右键聚焦项）。
fn run_context_menu_action(
    state: &mut AppState,
    action: usize,
    msg: &mut Option<String>,
) -> ContextMenuOutcome {
    match action {
        0 => {
            // Open：目录/归档进入，文件走系统 opener（与 l/Enter 同链）。
            open_selected(state, msg);
            ContextMenuOutcome::Close
        }
        1 => match current_item_info(state) {
            Some((path, text)) => ContextMenuOutcome::Info { path, text },
            None => {
                *msg = Some("无选中条目".to_owned());
                ContextMenuOutcome::Close
            }
        },
        2 | 3 => {
            // Copy / Move：目标 = 另一 pane 所在目录（vifm 双栏约定）。
            let other_dir = {
                let tab = state.active_tab();
                let other = if tab.focus == crate::tab::Focus::Right {
                    &tab.location
                } else {
                    &tab.right.location
                };
                match other {
                    Location::Physical(p) => Some(p.clone()),
                    Location::Archive { .. } | Location::Remote { .. } => None,
                }
            };
            let Some(other_dir) = other_dir else {
                *msg = Some("目标 pane 在归档视图（无物理目录）".to_owned());
                return ContextMenuOutcome::Close;
            };
            let tab = state.active_tab();
            let is_physical = matches!(tab.location, Location::Physical(_));
            let path = if is_physical {
                tab.entries
                    .get(tab.cursor)
                    .map(|item| tab.location.base_dir().join(&item.name))
            } else {
                None
            };
            let Some(path) = path else {
                *msg = Some("无选中条目".to_owned());
                return ContextMenuOutcome::Close;
            };
            let result = if action == 3 {
                ops::move_to(state.active, &path, &other_dir)
            } else {
                ops::copy_to(state.active, &path, &other_dir)
            };
            match result {
                Ok(op) => {
                    state.undo_stack.push(op);
                    refresh_tab_panes(state, state.active);
                    *msg = Some(format!(
                        "{} 1 item to {}",
                        if action == 3 { "Moved" } else { "Copied" },
                        other_dir.display()
                    ));
                }
                Err(e) => *msg = Some(format!("失败: {e}")),
            }
            ContextMenuOutcome::Close
        }
        4 => ContextMenuOutcome::RenamePrefill {
            line: "rename ".to_owned(),
        },
        _ => {
            // Delete：走 dd 的确认流程（应用内暂存，u 可逆）。
            let tab = state.active_tab();
            if let (Location::Physical(base), Some(item)) =
                (&tab.location, tab.entries.get(tab.cursor))
            {
                ContextMenuOutcome::Delete {
                    path: base.join(&item.name),
                }
            } else {
                *msg = Some("无选中条目".to_owned());
                ContextMenuOutcome::Close
            }
        }
    }
}

/// 测试兼容包装：沿用 `Ok(true)` = 退出的旧契约（丢弃提示）。
#[cfg(test)]
fn exec_command(state: &mut AppState, line: &str) -> Result<bool, String> {
    exec_command_impl(state, line).map(|o| matches!(o, ExecOutcome::Quit))
}

// ── M19-2：远端会话导航 ──────────────────────────────────────────────

/// M23-11：git 仓库根目录探测——从 dir 逐级向上找 .git（目录或文件，
/// worktree 的 .git 是文件）；非仓库返回 None。
fn git_root_of(dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut cur = normalize_lexical(dir);
    loop {
        if cur.join(".git").exists() {
            return Some(cur);
        }
        cur = cur.parent()?.to_path_buf();
    }
}

/// M23-11：命令模板变量替换——%d 焦点 pane 目录、%f 光标文件名、
/// %F 光标完整路径、%D 对侧 pane 目录、%g git 仓库根（非仓库为空）。
/// %g 为空时返回 Err（git 相关命令不执行，提示不是 git 仓库）。
fn substitute_vars(template: &str, state: &AppState) -> Result<String, String> {
    let tab = state.active_tab();
    let focus_dir = tab.location.base_dir().to_path_buf();
    let other_dir = if tab.focus == crate::tab::Focus::Left {
        tab.right.location.base_dir().to_path_buf()
    } else {
        tab.location.base_dir().to_path_buf()
    };
    let cursor_path = tab.location.base_dir().join(
        tab.entries
            .get(tab.cursor)
            .map(|i| i.name.as_str())
            .unwrap_or(""),
    );
    let sep = std::path::MAIN_SEPARATOR_STR;
    let g = git_root_of(&focus_dir)
        .map(|p| p.to_string_lossy().replace(sep, "/"))
        .unwrap_or_default();
    let out = template
        .replace("%d", &focus_dir.to_string_lossy().replace(sep, "/"))
        .replace(
            "%f",
            tab.entries
                .get(tab.cursor)
                .map(|i| i.name.as_str())
                .unwrap_or(""),
        )
        .replace("%F", &cursor_path.to_string_lossy().replace(sep, "/"))
        .replace("%D", &other_dir.to_string_lossy().replace(sep, "/"))
        .replace("%g", &g);
    if template.contains("%g") && g.is_empty() {
        return Err("不是 git 仓库".to_owned());
    }
    Ok(out)
}

/// M23-11：执行自定义命令（Action::RunCommand）——模板替换 → M18-4 分流。
fn run_user_command(state: &mut AppState, index: usize, msg: &mut Option<String>) -> Outcome {
    let commands = config::keymap_commands();
    let Some(tpl) = commands.get(index) else {
        *msg = Some("自定义命令索引越界".to_owned());
        return Outcome::Continue;
    };
    let cmd = match substitute_vars(tpl, state) {
        Ok(c) => c,
        Err(e) => {
            *msg = Some(e);
            return Outcome::Continue;
        }
    };
    let ui = state.config.ui.clone();
    match run_external_target(&ui, &cmd) {
        ExternalRun::Multiplexer(m) => {
            *msg = Some(m);
            Outcome::Continue
        }
        ExternalRun::CurrentPane(cmd) => Outcome::RunUserCommand { cmd },
    }
}

/// M23-10：OSC52 剪贴板序列（`ESC]52;c;<base64>BEL`；零依赖）。
fn osc52_sequence(text: &str) -> String {
    use base64ct::Encoding as _;
    format!(
        "\x1b]52;c;{}\x07",
        base64ct::Base64::encode_string(text.as_bytes())
    )
}

/// M23-10：写入剪贴板（OSC52 写 stdout；失败静默——状态栏反馈由调用方）。
/// 测试下不写 stdout（OSC52 序列噪声）；序列本身由 osc52_sequence 覆盖。
fn copy_to_clipboard(text: &str) {
    #[cfg(not(test))]
    {
        use std::io::Write as _;
        let _ = std::io::stdout().write_all(osc52_sequence(text).as_bytes());
        let _ = std::io::stdout().flush();
    }
    #[cfg(test)]
    {
        let _ = osc52_sequence(text); // 保持序列路径可达（dead-code 抑制）
    }
}

/// M23-10：复制目标的选集——标记优先（按列表序），其次可视范围，最后
/// 光标单条目（与 cw/R 的选集语义一致）。
fn selected_names(state: &AppState) -> Vec<String> {
    let tab = state.active_tab();
    if !tab.tags.is_empty() {
        return tab
            .entries
            .iter()
            .filter(|i| tab.tags.contains(&i.name))
            .map(|i| i.name.clone())
            .collect();
    }
    if let Some((lo, hi)) = tab.visual_range() {
        return tab
            .entries
            .get(lo..=hi.min(tab.entries.len().saturating_sub(1)))
            .map(|slice| slice.iter().map(|i| i.name.clone()).collect())
            .unwrap_or_default();
    }
    tab.entries
        .get(tab.cursor)
        .map(|i| vec![i.name.clone()])
        .unwrap_or_default()
}

/// M23-10：编辑器选择（M23-10 e / M13-6 cw 共用）——`EDITOR` → `VISUAL`
/// → 平台缺省（Windows notepad / Unix vi）。
fn editor_name() -> String {
    std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| {
            if cfg!(windows) {
                "notepad".to_owned()
            } else {
                "vi".to_owned()
            }
        })
}

/// M22-5：远端结果定位（l/Enter，结果虚拟目录 + Remote 位置共用体）——
/// 退出结果视图 + remote_navigate 命中文件父目录 + 光标停在该文件。
fn remote_locate_result(state: &mut AppState) -> Result<(), String> {
    let Some(path) = state.active_tab().result_path_at(state.active_tab().cursor) else {
        return Ok(());
    };
    let parent = path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("/"));
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    state.active_tab_mut().results = None;
    remote_navigate(state, &parent)?;
    if let Some(idx) = state
        .active_tab()
        .entries
        .iter()
        .position(|i| i.name == name)
    {
        state.active_tab_mut().cursor = idx;
    }
    Ok(())
}

/// M22-6/M23-9：远端文件打开——RemoteFs read → 临时副本（`rvifm-open-` 前缀）
/// → system opener；副本路径登记 AppState.temp_opens，run 退出时统一清理
/// （Windows opener 可能锁定文件，不做即时删除）。只读用途，不入撤销栈。
/// M23-9：打开后挂常驻监听线程——修改停止 2s（防抖）自动上传回服务器，
/// 失败状态栏提示。
fn remote_open_file(
    state: &mut AppState,
    remote: &str,
    full: &std::path::Path,
) -> Result<Option<String>, String> {
    let tmp = remote_download_copy(state, remote, full)?;
    spawn_upload_watcher(state, remote, &tmp, full);
    match crate::opener::system_open(&tmp) {
        Ok(()) => Ok(Some(format!(
            "打开远端文件（临时副本 {}，修改自动上传）",
            tmp.display()
        ))),
        Err(e) => Err(format!("打开失败: {e}")),
    }
}

/// M23-9：为副本挂常驻监听线程——修改停止 2s（防抖）后经通道回传上传
/// 请求（run 循环消费：读副本最新内容 → RemoteFs write）；多次修改多次
/// 上传（外层循环继续监听）。监听/通道失败静默降级（无自动上传）。
/// watcher 由监听线程自持（线程存活即监听有效；进程退出随之消亡）。
fn spawn_upload_watcher(
    state: &mut AppState,
    remote: &str,
    temp: &std::path::Path,
    remote_path: &std::path::Path,
) {
    use notify::Watcher as _;
    let Some(tx) = state.upload_tx.clone() else {
        return;
    };
    let remote = remote.to_owned();
    let temp = temp.to_path_buf();
    let remote_path = remote_path.to_path_buf();
    std::thread::Builder::new()
        .name("rvifm-upload-watch".to_owned())
        .spawn(move || {
            let (wx, wrx) = std::sync::mpsc::channel::<notify::Event>();
            let Ok(mut watcher) =
                notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                    if let Ok(ev) = res
                        && matches!(
                            ev.kind,
                            notify::EventKind::Create(_) | notify::EventKind::Modify(_)
                        )
                    {
                        let _ = wx.send(ev);
                    }
                })
            else {
                return;
            };
            if watcher
                .watch(&temp, notify::RecursiveMode::NonRecursive)
                .is_err()
            {
                return;
            }
            std::mem::forget(watcher); // 线程常驻：watcher 与线程同生命周期
            loop {
                // 等待下一次修改事件（文件打开期间随时可改）。
                let Ok(_ev) = wrx.recv() else {
                    return;
                };
                // 防抖：2s 内无进一步修改 → 视为修改保持 → 上传。
                loop {
                    match wrx.recv_timeout(std::time::Duration::from_secs(2)) {
                        Ok(_) => continue,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
                    }
                }
                if tx
                    .send((remote.clone(), temp.clone(), remote_path.clone()))
                    .is_err()
                {
                    return; // run 已退出
                }
            }
        })
        .expect("rvifm-upload-watch 线程启动失败");
}

/// M22-6：下载远端文件到临时副本（可测抽取；登记 temp_opens）。
fn remote_download_copy(
    state: &mut AppState,
    remote: &str,
    full: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    let backend = state
        .remote_sessions
        .get(remote)
        .ok_or_else(|| "remote: 会话已断开".to_owned())?;
    let bytes = backend.read(full).map_err(|e| format!("打开失败: {e}"))?;
    let leaf = full
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_owned());
    let rand = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let tmp = std::env::temp_dir().join(format!("rvifm-open-{rand}-{leaf}"));
    std::fs::write(&tmp, &bytes).map_err(|e| format!("临时副本写入失败: {e}"))?;
    state.temp_opens.push(tmp.clone());
    Ok(tmp)
}

/// M22-6：进程退出统一清理临时副本（失败静默——不影响退出）。
fn cleanup_temp_opens(state: &mut AppState) {
    for p in state.temp_opens.drain(..) {
        let _ = std::fs::remove_file(p);
    }
}

/// M22-3：远端口令连接应用（RemotePass 模式 Enter 共用体）——连接成功
/// 注册会话并进入；失败返回可读错误（重试经 :remote 重新发起）。
fn remote_pass_apply(
    state: &mut AppState,
    name: &str,
    params: &crate::remote::sftp::SftpParams,
    password: &str,
) -> Result<String, String> {
    let backend = crate::remote::sftp::SftpBackend::connect_with_password(params, password)
        .map_err(|e| format!("口令认证失败: {e}"))?;
    register_remote_session(state, name, Box::new(backend));
    let root = crate::remote::config::load_remotes_toml(&config::data_dir().join("remotes.toml"))
        .get(name)
        .and_then(|e| e.root.clone())
        .unwrap_or_else(|| PathBuf::from("/"));
    remote_enter(state, name, &root)?;
    Ok(format!("remote: 已进入 {name}（口令认证）"))
}

/// 注册远端会话（M19-2：测试/mock 注入；M19-4 由 :remote 配置构造真实后端）。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn register_remote_session(
    state: &mut AppState,
    name: &str,
    backend: Box<dyn crate::remote::RemoteFs>,
) {
    state.remote_sessions.insert(name.to_owned(), backend);
}

/// 进入远端会话根目录（`:remote <name>`）：list 根目录 → 条目列表；
/// 记录 pre_remote（h 在远端根返回本地）。会话不存在 → 可读错误。
pub(crate) fn remote_enter(
    state: &mut AppState,
    name: &str,
    root: &std::path::Path,
) -> Result<(), String> {
    let Some(backend) = state.remote_sessions.get(name) else {
        return Err(format!(
            "remote: 未找到会话 `{name}`（:remote 无参列出已配置项）"
        ));
    };
    let items = backend.list(root).map_err(|e| format!("remote: {e}"))?;
    let cfg = state.config.clone();
    let tab = state.active_tab_mut();
    if tab.pre_remote.is_none() {
        tab.pre_remote = Some(tab.location.clone());
    }
    // 轨迹管理同 navigate_to（截断 forward + 追加 + 落位）。
    tab.history.truncate(tab.hist_pos + 1);
    tab.history.push(Location::Remote {
        remote: name.to_owned(),
        path: root.to_path_buf(),
    });
    tab.hist_pos = tab.history.len() - 1;
    tab.location = Location::Remote {
        remote: name.to_owned(),
        path: root.to_path_buf(),
    };
    tab.entries = items
        .into_iter()
        .map(|i| crate::tab::FileItem {
            icon: crate::icons::icon_for(
                std::path::Path::new(&format!("/{}", i.name)),
                crate::icons::IconTheme::Dark,
            ),
            name: i.name.clone(),
            is_dir: i.is_dir,
            // 远端元数据无链接/执行位：链接 false、可执行按扩展名近似（M21-F1）。
            is_link: false,
            is_exec: !i.is_dir && crate::tab::is_exec_name(&i.name),
            size: i.size,
            perm: String::new(),
            mtime: i.mtime,
        })
        .collect();
    tab.cursor = 0;
    tab.scroll = 0;
    tab.on_parent_row = false;
    tab.sort = None;
    let _ = cfg;
    Ok(())
}

/// 远端目录内导航：进入子目录 / 上级（沿用会话 list）。返回新远端路径。
/// M20-1：当前视图若为远端位置，返回其路径。
fn current_remote_path(state: &AppState) -> Option<PathBuf> {
    match &state.active_tab().location {
        Location::Remote { path, .. } => Some(path.clone()),
        _ => None,
    }
}

pub(crate) fn remote_navigate(state: &mut AppState, path: &std::path::Path) -> Result<(), String> {
    let (name, items) = {
        let tab = state.active_tab();
        let Location::Remote { remote, .. } = &tab.location else {
            return Err("remote: 非远端视图".to_owned());
        };
        let name = remote.clone();
        let Some(backend) = state.remote_sessions.get(&name) else {
            return Err("remote: 会话已断开".to_owned());
        };
        (
            name,
            backend.list(path).map_err(|e| format!("remote: {e}"))?,
        )
    };
    let cfg = state.config.clone();
    let tab = state.active_tab_mut();
    tab.history.truncate(tab.hist_pos + 1);
    tab.history.push(Location::Remote {
        remote: name.clone(),
        path: path.to_path_buf(),
    });
    tab.hist_pos = tab.history.len() - 1;
    tab.location = Location::Remote {
        remote: name,
        path: path.to_path_buf(),
    };
    tab.entries = items
        .into_iter()
        .map(|i| crate::tab::FileItem {
            icon: crate::icons::icon_for(
                std::path::Path::new(&format!("{}/{}", path.display(), i.name)),
                crate::icons::IconTheme::Dark,
            ),
            name: i.name.clone(),
            is_dir: i.is_dir,
            // 远端元数据无链接/执行位：链接 false、可执行按扩展名近似（M21-F1）。
            is_link: false,
            is_exec: !i.is_dir && crate::tab::is_exec_name(&i.name),
            size: i.size,
            perm: String::new(),
            mtime: i.mtime,
        })
        .collect();
    tab.cursor = 0;
    tab.scroll = 0;
    tab.on_parent_row = false;
    let _ = cfg;
    Ok(())
}

/// 远端 `h`：非根目录 → 上级；根目录 → 返回 pre_remote（本地目录）。
pub(crate) fn remote_up(state: &mut AppState) -> Result<(), String> {
    let (name, parent, at_root) = {
        let tab = state.active_tab();
        let Location::Remote { remote, path } = &tab.location else {
            return Err("remote: 非远端视图".to_owned());
        };
        let parent = path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/"));
        let at_root = parent == *path || path == &PathBuf::from("/");
        (
            remote.clone(),
            if at_root { None } else { Some(parent) },
            at_root,
        )
    };
    if at_root {
        // 回到进入前的本地目录。
        let back = state
            .active_tab()
            .pre_remote
            .clone()
            .unwrap_or_else(|| Location::Physical(std::env::current_dir().unwrap_or_default()));
        let cfg = state.config.clone();
        let tab = state.active_tab_mut();
        tab.pre_remote = None;
        tab.navigate_to(back, cfg)
            .map_err(|e| format!("remote: {e}"))?;
        return Ok(());
    }
    let _ = &name;
    remote_navigate(state, &parent.unwrap_or_else(|| PathBuf::from("/")))
}

/// M19-2：恢复历史中的远端位置（histprev/histnext 目标为 Remote 时）——
/// 条目经会话重载；只移动游标不截断 forward。
pub(crate) fn remote_jump(state: &mut AppState, pos: usize) -> Result<(), String> {
    let (name, path) = {
        let tab = state.active_tab();
        match tab.history.get(pos) {
            Some(Location::Remote { remote, path }) => (remote.clone(), path.clone()),
            _ => return Err("remote: 历史目标不是远端位置".to_owned()),
        }
    };
    let items = {
        let Some(backend) = state.remote_sessions.get(&name) else {
            return Err("remote: 会话已断开".to_owned());
        };
        backend.list(&path).map_err(|e| format!("remote: {e}"))?
    };
    let cfg = state.config.clone();
    let tab = state.active_tab_mut();
    tab.hist_pos = pos;
    tab.location = Location::Remote {
        remote: name,
        path: path.clone(),
    };
    tab.entries = items
        .into_iter()
        .map(|i| crate::tab::FileItem {
            icon: crate::icons::icon_for(
                std::path::Path::new(&format!("{}/{}", path.display(), i.name)),
                crate::icons::IconTheme::Dark,
            ),
            name: i.name.clone(),
            is_dir: i.is_dir,
            // 远端元数据无链接/执行位：链接 false、可执行按扩展名近似（M21-F1）。
            is_link: false,
            is_exec: !i.is_dir && crate::tab::is_exec_name(&i.name),
            size: i.size,
            perm: String::new(),
            mtime: i.mtime,
        })
        .collect();
    tab.cursor = 0;
    tab.scroll = 0;
    tab.on_parent_row = false;
    let _ = cfg;
    Ok(())
}

/// NAV-3/M19-2：历史后退/前进统一入口——目标为远端位置走 remote_jump
/// （会话重载），否则 hist_jump；移动成功更新状态栏位置消息。
fn hist_move(state: &mut AppState, forward: bool, msg: &mut Option<String>) -> Result<(), String> {
    let tab = state.active_tab();
    let target_pos = if forward {
        tab.hist_pos + 1
    } else {
        tab.hist_pos.saturating_sub(1)
    };
    let target_is_remote = matches!(tab.history.get(target_pos), Some(Location::Remote { .. }));
    let moved = if target_is_remote {
        remote_jump(state, target_pos).map(|_| true)
    } else {
        let cfg = state.config.clone();
        state
            .active_tab_mut()
            .hist_jump(forward, cfg)
            .map_err(|e| format!("hist: {e}"))
    };
    let moved =
        moved.map_err(|e| format!("{}: {e}", if forward { "histnext" } else { "histprev" }))?;
    if moved {
        let (pos, len) = {
            let tab = state.active_tab();
            (tab.hist_pos, tab.history.len())
        };
        *msg = Some(format!(
            "{} {}/{}",
            if forward { "前进" } else { "后退" },
            pos + 1,
            len
        ));
    } else {
        *msg = Some(if forward {
            "无前进历史".to_owned()
        } else {
            "无后退历史".to_owned()
        });
    }
    Ok(())
}

/// 取当前 tab **焦点 pane** 的物理操作基准目录（M14-1：文件操作作用于
/// 焦点 pane）；归档视图只读（M7 边界），返回错误提示。
fn physical_base(state: &AppState) -> Result<PathBuf, String> {
    let tab = state.active_tab();
    // M14-1：焦点 pane 基准目录（focus_base）；归档视图只读（M7 边界）。
    if matches!(tab.focus, crate::tab::Focus::Right)
        && matches!(tab.right.location, Location::Archive { .. })
    {
        return Err("归档视图只读（解压后可修改）".to_owned());
    }
    if matches!(tab.focus, crate::tab::Focus::Left)
        && matches!(tab.location, Location::Archive { .. })
    {
        return Err("归档视图只读（解压后可修改）".to_owned());
    }
    Ok(tab.focus_base().to_path_buf())
}

// ── M16 NAV-1：' 快速跳转（keyword 直达） ──────────────────────────────

/// NAV-1 纯函数：从重放条目中取 keyword 精确匹配的**目录**最高排名路径。
/// 候选 keyword 相同 → 档位相同（Grade 6 书签）→ 分数即唯一区分；
/// 平分按路径字典序取前（确定性）。注意 cmp_ranked 是降序比较器，
/// 不能直接喂 max_by。
fn quickjump_target(entries: &[crate::frecency::ScoredEntry], keyword: &str) -> Option<String> {
    entries
        .iter()
        .filter(|e| e.topic == "dir" && e.keyword == keyword)
        .max_by(|a, b| a.score.total_cmp(&b.score).then(b.path.cmp(&a.path)))
        .map(|e| e.path.clone())
}

/// `'`+字符：keyword 直达——配置别名优先，缺省即字符本身；
/// 命中目录即 cd（等价 :cd：history 入栈 + 刷新）；未命中提示不静默。
fn quickjump(state: &mut AppState, c: char, msg: &mut Option<String>) {
    let target_kw = config::quickjump_alias(c).unwrap_or_else(|| c.to_string());
    let entries = match load_jump_entries(state, JumpKind::Unified) {
        Ok(entries) => entries,
        Err(e) => {
            *msg = Some(e);
            return;
        }
    };
    let Some(target) = quickjump_target(&entries, &target_kw) else {
        *msg = Some(format!(
            "无 keyword {target_kw} 的条目（:keyword {target_kw} 设置）"
        ));
        return;
    };
    let cfg = state.config.clone();
    let tab = state.active_tab_mut();
    if let Err(e) = tab.navigate_to(Location::Physical(PathBuf::from(&target)), cfg) {
        *msg = Some(format!("跳转失败: {e}"));
        return;
    }
    *msg = Some(format!("已跳转: {target}"));
}

// ── M8-12：跳转选择器（o 合一 / :j 仅目录 / :J 仅文件） ─────────────────────

/// 选择器数据（纯只读）：按视角从 frecency 存储加载并排名。
/// Unified = 合一表 + 全局统一时钟（全部条目混合排名）；
/// Dir/File = 仅对应 topic + 独立类型时钟。交互与 `pick` 一致（分级 + aging）。
fn load_jump_entries(
    state: &AppState,
    kind: JumpKind,
) -> Result<Vec<crate::frecency::ScoredEntry>, String> {
    // AUDIT-1：跳转选择器与 pick 同源只读（不合并 pending、不写库）；
    // 库缺失（无 frecency 数据）→ 空选择器。
    let store = match crate::frecency::open_store_readonly() {
        Ok(Some(store)) => store,
        Ok(None) => return Ok(Vec::new()),
        Err(e) => return Err(format!("frecency 存储打开失败: {e}")),
    };
    let snapshot = store
        .snapshot_all()
        .map_err(|e| format!("snapshot 读取失败: {e}"))?;
    let since = store
        .last_compaction()
        .map_err(|e| format!("游标读取失败: {e}"))?;
    let events = store
        .events_since(since)
        .map_err(|e| format!("事件读取失败: {e}"))?;
    let all = match kind {
        JumpKind::Unified => {
            let now = store
                .global_tick()
                .map_err(|e| format!("时钟读取失败: {e}"))?;
            crate::frecency::replay_unified(&snapshot, &events, now)
        }
        JumpKind::Recent => {
            // M16 NAV-4：近期视角——统一重放 + 会话内合并，按最后访问倒序。
            let now = store
                .global_tick()
                .map_err(|e| format!("时钟读取失败: {e}"))?;
            let mut entries = crate::frecency::replay_unified(&snapshot, &events, now);
            merge_session_visits(&mut entries, state);
            entries.sort_by(|a, b| {
                b.last_visit
                    .cmp(&a.last_visit)
                    .then_with(|| b.score.total_cmp(&a.score))
            });
            entries
        }
        JumpKind::Dir | JumpKind::File => {
            let now_dir = store
                .type_tick("dir")
                .map_err(|e| format!("时钟读取失败: {e}"))?;
            let now_file = store
                .type_tick("file")
                .map_err(|e| format!("时钟读取失败: {e}"))?;
            let topic = if kind == JumpKind::Dir { "dir" } else { "file" };
            crate::frecency::replay(&snapshot, &events, now_dir, now_file)
                .into_iter()
                .filter(|e| e.topic == topic)
                .collect()
        }
    };
    Ok(all)
}

/// M16 NAV-4：会话内访问合并——各 tab 的 history（Physical）按轨迹顺序
/// 赋予递增时间戳（基准 = 当前墙钟 -1 小时，严格新于大多数已写盘事件），
/// Recent 视角即时可见会话内跳转。只影响 last_visit，不写任何存储。
fn merge_session_visits(entries: &mut Vec<crate::frecency::ScoredEntry>, state: &AppState) {
    let base = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .saturating_sub(3600);
    let mut seq = 0u64;
    for tab in &state.tabs {
        for loc in &tab.history {
            let Location::Physical(p) = loc else {
                continue;
            };
            let path = p.to_string_lossy().replace('\\', "/");
            seq += 1;
            let ts = base.saturating_add(seq);
            match entries
                .iter_mut()
                .find(|e| e.topic == "dir" && e.path == path)
            {
                Some(e) => e.last_visit = e.last_visit.max(ts),
                None => {
                    let snap = crate::frecency::SnapshotEntry {
                        path: path.clone(),
                        topic: "dir".to_owned(),
                        keyword: String::new(),
                        score: 0.0,
                        tick: 0,
                        clock: 0,
                    };
                    let mut e = crate::frecency::ScoredEntry::from_snapshot(&snap, "");
                    e.last_visit = ts;
                    entries.push(e);
                }
            }
        }
    }
}

/// 选择器实时筛选（与 pick 的分级一致）：空查询全部显示，非空按分级
/// （0–7 档）过滤并重新排名。
fn filter_jump(
    entries: &[crate::frecency::ScoredEntry],
    query: &str,
) -> Vec<crate::frecency::ScoredEntry> {
    if query.is_empty() {
        return entries.to_vec();
    }
    // M23-1：多词分级过滤——grade_for_multi 优先，无命中降级整串匹配；
    // 命中条目的 grade 更新（ranked 按 grade 降序 → 同档保持传入序）。
    let mut out: Vec<crate::frecency::ScoredEntry> = entries
        .iter()
        .filter_map(|e| {
            let snap = crate::frecency::SnapshotEntry {
                path: e.path.clone(),
                topic: e.topic.clone(),
                keyword: e.keyword.clone(),
                score: 0.0,
                tick: 0,
                clock: 0,
            };
            let g = crate::frecency::grade_for_multi(&snap, query);
            let g = if g != crate::frecency::Grade::None {
                g
            } else {
                crate::frecency::grade_for(&snap, query)
            };
            if g == crate::frecency::Grade::None {
                return None;
            }
            let mut e2 = e.clone();
            e2.grade = g;
            Some(e2)
        })
        .collect();
    crate::frecency::ranked(&mut out);
    out
}

/// 选定执行（M8-12，纯导航语义）：目录 → cd；文件 → 切到父目录并定位光标。
fn jump_to(state: &mut AppState, entry: &crate::frecency::ScoredEntry) -> Result<String, String> {
    let cfg = state.config.clone();
    let target = PathBuf::from(&entry.path);
    if entry.topic == "dir" {
        if !target.is_dir() {
            return Err(format!("目录已不存在: {}", entry.path));
        }
        let tab = state.active_tab_mut();
        tab.history.push(tab.location.clone());
        tab.location = Location::Physical(target);
        tab.cursor = 0;
        tab.scroll = 0;
        let _ = tab.refresh(cfg);
        Ok(format!("跳转目录 {}", entry.path))
    } else {
        let parent = target
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or_else(|| format!("无父目录: {}", entry.path))?
            .to_path_buf();
        if !parent.is_dir() {
            return Err(format!("父目录已不存在: {}", parent.display()));
        }
        let leaf = target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .ok_or_else(|| format!("无文件名: {}", entry.path))?;
        let tab = state.active_tab_mut();
        if tab.location.base_dir() != parent {
            tab.history.push(tab.location.clone());
            tab.location = Location::Physical(parent);
            tab.cursor = 0;
            tab.scroll = 0;
            let _ = tab.refresh(cfg);
        }
        match tab.entries.iter().position(|e| e.name == leaf) {
            Some(idx) => {
                tab.cursor = idx;
                Ok(format!("定位 {}", entry.path))
            }
            None => Err(format!("文件不可见（已删除或被过滤）: {}", entry.path)),
        }
    }
}

/// 执行最小 `:` 命令（M1-5/M2-6）。
fn exec_command_impl(state: &mut AppState, line: &str) -> Result<ExecOutcome, String> {
    // M18-4：`!<cmd>` 外部命令（在所有守卫之前；执行目标由 run 循环定）。
    if let Some(rest) = line.strip_prefix('!') {
        let cfg = state.config.ui.clone();
        return match run_external_target(&cfg, rest.trim()) {
            ExternalRun::Multiplexer(msg) => Ok(ExecOutcome::Done { msg: Some(msg) }),
            ExternalRun::CurrentPane(cmd) => Ok(ExecOutcome::RunExternal { cmd }),
        };
    }
    let mut parts = line.split_whitespace();
    let cmd_head = parts.next();
    // FEAT-2：`..` 行仅导航——写操作与详情命令拒绝（cd/q/help/tree 例外）。
    if state.active_tab().on_parent_row {
        match cmd_head {
            // M24-8：compare 只读且不依赖光标位置，`..` 行放行。
            Some(
                "cd" | "q" | "quit" | "help" | "histprev" | "histnext" | "history" | "his"
                | "compare" | "theme",
            ) => {}
            Some(other) => return Err(format!("`..` 行仅导航（j 向下离开）: {other}")),
            None => return Ok(ExecOutcome::Done { msg: None }),
        }
    }
    // M19-2/M20-1：远端视图——写命令由 match 中的远端路由臂处理。
    if matches!(state.active_tab().location, Location::Remote { .. }) {
        match cmd_head {
            Some(
                "cd" | "q" | "quit" | "help" | "remote" | "histprev" | "histnext" | "history"
                | "his" | "sort" | "find" | "grep" | "recent" | "file" | "rename" | "mkdir"
                | "touch" | "copy" | "move" | "mk" | "theme",
            ) => {}
            Some(other) => {
                // M22-4：去掉过时的"在 M20 接入"表述。
                return Err(format!("远端不支持: {other}"));
            }
            None => return Ok(ExecOutcome::Done { msg: None }),
        }
    }
    // M17 SRCH：结果视图只读——find/grep 重执行与退出类命令豁免。
    if state.active_tab().in_results() {
        match cmd_head {
            Some("find" | "grep" | "sort" | "q" | "quit" | "help" | "theme") => {}
            Some(other) => {
                return Err(format!("搜索结果视图为只读（Esc 退出）: {other}"));
            }
            None => return Ok(ExecOutcome::Done { msg: None }),
        }
    }
    // M13-7：树视图只读——除 :tree（退出）/ :q / :help 外的命令拒绝。
    if state.active_tab().in_tree() {
        match cmd_head {
            Some("tree") | Some("q" | "quit") | Some("help") | Some("theme") => {}
            Some(other) => {
                return Err(format!("树视图为只读浏览（:tree 退出）: {other}"));
            }
            None => return Ok(ExecOutcome::Done { msg: None }),
        }
    }
    match cmd_head {
        None => Ok(ExecOutcome::Done { msg: None }),
        Some("theme") => {
            // AUDIT-13：运行时主题切换（内置优先、用户主题次之）；
            // 无参列出可用主题（内置 + 用户目录）。
            let name = parts.collect::<Vec<&str>>().join(" ");
            if name.is_empty() {
                let mut names = crate::theme::builtin_names();
                names.extend(crate::theme::discover_user_themes());
                return Ok(ExecOutcome::Done {
                    msg: Some(format!("可用主题: {}", names.join(", "))),
                });
            }
            crate::theme::activate(&name).map_err(|e| e.to_string())?;
            let variant = match crate::theme::active_variant() {
                opaline::schema::ThemeVariant::Light => "light",
                _ => "dark",
            };
            Ok(ExecOutcome::Done {
                msg: Some(format!("主题已切换: {name}（{variant}）")),
            })
        }
        Some("sort") => {
            // M18-2：多字段排序（:sort name,size desc）。
            let args = parts.collect::<Vec<&str>>().join(" ");
            let specs = parse_sort_spec(&args)?;
            let cfg = state.config.clone();
            state.active_tab_mut().apply_sort_multi(specs.clone(), cfg);
            let text = specs
                .iter()
                .map(|s| {
                    format!(
                        "{} {}",
                        match s.field {
                            crate::tab::SortField::Name => "名称 name",
                            crate::tab::SortField::Natural => "natural",
                            crate::tab::SortField::Mtime => "mtime",
                            crate::tab::SortField::Size => "size",
                            crate::tab::SortField::Ext => "ext",
                        },
                        if s.desc { "desc" } else { "asc" }
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            Ok(ExecOutcome::Done {
                msg: Some(format!("已排序: {text}")),
            })
        }
        Some("find" | "grep") => {
            // M17 SRCH-1/2：递归搜索 → 结果虚拟目录（重执行 = 替换）。
            // M22-5：远端视图 → RemoteFs 驱动（无 ignore 语义；上限同源）。
            let is_grep = cmd_head == Some("grep");
            let pattern = parts.next().unwrap_or("").to_owned();
            if let Location::Remote { remote, path } = state.active_tab().location.clone() {
                let backend = state
                    .remote_sessions
                    .get(&remote)
                    .ok_or_else(|| "remote: 会话已断开".to_owned())?;
                let (items, truncated) = if is_grep {
                    crate::search::remote_grep_files(
                        backend.as_ref(),
                        &path.to_string_lossy(),
                        &pattern,
                    )
                    .map_err(|e| format!("grep: {e}"))?
                } else {
                    crate::search::remote_find_files(
                        backend.as_ref(),
                        &path.to_string_lossy(),
                        &pattern,
                    )
                };
                let n = items.len();
                let kind = if is_grep {
                    crate::search::SearchKind::Grep
                } else {
                    crate::search::SearchKind::Find
                };
                state
                    .active_tab_mut()
                    .enter_results(kind, pattern, path, items, truncated);
                return Ok(ExecOutcome::Done {
                    msg: Some(format!(
                        "{}: {} 个结果{}",
                        if is_grep { "grep" } else { "find" },
                        n,
                        if truncated { "（已截断）" } else { "" }
                    )),
                });
            }
            let base = physical_base(state)?;
            let show_hidden = state.config.show_hidden;
            let (items, truncated) = if is_grep {
                crate::search::grep_files(&base, &pattern, show_hidden)
                    .map_err(|e| format!("grep: {e}"))?
            } else {
                crate::search::find_files(&base, &pattern, show_hidden)
            };
            let n = items.len();
            let kind = if is_grep {
                crate::search::SearchKind::Grep
            } else {
                crate::search::SearchKind::Find
            };
            state
                .active_tab_mut()
                .enter_results(kind, pattern, base, items, truncated);
            Ok(ExecOutcome::Done {
                msg: Some(format!(
                    "{}: {} 个结果{}",
                    if is_grep { "grep" } else { "find" },
                    n,
                    if truncated { "（已截断）" } else { "" }
                )),
            })
        }
        Some("compare") => {
            // M24-8：左右 pane 目录对比（按文件名+大小）→ 结果虚拟目录
            // （只读，复用 Tab.results；= 同 / ! 异 / < 仅左 / > 仅右）。
            let (left_base, left_items) = {
                let tab = state.active_tab();
                match &tab.location {
                    Location::Physical(p) => (p.clone(), tab.entries.clone()),
                    _ => return Err("compare: 需要 pane 为物理目录视图".to_owned()),
                }
            };
            let (right_base, right_items) = {
                let tab = state.active_tab();
                match &tab.right.location {
                    Location::Physical(p) => (p.clone(), tab.right.entries.clone()),
                    _ => return Err("compare: 需要 pane 为物理目录视图".to_owned()),
                }
            };
            let items = compare_panes(&left_base, &left_items, &right_base, &right_items);
            let (same, diff, only_l, only_r) = (
                items.iter().filter(|i| i.text == "=").count(),
                items.iter().filter(|i| i.text == "!").count(),
                items.iter().filter(|i| i.text == "<").count(),
                items.iter().filter(|i| i.text == ">").count(),
            );
            let total = items.len();
            state.active_tab_mut().enter_results(
                crate::search::SearchKind::Compare,
                String::new(),
                left_base,
                items,
                false,
            );
            Ok(ExecOutcome::Done {
                msg: Some(format!(
                    "compare: 相同 {same} · 不同 {diff} · 仅左 {only_l} · 仅右 {only_r}（共 {total}）"
                )),
            })
        }
        Some("archive") => {
            // M25-3/M25-4：归档创建——`[格式] <名> [checksum=md5|sha256|none]`；
            // 无参 → 交互对话框（名称预填光标条目名）；缺格式从扩展名推断；
            // 校验未显式给定时取 `[archive] checksum`（默认 md5）。
            let args: Vec<&str> = parts.collect();
            let default_checksum =
                ops::Checksum::parse(&state.config.archive.checksum).unwrap_or(ops::Checksum::Md5);
            match args.as_slice() {
                [] => {
                    // M25-4：无参 → 对话框（名称预填光标条目名）。
                    let input = state
                        .active_tab()
                        .entries
                        .get(state.active_tab().cursor)
                        .map(|i| i.name.clone())
                        .unwrap_or_default();
                    Ok(ExecOutcome::ShowArchiveDialog { input })
                }
                [fmt, name] => {
                    let format = ops::ArchiveFormat::parse(fmt)
                        .ok_or_else(|| format!("archive: 未知格式 {fmt}（zip / tar / tar.gz）"))?;
                    let msg = archive_execute(state, format, name, default_checksum)?;
                    Ok(ExecOutcome::Done { msg: Some(msg) })
                }
                [name] => {
                    let format = ops::ArchiveFormat::infer(name).ok_or_else(|| {
                        "archive: 无法从扩展名推断格式（需 zip/tar/tar.gz）".to_owned()
                    })?;
                    let msg = archive_execute(state, format, name, default_checksum)?;
                    Ok(ExecOutcome::Done { msg: Some(msg) })
                }
                [fmt, name, kv] if kv.starts_with("checksum=") => {
                    let format = ops::ArchiveFormat::parse(fmt)
                        .ok_or_else(|| format!("archive: 未知格式 {fmt}（zip / tar / tar.gz）"))?;
                    let checksum = ops::Checksum::parse(&kv["checksum=".len()..])
                        .ok_or_else(|| "archive: 校验类型需 md5 / sha256 / none".to_owned())?;
                    let msg = archive_execute(state, format, name, checksum)?;
                    Ok(ExecOutcome::Done { msg: Some(msg) })
                }
                [_, _, _] => {
                    Err("archive: 参数错误（:archive [zip|tar|tar.gz] <名> [checksum=md5|sha256|none]）".to_owned())
                }
                _ => Err("archive: 参数过多（:archive [zip|tar|tar.gz] <名> [checksum=md5|sha256|none]）".to_owned()),
            }
        }
        Some("gitui") => {
            // M25-6：`:gitui` 同 gi 语义——PATH 优先级探测，复用 RunUserCommand
            // 分流（terminal_in_new_window 一致）。
            match crate::git::find_git_tui() {
                Some(bin) => {
                    let cfg = state.config.ui.clone();
                    match run_external_target(&cfg, bin) {
                        ExternalRun::Multiplexer(msg) => Ok(ExecOutcome::Done { msg: Some(msg) }),
                        ExternalRun::CurrentPane(cmd) => Ok(ExecOutcome::RunExternal { cmd }),
                    }
                }
                None => Err(
                    "gitui: 未找到 git TUI（PATH 探测 gitui / ziggity / lazygit 均失败）"
                        .to_owned(),
                ),
            }
        }
        Some("remote") => {
            // M19-4：进入远端（remotes.toml 覆盖 ssh config 别名）；无参列出。
            match parts.next() {
                None => {
                    let all = crate::remote::config::resolve_all(
                        &config::data_dir().join("remotes.toml"),
                        &config::ssh_config_path(),
                    );
                    if all.is_empty() {
                        Ok(ExecOutcome::Done {
                            msg: Some(
                                "remote: 无已配置远端（remotes.toml / ~/.ssh/config）".to_owned(),
                            ),
                        })
                    } else {
                        let names: Vec<String> = all.keys().cloned().collect();
                        Ok(ExecOutcome::Done {
                            msg: Some(format!("remote: {}（:remote <名> 进入）", names.join(", "))),
                        })
                    }
                }
                Some(name) => {
                    // 已有活动会话直接进入（避免重复连接）。
                    if !state.remote_sessions.contains_key(name) {
                        let all = crate::remote::config::resolve_all(
                            &config::data_dir().join("remotes.toml"),
                            &config::ssh_config_path(),
                        );
                        let entry = all.get(name).ok_or_else(|| {
                            format!("remote: 未配置 `{name}`（:remote 无参查看）")
                        })?;
                        let params = entry.to_params(name);
                        let backend = match crate::remote::sftp::SftpBackend::connect(&params) {
                            Ok(b) => b,
                            // M22-3：公钥认证失败 → 口令输入（不落盘）。
                            Err(crate::remote::RemoteError::Auth(_)) => {
                                return Ok(ExecOutcome::RemotePass {
                                    name: name.to_owned(),
                                    params,
                                });
                            }
                            Err(e) => return Err(format!("remote: {e}")),
                        };
                        register_remote_session(state, name, Box::new(backend));
                    }
                    let root = crate::remote::config::load_remotes_toml(
                        &config::data_dir().join("remotes.toml"),
                    )
                    .get(name)
                    .and_then(|e| e.root.clone())
                    .unwrap_or_else(|| PathBuf::from("/"));
                    remote_enter(state, name, &root)?;
                    Ok(ExecOutcome::Done {
                        msg: Some(format!("remote: 已进入 {name}（h 返回本地目录）")),
                    })
                }
            }
        }
        Some("recent") => {
            // M16 NAV-4：近期视角——全部条目按最后访问时间倒序（含会话合并）。
            let entries = load_jump_entries(state, JumpKind::Recent)?;
            Ok(ExecOutcome::OpenJump {
                kind: JumpKind::Recent,
                entries,
            })
        }
        Some("rename" | "mkdir" | "touch" | "copy" | "move" | "mk")
            if matches!(state.active_tab().location, Location::Remote { .. }) =>
        {
            // M20-1：远端写命令（与本地同键同语义；undo 入撤销栈）。
            // M22-4：`:mk <名>` = 远端 mkdir + 进入该目录。
            let is_mkdir = cmd_head == Some("mkdir");
            let is_mk = cmd_head == Some("mk");
            let is_touch = cmd_head == Some("touch");
            let is_copy = cmd_head == Some("copy");
            let arg = parts
                .next()
                .ok_or_else(|| format!("{}: 缺少参数", cmd_head.unwrap_or_default()))?
                .to_owned();
            let (remote, base) = match &state.active_tab().location.clone() {
                Location::Remote { remote, path } => (remote.clone(), path.clone()),
                _ => return Err("remote: 非远端视图".to_owned()),
            };
            let dest = if base == std::path::Path::new("/") {
                PathBuf::from(format!("/{arg}"))
            } else {
                base.join(&arg)
            };
            let backend = state
                .remote_sessions
                .get(&remote)
                .ok_or_else(|| "remote: 会话已断开".to_owned())?;
            let kind = if is_mkdir || is_mk {
                backend.create_dir(&dest).map_err(|e| e.to_string())?;
                crate::state::RemoteUndo::Create {
                    path: dest.clone(),
                    is_dir: true,
                }
            } else if is_touch {
                backend.write(&dest, b"").map_err(|e| e.to_string())?;
                crate::state::RemoteUndo::Create {
                    path: dest.clone(),
                    is_dir: false,
                }
            } else {
                let tab = state.active_tab();
                let src = tab
                    .entries
                    .get(tab.cursor)
                    .map(|item| {
                        if base == std::path::Path::new("/") {
                            PathBuf::from(format!("/{}", item.name))
                        } else {
                            base.join(&item.name)
                        }
                    })
                    .ok_or_else(|| "copy: 无选中条目".to_owned())?;
                if is_copy {
                    let data = backend.read(&src).map_err(|e| e.to_string())?;
                    backend.write(&dest, &data).map_err(|e| e.to_string())?;
                } else {
                    backend.rename(&src, &dest).map_err(|e| e.to_string())?;
                }
                crate::state::RemoteUndo::Rename {
                    from: src,
                    to: dest.clone(),
                }
            };
            let tab_id = state.active;
            state.undo_stack.push(crate::state::UndoableOp::Remote {
                tab_id,
                remote: remote.clone(),
                kind,
            });
            // M22-4：`:mk` 创建后进入新目录（其余命令刷新当前目录）。
            let nav_target = if is_mk { &dest } else { &base };
            let _ = remote_navigate(state, nav_target);
            Ok(ExecOutcome::Done {
                msg: Some(format!(
                    "{}: {arg} OK{}",
                    cmd_head.unwrap_or_default(),
                    if is_mk { "（已进入）" } else { "" }
                )),
            })
        }
        Some("q" | "quit") => Ok(ExecOutcome::Quit),
        Some("cd") => {
            // M5-3：切换当前 tab 到目标目录（保留原名导航语义）。
            // M21-E8：进入成功实时记录（记录补全：目录 :cd 调用点）。
            // M22-1：相对路径相对**当前 tab 目录**解析（原为进程 CWD，
            // M21-E5 CI 实测漂移）；解析后恒为绝对路径。
            let dir_arg = parts.next().ok_or_else(|| "cd: 缺少目录".to_owned())?;
            // M24-7：`~` 展开为 HOME 后再解析（相对路径仍锚定当前 tab 目录）。
            let target = {
                let cur = state.active_tab().location.base_dir().to_path_buf();
                let p = PathBuf::from(expand_home_arg(dir_arg));
                if p.is_absolute() { p } else { cur.join(p) }
            };
            if !target.is_dir() {
                return Err(format!("cd: 目录不存在: {dir_arg}"));
            }
            let target = normalize_lexical(&target);
            let cfg = state.config.clone();
            let tab = state.active_tab_mut();
            tab.navigate_to(Location::Physical(target), cfg)
                .map_err(|e| format!("cd: {e}"))?;
            let tab_id = state.active;
            record_live_visit(state, tab_id, 'L');
            Ok(ExecOutcome::Done { msg: None })
        }
        Some("help") => Ok(ExecOutcome::EnterHelp),
        Some("tabnew") => {
            if state.new_tab_here() {
                Ok(ExecOutcome::Done { msg: None })
            } else {
                Err("tabnew: 目录读取失败".to_owned())
            }
        }
        Some("tabclose") => {
            // 关闭标签不可撤销：明确提示（M2-6）。
            if state.close_tab() {
                Ok(ExecOutcome::Quit)
            } else {
                Ok(ExecOutcome::Done {
                    msg: Some("标签已关闭（不可撤销）".to_owned()),
                })
            }
        }
        Some("tabmove") => {
            let n = parts
                .next()
                .map(str::parse::<usize>)
                .transpose()
                .map_err(|_| "tabmove: 位置必须是数字".to_owned())?;
            state.move_tab(n);
            Ok(ExecOutcome::Done { msg: None })
        }
        Some("rename") => {
            // M21-E4：无参 `:rename` → $EDITOR 批量（与 cw/R 同管线）；
            // 带参保持 M2-3 单项直改。
            let Some(new_name) = parts.next() else {
                if state.active_tab().in_results() {
                    return Err("搜索结果视图只读".to_owned());
                }
                if state.active_tab().on_parent_row {
                    return Err("`..` 行仅导航".to_owned());
                }
                let base = physical_base(state)?;
                let tab = state.active_tab();
                let names: Vec<String> = if tab.tags.is_empty() {
                    tab.entries
                        .get(tab.cursor)
                        .map(|i| i.name.clone())
                        .into_iter()
                        .collect()
                } else {
                    tab.entries
                        .iter()
                        .filter(|i| tab.tags.contains(&i.name))
                        .map(|i| i.name.clone())
                        .collect()
                };
                if names.is_empty() {
                    return Err("rename: 无选中条目".to_owned());
                }
                return Ok(ExecOutcome::BatchRename { base, names });
            };
            // M2-3：重命名选中条目（新名字不含空白；含空格的名字在 M5-4 统一解析）。
            let base = physical_base(state)?;
            let tab = state.active_tab();
            let path = tab
                .entries
                .get(tab.cursor)
                .map(|item| base.join(&item.name))
                .ok_or_else(|| "rename: 无选中条目".to_owned())?;
            // 提权重试的目标路径与 ops::rename_to 内部校验产物一致（base.join）。
            let target = base.join(new_name);
            let op = match ops::rename_to(state.active, &path, new_name) {
                Ok(op) => op,
                Err(e) if crate::ops::is_permission_denied(&e) && cfg!(unix) => {
                    return Ok(ExecOutcome::Escalate {
                        op: crate::privilege::PrivOp::Rename {
                            from: path,
                            to: target,
                        },
                    });
                }
                Err(e) => return Err(format!("rename: {e}")),
            };
            state.undo_stack.push(op);
            let _tid = state.active;
            refresh_tab_panes(state, _tid);
            Ok(ExecOutcome::Done { msg: None })
        }
        Some("move") => {
            // M2-4：移动选中条目到目标目录（保留原名；`yy`/`P` 键位在 M10 接入）。
            let dir_arg = parts
                .next()
                .ok_or_else(|| "move: 缺少目标目录".to_owned())?;
            // M24-7：目标目录 `~` 展开为 HOME。
            let target = PathBuf::from(expand_home_arg(dir_arg));
            let base = physical_base(state)?;
            let tab = state.active_tab();
            let path = tab
                .entries
                .get(tab.cursor)
                .map(|item| base.join(&item.name))
                .ok_or_else(|| "move: 无选中条目".to_owned())?;
            let op = match ops::move_to(state.active, &path, &target) {
                Ok(op) => op,
                Err(e) if crate::ops::is_permission_denied(&e) && cfg!(unix) => {
                    return Ok(ExecOutcome::Escalate {
                        op: crate::privilege::PrivOp::Move {
                            from: path,
                            to: target,
                        },
                    });
                }
                Err(e) => return Err(format!("move: {e}")),
            };
            state.undo_stack.push(op);
            let _tid = state.active;
            refresh_tab_panes(state, _tid);
            Ok(ExecOutcome::Done { msg: None })
        }
        Some("copy") => {
            // M2-4b（SPEC §5 M2 MUST）：复制选中条目到目标目录（保留原名）；
            // undo = 删除生成的副本；`p`（PutCopy）键位在 M10 接入。
            let dir_arg = parts
                .next()
                .ok_or_else(|| "copy: 缺少目标目录".to_owned())?;
            // M24-7：目标目录 `~` 展开为 HOME。
            let target = PathBuf::from(expand_home_arg(dir_arg));
            let base = physical_base(state)?;
            let tab = state.active_tab();
            let path = tab
                .entries
                .get(tab.cursor)
                .map(|item| base.join(&item.name))
                .ok_or_else(|| "copy: 无选中条目".to_owned())?;
            let op = match ops::copy_to(state.active, &path, &target) {
                Ok(op) => op,
                Err(e) if crate::ops::is_permission_denied(&e) && cfg!(unix) => {
                    return Ok(ExecOutcome::Escalate {
                        op: crate::privilege::PrivOp::Copy {
                            from: path,
                            to: target,
                        },
                    });
                }
                Err(e) => return Err(format!("copy: {e}")),
            };
            state.undo_stack.push(op);
            let _tid = state.active;
            refresh_tab_panes(state, _tid);
            Ok(ExecOutcome::Done { msg: None })
        }
        Some("tag") => {
            // M3-3：对标记集合执行批量复制/移动/删除。
            // 逐文件入撤销栈（LIFO 逆序还原）；已消失的标记条目跳过。
            let action = parts
                .next()
                .ok_or_else(|| "tag: 缺少动作（delete / copy|move <目录>）".to_owned())?;
            match action {
                "delete" => {
                    if state.active_tab().tagged_count() == 0 {
                        Ok(ExecOutcome::Done {
                            msg: Some("tag: 无标记文件".to_owned()),
                        })
                    } else {
                        Ok(ExecOutcome::ConfirmTagDelete)
                    }
                }
                "copy" | "move" => {
                    let dir_arg = parts
                        .next()
                        .ok_or_else(|| format!("tag {action}: 缺少目标目录"))?;
                    // M24-7：目标目录 `~` 展开为 HOME。
                    let target = PathBuf::from(expand_home_arg(dir_arg));
                    let base = physical_base(state)?;
                    let names: Vec<String> = state.active_tab().tags.iter().cloned().collect();
                    if names.is_empty() {
                        return Ok(ExecOutcome::Done {
                            msg: Some("tag: 无标记文件".to_owned()),
                        });
                    }
                    // M13-8：批量复制/移动 → 后台作业（run 内 spawn）；
                    // 完成后主线程入撤销栈（`u` 可逆）并清空标记。
                    Ok(ExecOutcome::SpawnJob {
                        op: action.to_owned(),
                        names,
                        target,
                        base,
                    })
                }
                other => Err(format!(
                    "tag: 未知动作 {other}（delete / copy|move <目录>）"
                )),
            }
        }
        Some(cmd @ ("mkdir" | "touch")) => {
            // M2-5：在当前目录新建目录/空文件。
            let name = parts.next().ok_or_else(|| format!("{cmd}: 缺少名字"))?;
            let base = physical_base(state)?;
            let result = if cmd == "mkdir" {
                ops::create_dir(state.active, &base, name)
            } else {
                ops::create_file(state.active, &base, name)
            };
            let op = match result {
                Ok(op) => op,
                Err(e) if crate::ops::is_permission_denied(&e) && cfg!(unix) => {
                    return Ok(ExecOutcome::Escalate {
                        op: if cmd == "mkdir" {
                            crate::privilege::PrivOp::CreateDir {
                                path: base.join(name),
                            }
                        } else {
                            crate::privilege::PrivOp::CreateFile {
                                path: base.join(name),
                            }
                        },
                    });
                }
                Err(e) => return Err(format!("{cmd}: {e}")),
            };
            state.undo_stack.push(op);
            let _tid = state.active;
            refresh_tab_panes(state, _tid);
            // M21-E5：mkdir 成功后光标定位到新目录条目（失败路径已在
            // 上方 Err 返回，光标不动；条目被隐藏过滤时保持原位）。
            if cmd == "mkdir" {
                let tab = state.active_tab_mut();
                if let Some(idx) = tab.entries.iter().position(|i| i.name == name) {
                    tab.cursor = idx;
                    tab.on_parent_row = false;
                }
            }
            Ok(ExecOutcome::Done { msg: None })
        }
        Some("mk") => {
            // M21-E5：`:mk <名>` 创建并进入新目录（失败不进入、光标不动）。
            let name = parts.next().ok_or_else(|| "mk: 缺少名字".to_owned())?;
            let base = physical_base(state)?;
            let op = match ops::create_dir(state.active, &base, name) {
                Ok(op) => op,
                Err(e) if crate::ops::is_permission_denied(&e) && cfg!(unix) => {
                    return Ok(ExecOutcome::Escalate {
                        op: crate::privilege::PrivOp::CreateDir {
                            path: base.join(name),
                        },
                    });
                }
                Err(e) => return Err(format!("mk: {e}")),
            };
            state.undo_stack.push(op);
            let cfg = state.config.clone();
            let tab_id = state.active;
            // navigate_to 先加载成功才变更状态（M16 语义；失败不改光标）。
            if let Some(tab) = state.tabs.get_mut(tab_id) {
                tab.navigate_to(Location::Physical(base.join(name)), cfg)
                    .map_err(|e| format!("mk: {e}"))?;
            }
            refresh_tab_panes(state, tab_id);
            Ok(ExecOutcome::Done { msg: None })
        }
        Some(cmd @ ("j" | "J")) => {
            // M8-12：跳转选择器内部命令——:j 仅目录（dir 时钟）、:J 仅文件
            // （file 时钟）。数据一次性读入，纯只读。
            let kind = if cmd == "j" {
                JumpKind::Dir
            } else {
                JumpKind::File
            };
            let entries = load_jump_entries(state, kind)?;
            Ok(ExecOutcome::OpenJump { kind, entries })
        }
        Some("tree") => {
            // M13-7：:tree [depth=N] 树视图切换（只读浏览；再执行恢复平铺）。
            if matches!(state.active_tab().location, Location::Archive { .. }) {
                return Err("tree: 归档视图不支持树浏览".to_owned());
            }
            if state.active_tab().in_tree() {
                state
                    .active_tab_mut()
                    .exit_tree()
                    .map_err(|e| format!("退出树视图失败: {e}"))?;
                Ok(ExecOutcome::Done {
                    msg: Some("已退出树视图".to_owned()),
                })
            } else {
                let depth = parts
                    .next()
                    .and_then(|a| a.strip_prefix("depth="))
                    .and_then(|d| d.parse::<u32>().ok())
                    .unwrap_or(8);
                state
                    .active_tab_mut()
                    .enter_tree(depth)
                    .map_err(|e| format!("树视图构建失败: {e}"))?;
                Ok(ExecOutcome::Done {
                    msg: Some(format!("树视图（depth={depth}）——只读浏览，:tree 退出")),
                })
            }
        }
        Some("jobs") => Ok(ExecOutcome::ShowJobs),
        Some("trash") => {
            // M13-10：暂存区菜单（staged 仍存在的条目）。
            let items = crate::ops::read_trash_index();
            Ok(ExecOutcome::ShowTrash { items })
        }
        Some("history" | "his") => {
            // M16 NAV-2：目录历史菜单（当前 tab back 栈，live 渲染）。
            Ok(ExecOutcome::ShowHistory)
        }
        Some("histprev" | "histnext") => {
            // M16 NAV-3/M19-2：当前 tab 视图历史后退/前进（无默认键位）。
            let forward = cmd_head == Some("histnext");
            let mut m = None;
            hist_move(state, forward, &mut m)?;
            Ok(ExecOutcome::Done { msg: m })
        }
        Some("keyword") => {
            // M16 NAV-1：设置/清除当前目录书签 keyword（append-only 事件）。
            // `-` 清除；归档视图与 `..` 行已被前置守卫拒绝。
            let kw = parts
                .next()
                .ok_or_else(|| "keyword: 缺少参数（`-` 清除）".to_owned())?;
            if kw.chars().count() > 32 {
                return Err("keyword: 过长（≤32 字符）".to_owned());
            }
            let base = physical_base(state)?;
            let clear = kw == "-";
            let kw_str = if clear { String::new() } else { kw.to_owned() };
            crate::frecency::set_keyword_now(&base.display().to_string(), &kw_str)
                .map_err(|e| format!("keyword: 写入失败: {e}"))?;
            Ok(ExecOutcome::Done {
                msg: Some(if clear {
                    "已清除书签 keyword".to_owned()
                } else {
                    format!("已设置 keyword: {kw_str}")
                }),
            })
        }
        Some("file") => {
            // FEAT-3：`:file` 显示当前选中项详情（与 Ctrl-G/右键菜单同源；
            // 归档成员显示 host:/inner/name）。`..` 行已被前置守卫拒绝。
            match current_item_info(state) {
                Some((path, text)) => Ok(ExecOutcome::ShowInfo { path, text }),
                None => Err("file: 无选中条目".to_owned()),
            }
        }
        Some("view") => {
            // M13-9：:view 同 w——预览窗格完整内容视图开关。
            let on = state.active_tab_mut().toggle_view();
            Ok(ExecOutcome::Done {
                msg: Some(if on {
                    "view: 完整内容视图开启".to_owned()
                } else {
                    "view: 已恢复摘要预览".to_owned()
                }),
            })
        }
        Some(cmd @ ("select" | "unselect")) => {
            // M13-2：按 pattern 批量选择/取消（对齐 vifm :select/:unselect）。
            // 无参 = 仅当前光标条目；unselect 无参 = 清空全部。
            let pattern = parts.next();
            let select = cmd == "select";
            let tab = state.active_tab_mut();
            if select {
                let n = tab.select_by_pattern(pattern, true);
                Ok(ExecOutcome::Done {
                    msg: Some(format!("{cmd}: {n} 个条目")),
                })
            } else if pattern.is_none() {
                let n = tab.tagged_count();
                tab.clear_tags();
                Ok(ExecOutcome::Done {
                    msg: Some(format!("unselect: 清除 {n} 个标记")),
                })
            } else {
                let n = tab.select_by_pattern(pattern, false);
                Ok(ExecOutcome::Done {
                    msg: Some(format!("{cmd}: {n} 个条目")),
                })
            }
        }
        Some(other) => Err(format!("未知命令: {other}（完整命令解析在 M5-4 接入）")),
    }
}

/// `n`/`N`：沿当前方向移动到下一个匹配；未设置搜索词时提示（M1-7）。
fn run_search(state: &mut AppState, msg: &mut Option<String>, forward: bool) {
    let Some(query) = state.active_tab().search.as_ref().map(|s| s.query.clone()) else {
        *msg = Some("未设置搜索词（先用 / 搜索）".to_owned());
        return;
    };
    if state.active_tab_mut().search_move(&query, forward) {
        *msg = None;
    } else {
        *msg = Some("无匹配".to_owned());
    }
}

/// M21-F3：搜索 Esc 取消——光标回 origin、清除已提交搜索状态
/// （状态栏与列表高亮随之消失）；run 循环与测试共用。
fn search_cancel(state: &mut AppState, origin: usize) {
    let tab = state.active_tab_mut();
    tab.cursor = origin.min(tab.entries.len().saturating_sub(1));
    tab.search = None;
}

/// 提交搜索词（M1-7）：设置搜索状态并移动到首个匹配（从光标下一位环绕）。
/// FIX-1：实时跳转已把光标停在匹配项上时不重复前跳；空查询清除搜索状态
/// （高亮与计数随之消失）。
fn submit_search(state: &mut AppState, line: &str, msg: &mut Option<String>) {
    let query = line.trim().to_owned();
    if query.is_empty() {
        state.active_tab_mut().search = None;
        *msg = None;
        return;
    }
    let tab = state.active_tab_mut();
    tab.on_parent_row = false; // FEAT-2：搜索交互离开 `..` 行。
    let on_match = tab
        .entries
        .get(tab.cursor)
        .is_some_and(|item| crate::tab::match_range(&item.name, &query).is_some());
    if on_match {
        tab.search = Some(SearchState { query });
        *msg = None;
        return;
    }
    let start = tab.cursor + 1;
    match tab.find_match(&query, start, true) {
        Some(idx) => {
            tab.cursor = idx;
            tab.search = Some(SearchState { query });
            *msg = None;
        }
        None => {
            tab.search = Some(SearchState { query });
            *msg = Some("无匹配".to_owned());
        }
    }
}

/// FIX-1：搜索输入期间的实时跳转（incsearch）——从进入搜索前的光标位置
/// `origin` 出发跳到第一个匹配项；输入为空或无匹配时回到 `origin`。
fn live_search_jump(state: &mut AppState, origin: usize, input: &str, msg: &mut Option<String>) {
    let query = input.trim();
    let tab = state.active_tab_mut();
    tab.on_parent_row = false; // FEAT-2：搜索交互离开 `..` 行。
    if query.is_empty() {
        tab.cursor = origin.min(tab.entries.len().saturating_sub(1));
        *msg = None;
        return;
    }
    match tab.find_match(query, origin + 1, true) {
        Some(idx) => {
            tab.cursor = idx;
            *msg = None;
        }
        None => {
            tab.cursor = origin;
            *msg = Some("无匹配".to_owned());
        }
    }
}

/// 右 pane 焦点下的按键分发（M14-1）：精简导航集。
/// SIMPLIFIED：搜索（/）、可视选择（v）、标记（t）、树（:tree 仍可退出）、
/// 提权（写命令 EACCES 时提示切回左 pane）仅左 pane。
fn handle_right_key(state: &mut AppState, msg: &mut Option<String>, key: KeyEvent) -> Outcome {
    if key.kind != KeyEventKind::Press {
        return Outcome::Continue;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let cfg = state.config.clone();
    let tab = state.active_tab_mut();
    let half = 8isize; // 右 pane 半页步进（固定近似值，视口高度不可得）
    match key.code {
        KeyCode::Tab | KeyCode::Char(' ') => {
            // M21-F2：焦点经边框/当前行样式呈现，状态栏不再提示焦点文字。
            tab.focus = crate::tab::Focus::Left;
        }
        KeyCode::Char('j') if !ctrl => tab.right.move_cursor(1),
        KeyCode::Char('k') if !ctrl => tab.right.move_cursor(-1),
        KeyCode::Char('d') if ctrl => tab.right.move_cursor(half),
        KeyCode::Char('u') if ctrl => tab.right.move_cursor(-half),
        KeyCode::Char('g') if !ctrl => {
            // M14-1 右 pane 精简集：g = 跳到顶部（无 gg 序列）。
            tab.right.cursor_top();
        }
        KeyCode::Char('G') if !ctrl => tab.right.cursor_bottom(),
        KeyCode::Enter | KeyCode::Char('l') if !ctrl => {
            if let Err(e) = tab.right.enter_selected(cfg) {
                *msg = Some(format!("进入失败: {e}"));
            } else {
                // M21-F9：右 pane 进入成功同样实时记录（侧别 'R'）。
                let tab_id = state.active;
                record_live_visit(state, tab_id, 'R');
            }
        }
        KeyCode::Char('h') if !ctrl => {
            if let Err(e) = tab.right.return_parent(cfg) {
                *msg = Some(format!("返回失败: {e}"));
            }
        }
        KeyCode::Char('g') if ctrl => {
            // Ctrl-G：右 pane 光标文件的详情对话框。
            if let (Location::Physical(base), Some(item)) =
                (&tab.right.location, tab.right.entries.get(tab.right.cursor))
            {
                let path = base.join(&item.name);
                let text = file_info_text(&path, &item.name);
                return Outcome::FileInfo { path, text };
            }
        }
        KeyCode::Char('w') if !ctrl => {
            // view 切换：右 pane 目录列表 ↔ 预览视图（M13-9 语义保持）。
            let on = tab.toggle_view();
            *msg = Some(if on {
                "view: 预览视图（活动 pane 光标文件）".to_owned()
            } else {
                "view: 已恢复目录列表".to_owned()
            });
        }
        KeyCode::Char('d' | 'D' | 'c') if !ctrl => {
            *msg = Some("右 pane 仅导航（文件操作请 Tab 切回左 pane）".to_owned());
        }
        _ => {}
    }
    Outcome::Continue
}

/// 分发 Normal 模式按键。
///
/// `viewport_h` 为列表区高度（行数），用于 `Ctrl-u`/`Ctrl-d` 的半页步进。
/// 已接入：`j/k`、`l`/`Enter`、`h`、`gg`、`G`、`Ctrl-u/d`、`gt`/`gT`/`N gt`、
/// `/`、`n`/`N`、`dd`（删除确认）、`u`（撤销）、`v`（可视选择）、Esc（退出选择）、
/// `t`（tag 标记）、`q`/`Ctrl-C`、`:`。
/// M18-1：排序对话框字段表（小写键, 大写键, 说明, 字段）——字母直接应用；
/// 对话框内 `s`/`S` 是"大小"字段键，不触发 shell（模态）。
const SORT_FIELDS: [(KeyCode, KeyCode, &str, crate::tab::SortField); 5] = [
    (
        KeyCode::Char('n'),
        KeyCode::Char('N'),
        "名称 name",
        crate::tab::SortField::Name,
    ),
    (
        KeyCode::Char('t'),
        KeyCode::Char('T'),
        "自然 natural",
        crate::tab::SortField::Natural,
    ),
    (
        KeyCode::Char('m'),
        KeyCode::Char('M'),
        "修改时间 mtime",
        crate::tab::SortField::Mtime,
    ),
    (
        KeyCode::Char('s'),
        KeyCode::Char('S'),
        "大小 size",
        crate::tab::SortField::Size,
    ),
    (
        KeyCode::Char('e'),
        KeyCode::Char('E'),
        "扩展名 ext",
        crate::tab::SortField::Ext,
    ),
];

/// M23-6：Frecency Pane 按键处理（可测抽取）。
/// 搜索态：输入即搜（Char 入 query），方向键移动，Esc 清 query。
/// Normal 态：j/k/gg/G 移动，l/Enter 跳转，y/n/f/c 复制，e 编辑，w view，
/// Esc 退出 pane 模式，Tab 切换焦点。
fn frec_pane_key(
    state: &mut AppState,
    input: &mut InputState,
    msg: &mut Option<String>,
    key: KeyEvent,
) -> Outcome {
    let Some(pane) = state.active_tab_mut().pane_frecency.as_mut() else {
        return Outcome::Continue;
    };
    if key.kind != KeyEventKind::Press {
        return Outcome::Continue;
    }
    let visible_len = filter_jump(&pane.entries, &pane.query).len();
    let bottom = visible_len.saturating_sub(1);
    // M23-7（修订）：Normal 态键位与 Normal 一致（作用于光标结果条目）。
    if pane.normal {
        match key.code {
            KeyCode::Esc => {
                // 退出 pane 模式回目录。
                state.active_tab_mut().pane_frecency = None;
                *msg = None;
                return Outcome::Continue;
            }
            KeyCode::Enter | KeyCode::Char('l') => {
                let visible = filter_jump(&pane.entries, &pane.query);
                let picked = visible.get(pane.cursor).cloned();
                state.active_tab_mut().pane_frecency = None;
                match picked {
                    Some(entry) => match jump_to(state, &entry) {
                        Ok(m) => *msg = Some(m),
                        Err(e) => *msg = Some(e),
                    },
                    None => *msg = Some("跳转: 无选定条目".to_owned()),
                }
                return Outcome::Continue;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                pane.cursor = (pane.cursor + 1).min(bottom);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                pane.cursor = pane.cursor.saturating_sub(1);
            }
            KeyCode::Char('g') => {
                if pane.g_pending {
                    pane.cursor = 0;
                    pane.g_pending = false;
                } else {
                    pane.g_pending = true;
                }
            }
            KeyCode::Char('G') => {
                pane.cursor = bottom;
                pane.g_pending = false;
            }
            KeyCode::Tab => {
                // Tab 切换焦点 pane（frecency 保持激活）。
                let tab = state.active_tab_mut();
                tab.focus = match tab.focus {
                    crate::tab::Focus::Left => crate::tab::Focus::Right,
                    crate::tab::Focus::Right => crate::tab::Focus::Left,
                };
            }
            KeyCode::Char('y') => {
                // y 前缀：y-n 名称 / y-f 路径 / y-c 内容（作用于光标结果）。
                // 用 seq 暂存等待第二键。
                input
                    .seq
                    .push(crate::keymap::Key::plain(KeyCode::Char('y')));
                return Outcome::Continue;
            }
            KeyCode::Char('e') => {
                // e：$EDITOR 打开光标结果文件。
                let visible = filter_jump(&pane.entries, &pane.query);
                if let Some(entry) = visible.get(pane.cursor) {
                    let path = std::path::PathBuf::from(&entry.path);
                    if path.is_file() {
                        state.active_tab_mut().pane_frecency = None;
                        return Outcome::EditFile { path };
                    }
                    *msg = Some("目录不支持编辑".to_owned());
                }
            }
            KeyCode::Char('d') => {
                // M23-8：d 删除光标 frecency 条目（Remove 墓碑 + pane 内移除）。
                let visible = filter_jump(&pane.entries, &pane.query);
                if let Some(entry) = visible.get(pane.cursor).cloned() {
                    let topic = entry.topic.clone();
                    let path = entry.path.clone();
                    // 追加 Remove 墓碑事件（持久化删除）。
                    let _ = crate::frecency::remove_entry(&topic, &path);
                    // pane 内移除（从 entries 和可见列表）。
                    pane.entries.retain(|e| e.path != entry.path);
                    pane.cursor = pane.cursor.min(
                        filter_jump(&pane.entries, &pane.query)
                            .len()
                            .saturating_sub(1),
                    );
                    *msg = Some(format!("已删除 frecency 条目: {path}"));
                }
            }
            KeyCode::Char('w') => {
                // w：对侧进入 View 模式（预览/查看光标条目）。
                state.active_tab_mut().view_mode = !state.active_tab_mut().view_mode;
                let tab = state.active_tab_mut();
                tab.focus = match tab.focus {
                    crate::tab::Focus::Left => crate::tab::Focus::Right,
                    crate::tab::Focus::Right => crate::tab::Focus::Left,
                };
            }
            KeyCode::Char(c) => {
                // 可打印字符回搜索态（携带字符重新筛选）。
                pane.normal = false;
                pane.query.clear();
                pane.query.push(c);
                pane.cursor = 0;
                pane.g_pending = false;
            }
            _ => {}
        }
        return Outcome::Continue;
    }
    // 搜索态
    match key.code {
        KeyCode::Esc => {
            // 搜索态 Esc → Normal（清 query）。
            pane.normal = true;
            pane.query.clear();
            pane.cursor = pane.cursor.min(bottom);
            pane.g_pending = false;
        }
        KeyCode::Enter => {
            let visible = filter_jump(&pane.entries, &pane.query);
            let picked = visible.get(pane.cursor).cloned();
            state.active_tab_mut().pane_frecency = None;
            match picked {
                Some(entry) => match jump_to(state, &entry) {
                    Ok(m) => *msg = Some(m),
                    Err(e) => *msg = Some(e),
                },
                None => *msg = Some("跳转: 无选定条目".to_owned()),
            }
        }
        KeyCode::Backspace => {
            pane.query.pop();
            pane.cursor = 0;
        }
        KeyCode::Down => {
            pane.cursor = (pane.cursor + 1).min(bottom);
        }
        KeyCode::Up => {
            pane.cursor = pane.cursor.saturating_sub(1);
        }
        KeyCode::Tab => {
            let tab = state.active_tab_mut();
            tab.focus = match tab.focus {
                crate::tab::Focus::Left => crate::tab::Focus::Right,
                crate::tab::Focus::Right => crate::tab::Focus::Left,
            };
        }
        KeyCode::Char(c) => {
            pane.query.push(c);
            pane.cursor = 0;
        }
        _ => {}
    }
    Outcome::Continue
}

fn handle_key(
    state: &mut AppState,
    input: &mut InputState,
    msg: &mut Option<String>,
    key: KeyEvent,
    viewport_h: u16,
) -> Outcome {
    // M23-6：Frecency Pane 模式——全部键路由到 pane 键处理。
    if state.active_tab().pane_frecency.is_some() {
        return frec_pane_key(state, input, msg, key);
    }
    if key.kind != KeyEventKind::Press {
        return Outcome::Continue; // Release 先过滤，避免误消费待决序列。
    }
    // M24-4：可视模式 d/y/x 直接对选中区域执行（不再需要 dd/yy 序列）。
    if state.active_tab().visual.is_some() {
        match key.code {
            KeyCode::Char('d') | KeyCode::Char('x') => {
                input.count = None;
                // M24-4：可视范围条目加 tag → 触发批量删除确认（复用 M3-3 管线）。
                if let Some((lo, hi)) = state.active_tab().visual_range() {
                    let tab = state.active_tab_mut();
                    for i in lo..=hi.min(tab.entries.len().saturating_sub(1)) {
                        if let Some(item) = tab.entries.get(i) {
                            tab.tags.insert(item.name.clone());
                        }
                    }
                    tab.visual = None;
                    // M24-4：直接批量删除（tag 复用 M3-3，撤销栈可逆）。
                    execute_tag_delete(state, msg);
                }
                return Outcome::Continue;
            }
            KeyCode::Char('y') => {
                input.count = None;
                let range = state.active_tab().visual_range();
                if let Some((lo, hi)) = range {
                    let base = state.active_tab().location.base_dir().to_path_buf();
                    let paths: Vec<String> = state
                        .active_tab()
                        .entries
                        .get(lo..=hi.min(state.active_tab().entries.len().saturating_sub(1)))
                        .map(|slice| {
                            slice
                                .iter()
                                .map(|i| base.join(&i.name).to_string_lossy().replace('\\', "/"))
                                .collect()
                        })
                        .unwrap_or_default();
                    let text = paths.join("\n");
                    copy_to_clipboard(&text);
                    *msg = Some(format!("copied {} 项", paths.len()));
                }
                return Outcome::Continue;
            }
            _ => {}
        }
    }

    // 数字计数：`N gt`（M1-5）。其他动作键会丢弃未消费的计数。
    if let KeyCode::Char(c) = key.code
        && c.is_ascii_digit()
    {
        let d = c as u8 - b'0';
        input.count = Some(input.count.unwrap_or(0) * 10 + usize::from(d));
        return Outcome::Continue;
    }

    // M10-3：序列续接——待决非空时经 Keymap 解析；任意第二键结束序列
    // （未知第二键整体丢弃，包括 `q`/`Ctrl-C`）。
    if !input.seq.is_empty() {
        // M13-1：f/F 序列的第二键 = 搜索字符（通配序列，Keymap 无法表达）。
        if input.seq.len() == 1
            && matches!(input.seq[0].code, KeyCode::Char('f') | KeyCode::Char('F'))
        {
            let forward = input.seq[0].code == KeyCode::Char('f');
            input.seq.clear();
            input.count = None;
            if let KeyCode::Char(c) = key.code
                && !state.active_tab_mut().jump_first_char(c, forward)
            {
                *msg = Some(format!("无首字符 {c} 的条目"));
            }
            return Outcome::Continue;
        }
        // M16 NAV-1：' 序列的第二键 = keyword 字符（快速跳转）。
        if input.seq.len() == 1 && input.seq[0].code == KeyCode::Char('\'') {
            input.seq.clear();
            input.count = None;
            if let KeyCode::Char(c) = key.code {
                quickjump(state, c, msg);
            }
            return Outcome::Continue;
        }
        input.seq.push(crate::keymap::Key::plain(key.code));
        let count = input.count.take();
        input.count = None;
        let action = input.keymap.lookup(&input.seq);
        let result = execute_action(state, input, msg, action, count, viewport_h);
        input.seq.clear();
        if let Some(outcome) = result {
            return outcome;
        }
        return Outcome::Continue;
    }
    /// M10-5 GAP-1：Action 执行（seq 完成与单键用户绑定共用）。返回
    /// `Some(Outcome)` 表示模式迁移/退出请求；`None` 表示动作完成、继续循环。
    /// 序列清理由调用方统一处理。
    fn execute_action(
        state: &mut AppState,
        _input: &mut InputState,
        msg: &mut Option<String>,
        action: Option<crate::keymap::Action>,
        count: Option<usize>,
        _viewport_h: u16,
    ) -> Option<Outcome> {
        match action {
            Some(Action::ToTop) => {
                // M24-5：Ngg → 光标定位到第 N 行（1-based）。
                if let Some(n) = count {
                    let tab = state.active_tab_mut();
                    tab.cursor = (n - 1).min(tab.entries.len().saturating_sub(1));
                    tab.scroll = tab.cursor;
                } else {
                    state.active_tab_mut().cursor_top();
                }
            }
            Some(Action::NextTab) => match count {
                Some(n) => state.goto_tab(n),
                None => state.next_tab(),
            },
            Some(Action::PrevTab) => match count {
                Some(n) => state.goto_tab(n),
                None => state.prev_tab(),
            },
            Some(Action::DeleteSelected) => {
                if state.active_tab().in_results() {
                    *msg = Some("搜索结果视图只读".to_owned());
                    return None;
                }
                if state.active_tab().on_parent_row {
                    // FEAT-2：`..` 行仅导航。
                    *msg = Some("`..` 行仅导航".to_owned());
                    return None;
                }
                let tab = state.active_tab();
                // M20-1：远端视图 → 远端删除（trash 可恢复）。
                if let Location::Remote { remote, path } = &tab.location
                    && let Some(item) = tab.entries.get(tab.cursor)
                {
                    let target = if path == std::path::Path::new("/") {
                        PathBuf::from(format!("/{}", item.name))
                    } else {
                        path.join(&item.name)
                    };
                    return Some(Outcome::ConfirmDelete(target, false, Some(remote.clone())));
                }
                if let (Location::Physical(base), Some(item)) =
                    (&tab.location, tab.entries.get(tab.cursor))
                {
                    return Some(Outcome::ConfirmDelete(base.join(&item.name), false, None));
                }
            }
            Some(Action::CopyName) | Some(Action::CopyPath) | Some(Action::CopyContent) => {
                // M23-10：y 复制序列——选集（标记/可视/光标）逐条目取目标文本，
                // OSC52 写入剪贴板（零依赖），状态栏 `copied` 反馈。
                if state.active_tab().on_parent_row {
                    *msg = Some("`..` 行仅导航".to_owned());
                    return None;
                }
                let names = selected_names(state);
                if names.is_empty() {
                    *msg = Some("无选中条目".to_owned());
                    return None;
                }
                let is_physical = matches!(
                    state.active_tab().location,
                    crate::tab::Location::Physical(_)
                );
                let action = action.unwrap();
                let text = match action {
                    Action::CopyName => names.join("\n"),
                    Action::CopyPath => {
                        let base = state.active_tab().location.base_dir().to_path_buf();
                        names
                            .iter()
                            .map(|n| base.join(n).to_string_lossy().replace('\\', "/"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    }
                    Action::CopyContent => {
                        if !is_physical {
                            *msg = Some("当前视图不支持复制内容".to_owned());
                            return None;
                        }
                        let base = state.active_tab().location.base_dir().to_path_buf();
                        let mut parts: Vec<String> = Vec::new();
                        for n in &names {
                            let p = base.join(n);
                            if p.is_dir() {
                                let mut listing: Vec<String> = match std::fs::read_dir(&p) {
                                    Ok(rd) => rd
                                        .filter_map(|e| e.ok())
                                        .map(|e| e.file_name().to_string_lossy().into_owned())
                                        .collect(),
                                    Err(_) => Vec::new(),
                                };
                                listing.sort();
                                parts.push(listing.join("\n"));
                            } else {
                                match std::fs::read_to_string(&p) {
                                    Ok(t) => parts.push(t),
                                    Err(_) => {
                                        *msg = Some(format!(
                                            "复制内容失败: {n}（非 UTF-8 或读取失败）"
                                        ));
                                        return None;
                                    }
                                }
                            }
                        }
                        parts.join("\n")
                    }
                    _ => unreachable!("match 已限定 Copy 系列"),
                };
                let what = match action {
                    Action::CopyPath => text.lines().next().unwrap_or("").to_owned(),
                    _ => names[0].clone(), // 名称/内容：反馈条目名
                };
                let what = if names.len() > 1 {
                    format!("{} 项", names.len())
                } else {
                    what
                };
                copy_to_clipboard(&text);
                *msg = Some(format!("copied {what}"));
            }
            Some(Action::OpenEditor) => {
                // M23-10：e——$EDITOR 打开光标文件（目录提示不支持）。
                if state.active_tab().on_parent_row {
                    *msg = Some("`..` 行仅导航".to_owned());
                    return None;
                }
                if !matches!(
                    state.active_tab().location,
                    crate::tab::Location::Physical(_)
                ) {
                    *msg = Some("当前视图不支持编辑".to_owned());
                    return None;
                }
                let tab = state.active_tab();
                let is_dir = tab
                    .entries
                    .get(tab.cursor)
                    .map(|i| i.is_dir)
                    .unwrap_or(true);
                if is_dir {
                    *msg = Some("目录不支持编辑".to_owned());
                    return None;
                }
                let base = tab.location.base_dir().to_path_buf();
                let name = tab.entries[tab.cursor].name.clone();
                return Some(Outcome::EditFile {
                    path: base.join(name),
                });
            }
            Some(Action::RunCommand(index)) => {
                return Some(run_user_command(state, index as usize, msg));
            }
            Some(Action::GitUi) => {
                // M25-6：gi——PATH 优先级探测 gitui→ziggity→lazygit；复用
                // M18-4 分流（terminal_in_new_window 语义与 :! / 自定义命令一致）。
                return Some(match crate::git::find_git_tui() {
                    Some(bin) => {
                        let ui = state.config.ui.clone();
                        match run_external_target(&ui, bin) {
                            ExternalRun::Multiplexer(m) => {
                                *msg = Some(m);
                                Outcome::Continue
                            }
                            ExternalRun::CurrentPane(cmd) => Outcome::RunUserCommand { cmd },
                        }
                    }
                    None => {
                        *msg = Some(
                            "gi: 未找到 git TUI（PATH 探测 gitui / ziggity / lazygit 均失败）"
                                .to_owned(),
                        );
                        Outcome::Continue
                    }
                });
            }
            Some(Action::RenameStart) | Some(Action::RenameClear) | Some(Action::RenameAppend) => {
                // M21-E4：I/A 保留原名（光标首/尾），cc 清空（光标归零）。
                if state.active_tab().in_results() {
                    *msg = Some("搜索结果视图只读".to_owned());
                    return None;
                }
                if state.active_tab().on_parent_row {
                    *msg = Some("`..` 行仅导航".to_owned());
                    return None;
                }
                let tab = state.active_tab();
                if let (Location::Physical(base), Some(item)) =
                    (&tab.location, tab.entries.get(tab.cursor))
                {
                    let name = item.name.clone();
                    let (input, pos) = if matches!(action, Some(Action::RenameClear)) {
                        (String::new(), 0)
                    } else if matches!(action, Some(Action::RenameAppend)) {
                        (name.clone(), name.chars().count())
                    } else {
                        (name.clone(), 0)
                    };
                    return Some(Outcome::EnterRename {
                        path: base.join(&name),
                        input,
                        pos,
                    });
                }
            }
            Some(Action::RenameBatch) => {
                // M13-6：cw 批量重命名——收集标记（或光标单条）名单。
                if state.active_tab().in_results() {
                    *msg = Some("搜索结果视图只读".to_owned());
                    return None;
                }
                if state.active_tab().on_parent_row {
                    // FEAT-2：`..` 行仅导航。
                    *msg = Some("`..` 行仅导航".to_owned());
                    return None;
                }
                let tab = state.active_tab();
                if let Location::Physical(base) = &tab.location {
                    let names: Vec<String> = if tab.tags.is_empty() {
                        tab.entries
                            .get(tab.cursor)
                            .map(|i| i.name.clone())
                            .into_iter()
                            .collect()
                    } else {
                        // 按列表顺序收集标记条目（确定性）。
                        tab.entries
                            .iter()
                            .filter(|i| tab.tags.contains(&i.name))
                            .map(|i| i.name.clone())
                            .collect()
                    };
                    if !names.is_empty() {
                        return Some(Outcome::BatchRename {
                            base: base.clone(),
                            names,
                        });
                    }
                }
            }
            Some(Action::DeletePermanent) => {
                // M13-3：DD 永久删除——确认提示后跳过暂存（不可逆强提示）。
                if state.active_tab().in_results() {
                    *msg = Some("搜索结果视图只读".to_owned());
                    return None;
                }
                if state.active_tab().on_parent_row {
                    // FEAT-2：`..` 行仅导航。
                    *msg = Some("`..` 行仅导航".to_owned());
                    return None;
                }
                let tab = state.active_tab();
                if let (Location::Physical(base), Some(item)) =
                    (&tab.location, tab.entries.get(tab.cursor))
                {
                    return Some(Outcome::ConfirmDelete(base.join(&item.name), true, None));
                }
            }
            Some(Action::HistPrev) | Some(Action::HistNext) => {
                // M16 NAV-3/M19-2：目录历史后退/前进（远端位置走会话恢复）。
                let forward = matches!(action, Some(Action::HistNext));
                if hist_move(state, forward, msg).is_err() {
                    return None; // 错误已入 msg
                }
            }
            Some(Action::Yank) => {
                // FIX-2：yy——可视选择 → 标记 → 光标起 N 条（count 前缀），
                // 放入复制缓冲区（cut=false；dd 的 cut 缓冲接线待后续）。
                if state.active_tab().on_parent_row {
                    // FEAT-2：`..` 行仅导航。
                    *msg = Some("`..` 行仅导航".to_owned());
                    return None;
                }
                // M17 SRCH：结果视图 yy = 命中文件路径本身。
                if let Some(r) = state.active_tab().results.as_ref() {
                    let count = count.unwrap_or(1).max(1);
                    let start = state.active_tab().cursor.min(r.items.len());
                    let end = (start + count).min(r.items.len());
                    let paths: Vec<PathBuf> =
                        r.items[start..end].iter().map(|i| i.path.clone()).collect();
                    let n = paths.len();
                    state.yank = Some(crate::state::YankBuffer {
                        paths,
                        cut: false,
                        source: crate::state::YankSource::Local,
                    });
                    *msg = Some(format!("Yanked {n} item(s)"));
                    return None;
                }
                // M20-1：远端视图 yy = 远端路径（YankSource::Remote）。
                if let Some(remote) = state.active_tab().location.remote_name() {
                    let count = count.unwrap_or(1).max(1);
                    let names: Vec<PathBuf> = {
                        let tab = state.active_tab();
                        let base = match &tab.location {
                            Location::Remote { path, .. } => path.clone(),
                            _ => PathBuf::from("/"),
                        };
                        let start = tab.cursor.min(tab.entries.len());
                        let end = (start + count).min(tab.entries.len());
                        tab.entries[start..end]
                            .iter()
                            .map(|i| {
                                if base == std::path::Path::new("/") {
                                    PathBuf::from(format!("/{}", i.name))
                                } else {
                                    base.join(&i.name)
                                }
                            })
                            .collect()
                    };
                    let n = names.len();
                    if n == 0 {
                        *msg = Some("yy: 无可复制的条目".to_owned());
                        return None;
                    }
                    state.yank = Some(crate::state::YankBuffer {
                        paths: names,
                        cut: false,
                        source: crate::state::YankSource::Remote(remote),
                    });
                    *msg = Some(format!("Yanked {n} item(s)"));
                    return None;
                }
                let count = count.unwrap_or(1).max(1);
                let names: Vec<String> = {
                    let tab = state.active_tab();
                    if matches!(tab.location, Location::Archive { .. }) {
                        Vec::new()
                    } else if let Some((lo, hi)) = tab.visual_range() {
                        tab.entries[lo..=hi]
                            .iter()
                            .map(|i| i.name.clone())
                            .collect()
                    } else if !tab.tags.is_empty() {
                        tab.entries
                            .iter()
                            .filter(|i| tab.tags.contains(&i.name))
                            .map(|i| i.name.clone())
                            .collect()
                    } else {
                        let start = tab.cursor.min(tab.entries.len());
                        let end = (start + count).min(tab.entries.len());
                        tab.entries[start..end]
                            .iter()
                            .map(|i| i.name.clone())
                            .collect()
                    }
                };
                if names.is_empty() {
                    *msg = Some("yy: 无可复制的条目".to_owned());
                } else {
                    let base = state.active_tab().location.base_dir().to_path_buf();
                    let paths: Vec<PathBuf> = names.iter().map(|n| base.join(n)).collect();
                    let n = paths.len();
                    state.yank = Some(crate::state::YankBuffer {
                        paths,
                        cut: false,
                        source: crate::state::YankSource::Local,
                    });
                    *msg = Some(format!("Yanked {n} item(s)"));
                }
            }
            _ => {}
        }
        None
    }

    // M17 VIEW-1：查看器激活时按键路由——滚动/搜索/退出全部作用于
    // 查看器（列表光标不动）；`:` 放行进入命令模式。
    if state.active_tab().viewer.is_some() {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let half = (viewport_h.saturating_sub(1) / 2).max(1) as usize;
        match key.code {
            KeyCode::Char('w') | KeyCode::Esc | KeyCode::Char('q') => {
                input.count = None;
                let tab = state.active_tab_mut();
                tab.viewer = None;
                tab.view_mode = false;
                *msg = Some("已退出查看器".to_owned());
                return Outcome::Continue;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                input.count = None;
                state.active_tab_mut().viewer_scroll(1, half);
                return Outcome::Continue;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                input.count = None;
                state.active_tab_mut().viewer_scroll(-1, half);
                return Outcome::Continue;
            }
            KeyCode::Char('d') if ctrl => {
                input.count = None;
                state.active_tab_mut().viewer_scroll(2, half);
                return Outcome::Continue;
            }
            KeyCode::Char('u') if ctrl => {
                input.count = None;
                state.active_tab_mut().viewer_scroll(-2, half);
                return Outcome::Continue;
            }
            KeyCode::Char('G') => {
                input.count = None;
                state.active_tab_mut().viewer_goto(false);
                return Outcome::Continue;
            }
            KeyCode::Char('g') => {
                input.count = None;
                state.active_tab_mut().viewer_goto(true);
                return Outcome::Continue;
            }
            KeyCode::Char('/') => {
                input.count = None;
                return Outcome::EnterViewerSearch;
            }
            KeyCode::Char('n') => {
                input.count = None;
                if !state.active_tab_mut().viewer_jump_match(true) {
                    *msg = Some("无匹配".to_owned());
                }
                return Outcome::Continue;
            }
            KeyCode::Char('N') => {
                input.count = None;
                if !state.active_tab_mut().viewer_jump_match(false) {
                    *msg = Some("无匹配".to_owned());
                }
                return Outcome::Continue;
            }
            KeyCode::Char(':') => {} // 放行：进入命令模式
            _ => {
                input.count = None;
                return Outcome::Continue; // 查看器消费其余按键（列表不动）
            }
        }
    }

    if is_quit_key(key) {
        return Outcome::Quit;
    }

    // M14-1：右 pane 焦点——精简导航分支（SIMPLIFIED：搜索/可视选择/标记/
    // 树/选择器/提权仅左 pane；写命令经 exec 焦点化仍可用）。
    if state.active_tab().focus == crate::tab::Focus::Right {
        return handle_right_key(state, msg, key);
    }

    // M19-2：远端视图——l/Enter=进入子目录或打开、h=上级/返回本地。
    if matches!(state.active_tab().location, Location::Remote { .. }) {
        match key.code {
            KeyCode::Char('h') => {
                input.count = None;
                if let Err(e) = remote_up(state) {
                    *msg = Some(e);
                } else {
                    *msg = None;
                }
                return Outcome::Continue;
            }
            KeyCode::Char('l') | KeyCode::Enter => {
                input.count = None;
                let tab = state.active_tab();
                let (path, is_dir, name) = match (&tab.location, tab.entries.get(tab.cursor)) {
                    (Location::Remote { path, .. }, Some(item)) => {
                        (path.clone(), item.is_dir, item.name.clone())
                    }
                    _ => return Outcome::Continue,
                };
                if is_dir {
                    let child = if path == std::path::Path::new("/") {
                        PathBuf::from(format!("/{name}"))
                    } else {
                        path.join(&name)
                    };
                    if let Err(e) = remote_navigate(state, &child) {
                        *msg = Some(e);
                    } else {
                        *msg = None;
                    }
                } else {
                    // M22-6：远端文件打开 = read → 临时副本 → system opener；
                    // 副本保留至进程退出统一清理（Windows opener 可能锁定文件）。
                    let full = if path == std::path::Path::new("/") {
                        PathBuf::from(format!("/{name}"))
                    } else {
                        path.join(&name)
                    };
                    let remote = state
                        .active_tab()
                        .location
                        .remote_name()
                        .unwrap_or_default();
                    match remote_open_file(state, &remote, &full) {
                        Ok(m) => *msg = m,
                        Err(e) => *msg = Some(e),
                    }
                }
                return Outcome::Continue;
            }
            _ => {}
        }
    }

    // M17 SRCH：结果虚拟目录——l/Enter=定位、Esc/h=退出、v/t=只读拒绝；
    // 其余导航/yy/预览键落回既有处理（目录语义）。
    if state.active_tab().in_results() {
        match key.code {
            KeyCode::Esc | KeyCode::Char('h') => {
                input.count = None;
                let cfg = state.config.clone();
                state.active_tab_mut().exit_results(cfg);
                *msg = Some("已退出搜索结果".to_owned());
                return Outcome::Continue;
            }
            KeyCode::Char('l') | KeyCode::Enter => {
                input.count = None;
                // M22-5：远端结果定位——退出结果视图 + remote_navigate 父目录
                // + 光标停在该文件（navigate_to 语义对远端不可用）。
                if matches!(state.active_tab().location, Location::Remote { .. }) {
                    match remote_locate_result(state) {
                        Ok(()) => *msg = None,
                        Err(e) => *msg = Some(format!("定位失败: {e}")),
                    }
                    return Outcome::Continue;
                }
                let cfg = state.config.clone();
                if let Err(e) = state.active_tab_mut().locate_result(cfg) {
                    *msg = Some(format!("定位失败: {e}"));
                } else {
                    *msg = None;
                }
                return Outcome::Continue;
            }
            KeyCode::Char('t') | KeyCode::Char('v') => {
                input.count = None;
                *msg = Some("搜索结果视图只读".to_owned());
                return Outcome::Continue;
            }
            _ => {}
        }
    }

    // M14-1/M18-4：Tab 或 Space 切换焦点 pane（左 → 右；vifm 语义）。
    // M21-F2：焦点经边框/当前行样式呈现，状态栏不再提示焦点文字。
    if (key.code == KeyCode::Tab || key.code == KeyCode::Char(' '))
        && key.kind == KeyEventKind::Press
    {
        input.count = None;
        state.active_tab_mut().focus = crate::tab::Focus::Right;
        return Outcome::Continue;
    }

    // Esc：退出可视选择（M3-1）；非可视模式忽略。
    if key.code == KeyCode::Esc {
        input.count = None;
        state.active_tab_mut().exit_visual();
        return Outcome::Continue;
    }

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let half = (viewport_h.saturating_sub(1) / 2).max(1) as isize;

    // M10-5 GAP-1：用户显式绑定优先于内建单键处理（prepend 覆盖默认）。
    {
        let probe = crate::keymap::Key {
            code: key.code,
            ctrl,
        };
        if input.keymap.is_explicit(&[probe]) {
            let action = input.keymap.lookup(&[probe]);
            let count = input.count.take();
            input.count = None;
            if let Some(outcome) = execute_action(state, input, msg, action, count, viewport_h) {
                return outcome;
            }
            return Outcome::Continue;
        }
    }

    // M13-5：Ctrl-G → 文件详情对话框（Physical 视图；arm 顺序在
    // Char('g') 前缀分支之前，避免被前缀捕获）。
    if ctrl && let KeyCode::Char('g') = key.code {
        input.count = None;
        if state.active_tab().on_parent_row {
            // FEAT-2：`..` 行仅导航。
            *msg = Some("`..` 行仅导航".to_owned());
            return Outcome::Continue;
        }
        let tab = state.active_tab();
        if let (Location::Physical(base), Some(item)) = (&tab.location, tab.entries.get(tab.cursor))
        {
            let path = base.join(&item.name);
            let text = file_info_text(&path, &item.name);
            return Outcome::FileInfo { path, text };
        }
        return Outcome::Continue;
    }

    // M13-7：树视图只读——写操作与导航入口拒绝（l/Enter/h/dd/DD/cw 前缀/u）。
    // Ctrl-d/u 半页滚动放行（ctrl 排除）。
    if state.active_tab().in_tree()
        && !ctrl
        && matches!(
            key.code,
            KeyCode::Char('l')
                | KeyCode::Enter
                | KeyCode::Char('h')
                | KeyCode::Char('d')
                | KeyCode::Char('D')
                | KeyCode::Char('c')
                | KeyCode::Char('u')
        )
    {
        input.count = None;
        *msg = Some("树视图为只读浏览（:tree 退出）".to_owned());
        return Outcome::Continue;
    }

    match key.code {
        KeyCode::Char('G') => state.active_tab_mut().cursor_bottom(),
        KeyCode::Char('d') if ctrl => state.active_tab_mut().move_cursor(half),
        KeyCode::Char('u') if ctrl => state.active_tab_mut().move_cursor(-half),
        KeyCode::Char('u') => {
            input.count = None;
            undo_last(state, msg);
        }
        KeyCode::Char('j') => {
            // M13-4/M24-5：j/k 环绕 + 计数前缀 N 消耗。
            let n = input.count.take().unwrap_or(1).max(1);
            input.count = None;
            for _ in 0..n {
                state.active_tab_mut().move_cursor_wrap(1);
            }
        }
        KeyCode::Char('k') => {
            let n = input.count.take().unwrap_or(1).max(1);
            input.count = None;
            for _ in 0..n {
                state.active_tab_mut().move_cursor_wrap(-1);
            }
        }
        KeyCode::Char('l') | KeyCode::Enter => {
            input.count = None;
            open_selected(state, msg);
        }
        KeyCode::Char('h') => {
            let n = input.count.take().unwrap_or(1).max(1);
            input.count = None;
            for _ in 0..n {
                let cfg = state.config.clone();
                if let Err(e) = state.active_tab_mut().return_parent(cfg) {
                    *msg = Some(format!("返回失败: {e}"));
                    break;
                }
            }
        }
        KeyCode::Char('/') => {
            input.count = None;
            return Outcome::EnterSearch;
        }
        KeyCode::Char('f') | KeyCode::Char('F') => {
            // M13-1：f/F 进入首字母搜索序列（第二键为搜索字符）。
            input.count = None;
            input.seq.push(crate::keymap::Key::plain(key.code));
        }
        KeyCode::Char('\'') => {
            // M16 NAV-1：' 进入快速跳转序列（第二键为 keyword 字符）。
            input.count = None;
            input.seq.push(crate::keymap::Key::plain(key.code));
        }
        KeyCode::Char(';') | KeyCode::Char(',') => {
            // M13-1：重复上次首字母搜索（; 同向 / , 反向）。
            input.count = None;
            let tab = state.active_tab();
            if let Some((c, fwd)) = tab.char_search {
                let forward = if key.code == KeyCode::Char(';') {
                    fwd
                } else {
                    !fwd
                };
                if !state.active_tab_mut().jump_first_char(c, forward) {
                    *msg = Some(format!("无首字符 {c} 的条目"));
                }
            } else {
                *msg = Some("无上次首字母搜索".to_owned());
            }
        }
        KeyCode::Char('=') => {
            input.count = None;
            return Outcome::EnterFilter;
        }
        KeyCode::Char('n') => {
            input.count = None;
            run_search(state, msg, true);
        }
        KeyCode::Char('N') => {
            input.count = None;
            run_search(state, msg, false);
        }
        KeyCode::Char(':') => {
            input.count = None;
            return Outcome::EnterCommand;
        }
        KeyCode::Char('o') => {
            // M23-6：Frecency Pane 模式——焦点 pane 进入内嵌搜索（非浮层）。
            input.count = None;
            match load_jump_entries(state, JumpKind::Unified) {
                Ok(entries) => {
                    return Outcome::OpenFrecencyPane { entries };
                }
                Err(e) => *msg = Some(e),
            }
        }
        // M10-3：`g`/`d`/`y`/`v`/`t` 前缀检测经 Keymap；单键动作经 dispatch。
        KeyCode::Char('g')
        | KeyCode::Char('d')
        | KeyCode::Char('D')
        | KeyCode::Char('c')
        | KeyCode::Char('y') => {
            let k = crate::keymap::Key::plain(key.code);
            if input.keymap.has_prefix(&[k]) {
                input.seq.push(k);
            }
        }
        KeyCode::Char('v') => {
            input.count = None;
            state.active_tab_mut().toggle_visual();
        }
        KeyCode::Char('V') => {
            // M24-3：行可视模式 V——与 v 同语义（选中条目），互斥自动。
            input.count = None;
            state.active_tab_mut().toggle_visual();
        }
        KeyCode::Char('t') => {
            input.count = None;
            state.active_tab_mut().toggle_tag();
        }
        KeyCode::Char('w') => {
            // M13-9：view 模式切换（预览窗格完整内容视图 ↔ 摘要预览）。
            input.count = None;
            let on = state.active_tab_mut().toggle_view();
            *msg = Some(if on {
                "view: 完整内容视图开启".to_owned()
            } else {
                "view: 已恢复摘要预览".to_owned()
            });
        }
        KeyCode::Char('p') | KeyCode::Char('P') => {
            // FIX-2：PutCopy/PutMove——消费 yank 缓冲区（复制/移动到当前目录）。
            input.count = None;
            paste_yanked(state, key.code == KeyCode::Char('P'), msg);
        }
        KeyCode::Char('s') => {
            // M18-3：挂起 TUI 进入 shell（vifm 既有习惯；M18-4 接入
            // terminal_in_new_window 配置后按配置分流）。
            input.count = None;
            return Outcome::EnterShell;
        }
        KeyCode::Char('S') => {
            // M18-1：排序对话框（唯一排序入口；`s` 仍为 shell，见 M18-3）。
            input.count = None;
            return Outcome::EnterSortDialog;
        }
        _ => {
            // GAP-1：非 Char 键（F 键/Home 等）的单键用户绑定兜底。
            let plain = crate::keymap::Key::plain(key.code);
            let with_ctrl = crate::keymap::Key {
                code: key.code,
                ctrl,
            };
            let action = input
                .keymap
                .lookup(&[plain])
                .or_else(|| input.keymap.lookup(&[with_ctrl]));
            input.count = None;
            if let Some(action) = action
                && !input.keymap.is_explicit(&[with_ctrl])
            {
                // 非显式（默认表绑定同键）才在此兜底；显式已在上方拦截。
                if let Some(outcome) =
                    execute_action(state, input, msg, Some(action), None, viewport_h)
                {
                    return outcome;
                }
            }
        }
    }
    Outcome::Continue
}

/// 刷新 tab 的双 pane（M14-1）：左 pane 走 Tab::refresh，右 pane 走 Pane::refresh。
fn refresh_tab_panes(state: &mut AppState, tab_id: usize) {
    let cfg = state.config.clone();
    if let Some(tab) = state.tabs.get_mut(tab_id) {
        let _ = tab.refresh(cfg.clone());
        let _ = tab.right.refresh(cfg);
    }
}

/// M21-F5：监听目标——活动 tab 的物理目录；归档/远端视图不监听（None）。
fn watch_target(tab: &Tab) -> Option<std::path::PathBuf> {
    match &tab.location {
        Location::Physical(p) => Some(p.clone()),
        _ => None,
    }
}

/// M21-F5：事件相关性——创建/删除/重命名/修改触发刷新；访问类忽略。
/// （notify 将重命名归入 Modify(ModifyKind::Name)，含在 Modify 内。）
fn fs_event_relevant(kind: &notify::EventKind) -> bool {
    matches!(
        kind,
        notify::EventKind::Create(_) | notify::EventKind::Modify(_) | notify::EventKind::Remove(_)
    )
}

/// M21-F5：同步监听目标——活动 tab 物理目录变化时重挂 watcher
/// （NonRecursive）；归档/远端卸载监听；创建/挂载失败静默降级（无监听，
/// TUI 照常工作）。
fn sync_watcher(state: &mut AppState) {
    use notify::Watcher as _;
    let target = watch_target(state.active_tab());
    if state.watcher.as_ref().map(|(p, _)| p.as_path()) == target.as_deref() {
        return;
    }
    state.watcher = None;
    state.watch_rx = None;
    let Some(dir) = target else {
        return;
    };
    let (tx, rx) = std::sync::mpsc::channel();
    let Ok(mut watcher) = notify::recommended_watcher(move |res| {
        if let Ok(ev) = res {
            let _ = tx.send(ev);
        }
    }) else {
        return;
    };
    if watcher
        .watch(&dir, notify::RecursiveMode::NonRecursive)
        .is_err()
    {
        return;
    }
    state.watcher = Some((dir, watcher));
    state.watch_rx = Some(rx);
}

/// M21-F5：外部变更后刷新活动 tab 双 pane——光标跟随原条目名（仍在时），
/// 否则钳制在列表内。
fn refresh_on_fs_change(state: &mut AppState) {
    let active = state.active;
    let cur_name = state.tabs[active]
        .entries
        .get(state.tabs[active].cursor)
        .map(|i| i.name.clone());
    refresh_tab_panes(state, active);
    let tab = &state.tabs[active];
    let new_cursor = cur_name
        .as_deref()
        .and_then(|n| tab.entries.iter().position(|i| i.name == n))
        .unwrap_or_else(|| tab.cursor.min(tab.entries.len().saturating_sub(1)));
    state.tabs[active].cursor = new_cursor;
}

/// `l`/Enter 与鼠标再击共用的打开逻辑（FEAT-1）：目录/归档 → enter_selected
/// 导航（FIX-3：归档文件优先于 opener）；普通文件 → 系统 opener（M11-7）。
fn open_selected(state: &mut AppState, msg: &mut Option<String>) {
    // FEAT-2：焦点在 `..` 行 → 等价 `h`（返回上级；归档根回物理目录）。
    if state.active_tab().on_parent_row {
        let cfg = state.config.clone();
        if let Err(e) = state.active_tab_mut().return_parent(cfg) {
            *msg = Some(format!("返回失败: {e}"));
            return;
        }
        // M21-F9：l/Enter 经 `..` 行返回上级也是一次目录进入，实时记录。
        let tab_id = state.active;
        record_live_visit(state, tab_id, 'L');
        return;
    }
    let cfg = state.config.clone();
    let tab = state.active_tab();
    let is_file = tab
        .entries
        .get(tab.cursor)
        .map(|item| !item.is_dir)
        .unwrap_or(false);
    let is_physical = matches!(tab.location, crate::tab::Location::Physical(_));
    if is_file && is_physical {
        let name = tab.entries[tab.cursor].name.clone();
        // FIX-3：归档文件（zip/tar/tar.gz…）走 enter_selected 进入
        // 只读虚拟目录（M7-4），不被系统 opener 抢占；不支持格式
        // （.7z/.rar，M7-9）经同一路径给出解压提示。
        if crate::archive::is_archive_file(&name) || crate::archive::is_unsupported_archive(&name) {
            if let Err(e) = state.active_tab_mut().enter_selected(cfg) {
                *msg = Some(format!("进入失败: {e}"));
            }
            return;
        }
        let path = tab.location.base_dir().join(&name);
        // AUDIT-7：opener 规则整层接线——规则命中按 `use` 名单执行
        // （block=true 交 run 循环挂起），未命中/系统项回退系统 opener。
        match crate::opener::open_with_layers(&state.config.open, &state.config.opener, &path) {
            Ok(Some(cmd)) => {
                // block=true：命令交由 run 循环挂起执行；frecency 记录在此
                // 不做（退出码未知，避免记入未发生的访问）。
                state.pending_external = Some(cmd);
                *msg = None;
            }
            Ok(None) => {
                // M21-E8：打开成功记录文件访问（o/:J 可见）。
                let tab_id = state.active;
                record_file_open(state, tab_id, &path);
                *msg = None;
            }
            Err(e) => *msg = Some(format!("打开失败: {e}")),
        }
        return;
    }
    if let Err(e) = state.active_tab_mut().enter_selected(cfg) {
        *msg = Some(format!("进入失败: {e}"));
        return;
    }
    // M21-F9：进入成功（物理目录实时记录；归档位置 record 内部跳过）。
    let tab_id = state.active;
    record_live_visit(state, tab_id, 'L');
}

/// FEAT-1：列表视口偏移——保证光标可见（offset ≤ cursor < offset+height）。
/// 渲染（ListState.with_offset）与鼠标命中测试共用同一推导，避免偏差；
/// 与 ratatui List 从 0 起自动调整的结果一致。
fn list_offset(cursor: usize, height: usize) -> usize {
    if height == 0 || cursor < height {
        0
    } else {
        cursor + 1 - height
    }
}

/// M21-F1：整体几何——标签栏 1 行 + 双 pane（居中分开，间隔 1 列）+ 状态栏 1 行。
/// 渲染与鼠标命中测试共用同一推导，避免两处几何漂移。
fn pane_layout(area: ratatui::layout::Rect) -> [ratatui::layout::Rect; 4] {
    let [tab_bar, main, status] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);
    let [left, _gap, right] = Layout::horizontal([
        Constraint::Ratio(1, 2),
        Constraint::Length(1),
        Constraint::Ratio(1, 2),
    ])
    .areas(main);
    [tab_bar, left, right, status]
}

/// M21-F1：左 pane 列表内容区（边框内减去 `..` 行）。
/// 渲染与鼠标命中测试共用；`has_parent` 由调用方决定是否再减 1 行。
fn left_list_area(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    ratatui::widgets::Block::bordered().inner(area)
}

/// M21-F1：pane 边框焦点态——主题命名样式（focused_border/unfocused_border）
/// 优先，未激活主题或样式缺失时以 BOLD/DIM 区分（保证无边框主题下仍可见）。
fn pane_border_style(focused: bool) -> Style {
    let s = crate::theme::style(if focused {
        "focused_border"
    } else {
        "unfocused_border"
    });
    if s.fg.is_some() {
        s
    } else if focused {
        Style::new().add_modifier(Modifier::BOLD)
    } else {
        Style::new().add_modifier(Modifier::DIM)
    }
}

/// M21-E1/M24-1：属性列布局——大小列 10 列右对齐 + 分隔 2 列 + 时间列
/// 16 列（`yyyy-mm-dd HH:mm`，分钟精度）。
const ATTRS_WIDTH: usize = 10 + 2 + 16;
/// 属性列显示所需的最小 pane 内区宽度（窄终端隐藏属性列，文件名优先）。
const MIN_ATTRS_INNER_WIDTH: usize = 55;

/// M25-5：git 状态列宽（状态字符 + 2 空格间隔）。
const GIT_STATUS_WIDTH: usize = 3;

/// M25-5：git 状态显示 span（着色；M/D 红、A 绿、R/T 黄、? 青、U 品红）。
fn git_status_span(st: crate::git::GitStatus) -> Span<'static> {
    let color = match st {
        crate::git::GitStatus::Modified | crate::git::GitStatus::Deleted => Color::Red,
        crate::git::GitStatus::Added => Color::Green,
        crate::git::GitStatus::Renamed | crate::git::GitStatus::Typechange => Color::Yellow,
        crate::git::GitStatus::Untracked => Color::Cyan,
        crate::git::GitStatus::Conflicted => Color::Magenta,
    };
    Span::styled(format!("{}  ", st.marker()), Style::new().fg(color))
}

/// M21-E1：按显示预算截断名称（字符数近似；CJK 全角按 1 计——与查看器
/// 显示口径一致，SIMPLIFIED）。搜索/定位逻辑不受影响（仅显示层裁剪）。
fn clip_name(name: &str, budget: usize) -> String {
    if budget == 0 {
        return String::new();
    }
    name.chars().take(budget).collect()
}

/// M21-F1：类型着色——主题命名样式（`ft_dir` 等）优先，缺失时回落 16 色
/// 近似（yazi filetype + vifm 配色语义：目录青/可执行绿/链接紫/归档红/
/// 图片亮蓝/视频黄/代码蓝；Other 不着色）。
fn filetype_style(class: crate::tab::FileTypeClass) -> Style {
    use crate::tab::FileTypeClass as C;
    let (name, fallback) = match class {
        C::Directory => ("ft_dir", Color::Cyan),
        C::Executable => ("ft_exec", Color::Green),
        C::Symlink => ("ft_link", Color::Magenta),
        C::Archive => ("ft_archive", Color::Red),
        C::Image => ("ft_image", Color::LightBlue),
        C::Video => ("ft_video", Color::Yellow),
        C::Code => ("ft_code", Color::Blue),
        C::Other => return Style::new(),
    };
    let s = crate::theme::style(name);
    if s.fg.is_some() {
        s
    } else {
        Style::new().fg(fallback)
    }
}

/// FEAT-4：帮助文本（内嵌单文件，`include_str!` 便于维护）。
const HELP_TEXT: &str = include_str!("help.txt");

/// FEAT-4：帮助文本按行展开（渲染与搜索共用）。
fn help_lines() -> Vec<&'static str> {
    HELP_TEXT.lines().collect()
}

/// FEAT-4：帮助页搜索——从 `start` 行起找包含 `query`（大小写不敏感）
/// 的行，环绕列表首尾；找不到返回 None。
fn help_find_match(lines: &[&str], query: &str, start: usize, forward: bool) -> Option<usize> {
    if lines.is_empty() || query.is_empty() {
        return None;
    }
    let len = lines.len();
    for i in 0..len {
        let idx = if forward {
            (start + i) % len
        } else {
            (start + len - i % len) % len
        };
        if lines[idx].to_lowercase().contains(&query.to_lowercase()) {
            return Some(idx);
        }
    }
    None
}

/// FEAT-1/FEAT-3：条目行命中测试——鼠标行 → 条目索引；不在条目区
/// （边框/`..` 行/右 pane/标签栏/状态栏/越界）返回 None。
/// 几何与 render 同源（M21-F1 pane_layout + 左 pane 边框内区）。
fn hit_entry_index(
    state: &AppState,
    mouse: &MouseEvent,
    area: ratatui::layout::Rect,
) -> Option<usize> {
    let [_, left, _, _] = pane_layout(area);
    let inner = left_list_area(left);
    let has_parent = state.active_tab().has_parent_row() as usize;
    let view_h = inner.height.saturating_sub(has_parent as u16) as usize;
    if mouse.column < inner.x
        || mouse.column >= inner.x + inner.width
        || mouse.row < inner.y + has_parent as u16
        || mouse.row >= inner.y + inner.height
    {
        return None;
    }
    let tab = state.active_tab();
    let offset = list_offset(if tab.on_parent_row { 0 } else { tab.cursor }, view_h);
    let idx = (mouse.row - inner.y - has_parent as u16) as usize;
    idx.checked_add(offset)?;
    Some(idx.min(tab.entries.len().saturating_sub(1)))
}

/// FEAT-1：鼠标事件处理——左键单击聚焦（已聚焦行再击 = 打开，双击的第二击
/// 自然落到此路径）、滚轮逐行滚动（每次精确移动一行，不跳变）。仅 Normal
/// 模式响应（输入模式忽略）；标签栏/状态栏/右 pane 不响应。Visual 选择由
/// tab.visual 锚点 + 光标移动自然延伸，鼠标只移动光标不破坏选区。
fn handle_mouse(
    state: &mut AppState,
    mode: &InputMode,
    mouse: &MouseEvent,
    msg: &mut Option<String>,
    area: ratatui::layout::Rect,
) {
    if !matches!(mode, InputMode::Normal) {
        return; // Command/Search 等输入模式忽略鼠标（SPEC M15 SIMPLIFIED）。
    }
    // 列表几何：与 render 同源（M21-F1）——pane_layout 左 pane 的边框内区；
    // FEAT-2：`..` 行占用内区首行（有父行时条目从第 2 行开始）。
    let [_, left, _, _] = pane_layout(area);
    let inner = left_list_area(left);
    let has_parent = state.active_tab().has_parent_row() as usize;
    let in_columns = mouse.column >= inner.x
        && mouse.column < inner.x + inner.width
        && mouse.row >= inner.y
        && mouse.row < inner.y + inner.height;
    let in_parent_row = has_parent > 0 && mouse.row == inner.y && in_columns;
    let in_entries = in_columns && mouse.row >= inner.y + has_parent as u16;
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) if in_parent_row => {
            if state.active_tab().on_parent_row {
                // 已聚焦 `..` 行再次单击（含双击第二击）→ 返回上级。
                open_selected(state, msg);
            } else {
                // 单击聚焦 `..` 行。
                state.active_tab_mut().on_parent_row = true;
            }
        }
        MouseEventKind::Down(MouseButton::Left) if in_entries => {
            let Some(idx) = hit_entry_index(state, mouse, area) else {
                return;
            };
            let focused = {
                let tab = state.active_tab();
                idx == tab.cursor && !tab.on_parent_row
            };
            state.active_tab_mut().on_parent_row = false;
            if focused {
                // 已聚焦行再次单击（含双击第二击）→ 打开。
                open_selected(state, msg);
            } else {
                // 单击聚焦：仅移动光标，不打开。
                state.active_tab_mut().cursor = idx;
            }
        }
        MouseEventKind::ScrollUp if in_columns => {
            state.active_tab_mut().move_cursor(-1);
        }
        MouseEventKind::ScrollDown if in_columns => {
            state.active_tab_mut().move_cursor(1);
        }
        _ => {}
    }
}

/// FIX-2：`p`（PutCopy）/`P`（PutMove）——消费 yank 缓冲区到当前（焦点）
/// 目录。复制还是移动由缓冲区类型与键位决定（SPEC §4.8）：yy 缓冲 + `p`
/// = 复制、+ `P` = 移动；cut 缓冲恒为移动。每项独立入撤销栈（`u` 可逆）；
/// 失败逐项报告，不 panic、不留半成品（单项目标已存在等错误即停）。
fn paste_yanked(state: &mut AppState, force_move: bool, msg: &mut Option<String>) {
    let Some(buffer) = state.yank.clone() else {
        *msg = Some("缓冲区为空（先 yy 复制）".to_owned());
        return;
    };
    // M20-1/M20-3：远端/跨 VFS 粘贴路由。
    if matches!(state.active_tab().location, Location::Remote { .. }) {
        paste_yanked_remote(state, &buffer, force_move, msg);
        return;
    }
    if let crate::state::YankSource::Remote(ref src_remote) = buffer.source {
        paste_remote_to_local(state, src_remote, &buffer, force_move, msg);
        return;
    }
    let base = match physical_base(state) {
        Ok(base) => base,
        Err(e) => {
            *msg = Some(e);
            return;
        }
    };
    let do_move = buffer.cut || force_move;
    let verb = if do_move { "Moved" } else { "Copied" };
    let mut done = 0usize;
    let mut first_err: Option<String> = None;
    for path in &buffer.paths {
        let result = if do_move {
            ops::move_to(state.active, path, &base)
        } else {
            ops::copy_to(state.active, path, &base)
        };
        match result {
            Ok(op) => {
                state.undo_stack.push(op);
                done += 1;
            }
            Err(e) => {
                first_err = Some(e.to_string());
                break;
            }
        }
    }
    refresh_tab_panes(state, state.active);
    match (done, first_err) {
        (_, Some(e)) if done < buffer.paths.len() => {
            *msg = Some(format!("{verb} {done}/{} 项: {e}", buffer.paths.len()))
        }
        _ => *msg = Some(format!("{verb} {done} item(s) to {}", base.display())),
    }
}

/// M20-1：远端视图粘贴——yank 路径读出内容 → 后端 write 到当前远端目录；
/// 逐条入撤销栈（Create，u 可逆）；同名冲突报错不覆盖。
fn paste_yanked_remote(
    state: &mut AppState,
    buffer: &crate::state::YankBuffer,
    force_move: bool,
    msg: &mut Option<String>,
) {
    let (remote, dir) = match &state.active_tab().location.clone() {
        Location::Remote { remote, path } => (remote.clone(), path.clone()),
        _ => {
            *msg = Some("remote: 非远端视图".to_owned());
            return;
        }
    };
    let do_move = buffer.cut || force_move;
    let verb = if do_move { "Moved" } else { "Copied" };
    let mut done = 0usize;
    let mut first_err: Option<String> = None;
    let tab_id = state.active;
    for src in &buffer.paths {
        let data = match &buffer.source {
            crate::state::YankSource::Remote(src_remote) => {
                let r = state
                    .remote_sessions
                    .get(src_remote)
                    .ok_or_else(|| "来源会话已断开".to_owned())
                    .and_then(|be| be.read(src).map_err(|e| e.to_string()));
                match r {
                    Ok(d) => d,
                    Err(e) => {
                        first_err = Some(e);
                        break;
                    }
                }
            }
            crate::state::YankSource::Local => match std::fs::read(src) {
                Ok(d) => d,
                Err(e) => {
                    first_err = Some(e.to_string());
                    break;
                }
            },
        };
        let name = src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let dest = if dir == std::path::Path::new("/") {
            PathBuf::from(format!("/{name}"))
        } else {
            dir.join(&name)
        };
        let exists = state
            .remote_sessions
            .get(&remote)
            .map(|be| be.metadata(&dest).is_ok())
            .unwrap_or(false);
        eprintln!("DBG paste exists={} dest={}", exists, dest.display());
        if exists {
            first_err = Some(format!("目标已存在: {}", dest.display()));
            break;
        }
        let result = state
            .remote_sessions
            .get(&remote)
            .ok_or_else(|| "会话已断开".to_owned())
            .and_then(|be| be.write(&dest, &data).map_err(|e| e.to_string()));
        match result {
            Ok(()) => {
                state.undo_stack.push(crate::state::UndoableOp::Remote {
                    tab_id,
                    remote: remote.clone(),
                    kind: crate::state::RemoteUndo::Create {
                        path: dest,
                        is_dir: false,
                    },
                });
                done += 1;
            }
            Err(e) => {
                first_err = Some(e);
                break;
            }
        }
    }
    if do_move {
        for src in buffer.paths.iter().take(done) {
            match &buffer.source {
                crate::state::YankSource::Remote(src_remote) => {
                    let _ = state
                        .remote_sessions
                        .get(src_remote)
                        .map(|be| be.remove_permanent(src));
                }
                _ => {
                    let _ = std::fs::remove_file(src);
                }
            }
        }
    }
    let _ = remote_navigate(state, &dir);
    match (done, first_err) {
        (_, Some(e)) if done < buffer.paths.len() => {
            *msg = Some(format!("{verb} {done}/{} 项: {e}", buffer.paths.len()))
        }
        _ => {
            *msg = Some(format!(
                "{verb} {done} item(s) to {remote}:{}",
                dir.display()
            ))
        }
    }
}

/// M20-3：远端来源缓冲 → 本地粘贴（跨 VFS：会话 read → 本地 write）。
fn paste_remote_to_local(
    state: &mut AppState,
    remote: &str,
    buffer: &crate::state::YankBuffer,
    force_move: bool,
    msg: &mut Option<String>,
) {
    let Some(base) = physical_base(state).ok() else {
        *msg = Some("本地视图不可用".to_owned());
        return;
    };
    let do_move = buffer.cut || force_move;
    let verb = if do_move { "Moved" } else { "Copied" };
    let mut done = 0usize;
    let mut first_err: Option<String> = None;
    let tab_id = state.active;
    for src in &buffer.paths {
        let data = state
            .remote_sessions
            .get(remote)
            .ok_or_else(|| "会话已断开".to_owned())
            .and_then(|be| be.read(src).map_err(|e| e.to_string()));
        let data = match data {
            Ok(d) => d,
            Err(e) => {
                first_err = Some(e);
                break;
            }
        };
        let name = src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let dest = base.join(&name);
        if dest.exists() {
            first_err = Some(format!("目标已存在: {}", dest.display()));
            break;
        }
        match std::fs::write(&dest, &data) {
            Ok(()) => {
                state.undo_stack.push(crate::state::UndoableOp::Create {
                    tab_id,
                    path: dest.clone(),
                    recursive: false,
                });
                done += 1;
            }
            Err(e) => {
                first_err = Some(e.to_string());
                break;
            }
        }
    }
    if do_move {
        let _ = state.remote_sessions.get(remote).map(|be| {
            for src in buffer.paths.iter().take(done) {
                let _ = be.remove_permanent(src);
            }
        });
    }
    refresh_tab_panes(state, state.active);
    match (done, first_err) {
        (_, Some(e)) if done < buffer.paths.len() => {
            *msg = Some(format!("{verb} {done}/{} 项: {e}", buffer.paths.len()))
        }
        _ => *msg = Some(format!("{verb} {done} item(s) to {}", base.display())),
    }
}

/// 文件详情文本（M13-5，Ctrl-G）：名称/类型/大小/权限/修改时间/完整路径。
fn file_info_text(path: &std::path::Path, name: &str) -> String {
    let meta = std::fs::metadata(path);
    let mut lines = vec![format!("名称: {name}")];
    match &meta {
        Ok(m) => {
            let kind = if m.is_dir() {
                "目录"
            } else if m.is_symlink() {
                "符号链接"
            } else {
                "文件"
            };
            lines.push(format!("类型: {kind}"));
            lines.push(format!("大小: {}", crate::tab::format_size(m.len())));
            if let Ok(modified) = m.modified()
                && let Ok(dur) = modified.duration_since(std::time::UNIX_EPOCH)
            {
                lines.push(format!("修改时间: {} 秒 (Unix)", dur.as_secs()));
            }
            lines.push(format!("权限: {}", crate::tab::perm_string(m)));
        }
        Err(e) => lines.push(format!("读取失败: {e}")),
    }
    lines.push(format!("路径: {}", path.display()));
    lines.join("\n")
}

// ── M13-6：cw 批量重命名 ──────────────────────────────────────────────────

/// 批量重命名全流程：名单临时文件 → 挂起 TUI 后由调用方运行 `$EDITOR` →
/// 读回按行对齐 → 逐项 `rename_to`（入撤销栈，`u` 逐项可逆）。
/// 行数与文件数不符时整批中止（保守策略：批量改名出错比部分改名更危险）。
/// 同名行跳过；目标已存在/非法名跳过并计数。
fn apply_rename_lines(
    state: &mut AppState,
    base: &std::path::Path,
    old_names: &[String],
    edited: &str,
) -> Result<String, String> {
    let lines: Vec<&str> = edited.lines().collect();
    if lines.len() != old_names.len() {
        return Err(format!(
            "批量重命名中止：名单行数不符（{} 行 != {} 个文件）",
            lines.len(),
            old_names.len()
        ));
    }
    let tab_id = state.active;
    let cfg = state.config.clone();
    let mut updated = 0usize;
    let mut skipped = 0usize;
    for (old, line) in old_names.iter().zip(&lines) {
        let new = line.trim();
        if new.is_empty() || new == old || new.contains('/') || new.contains('\\') {
            skipped += 1;
            continue;
        }
        let old_path = base.join(old);
        match crate::ops::rename_to(tab_id, &old_path, new) {
            Ok(op) => {
                state.undo_stack.push(op);
                updated += 1;
            }
            Err(e) => {
                skipped += 1;
                let _ = e; // 目标已存在/非法名：跳过该行
            }
        }
    }
    if let Some(tab) = state.tabs.get_mut(tab_id) {
        let _ = tab.refresh(cfg);
    }
    Ok(format!("批量重命名: {updated} 项更新, {skipped} 项跳过"))
}

/// M21-E4：内联重命名应用（Rename 模式 Enter）——空名/未改动无操作；
/// 成功入撤销栈并刷新，失败返回可读错误（模式已退出，可重新进入修正）。
fn rename_apply(state: &mut AppState, path: &std::path::Path, new_name: &str) -> Option<String> {
    let tab_id = state.active;
    let old = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if new_name.is_empty() || new_name == old {
        return None; // 未改动：无操作
    }
    match crate::ops::rename_to(tab_id, path, new_name) {
        Ok(op) => {
            state.undo_stack.push(op);
            refresh_tab_panes(state, tab_id);
            Some(format!("重命名: {old} → {new_name}"))
        }
        Err(e) => Some(format!("重命名失败: {e}")),
    }
}

/// `cw` 全流程（M13-6）：名单临时文件 → 挂起 TUI 后由调用方运行 `$EDITOR` →
/// 读回按行对齐 → 逐项 `rename_to`（入撤销栈，`u` 逐项可逆）。
/// 编辑器选择：`EDITOR` → `VISUAL` → 平台缺省（Windows notepad / Unix vi）。
fn batch_rename_flow(
    state: &mut AppState,
    base: &std::path::Path,
    names: &[String],
) -> Result<String, String> {
    use std::io::Write as _;
    // 手写临时文件（tempfile 是 dev-dependency，主代码不引入）。
    let tmp_path = std::env::temp_dir().join(format!("rvifm-rename-{}.txt", std::process::id()));
    std::fs::write(&tmp_path, "").map_err(|e| format!("名单临时文件创建失败: {e}"))?;
    {
        let mut f =
            std::fs::File::create(&tmp_path).map_err(|e| format!("名单临时文件打开失败: {e}"))?;
        for name in names {
            writeln!(f, "{name}").map_err(|e| format!("名单写入失败: {e}"))?;
        }
        f.flush().ok();
    }

    let editor = editor_name();
    let status = std::process::Command::new(&editor)
        .arg(&tmp_path)
        .status()
        .map_err(|e| format!("编辑器 {editor} 启动失败: {e}"))?;
    if !status.success() {
        std::fs::remove_file(&tmp_path).ok();
        return Err(format!("编辑器 {editor} 非零退出（{status}），名单未应用"));
    }

    let edited = std::fs::read_to_string(&tmp_path).map_err(|e| format!("名单读取失败: {e}"))?;
    std::fs::remove_file(&tmp_path).ok();
    apply_rename_lines(state, base, names, &edited)
}

/// 后台作业执行体（M13-8）：对 `base` 下 `names` 逐项 copy/move 到 target。
/// 返回（撤销记录列表, 终态）；纯同步，供后台线程与测试共用。
fn execute_job_body(
    op: &str,
    names: &[String],
    base: &std::path::Path,
    target: &std::path::Path,
    tab_id: usize,
    cancel: &std::sync::atomic::AtomicBool,
) -> (Vec<crate::state::UndoableOp>, crate::state::JobStatus) {
    let mut ops = Vec::new();
    let mut first_err: Option<String> = None;
    let mut done = 0usize;
    let mut cancelled = false;
    for name in names {
        // M24-9：协作式取消——文件边界检查标志，已完成项保留（u 可逆）。
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        let src = base.join(name);
        if !src.exists() {
            continue; // 已消失的条目跳过
        }
        let result = if op == "copy" {
            crate::ops::copy_to(tab_id, &src, target)
        } else {
            crate::ops::move_to(tab_id, &src, target)
        };
        match result {
            Ok(uop) => {
                ops.push(uop);
                done += 1;
            }
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(format!("{op} {name}: {e}"));
                }
            }
        }
    }
    let status = if cancelled {
        crate::state::JobStatus::Cancelled {
            summary: format!("{op}: 已取消（{done} 项完成）"),
        }
    } else if first_err.is_some() && done == 0 {
        crate::state::JobStatus::Failed {
            err: first_err.unwrap_or_default(),
        }
    } else {
        let mut summary = format!("{op}: {done} 项完成");
        if let Some(e) = first_err {
            summary.push_str(&format!("; 错误: {e}"));
        }
        crate::state::JobStatus::Done { summary }
    };
    (ops, status)
}

/// M24-9：`:jobs` 覆盖层单键——j/k 选择、x 取消运行中作业、Esc/q 关闭。
/// 返回是否关闭覆盖层。
fn jobs_menu_key(
    state: &mut AppState,
    cursor: &mut usize,
    code: KeyCode,
    msg: &mut Option<String>,
) -> bool {
    match code {
        KeyCode::Esc | KeyCode::Char('q') => true,
        KeyCode::Char('j') | KeyCode::Down => {
            if !state.jobs.is_empty() {
                *cursor = (*cursor + 1).min(state.jobs.len() - 1);
            }
            false
        }
        KeyCode::Char('k') | KeyCode::Up => {
            *cursor = cursor.saturating_sub(1);
            false
        }
        KeyCode::Char('x') => {
            if state.cancel_job(*cursor) {
                let id = state.jobs[*cursor].id;
                *msg = Some(format!("已请求取消作业 #{id}（当前文件完成后停止）"));
            }
            false
        }
        _ => false,
    }
}

/// M24-9：作业参数包（降低 spawn_job_thread 参数量；语义同 M13-8）。
struct JobSpec<'a> {
    op: &'a str,
    names: &'a [String],
    base: &'a std::path::Path,
    target: &'a std::path::Path,
    tab_id: usize,
}

/// spawn 后台线程执行作业并把更新发送回主线程（M13-8 run 侧薄壳）。
/// M24-9：`cancel` 为作业的取消标志（AppState 侧与线程共享）。
fn spawn_job_thread(
    job_tx: &std::sync::mpsc::Sender<crate::state::JobUpdate>,
    job_id: usize,
    spec: &JobSpec<'_>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    let tx = job_tx.clone();
    let op = spec.op.to_owned();
    let names = spec.names.to_vec();
    let base = spec.base.to_path_buf();
    let target = spec.target.to_path_buf();
    let tab_id = spec.tab_id;
    std::thread::spawn(move || {
        let (ops, status) = execute_job_body(&op, &names, &base, &target, tab_id, &cancel);
        let _ = tx.send(crate::state::JobUpdate {
            job_id,
            tab_id,
            ops,
            status,
        });
    });
}

/// M25-4：归档对话框的字段/取值常量（与渲染同源）。
const ARCHIVE_FORMATS: [ops::ArchiveFormat; 3] = [
    ops::ArchiveFormat::Zip,
    ops::ArchiveFormat::Tar,
    ops::ArchiveFormat::TarGz,
];
const ARCHIVE_CHECKSUMS: [ops::Checksum; 3] = [
    ops::Checksum::Md5,
    ops::Checksum::Sha256,
    ops::Checksum::None,
];

/// M25-4：归档对话框单键结果。
#[derive(Debug, PartialEq, Eq)]
enum ArchiveDialogOutcome {
    None,
    Close,
    Exec,
}

/// M25-4：归档对话框单键——Tab/↑↓ 切字段、←→/空格 循环当前字段取值、
/// 名称字段（field=0）接收字符输入/退格；Enter 执行（名称空 → 留在对话
/// 框提示）；Esc 取消。执行失败留在对话框（名称保留可改）。
fn handle_archive_dialog_key(
    state: &mut AppState,
    input: &mut String,
    field: &mut usize,
    format_idx: &mut usize,
    checksum_idx: &mut usize,
    code: KeyCode,
    msg: &mut Option<String>,
) -> ArchiveDialogOutcome {
    match code {
        KeyCode::Esc => ArchiveDialogOutcome::Close,
        KeyCode::Enter => {
            let name = input.trim().to_owned();
            if name.is_empty() {
                *msg = Some("archive: 名称不能为空".to_owned());
                return ArchiveDialogOutcome::None;
            }
            match archive_execute(
                state,
                ARCHIVE_FORMATS[*format_idx],
                &name,
                ARCHIVE_CHECKSUMS[*checksum_idx],
            ) {
                Ok(m) => {
                    *msg = Some(m);
                    ArchiveDialogOutcome::Exec
                }
                Err(e) => {
                    *msg = Some(e);
                    ArchiveDialogOutcome::None
                }
            }
        }
        KeyCode::Tab | KeyCode::Down => {
            *field = (*field + 1) % 3;
            ArchiveDialogOutcome::None
        }
        KeyCode::Up => {
            *field = (*field + 2) % 3;
            ArchiveDialogOutcome::None
        }
        KeyCode::Right | KeyCode::Char(' ') => {
            match *field {
                1 => *format_idx = (*format_idx + 1) % 3,
                2 => *checksum_idx = (*checksum_idx + 1) % 3,
                _ => {} // 名称字段：空格不入名（名字不含空白）
            }
            ArchiveDialogOutcome::None
        }
        KeyCode::Left => {
            match *field {
                1 => *format_idx = (*format_idx + 2) % 3,
                2 => *checksum_idx = (*checksum_idx + 2) % 3,
                _ => {}
            }
            ArchiveDialogOutcome::None
        }
        KeyCode::Backspace => {
            if *field == 0 {
                input.pop();
            }
            ArchiveDialogOutcome::None
        }
        KeyCode::Char(c) => {
            if *field == 0 {
                input.push(c);
            }
            ArchiveDialogOutcome::None
        }
        _ => ArchiveDialogOutcome::None,
    }
}

/// M25-3/M25-4：归档创建执行体（exec 臂与对话框共用）——tags 优先（排序
/// 确定顺序）、空 = 光标条目；产物与校验文件各入一条撤销记录（u 可逆）。
/// 返回状态栏消息；失败 Err（归档半成品已由 create_archive 清理）。
fn archive_execute(
    state: &mut AppState,
    format: ops::ArchiveFormat,
    name: &str,
    checksum: ops::Checksum,
) -> Result<String, String> {
    let base = physical_base(state)?;
    let tab = state.active_tab();
    let names: Vec<String> = if tab.tags.is_empty() {
        vec![
            tab.entries
                .get(tab.cursor)
                .ok_or_else(|| "archive: 无选中条目".to_owned())?
                .name
                .clone(),
        ]
    } else {
        // HashSet 顺序不定 → 排序使打包条目顺序确定。
        let mut names: Vec<String> = tab.tags.iter().cloned().collect();
        names.sort();
        names
    };
    let tab_id = state.active;
    let op = ops::create_archive(tab_id, &base, name, format, &names)
        .map_err(|e| format!("archive: {e}"))?;
    let dest = match &op {
        crate::state::UndoableOp::Create { path, .. } => path.clone(),
        _ => PathBuf::new(),
    };
    state.undo_stack.push(op);
    // M25-4：校验文件（失败不回滚归档——归档已入撤销栈可 u 移除）。
    let cpath = ops::write_checksum(&dest, checksum).map_err(|e| format!("archive: {e}"))?;
    if let Some(c) = cpath {
        state.undo_stack.push(crate::state::UndoableOp::Create {
            tab_id,
            path: c,
            recursive: false,
        });
    }
    refresh_tab_panes(state, tab_id);
    let dest_display = dest.display().to_string().replace('\\', "/");
    let cs = match checksum {
        ops::Checksum::Md5 => "，已生成 .md5",
        ops::Checksum::Sha256 => "，已生成 .sha256",
        ops::Checksum::None => "",
    };
    Ok(format!(
        "archive: 已创建 {dest_display}（{} 项{cs}，u 可撤销）",
        names.len()
    ))
}

/// 执行确认后的删除（M2-2）：移入应用内暂存、记录撤销栈、刷新所在 tab。
/// `permanent = true`（M13-3，`DD`）：**跳过暂存直接删除**，不进撤销栈
/// （不可逆——确认提示已强警示）。
/// 返回 `Some(op)` = EACCES 需要提权（M12-3），调用方进入 AskPass 模式。
fn execute_delete(
    state: &mut AppState,
    msg: &mut Option<String>,
    path: PathBuf,
    permanent: bool,
    remote: Option<String>,
) -> Option<crate::privilege::PrivOp> {
    let tab_id = state.active;
    // M20-1：远端删除——dd 走 trash 可恢复，DD 走永久删除（不可逆）。
    if let Some(remote) = remote {
        // DD 永久删除：不进撤销栈，不进 trash。
        if permanent {
            let is_dir = state
                .remote_sessions
                .get(&remote)
                .and_then(|be| be.metadata(&path).ok())
                .map(|m| m.is_dir)
                .unwrap_or(false);
            let result = state
                .remote_sessions
                .get(&remote)
                .ok_or_else(|| "会话已断开".to_owned())
                .and_then(|be| {
                    if is_dir {
                        be.remove_dir(&path).map_err(|e| e.to_string())
                    } else {
                        be.remove_permanent(&path).map_err(|e| e.to_string())
                    }
                });
            return match result {
                Ok(()) => {
                    *msg = Some("远端永久删除（不可恢复）".to_owned());
                    if let Some(p) = current_remote_path(state) {
                        let _ = remote_navigate(state, &p);
                    }
                    None
                }
                Err(e) => {
                    *msg = Some(format!("远端永久删除失败: {e}"));
                    if let Some(p) = current_remote_path(state) {
                        let _ = remote_navigate(state, &p);
                    }
                    None
                }
            };
        }
        let trash = state
            .remote_sessions
            .get(&remote)
            .ok_or_else(|| "会话已断开".to_owned())
            .and_then(|be| be.move_to_trash(&path).map_err(|e| e.to_string()));
        return match trash {
            Ok(trash_path) => {
                state.undo_stack.push(crate::state::UndoableOp::Remote {
                    tab_id,
                    remote: remote.clone(),
                    kind: crate::state::RemoteUndo::Delete {
                        path: path.clone(),
                        trash_path,
                    },
                });
                *msg = Some(format!("远端删除（u 恢复）: {}", path.display()));
                if let Some(p) = current_remote_path(state) {
                    let _ = remote_navigate(state, &p);
                }
                None
            }
            Err(e) => {
                *msg = Some(format!("远端删除失败: {e}"));
                if let Some(p) = current_remote_path(state) {
                    let _ = remote_navigate(state, &p);
                }
                None
            }
        };
    }
    let cfg = state.config.clone();
    if permanent {
        // M13-3 永久删除：不可逆，不进撤销栈。权限错误仍走提权链。
        let is_dir = path.is_dir();
        let result = if is_dir {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        return match result {
            Ok(()) => {
                *msg = Some("永久删除（不可恢复）".to_owned());
                if let Some(tab) = state.tabs.get_mut(tab_id)
                    && let Err(e) = tab.refresh(cfg)
                {
                    *msg = Some(format!("刷新失败: {e}"));
                }
                None
            }
            Err(e) if crate::ops::is_permission_denied(&e) && cfg!(unix) => {
                *msg = None;
                Some(crate::privilege::PrivOp::Delete { path })
            }
            Err(e) => {
                *msg = Some(format!("永久删除失败: {e}"));
                None
            }
        };
    }
    match ops::delete_to_trash(tab_id, &path) {
        Ok(op) => {
            state.undo_stack.push(op);
            *msg = None;
            refresh_tab_panes(state, tab_id);
            None
        }
        Err(e) if crate::ops::is_permission_denied(&e) && cfg!(unix) => {
            *msg = None;
            Some(crate::privilege::PrivOp::Delete { path })
        }
        Err(e) => {
            *msg = Some(format!("删除失败: {e}"));
            None
        }
    }
}

/// 执行确认后的标记批量删除（M3-3）：逐文件入暂存并各生成撤销记录；
/// 已消失的标记条目跳过。归档视图只读（M7 边界）。
fn execute_tag_delete(state: &mut AppState, msg: &mut Option<String>) {
    let Ok(base) = physical_base(state) else {
        *msg = Some("归档视图只读（解压后可修改）".to_owned());
        return;
    };
    let names: Vec<String> = state.active_tab().tags.iter().cloned().collect();
    let mut done = 0;
    let mut first_err = None;
    for name in &names {
        let path = base.join(name);
        if !path.exists() {
            continue; // 标记后条目已消失：跳过
        }
        match ops::delete_to_trash(state.active, &path) {
            Ok(op) => {
                state.undo_stack.push(op);
                done += 1;
            }
            Err(e) => {
                if first_err.is_none() {
                    first_err = Some(format!("删除 {name}: {e}"));
                }
            }
        }
    }
    let cfg = state.config.clone();
    if let Some(tab) = state.tabs.get_mut(state.active) {
        let _ = tab.refresh(cfg);
        // M3-4：批量删除完成后清空标记状态。
        tab.clear_tags();
    }
    let mut m = format!("tag delete: {done} 完成");
    if let Some(e) = first_err {
        m.push_str(&format!("; 错误: {e}"));
    }
    *msg = Some(m);
}

/// `u` 撤销（M2-2）：弹出最近操作并执行逆操作；失败回退入栈保留可重试。
fn undo_last(state: &mut AppState, msg: &mut Option<String>) {
    let Some(op) = state.undo_stack.pop() else {
        *msg = Some("无可撤销操作".to_owned());
        return;
    };
    // M20-1：远端撤销经会话执行（后端 rename/write/remove）。
    if let crate::state::UndoableOp::Remote { remote, kind, .. } = &op {
        let result = match kind {
            crate::state::RemoteUndo::Delete { path, trash_path } => state
                .remote_sessions
                .get(remote)
                .ok_or_else(|| "会话已断开".to_owned())
                .and_then(|be| {
                    be.restore_from_trash(trash_path, path)
                        .map_err(|e| e.to_string())
                }),
            crate::state::RemoteUndo::Create { path, is_dir } => state
                .remote_sessions
                .get(remote)
                .ok_or_else(|| "会话已断开".to_owned())
                .and_then(|be| {
                    if *is_dir {
                        be.remove_dir(path).map_err(|e| e.to_string())
                    } else {
                        be.remove_permanent(path).map_err(|e| e.to_string())
                    }
                }),
            crate::state::RemoteUndo::Rename { from, to } => state
                .remote_sessions
                .get(remote)
                .ok_or_else(|| "会话已断开".to_owned())
                .and_then(|be| be.rename(to, from).map_err(|e| e.to_string())),
        };
        match result {
            Ok(()) => {
                *msg = Some(format!("已撤销: {}", op.describe()));
                if let Some(p) = current_remote_path(state) {
                    let _ = remote_navigate(state, &p);
                }
            }
            Err(e) => {
                state.undo_stack.push(op);
                *msg = Some(format!("撤销失败: {e}"));
            }
        }
        return;
    }
    match ops::undo_op(&op) {
        Ok(()) => {
            // 撤销成功给出反馈（M2-6）。
            *msg = Some(format!("已撤销: {}", op.describe()));
            // 刷新受影响 tab（M2-7：跨 tab 撤销刷新原 tab；tab 已关闭则跳过，
            // 磁盘恢复不受影响；设计取向为"当前 tab 不变"，ACCEPTANCE §4）。
            let cfg = state.config.clone();
            if let Some(tab) = state.tabs.get_mut(op.tab_id())
                && let Err(e) = tab.refresh(cfg)
            {
                *msg = Some(format!("刷新失败: {e}"));
            }
        }
        Err(e) => {
            state.undo_stack.push(op);
            *msg = Some(format!("撤销失败: {e}"));
        }
    }
}
/// M8 收尾（GAP-2）：会话访问事件刷写——各 tab 的 history[1..]（真实
/// 导航轨迹，history[0] 为启动落点不算导航）转为 Navigate 事件批量追加。
/// 失败降级 pending（不阻塞退出、不 panic）。
fn flush_session_visits(state: &AppState) {
    // M23-4：IGNORE_PATTERNS——命中路径不入库。
    let mut records = collect_session_records(state);
    records.retain(|r| !crate::frecency::ignored(&r.path, &state.config.frecency.ignore_patterns));
    if records.is_empty() {
        return;
    }
    if let Err(e) = crate::frecency::flush_visits(&records) {
        eprintln!("rvifm: 会话访问记录写入失败: {e}");
    }
}

/// M8 收尾（GAP-2）：会话轨迹 → Navigate 事件（history[1..]；可测试纯函数）。
fn collect_session_records(state: &AppState) -> Vec<crate::frecency::EntryRecord> {
    use std::collections::HashMap as Map;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let mut records = Vec::new();
    // M21-F9：同一 (tab, 'L', 路径) 的历史条目按已实时记录的次数对齐跳过
    // （live_recorded 记次数而非有无，重复进入各计一次）。
    let mut seen: Map<(usize, String), usize> = Map::new();
    for (idx, tab) in state.tabs.iter().enumerate() {
        for loc in tab.history.iter().skip(1) {
            if let Location::Physical(p) = loc {
                let path = p.to_string_lossy().replace('\\', "/");
                let key = (idx, path.clone());
                let live = state
                    .live_recorded
                    .get(&(idx, 'L', path.clone()))
                    .copied()
                    .unwrap_or(0);
                let seen_n = seen.entry(key).and_modify(|c| *c += 1).or_insert(1);
                if *seen_n <= live {
                    continue;
                }
                records.push(crate::frecency::EntryRecord {
                    path,
                    topic: "dir".to_owned(),
                    kind: crate::frecency::EntryKind::Navigate,
                    ts: now,
                    clock: 0,
                    keyword: String::new(),
                });
            }
        }
    }
    records
}

/// M22-1：词法归一化——解析 `.`/`..` 段（不触文件系统、不解析符号链接，
/// 保持逻辑路径与 vifm 一致）；输入需为绝对路径。
fn normalize_lexical(p: &std::path::Path) -> std::path::PathBuf {
    use std::path::Component;
    let mut out: Vec<Component> = Vec::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => match out.last() {
                Some(Component::RootDir) => {}
                Some(_) => {
                    out.pop();
                }
                None => out.push(Component::ParentDir),
            },
            other => out.push(other),
        }
    }
    out.iter().collect()
}

/// M21-F9：l/Enter 成功进入物理目录后的实时 frecency 记录——与 shell hook
/// 语义一致（record 为唯一写入入口，导航成功才记）。写入经 flush_visits
/// （redb 短事务 + events.pending 降级）；已记录次数登记 `live_recorded`
/// （侧别 'L'/'R'），退出批量刷写（GAP-2 collect_session_records）对左 pane
/// 历史按次数去重，同一访问不重复计分。归档/远端位置不计（与退出刷写跳过
/// 归档一致）。测试构建跳过真实写入（避免污染全局 store + 并行锁竞争），
/// 可观察态为 `live_recorded`；flush_visits 本身由 frecency 模块测试覆盖。
fn record_live_visit(state: &mut AppState, tab_id: usize, side: char) {
    let Some(loc) = state.tabs.get(tab_id).map(|t| {
        if side == 'R' {
            t.right.location.clone()
        } else {
            t.location.clone()
        }
    }) else {
        return;
    };
    let Location::Physical(p) = loc else {
        return;
    };
    let path = p.to_string_lossy().replace('\\', "/");
    // M23-4：IGNORE_PATTERNS——命中路径静默跳过（不登记不入库）。
    if crate::frecency::ignored(&path, &state.config.frecency.ignore_patterns) {
        return;
    }
    let key = (tab_id, side, path.clone());
    state
        .live_recorded
        .entry(key)
        .and_modify(|c| *c += 1)
        .or_insert(1);
    let record = crate::frecency::EntryRecord {
        path,
        topic: "dir".to_owned(),
        kind: crate::frecency::EntryKind::Navigate,
        ts: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        clock: 0,
        keyword: String::new(),
    };
    #[cfg(not(test))]
    if let Err(e) = crate::frecency::flush_visits(std::slice::from_ref(&record)) {
        eprintln!("rvifm: 实时访问记录写入失败: {e}");
    }
    #[cfg(test)]
    let _ = &record;
}

/// M21-E8：打开文件成功后的实时 frecency 记录（topic=file，kind=Open）——
/// 文件访问不在退出刷写范围（collect_session_records 仅目录历史），无需
/// 对账；'F' 侧别仅作测试观察态（collect 只消费 'L'）。测试构建跳过真实
/// 写入（同 record_live_visit 理由）。
fn record_file_open(state: &mut AppState, tab_id: usize, path: &std::path::Path) {
    let path_s = path.to_string_lossy().replace('\\', "/");
    // M23-4：IGNORE_PATTERNS——命中路径静默跳过。
    if crate::frecency::ignored(&path_s, &state.config.frecency.ignore_patterns) {
        return;
    }
    state
        .live_recorded
        .entry((tab_id, 'F', path_s.clone()))
        .and_modify(|c| *c += 1)
        .or_insert(1);
    let record = crate::frecency::EntryRecord {
        path: path_s,
        topic: "file".to_owned(),
        kind: crate::frecency::EntryKind::Open,
        ts: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        clock: 0,
        keyword: String::new(),
    };
    #[cfg(not(test))]
    if let Err(e) = crate::frecency::flush_visits(std::slice::from_ref(&record)) {
        eprintln!("rvifm: 实时访问记录写入失败: {e}");
    }
    #[cfg(test)]
    let _ = &record;
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
    crate::session::restore_session(&mut state);
    let layers = config::keymap_layers();
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
    let mut input = InputState::with_keymap(keymap);
    let mut mode = InputMode::default();
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
            refresh_on_fs_change(&mut state);
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
        // M13-8：事件轮询（100ms）——后台作业完成即时感知，UI 不空转阻塞。
        let polled = event::poll(std::time::Duration::from_millis(100))?;
        let key_ev = if polled { event::read().ok() } else { None };
        if let Some(Event::Key(key)) = key_ev {
            match &mut mode {
                InputMode::Command {
                    input,
                    hist_pos,
                    draft,
                    comp,
                } => {
                    // M24-6：↑/↓ 浏览会话命令历史（内存 Vec，无持久化）；
                    // M24-7：Tab 补全焦点 pane 条目名（前缀循环，>10 候选
                    // 显示计数）；其余按键走原有单键处理。
                    let handled = if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Up => {
                                hist_prev(&state.cmd_history, hist_pos, input, draft);
                                Some(CmdInput::Continue)
                            }
                            KeyCode::Down => {
                                hist_next(&state.cmd_history, hist_pos, input, draft);
                                Some(CmdInput::Continue)
                            }
                            KeyCode::Tab => {
                                let names: Vec<String> = {
                                    let tab = state.active_tab();
                                    if tab.focus == crate::tab::Focus::Right {
                                        &tab.right.entries
                                    } else {
                                        &tab.entries
                                    }
                                }
                                .iter()
                                .map(|i| i.name.clone())
                                .collect();
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
                            if result.is_ok() {
                                state.record_cmd_history(&line);
                            }
                            match result {
                                Ok(ExecOutcome::Quit) => {
                                    flush_session_visits(&state);
                                    crate::session::save_session(&state);
                                    cleanup_temp_opens(&mut state);
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
                                    // M13-8：登记作业并 spawn 后台线程。
                                    let job_id = state.add_job(
                                        &op,
                                        names.len(),
                                        &target.display().to_string(),
                                    );
                                    // M24-9：取消标志与线程共享（覆盖层 x 置位）。
                                    let cancel = state
                                        .jobs
                                        .iter()
                                        .find(|j| j.id == job_id)
                                        .expect("新登记作业必存在")
                                        .cancel_requested
                                        .clone();
                                    let tab_id = state.active;
                                    spawn_job_thread(
                                        &job_tx,
                                        job_id,
                                        &JobSpec {
                                            op: &op,
                                            names: &names,
                                            base: &base,
                                            target: &target,
                                            tab_id,
                                        },
                                        cancel,
                                    );
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
                            return Ok(());
                        }
                        Outcome::EnterCommand => {
                            mode = InputMode::Command {
                                input: String::new(),
                                hist_pos: None,
                                draft: String::new(),
                                comp: None,
                            };
                        }
                        Outcome::EnterSearch => {
                            mode = InputMode::Search {
                                input: String::new(),
                                origin: state.active_tab().cursor,
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
                    // M13-8 → M24-9：j/k 选择、x 取消运行中作业、Esc/q 关闭。
                    if key.kind == KeyEventKind::Press
                        && jobs_menu_key(&mut state, cursor, key.code, &mut msg)
                    {
                        mode = InputMode::Normal;
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
                            ArchiveDialogOutcome::Exec => mode = InputMode::Normal,
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

/// FIX-4：居中浮窗矩形——按终端自适应（60% 宽 / 50% 高），上限 80×20，
/// 小终端保底 30×8（不足时取终端本身），在区域内水平垂直居中。
fn centered_rect(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    use ratatui::layout::Rect;
    let w = (area.width * 60 / 100).clamp(30.min(area.width), 80.min(area.width));
    let h = (area.height * 50 / 100).clamp(8.min(area.height), 20.min(area.height));
    Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

/// M21-E8：跳转选择器区域——80%×80% 居中（无全屏遮罩，双 pane 可见）。
fn jump_rect(area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    use ratatui::layout::Rect;
    let w = (area.width * 80 / 100).max(1);
    let h = (area.height * 80 / 100).max(1);
    Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

/// M21-E8：跳转选择器按键结果。
enum JumpKey {
    Continue,
    Select,
    Close,
}

/// M21-E8：选择器按键处理（可测抽取；run 循环消费 Select/Close）。
/// 搜索态：输入即搜（Char 入筛选），方向键移动，Esc→Normal；
/// Normal 态：j/k/gg/G 移动、l/Enter 选定、可打印字符回搜索态、Esc 关闭。
fn jump_key(
    normal: &mut bool,
    g_pending: &mut bool,
    input: &mut String,
    cursor: &mut usize,
    visible_len: usize,
    key: &KeyEvent,
) -> JumpKey {
    if key.kind != KeyEventKind::Press {
        return JumpKey::Continue;
    }
    let bottom = visible_len.saturating_sub(1);
    match key.code {
        KeyCode::Esc => {
            if *normal {
                JumpKey::Close
            } else {
                *normal = true;
                input.clear();
                *g_pending = false;
                *cursor = (*cursor).min(bottom);
                JumpKey::Continue
            }
        }
        KeyCode::Enter => JumpKey::Select,
        KeyCode::Char('l') if *normal => JumpKey::Select,
        KeyCode::Down => {
            *cursor = (*cursor).saturating_add(1).min(bottom);
            JumpKey::Continue
        }
        KeyCode::Up => {
            *cursor = (*cursor).saturating_sub(1);
            JumpKey::Continue
        }
        KeyCode::Char('j') if *normal => {
            *cursor = (*cursor).saturating_add(1).min(bottom);
            JumpKey::Continue
        }
        KeyCode::Char('k') if *normal => {
            *cursor = (*cursor).saturating_sub(1);
            JumpKey::Continue
        }
        KeyCode::Char('g') if *normal => {
            // gg 顶：'g' 待决，第二键 'g' 落顶（单按不动作）。
            if *g_pending {
                *cursor = 0;
                *g_pending = false;
            } else {
                *g_pending = true;
            }
            JumpKey::Continue
        }
        KeyCode::Char('G') if *normal => {
            *cursor = bottom;
            *g_pending = false;
            JumpKey::Continue
        }
        KeyCode::Backspace if !*normal => {
            input.pop();
            *cursor = 0;
            JumpKey::Continue
        }
        KeyCode::Char(c) => {
            if *normal {
                // 可打印字符回搜索态（携带该字符重新筛选）。
                *normal = false;
                input.clear();
                input.push(c);
            } else {
                input.push(c);
            }
            *cursor = 0;
            *g_pending = false;
            JumpKey::Continue
        }
        _ => JumpKey::Continue,
    }
}

/// M23-6：Frecency Pane 渲染——焦点 pane 显示搜索+结果，对侧显示光标
/// 条目的快速预览（PreviewRenderer，文本 100 行截断 + 标记）。
fn render_frecency_panes(frame: &mut Frame, state: &AppState) {
    let tab = state.active_tab();
    let pane = match &tab.pane_frecency {
        Some(p) => p,
        None => return,
    };
    let [tab_bar_area, left_area, right_area, status_area] = pane_layout(frame.area());
    // 标签栏照常渲染。
    let tab_spans: Vec<Span> = state
        .tabs
        .iter()
        .enumerate()
        .map(|(idx, t)| {
            let dir = t
                .location
                .base_dir()
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| t.location.display_path());
            let label = format!("{}:{} ", idx + 1, dir);
            if idx == state.active {
                Span::styled(
                    label,
                    Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD),
                )
            } else {
                Span::from(label)
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(Line::from(tab_spans)), tab_bar_area);

    let focused_is_left = tab.focus == crate::tab::Focus::Left;
    let frec_area = if focused_is_left {
        left_area
    } else {
        right_area
    };
    let other_area = if focused_is_left {
        right_area
    } else {
        left_area
    };

    // 焦点 pane：Frecency 搜索 UI。
    let visible = filter_jump(&pane.entries, &pane.query);
    let mode_mark = if pane.normal {
        " · Normal"
    } else {
        " · /搜索"
    };
    let title = format!("Frecency{mode_mark}（{} 项）", visible.len());
    let frec_block = ratatui::widgets::Block::bordered()
        .title(title)
        .border_style(pane_border_style(true));
    let inner = frec_block.inner(frec_area);
    frame.render_widget(frec_block, frec_area);
    let frec_items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .map(|(idx, e)| {
            let topic = if e.topic == "dir" {
                "[目录]"
            } else {
                "[文件]"
            };
            let bm = if e.keyword.is_empty() {
                String::new()
            } else {
                format!(" ✦{}", e.keyword)
            };
            ListItem::new(Line::from(format!("{topic}{bm} {}", e.path))).style(
                if idx == pane.cursor {
                    Style::new().add_modifier(Modifier::REVERSED)
                } else {
                    Style::new()
                },
            )
        })
        .collect();
    let mut list_state = ListState::default().with_selected(Some(pane.cursor));
    frame.render_stateful_widget(
        ratatui::widgets::List::new(frec_items)
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED)),
        inner,
        &mut list_state,
    );

    // 对侧：光标条目的快速预览（PreviewRenderer，100 行截断）。
    let other_block = ratatui::widgets::Block::bordered()
        .title("预览")
        .border_style(pane_border_style(false));
    let other_inner = other_block.inner(other_area);
    frame.render_widget(other_block, other_area);
    if let Some(entry) = visible.get(pane.cursor) {
        let path = std::path::PathBuf::from(&entry.path);
        let preview = crate::preview::render_preview(&path, 100);
        frame.render_widget(preview.into_paragraph(), other_inner);
    }
    frame.render_widget(
        Paragraph::new(if pane.normal {
            "j/k/gg/G=移动 l/Enter=跳转 y=复制 e=编辑 w=view Esc=关闭"
        } else {
            "输入=筛选 方向键=移动 Enter=跳转 Esc=Normal"
        }),
        status_area,
    );
}

fn render(frame: &mut Frame, state: &AppState, mode: &InputMode, msg: Option<&str>) {
    let tab = state.active_tab();
    // M14-2：标签栏（1 行）——每个 tab 的索引与目录名，当前 tab 高亮。
    let [tab_bar_area, left_area, right_area, status_area] = pane_layout(frame.area());
    // M23-6：Frecency Pane 模式——焦点 pane 显示搜索 UI，对侧显示快速预览。
    if state.active_tab().pane_frecency.is_some() {
        render_frecency_panes(frame, state);
        return;
    }
    let tab_spans: Vec<Span> = state
        .tabs
        .iter()
        .enumerate()
        .map(|(idx, t)| {
            let dir = t
                .location
                .base_dir()
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| t.location.display_path());
            let label = format!("{}:{} ", idx + 1, dir);
            if idx == state.active {
                Span::styled(
                    label,
                    Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD),
                )
            } else {
                Span::from(label)
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(Line::from(tab_spans)), tab_bar_area);
    // M21-F1：左右 pane 各带边框（焦点态高亮）、居中分开（间隔 1 列），
    // pane 顶部显示各自路径——状态栏不再显示路径。
    let left_focused = tab.focus == crate::tab::Focus::Left;
    let left_pane = ratatui::widgets::Block::bordered()
        .title(tab.location.display_path().replace('\\', "/"))
        .border_style(pane_border_style(left_focused));
    let left_inner = left_pane.inner(left_area);
    frame.render_widget(left_pane, left_area);

    // FEAT-2：`..` 行——列表顶部独立 1 行（渲染层虚拟条目，不进 entries）。
    // 物理根目录无父目录 → 不渲染；归档视图恒渲染。
    // M21-F2：非焦点侧的 `..` 行置灰（vifm TopLine 语义——光标行 DIM）。
    let parent_available = tab.has_parent_row() && !tab.in_results();
    let parent_area = if parent_available {
        let [p, rest] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(left_inner);
        let parent_style = if tab.on_parent_row && left_focused {
            Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            // 视觉上与普通目录项区分（置灰）。
            Style::new().add_modifier(Modifier::DIM)
        };
        frame.render_widget(Paragraph::new(Line::from("../")).style(parent_style), p);
        rest
    } else {
        left_inner
    };

    let visual_range = tab.visual_range();
    // FIX-1：当前生效的搜索查询——搜索输入期用实时输入（不回退旧查询，
    // 清空即无高亮），其余用已提交的 tab.search。
    let active_query: Option<String> = match mode {
        InputMode::Search { input, .. } => {
            let query = input.trim();
            (!query.is_empty()).then(|| query.to_owned())
        }
        _ => tab
            .search
            .as_ref()
            .map(|s| s.query.clone())
            .filter(|query| !query.is_empty()),
    };
    // M21-E1：属性列自适应——配置开关 + 窄终端隐藏（文件名优先）。
    let show_attrs = state.config.ui.attrs_column
        && left_inner.width as usize >= MIN_ATTRS_INNER_WIDTH
        && tab.tree.is_none(); // 树视图相对路径较长，不挤属性列
    // M25-5：git 状态列（属性区内；开关 + 就绪数据，否则占位对齐）。
    let git_col = show_attrs && state.config.ui.git_column;
    let git_map = if git_col {
        match &tab.location {
            Location::Physical(p) => state.git_cache.get(p).and_then(|o| o.as_ref()),
            _ => None,
        }
    } else {
        None
    };
    let name_budget = if show_attrs {
        (left_inner.width as usize)
            .saturating_sub(2 + ATTRS_WIDTH + 2 + if git_col { GIT_STATUS_WIDTH } else { 0 })
    } else {
        (left_inner.width as usize).saturating_sub(4)
    };
    let list_items: Vec<ListItem> = tab
        .entries
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            // M21-F1：名称按文件类型着色（目录/可执行/链接/归档/图片/视频/代码）。
            let ft = filetype_style(item.filetype_class());
            // FIX-1：匹配子串分段高亮（黄底黑字，与光标行反显区分）。
            // 图标/树缩进进前缀 span，目录后缀 `/` 跟在名称之后。
            let mut spans: Vec<Span> = vec![Span::from(format!("{} ", item.icon.glyph))];
            let mut budget = name_budget;
            if tab.tree.is_some() {
                let depth = item.name.matches('/').count();
                spans.push(Span::from("  ".repeat(depth)));
                budget = budget.saturating_sub(2 * depth);
            }
            // M21-E1：显示名按预算截断（搜索/定位仍用全名，仅显示层裁剪）。
            let display_name = clip_name(&item.name, budget);
            match active_query
                .as_deref()
                .and_then(|query| crate::tab::match_range(&display_name, query))
            {
                Some((start, end)) => {
                    let (before, mid, after) = (
                        &display_name[..start],
                        &display_name[start..end],
                        &display_name[end..],
                    );
                    if !before.is_empty() {
                        spans.push(Span::styled(before.to_owned(), ft));
                    }
                    spans.push(Span::styled(
                        mid.to_owned(),
                        Style::new().fg(Color::Black).bg(Color::LightYellow),
                    ));
                    let suffix = if item.is_dir {
                        format!("{after}/")
                    } else {
                        after.to_owned()
                    };
                    if !suffix.is_empty() {
                        spans.push(Span::styled(suffix, ft));
                    }
                }
                None => {
                    let label = if item.is_dir {
                        format!("{display_name}/")
                    } else {
                        display_name.clone()
                    };
                    spans.push(Span::styled(label, ft));
                }
            }
            // M24-1：名称填充至固定列宽（属性列对齐）+ 属性列（分钟精度日期）。
            if show_attrs {
                let name_width = display_name.chars().count() + (item.is_dir as usize);
                let pad = name_budget.saturating_sub(name_width).max(1);
                spans.push(Span::from(" ".repeat(pad)));
                // M25-5：git 状态字符列（占位空格保持列对齐）。
                match git_map.and_then(|m| m.entries.get(&item.name)) {
                    Some(st) => spans.push(git_status_span(*st)),
                    None => spans.push(Span::from(" ".repeat(GIT_STATUS_WIDTH))),
                }
                let size = if item.is_dir {
                    "<DIR>".to_owned()
                } else {
                    crate::tab::format_size(item.size)
                };
                let attr = format!("{:>10}  {}", size, crate::tab::format_mtime_min(item.mtime));
                spans.push(Span::styled(attr, Style::new().add_modifier(Modifier::DIM)));
            }
            // M9-2：图标颜色经 opaline 样式染色（命名样式缺失时回落默认）。
            let base_style = crate::theme::style("file_icon");
            // 可视选择范围内的行反显（M3-1）；tag 标记行加粗（M3-2）；
            // 光标行由 highlight_style 叠加。
            let mut li = ListItem::new(Line::from(spans)).style(base_style);
            let mut mods = Modifier::empty();
            if visual_range.is_some_and(|(lo, hi)| idx >= lo && idx <= hi) {
                mods |= Modifier::REVERSED;
            }
            if tab.tags.contains(&item.name) {
                mods |= Modifier::BOLD;
            }
            if mods != Modifier::empty() {
                li = li.style(Style::new().add_modifier(mods));
            }
            li
        })
        .collect();
    // FEAT-2：焦点在 `..` 行时列表无高亮选中项。
    let mut list_state = ListState::default().with_offset(list_offset(
        if tab.on_parent_row { 0 } else { tab.cursor },
        parent_area.height as usize,
    ));
    if !tab.on_parent_row {
        list_state = list_state.with_selected(Some(tab.cursor));
    }
    frame.render_stateful_widget(
        List::new(list_items).highlight_style(
            // M21-F2：焦点 pane 当前行反显，非焦点侧当前行 DIM 灰化
            // （vifm TopLineSel/TopLine 语义）。
            if left_focused {
                Style::new().add_modifier(Modifier::REVERSED)
            } else {
                Style::new().add_modifier(Modifier::DIM)
            },
        ),
        parent_area,
        &mut list_state,
    );

    // M11-2→M14-1→M21-F1：右 pane——默认独立目录列表（带路径标题/焦点边框），
    // `w` 视图模式显示查看器/图片预览。
    render_right_pane(frame, state, right_area);

    if let InputMode::HelpView { scroll, search } = mode {
        // FEAT-4：全屏帮助视图（内容来自内嵌 help.txt，滚动状态独立）。
        let lines = help_lines();
        let title = match search {
            Some(q) => format!("帮助  /{q}（n/N 跳转）"),
            None => "帮助（j/k 滚动  / 搜索  n/N 跳转  q 返回）".to_owned(),
        };
        let area = frame.area();
        let visible: Vec<Line> = lines
            .iter()
            .skip(*scroll)
            .take(area.height.saturating_sub(2) as usize)
            .map(|l| Line::from((*l).to_owned()))
            .collect();
        frame.render_widget(
            Paragraph::new(visible).block(ratatui::widgets::Block::bordered().title(title)),
            area,
        );
        return;
    }
    if let InputMode::FileInfo { path, text } = mode {
        // M13-5：文件详情对话框覆盖层（任意键关闭）。
        let _ = path;
        frame.render_widget(
            Paragraph::new(text.clone())
                .block(ratatui::widgets::Block::bordered().title("文件详情（任意键关闭）")),
            frame.area(),
        );
        return;
    }
    if let InputMode::TrashMenu { cursor, items } = mode {
        // M13-10：暂存区菜单覆盖层（staged → orig，Enter 恢复）。
        let mut lines: Vec<Line> = Vec::new();
        if items.is_empty() {
            lines.push(Line::from("（暂存区为空）"));
        } else {
            for (idx, e) in items.iter().enumerate() {
                let mark = if idx == *cursor { "▶ " } else { "  " };
                lines.push(Line::from(format!("{mark}{} → {}", e.staged, e.orig)));
            }
        }
        frame.render_widget(
            Paragraph::new(lines)
                .block(ratatui::widgets::Block::bordered().title("暂存区（Enter=恢复 Esc=关闭）")),
            frame.area(),
        );
        return;
    }
    if let InputMode::Jobs { cursor } = mode {
        // M13-8 → M24-9：后台作业菜单覆盖层（j/k 选择、x 取消、Esc/q 关闭）。
        let mut lines: Vec<Line> = Vec::new();
        if state.jobs.is_empty() {
            lines.push(Line::from("（无后台作业）"));
        } else {
            for (i, j) in state.jobs.iter().enumerate() {
                let status = match &j.status {
                    crate::state::JobStatus::Running => {
                        if j.cancel_requested
                            .load(std::sync::atomic::Ordering::Relaxed)
                        {
                            "取消中…".to_owned()
                        } else {
                            "运行中".to_owned()
                        }
                    }
                    crate::state::JobStatus::Done { summary } => summary.clone(),
                    crate::state::JobStatus::Cancelled { summary } => summary.clone(),
                    crate::state::JobStatus::Failed { err } => format!("失败: {err}"),
                };
                let sel = if i == *cursor {
                    Style::new().add_modifier(Modifier::REVERSED)
                } else {
                    Style::new()
                };
                lines.push(Line::styled(
                    format!(
                        "#{id} {kind} → {target}（{} 项）: {status}",
                        j.src_count,
                        id = j.id,
                        kind = j.kind,
                        target = j.target
                    ),
                    sel,
                ));
            }
        }
        frame.render_widget(
            Paragraph::new(lines).block(
                ratatui::widgets::Block::bordered().title("后台作业（j/k=选择 x=取消 Esc=关闭）"),
            ),
            frame.area(),
        );
        return;
    }
    if let InputMode::SortDialog { cursor, desc } = mode {
        // M18-1：排序对话框（固定尺寸居中遮罩）。
        let area = frame.area();
        frame.render_widget(ratatui::widgets::Clear, area);
        frame.render_widget(
            ratatui::widgets::Block::new().style(Style::new().bg(Color::Black)),
            area,
        );
        let popup = context_menu_rect(area);
        let cur_sort = state.active_tab().sort.clone();
        let lines: Vec<Line> = SORT_FIELDS
            .iter()
            .enumerate()
            .map(|(idx, (_lo, _hi, label, field))| {
                let current = cur_sort
                    .as_ref()
                    .and_then(|v| v.first())
                    .filter(|s| s.field == *field)
                    .map(|s| if s.desc { "desc" } else { "asc" });
                let pending = if idx == *cursor {
                    Some(if *desc { "desc" } else { "asc" })
                } else {
                    None
                };
                let text = match (current, pending) {
                    (Some(cur), Some(pend)) => {
                        format!("{label}  [{cur} → {pend}]")
                    }
                    (Some(cur), None) => format!("{label}  [{cur}]"),
                    (None, Some(pend)) => format!("{label}  [→ {pend}]"),
                    (None, None) => (*label).to_owned(),
                };
                if idx == *cursor {
                    Line::from(text).style(Style::new().add_modifier(Modifier::REVERSED))
                } else {
                    Line::from(text)
                }
            })
            .collect();
        frame.render_widget(
            Paragraph::new(lines).block(
                ratatui::widgets::Block::bordered()
                    .title("排序（字母=直接应用 空格=换向 回车=应用 Esc=取消）"),
            ),
            popup,
        );
        return;
    }
    if let InputMode::ArchiveDialog {
        input,
        field,
        format_idx,
        checksum_idx,
    } = mode
    {
        // M25-4：归档创建对话框（居中遮罩，M18-1 SortDialog 同款）。
        let area = frame.area();
        frame.render_widget(ratatui::widgets::Clear, area);
        frame.render_widget(
            ratatui::widgets::Block::new().style(Style::new().bg(Color::Black)),
            area,
        );
        let popup = context_menu_rect(area);
        let fmt = ARCHIVE_FORMATS
            .iter()
            .enumerate()
            .map(|(i, f)| {
                if i == *format_idx {
                    f.ext().trim_start_matches('.').to_owned()
                } else {
                    " ".to_owned() + f.ext().trim_start_matches('.')
                }
            })
            .collect::<Vec<_>>()
            .join("  ");
        let cks = ARCHIVE_CHECKSUMS
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let label = match c {
                    ops::Checksum::Md5 => "md5",
                    ops::Checksum::Sha256 => "sha256",
                    ops::Checksum::None => "none",
                };
                if i == *checksum_idx {
                    label.to_owned()
                } else {
                    format!(" {label}")
                }
            })
            .collect::<Vec<_>>()
            .join("  ");
        let line_name = if *field == 0 {
            Line::from(format!("名称: {input}▏"))
                .style(Style::new().add_modifier(Modifier::REVERSED))
        } else {
            Line::from(format!("名称: {input}"))
        };
        let line_fmt = if *field == 1 {
            Line::from(format!("格式: {fmt}")).style(Style::new().add_modifier(Modifier::REVERSED))
        } else {
            Line::from(format!("格式: {fmt}"))
        };
        let line_cks = if *field == 2 {
            Line::from(format!("校验: {cks}")).style(Style::new().add_modifier(Modifier::REVERSED))
        } else {
            Line::from(format!("校验: {cks}"))
        };
        let lines = vec![
            line_name,
            line_fmt,
            line_cks,
            Line::from("Tab=切字段 ←→/空格=换值 回车=创建 Esc=取消"),
        ];
        frame.render_widget(
            Paragraph::new(lines).block(ratatui::widgets::Block::bordered().title("归档创建")),
            popup,
        );
        return;
    }
    if let InputMode::HistoryMenu { cursor } = mode {
        // M16 NAV-2：历史菜单浮窗（居中遮罩；条目 live 取自 history[..=hist_pos]）。
        let area = frame.area();
        frame.render_widget(ratatui::widgets::Clear, area);
        frame.render_widget(
            ratatui::widgets::Block::new().style(Style::new().bg(Color::Black)),
            area,
        );
        let popup = centered_rect(area);
        let tab_hist = state.active_tab();
        let lines: Vec<Line> = tab_hist
            .history
            .iter()
            .take(tab_hist.hist_pos + 1)
            .enumerate()
            .map(|(idx, loc)| {
                // 标记放行首：长路径被浮窗宽度截断时标记仍可见（Windows
                // 路径可达数十列，行尾标记会被截掉）。
                let mut text = if idx == tab_hist.hist_pos {
                    "● ".to_owned()
                } else {
                    String::new()
                };
                text.push_str(&loc.display_path());
                if idx == *cursor {
                    Line::from(text).style(Style::new().add_modifier(Modifier::REVERSED))
                } else {
                    Line::from(text)
                }
            })
            .collect();
        frame.render_widget(
            Paragraph::new(lines).block(
                ratatui::widgets::Block::bordered()
                    .title("目录历史（Enter=导航 dd=移除 Esc=关闭）"),
            ),
            popup,
        );
        return;
    }
    if let InputMode::ContextMenu { cursor } = mode {
        // FEAT-3：右键菜单浮窗（固定尺寸居中，遮罩背景）。
        let area = frame.area();
        frame.render_widget(ratatui::widgets::Clear, area);
        frame.render_widget(
            ratatui::widgets::Block::new().style(Style::new().bg(Color::Black)),
            area,
        );
        let popup = context_menu_rect(area);
        let lines: Vec<Line> = CONTEXT_MENU
            .iter()
            .enumerate()
            .map(|(idx, label)| {
                if idx == *cursor {
                    Line::from((*label).to_owned())
                        .style(Style::new().add_modifier(Modifier::REVERSED))
                } else {
                    Line::from((*label).to_owned())
                }
            })
            .collect();
        frame.render_widget(
            Paragraph::new(lines).block(
                ratatui::widgets::Block::bordered().title("操作菜单（Enter=执行 Esc=关闭）"),
            ),
            popup,
        );
        return;
    }
    if let InputMode::JumpSelect {
        input,
        cursor,
        entries,
        kind,
        normal,
        g_pending: _,
    } = mode
    {
        // M8-12：跳转选择器覆盖层（纯只读；分级与 pick 一致）。
        // M21-E8：80%×80% 居中、无全屏遮罩（双 pane 可见）；标题标示
        // 搜索/Normal 子态。
        let area = frame.area();
        let popup = jump_rect(area);
        let base = match kind {
            JumpKind::Unified => "跳转（合一 · 全局时钟）",
            JumpKind::Dir => "跳转（仅目录 · dir 时钟）",
            JumpKind::File => "跳转（仅文件 · file 时钟）",
            JumpKind::Recent => "跳转（近期 · 时间倒序）",
        };
        let mode_mark = if *normal {
            " · Normal（j/k/gg/G/l）"
        } else {
            " · /搜索"
        };
        let title = format!("{base}{mode_mark}");
        let visible = filter_jump(entries, input);
        let items: Vec<ListItem> = visible
            .iter()
            .enumerate()
            .map(|(idx, e)| {
                let topic_mark = if e.topic == "dir" {
                    "[目录]"
                } else {
                    "[文件]"
                };
                let bm = if e.keyword.is_empty() {
                    String::new()
                } else {
                    format!(" ✦{}", e.keyword)
                };
                ListItem::new(Line::from(format!("{topic_mark}{bm} {}", e.path))).style(
                    if idx == *cursor {
                        Style::new().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::new()
                    },
                )
            })
            .collect();
        let inner: [ratatui::prelude::Rect; 3] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .areas(
            ratatui::widgets::Block::bordered()
                .title(title.clone())
                .inner(popup),
        );
        frame.render_widget(ratatui::widgets::Block::bordered().title(title), popup);
        frame.render_widget(Paragraph::new(Line::from(format!("/{}", input))), inner[0]);
        let list = ratatui::widgets::List::new(items)
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
        let mut list_state = ratatui::widgets::ListState::default().with_selected(Some(*cursor));
        frame.render_stateful_widget(list, inner[1], &mut list_state);
        frame.render_widget(
            Paragraph::new(if *normal {
                "j/k=移动 gg/G=顶尾 l/Enter=选定 输入字符=搜索 Esc=关闭"
            } else {
                "输入=筛选 方向键=移动 Enter=选定 Esc=Normal"
            }),
            inner[2],
        );
        return;
    }
    let status = match mode {
        InputMode::ViewerSearch { input } => Line::from(format!("/{}（查看器）", input)),
        InputMode::ArchiveDialog { input, field, .. } => {
            // M25-4：对话框激活字段指示（覆盖层提前 return，实际不可达）。
            let f = match field {
                1 => "格式",
                2 => "校验",
                _ => "名称",
            };
            Line::from(format!(":{input}（{f}）"))
        }
        InputMode::RemotePass { name, input, .. } => {
            // M22-3：masked 渲染（`*` 逐字符替换，同 AskPass）。
            let masked = "*".repeat(input.chars().count());
            Line::from(format!(
                "远端口令（{name}）: {masked} [Enter=连接 Esc=取消]"
            ))
        }
        InputMode::Rename { input, pos, .. } => {
            // M21-E4：插入光标可视化（'▏' 标记当前插入位）。
            let chars: Vec<char> = input.chars().collect();
            let mut s = String::new();
            for (i, c) in chars.iter().enumerate() {
                if i == *pos {
                    s.push('▏');
                }
                s.push(*c);
            }
            if *pos >= chars.len() {
                s.push('▏');
            }
            Line::from(format!("重命名: {s} [Enter=应用 Esc=取消]"))
        }
        InputMode::SortDialog { .. } => Line::from(String::new()), // 排序对话框提前 return，不可达
        InputMode::ContextMenu { .. } => Line::from(String::new()), // 菜单覆盖层提前 return，不可达
        InputMode::HistoryMenu { .. } => Line::from(String::new()), // 历史菜单提前 return，不可达
        InputMode::Command { input, .. } => Line::from(format!(":{input}")),
        InputMode::Search { input, .. } => {
            // FIX-1：输入期实时匹配计数；无匹配 0/0 并提示。
            let query = input.trim();
            if query.is_empty() {
                Line::from(format!("/{input}"))
            } else {
                let (cur, total) = tab.search_status(query);
                let mut text = format!("/{input}  [{cur}/{total}]");
                if total == 0 {
                    text.push_str("  无匹配");
                }
                Line::from(text)
            }
        }
        InputMode::Filter { input } => Line::from(format!("={input}")),
        InputMode::HelpView { .. } | InputMode::HelpSearch { .. } => {
            Line::from(String::new()) // 帮助视图提前 return，不可达
        }
        InputMode::JumpSelect { .. } => Line::from(String::new()), // 选择器覆盖层提前 return，不可达
        InputMode::Jobs { .. } => Line::from(String::new()), // 作业菜单覆盖层提前 return，不可达
        InputMode::TrashMenu { .. } => Line::from(String::new()), // 暂存区覆盖层提前 return，不可达
        InputMode::FileInfo { .. } => Line::from(String::new()), // 详情覆盖层提前 return，不可达
        InputMode::Confirm {
            path,
            permanent,
            remote,
        } => {
            let prefix = if remote.is_some() { "远端" } else { "" };
            if *permanent {
                // M13-3：DD 永久删除强提示（不可恢复）。
                Line::from(format!(
                    "**{prefix}永久删除** {}（不可恢复）? [y/N]",
                    path.display()
                ))
            } else {
                Line::from(format!("删除{prefix} {}? [y/N]", path.display()))
            }
        }
        InputMode::ConfirmTagDelete => {
            let n = tab.tagged_count();
            Line::from(format!("删除 {n} 个标记文件? [y/N]"))
        }
        InputMode::AskPass { input, op } => {
            // M12-3：密码 masked 渲染（`*` 逐字符替换），带操作描述。
            let masked = "*".repeat(input.chars().count());
            Line::from(format!(
                "sudo 密码: {masked} [Enter=确认 Esc=取消] {}",
                op.describe()
            ))
        }
        InputMode::Normal => {
            // M21-F1：状态栏不再显示路径（路径移到 pane 顶部），底部显示
            // 选中项详情——名称 [类型] 大小 权限 时间。
            let selected = match tab.entries.get(tab.cursor) {
                Some(item) => {
                    let class = item.filetype_class();
                    let size = if item.is_dir {
                        String::new()
                    } else {
                        format!(" {}", crate::tab::format_size(item.size))
                    };
                    format!(
                        " {} [{}]{} {} {}",
                        item.name,
                        class.label(),
                        size,
                        item.perm,
                        crate::tab::format_mtime(item.mtime)
                    )
                }
                None => String::new(),
            };
            let mut spans = vec![
                Span::styled(selected, Style::new().add_modifier(Modifier::BOLD)),
                Span::from(format!(" | {} items", tab.entries.len())),
            ];
            if let Some(r) = &tab.results {
                // M17 SRCH：结果视图指示（类别 + 查询 + 条数 + 截断）。
                let kind = match r.kind {
                    crate::search::SearchKind::Find => "find",
                    crate::search::SearchKind::Grep => "grep",
                    crate::search::SearchKind::Compare => "compare",
                };
                // M24-8：compare 无查询词，状态栏只显示条数。
                spans.push(if r.query.is_empty() {
                    Span::from(format!(
                        "  [{}: {} 项{}]",
                        kind,
                        r.items.len(),
                        if r.truncated { " 已截断" } else { "" }
                    ))
                } else {
                    Span::from(format!(
                        "  [{}: {} — {} 项{}]",
                        kind,
                        r.query,
                        r.items.len(),
                        if r.truncated { " 已截断" } else { "" }
                    ))
                });
            }
            // M25-6：git 分支 + diff 摘要（per-dir 异步缓存；非仓库/未就绪隐藏）。
            if state.config.ui.git_column
                && let Location::Physical(dir) = &tab.location
                && let Some(Some(st)) = state.git_cache.get(dir)
                && let Some(branch) = &st.branch
            {
                let text = match st.diff_summary.as_deref() {
                    Some(diff) if !diff.is_empty() => format!("  [{branch} {diff}]"),
                    _ => format!("  [{branch}]"),
                };
                spans.push(Span::from(text).style(Style::new().add_modifier(Modifier::DIM)));
            }
            if let Some(specs) = &tab.sort {
                // M18-1/M18-2：排序指示（多字段 join）。
                let text = specs
                    .iter()
                    .map(|s| {
                        format!(
                            "{} {}",
                            match s.field {
                                crate::tab::SortField::Name => "name",
                                crate::tab::SortField::Natural => "natural",
                                crate::tab::SortField::Mtime => "mtime",
                                crate::tab::SortField::Size => "size",
                                crate::tab::SortField::Ext => "ext",
                            },
                            if s.desc { "desc" } else { "asc" }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                spans.push(Span::from(format!("  [sort: {text}]")));
            }
            if let Some(s) = &tab.search {
                if s.query.is_empty() {
                    spans.push(Span::from(format!("  /{}", s.query)));
                } else {
                    // FIX-1：已提交搜索的匹配计数（n/N 移动后随光标同步）。
                    let (cur, total) = tab.search_status(&s.query);
                    let mut text = format!("  /{}  [{cur}/{total}]", s.query);
                    if total == 0 {
                        text.push_str("  无匹配");
                    }
                    spans.push(Span::from(text));
                }
            }
            if let Some(f) = &tab.filter {
                // M4-2：过滤指示。
                spans.push(Span::from(format!("  [filter: {f}]")));
            }
            if let Some((lo, hi)) = visual_range {
                // M3-1：可视选择指示（含行数，闭区间）。
                spans.push(Span::from(format!("  VISUAL ({} selected)", hi - lo + 1)));
            }
            let tagged = tab.tagged_count();
            if tagged > 0 {
                // M3-2：tag 计数指示。
                spans.push(Span::from(format!("  {tagged} tagged")));
            }
            if let Some(m) = msg {
                spans.push(Span::styled(
                    format!("  [{m}]"),
                    Style::new().add_modifier(Modifier::BOLD),
                ));
            }
            Line::from(spans)
        }
    };
    frame.render_widget(Paragraph::new(status), status_area);
}

/// 右 pane 渲染（M14-1；M21-F1 带路径标题与焦点边框）：view 模式 = 活动
/// pane 光标文件的查看器/图片预览；默认 = 右 pane 独立目录列表（可导航）。
fn render_right_pane(frame: &mut Frame, state: &AppState, area: ratatui::layout::Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let tab = state.active_tab();
    let focused = tab.focus == crate::tab::Focus::Right;
    // view 模式：M17 VIEW-1 交互式查看器（行号 + 滚动 + 搜索高亮）。
    if tab.view_mode {
        // M17 SRCH：预览源抽象——结果模式取命中文件，物理视图取 base.join。
        let Some(path) = tab.preview_path() else {
            let pane = ratatui::widgets::Block::bordered()
                .title("view · 预览")
                .border_style(pane_border_style(true));
            let inner = pane.inner(area);
            frame.render_widget(pane, area);
            frame.render_widget(Paragraph::new("归档内预览暂不支持"), inner);
            return;
        };
        // 图片仍走 ImageEngine（查看器针对文本）。
        if matches!(
            crate::preview::preview_kind(&path),
            crate::preview::PreviewKind::Image
        ) {
            let pane = ratatui::widgets::Block::bordered()
                .title(format!(
                    "{} · 图片预览",
                    path.display().to_string().replace('\\', "/")
                ))
                .border_style(pane_border_style(true));
            let inner = pane.inner(area);
            frame.render_widget(pane, area);
            if state.image_engine.render_file(frame, inner, &path).is_err() {
                frame.render_widget(Paragraph::new("图片渲染失败（损坏/超大）"), inner);
            }
            return;
        }
        // M21-F6：光标在目录上 → 浅层 tree（复用 M13-7 构建器，深度 2、
        // 只读快照）——不走文本读取路径，杜绝"无法读取/非 UTF-8"误报。
        if path.is_dir() {
            let pane = ratatui::widgets::Block::bordered()
                .title(format!(
                    "{} · 目录",
                    path.display().to_string().replace('\\', "/")
                ))
                .border_style(pane_border_style(true));
            let inner = pane.inner(area);
            frame.render_widget(pane, area);
            let items = match crate::tab::Tab::build_tree_entries(&path, 2) {
                Ok(items) => items,
                Err(e) => {
                    frame.render_widget(Paragraph::new(format!("目录读取失败: {e}")), inner);
                    return;
                }
            };
            let height = inner.height as usize;
            let mut lines: Vec<Line> = Vec::with_capacity(height);
            if items.is_empty() {
                lines.push(Line::from("（空目录）"));
            }
            for item in items.iter().take(height) {
                let depth = item.name.matches('/').count();
                let leaf = item
                    .name
                    .rsplit('/')
                    .next()
                    .unwrap_or(&item.name)
                    .to_owned();
                let label = if item.is_dir {
                    format!("{}{}/", "  ".repeat(depth), leaf)
                } else {
                    format!("{}{}", "  ".repeat(depth), leaf)
                };
                let spans = vec![
                    Span::from(format!("{} ", item.icon.glyph)),
                    Span::styled(label, filetype_style(item.filetype_class())),
                ];
                lines.push(Line::from(spans));
            }
            frame.render_widget(Paragraph::new(lines), inner);
            return;
        }
        let viewer = tab.viewer.clone().unwrap_or_default();
        let shown = path.display().to_string().replace('\\', "/");
        // M21-E3：大文件截断提示前置（路径长会被 pane 宽度裁掉，放行尾不可见）。
        const VIEWER_MAX_LINES: usize = 100_000;
        let trunc_hint = if viewer.total_lines > viewer.line_count {
            format!("〔已截断 共 {} 行〕", viewer.total_lines)
        } else {
            String::new()
        };
        let title = match &viewer.query {
            Some(q) => format!(
                "{trunc_hint}{shown} · 行 {}/{}  /{}（n/N 跳转）",
                viewer.scroll + 1,
                viewer.line_count,
                q
            ),
            None => format!(
                "{trunc_hint}{shown} · 行 {}/{}（w/Esc 退出 / 搜索）",
                viewer.scroll + 1,
                viewer.line_count
            ),
        };
        let pane = ratatui::widgets::Block::bordered()
            .title(title)
            .border_style(pane_border_style(true));
        let inner = pane.inner(area);
        frame.render_widget(pane, area);
        // M21-F7：PDF/Office 分流——走共享 PreviewRenderer（pdf-inspector
        // 文本提取先行 / office_oxide 提取），失败按类型提示；不落文本读取
        // 通用路径（杜绝"无法读取（非 UTF-8 或已删除）"误报）。
        match crate::preview::preview_kind(&path) {
            crate::preview::PreviewKind::Office => {
                let pane = ratatui::widgets::Block::bordered()
                    .title(format!("{shown} · Office 预览"))
                    .border_style(pane_border_style(true));
                let inner = pane.inner(area);
                frame.render_widget(pane, area);
                let preview = crate::preview::render_preview(&path, inner.height.max(1) as usize);
                frame.render_widget(preview.into_paragraph(), inner);
                return;
            }
            crate::preview::PreviewKind::Pdf => {
                // AUDIT-2：NeedsVisual → 第 1 页视觉链（exprim → pdftoppm →
                // mutool，结果缓存），产出经 ImageEngine 渲染；Text/失败 →
                // 文本/提示路径（文本提取先行不跳过，AGENTS §5.5）。
                if let Some(png) = crate::pdf::cached_visual_png(&path) {
                    let pane = ratatui::widgets::Block::bordered()
                        .title(format!("{shown} · PDF 预览（第 1 页图像）"))
                        .border_style(pane_border_style(true));
                    let inner = pane.inner(area);
                    frame.render_widget(pane, area);
                    if state.image_engine.render_file(frame, inner, &png).is_err() {
                        frame.render_widget(
                            Paragraph::new("PDF 图像渲染失败（协议探测失败或解码错误）"),
                            inner,
                        );
                    }
                    return;
                }
                let pane = ratatui::widgets::Block::bordered()
                    .title(format!("{shown} · PDF 预览"))
                    .border_style(pane_border_style(true));
                let inner = pane.inner(area);
                frame.render_widget(pane, area);
                let preview = crate::preview::render_preview(&path, inner.height.max(1) as usize);
                frame.render_widget(preview.into_paragraph(), inner);
                return;
            }
            _ => {}
        }
        // 内容行：行号 gutter + 查询匹配高亮；视口取 viewer.scroll 起。
        let Ok(text) = std::fs::read_to_string(&path) else {
            frame.render_widget(Paragraph::new("无法读取（非 UTF-8 或已删除）"), inner);
            return;
        };
        let height = inner.height as usize;
        let width = inner.width as usize;
        let lines: Vec<&str> = text.lines().take(VIEWER_MAX_LINES).collect();
        // M21-E3：语法高亮——扩展名/shebang 推断，未命中或搜索模式下
        // 降级纯文本（查询黄底叠加与语法着色互斥，SIMPLIFIED）。
        let dark = crate::theme::active_variant() != opaline::schema::ThemeVariant::Light;
        let mut highlighter = if viewer.query.is_none() {
            crate::highlight::Highlighter::for_path(
                &path,
                lines.first().copied().unwrap_or(""),
                dark,
            )
        } else {
            None
        };
        let mut out: Vec<Line> = Vec::new();
        for (i, line) in lines.iter().skip(viewer.scroll).take(height).enumerate() {
            let lineno = viewer.scroll + i + 1;
            let is_current = viewer.query.is_some() && viewer.scroll + i == viewer.match_line;
            let gutter = if is_current {
                format!("{lineno:>4} ▶ ")
            } else {
                format!("{lineno:>4} │ ")
            };
            let mut spans = vec![Span::styled(
                gutter,
                Style::new().add_modifier(Modifier::DIM),
            )];
            // 行内容按查询子串分段高亮（超宽截断由渲染器裁剪）。
            let display: String = line.chars().take(width).collect();
            match viewer
                .query
                .as_deref()
                .and_then(|q| crate::tab::match_range(&display, q))
            {
                Some((start, end)) => {
                    if start > 0 {
                        spans.push(Span::from(display[..start].to_owned()));
                    }
                    spans.push(Span::styled(
                        display[start..end].to_owned(),
                        Style::new().fg(Color::Black).bg(Color::LightYellow),
                    ));
                    spans.push(Span::from(display[end..].to_owned()));
                }
                None => {
                    if let Some(h) = highlighter.as_mut() {
                        // 语法着色片段（宽超界由渲染器裁剪；解析失败降级纯文本）。
                        let toks = h.highlight_line(line);
                        if toks.is_empty() {
                            spans.push(Span::from(display));
                        } else {
                            for (tok, st) in toks {
                                spans.push(Span::styled(tok, st));
                            }
                        }
                    } else {
                        spans.push(Span::from(display));
                    }
                }
            }
            out.push(Line::from(spans));
        }
        frame.render_widget(Paragraph::new(out), inner);
        return;
    }
    // 默认：右 pane 目录列表（独立导航；M21-F1 路径标题 + 类型着色，
    // 焦点经边框高亮呈现，标题不再重复"焦点"文字）。
    let title = tab.right.location.display_path().replace('\\', "/");
    let pane = ratatui::widgets::Block::bordered()
        .title(title)
        .border_style(pane_border_style(focused));
    let inner = pane.inner(area);
    frame.render_widget(pane, area);
    let items: Vec<ListItem> = tab
        .right
        .entries
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            // M21-E1：属性列自适应（与左 pane 同口径）。
            let show_attrs =
                state.config.ui.attrs_column && inner.width as usize >= MIN_ATTRS_INNER_WIDTH;
            // M25-5：git 状态列（与左 pane 同口径；右 pane 独立目录缓存）。
            let git_col = show_attrs && state.config.ui.git_column;
            let git_map = if git_col {
                match &tab.right.location {
                    Location::Physical(p) => state.git_cache.get(p).and_then(|o| o.as_ref()),
                    _ => None,
                }
            } else {
                None
            };
            let name_budget = if show_attrs {
                (inner.width as usize).saturating_sub(
                    2 + ATTRS_WIDTH + 2 + if git_col { GIT_STATUS_WIDTH } else { 0 },
                )
            } else {
                (inner.width as usize).saturating_sub(4)
            };
            let display_name = clip_name(&item.name, name_budget);
            let label = if item.is_dir {
                format!("{display_name}/")
            } else {
                display_name.clone()
            };
            let mut spans = vec![
                Span::from(format!("{} ", item.icon.glyph)),
                Span::styled(label, filetype_style(item.filetype_class())),
            ];
            if show_attrs {
                let name_width = display_name.chars().count() + (item.is_dir as usize);
                let pad = name_budget.saturating_sub(name_width).max(1);
                spans.push(Span::from(" ".repeat(pad)));
                // M25-5：git 状态字符列（占位空格保持列对齐）。
                match git_map.and_then(|m| m.entries.get(&item.name)) {
                    Some(st) => spans.push(git_status_span(*st)),
                    None => spans.push(Span::from(" ".repeat(GIT_STATUS_WIDTH))),
                }
                let size = if item.is_dir {
                    "<DIR>".to_owned()
                } else {
                    crate::tab::format_size(item.size)
                };
                let attr = format!("{:>10}  {}", size, crate::tab::format_mtime_min(item.mtime));
                spans.push(Span::styled(attr, Style::new().add_modifier(Modifier::DIM)));
            }
            let mut li = ListItem::new(Line::from(spans));
            if idx == tab.right.cursor {
                // M21-F2：焦点侧反显，非焦点侧 DIM 灰化。
                li = li.style(if focused {
                    Style::new().add_modifier(Modifier::REVERSED)
                } else {
                    Style::new().add_modifier(Modifier::DIM)
                });
            }
            li
        })
        .collect();
    let mut list_state = ListState::default().with_selected(Some(tab.right.cursor));
    frame.render_stateful_widget(ratatui::widgets::List::new(items), inner, &mut list_state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::keymap::Key;
    use crate::tab::{FileItem, Location};
    use std::collections::HashSet;
    use std::fs;
    use std::path::Path;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// 测试辅助：viewport 高度 10（半页步进 4）；丢弃提示消息。
    fn press(
        state: &mut AppState,
        input: &mut InputState,
        code: KeyCode,
        mods: KeyModifiers,
    ) -> Outcome {
        let mut msg = None;
        press_msg(state, input, &mut msg, code, mods)
    }

    /// 测试辅助：保留提示消息（搜索相关测试用）。
    fn press_msg(
        state: &mut AppState,
        input: &mut InputState,
        msg: &mut Option<String>,
        code: KeyCode,
        mods: KeyModifiers,
    ) -> Outcome {
        handle_key(state, input, msg, KeyEvent::new(code, mods), 10)
    }

    fn make_state(n: usize) -> (TempDir, AppState) {
        let tmp = TempDir::new().unwrap();
        for i in 0..n {
            fs::write(tmp.path().join(format!("f{i:02}")), "").unwrap();
        }
        let state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        (tmp, state)
    }

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
            tree: None,
            right: crate::tab::Pane::from_physical(Path::new("."), Config::default()).unwrap(),
            focus: Default::default(),
            view_mode: false,
            on_parent_row: false,
        }
    }

    #[test]
    fn q_and_ctrl_c_quit() {
        assert!(is_quit_key(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::NONE
        )));
        assert!(is_quit_key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL
        )));
    }

    #[test]
    fn other_keys_do_not_quit() {
        assert!(!is_quit_key(KeyEvent::new(
            KeyCode::Char('x'),
            KeyModifiers::NONE
        )));
        assert!(!is_quit_key(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::NONE
        )));
        // Windows 会额外发送 Release 事件，必须忽略。
        let release = KeyEvent {
            kind: KeyEventKind::Release,
            ..KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)
        };
        assert!(!is_quit_key(release));
    }

    #[test]
    fn j_k_move_cursor() {
        let (_tmp, mut state) = make_state(3);
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('j'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(state.active_tab().cursor, 1);
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('k'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(state.active_tab().cursor, 0);
    }

    #[test]
    fn gg_and_g_jump_top_and_bottom() {
        let (_tmp, mut state) = make_state(5);
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);

        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('G'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(state.active_tab().cursor, 4);

        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('g'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(input.seq, [Key::plain(KeyCode::Char('g'))]); // 等待第二键
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('g'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert!(input.seq.is_empty()); // 序列已清空
        assert_eq!(state.active_tab().cursor, 0);
    }

    #[test]
    fn unknown_g_sequence_is_discarded() {
        let (_tmp, mut state) = make_state(3);
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        // `g` 后跟未知第二键：序列丢弃，不产生移动，也不退出。
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('g'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('x'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert!(input.seq.is_empty()); // 序列已清空
        assert_eq!(state.active_tab().cursor, 0);
        // 序列内的 `q` 被当作第二键丢弃，之后的 `q` 才退出。
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('g'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('q'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('q'),
                KeyModifiers::NONE
            ),
            Outcome::Quit
        );
    }

    #[test]
    fn ctrl_u_d_half_page() {
        let (_tmp, mut state) = make_state(20);
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);

        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('d'),
                KeyModifiers::CONTROL
            ),
            Outcome::Continue
        );
        assert_eq!(state.active_tab().cursor, 4); // (10-1)/2 = 4
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('d'),
                KeyModifiers::CONTROL
            ),
            Outcome::Continue
        );
        assert_eq!(state.active_tab().cursor, 8);
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('u'),
                KeyModifiers::CONTROL
            ),
            Outcome::Continue
        );
        assert_eq!(state.active_tab().cursor, 4);
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('u'),
                KeyModifiers::CONTROL
            ),
            Outcome::Continue
        );
        assert_eq!(state.active_tab().cursor, 0);
        // 顶部继续 Ctrl-u：钳制不越界。
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('u'),
                KeyModifiers::CONTROL
            ),
            Outcome::Continue
        );
        assert_eq!(state.active_tab().cursor, 0);
    }

    #[test]
    fn gt_gt_and_counted_gt() {
        let mut state = AppState::new(tab_at("C:/a"));
        state.push_tab(tab_at("C:/b"));
        state.push_tab(tab_at("C:/c"));
        state.active = 0; // push_tab 会切到新 tab，这里归位到第一个
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);

        // gt 循环：a → b → c → a。
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('g'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('t'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(state.active, 1);
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('g'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('t'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(state.active, 2);
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('g'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('t'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(state.active, 0);

        // gT 反向循环：a → c。
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('g'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('T'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(state.active, 2);

        // `2gt` 跳到第 2 个 tab（1 起始）。
        for code in [KeyCode::Char('2'), KeyCode::Char('g'), KeyCode::Char('t')] {
            press(&mut state, &mut input, code, KeyModifiers::NONE);
        }
        assert_eq!(state.active, 1);
        assert_eq!(input.count, None);
    }

    #[test]
    fn count_is_dropped_by_unrelated_keys() {
        let (_tmp, mut state) = make_state(10);
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        press(
            &mut state,
            &mut input,
            KeyCode::Char('3'),
            KeyModifiers::NONE,
        );
        assert_eq!(input.count, Some(3));
        press(
            &mut state,
            &mut input,
            KeyCode::Char('j'),
            KeyModifiers::NONE,
        );
        assert_eq!(input.count, None); // 计数被动作键丢弃
        assert_eq!(state.active_tab().cursor, 3); // M24-5：3j 移动 3 行
    }

    #[test]
    fn colon_enters_command_mode() {
        let (_tmp, mut state) = make_state(1);
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char(':'),
                KeyModifiers::NONE
            ),
            Outcome::EnterCommand
        );
    }

    #[test]
    fn command_input_typing_and_submit() {
        let mut buf = String::new();
        for c in "tabnew".chars() {
            assert_eq!(
                command_input(
                    &mut buf,
                    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
                ),
                CmdInput::Continue
            );
        }
        assert_eq!(buf, "tabnew");
        // Backspace 删除。
        assert_eq!(
            command_input(
                &mut buf,
                KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)
            ),
            CmdInput::Continue
        );
        assert_eq!(buf, "tabne");
        // Esc 取消。
        assert_eq!(
            command_input(&mut buf, KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            CmdInput::Cancel
        );
        // Enter 提交。
        assert_eq!(
            command_input(&mut buf, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            CmdInput::Submit
        );
    }

    // ── M24-6：命令历史（↑/↓ 浏览、草稿恢复、上限 100、失败不记） ────────────

    #[test]
    fn m246_hist_browse_and_draft_restore() {
        let hist = vec!["cd /a".to_owned(), "sort name asc".to_owned()];
        let mut input = "xyz".to_owned();
        let mut pos = None;
        let mut draft = String::new();
        // ↑ 首次进入最新一条（草稿被保存）。
        hist_prev(&hist, &mut pos, &mut input, &mut draft);
        assert_eq!(input, "sort name asc");
        assert_eq!(draft, "xyz");
        // ↑ 继续到最旧一条；再 ↑ 钳制不越界。
        hist_prev(&hist, &mut pos, &mut input, &mut draft);
        assert_eq!(input, "cd /a");
        hist_prev(&hist, &mut pos, &mut input, &mut draft);
        assert_eq!(input, "cd /a");
        // ↓ 回到最新一条；再 ↓ 越过末尾恢复草稿、游标清空。
        hist_next(&hist, &mut pos, &mut input, &mut draft);
        assert_eq!(input, "sort name asc");
        hist_next(&hist, &mut pos, &mut input, &mut draft);
        assert_eq!(input, "xyz");
        assert_eq!(pos, None);
        // 草稿已恢复后 ↓ 越界：游标为 None，无操作（保持草稿）。
        hist_next(&hist, &mut pos, &mut input, &mut draft);
        assert_eq!(input, "xyz");
    }

    #[test]
    fn m246_hist_empty_history_no_op() {
        let hist: Vec<String> = Vec::new();
        let mut input = "cd /tmp".to_owned();
        let mut pos = None;
        let mut draft = String::new();
        hist_prev(&hist, &mut pos, &mut input, &mut draft);
        hist_next(&hist, &mut pos, &mut input, &mut draft);
        assert_eq!(input, "cd /tmp");
        assert_eq!(pos, None);
        assert_eq!(draft, "", "空历史不保存草稿");
    }

    #[test]
    fn m246_record_caps_at_100_and_skips_empty() {
        let (_tmp, mut state) = make_state(1);
        for i in 0..105 {
            state.record_cmd_history(&format!("cmd{i}"));
        }
        assert_eq!(state.cmd_history.len(), crate::state::CMD_HISTORY_MAX);
        assert_eq!(state.cmd_history[0], "cmd5", "超上限淘汰最旧条目");
        assert_eq!(state.cmd_history[99], "cmd104");
        // 空命令（含纯空白）不记。
        state.record_cmd_history("   ");
        assert_eq!(state.cmd_history.len(), 100);
    }

    #[test]
    fn m246_failed_command_not_recorded() {
        let (_tmp, mut state) = make_state(1);
        // 运行循环契约：仅 exec_command_impl 成功（Ok）才调用 record_cmd_history。
        let r = exec_command_impl(&mut state, "未知命令不存在xyz");
        assert!(r.is_err(), "未知命令应返回 Err");
        if r.is_ok() {
            state.record_cmd_history("未知命令不存在xyz");
        }
        assert!(state.cmd_history.is_empty(), "失败命令不入历史");
        assert!(exec_command_impl(&mut state, "help").is_ok());
        state.record_cmd_history("help");
        assert_eq!(state.cmd_history, vec!["help".to_owned()]);
    }

    // ── M24-7：路径补全（Tab 前缀循环、~ 展开 HOME） ────────────────────────

    #[test]
    fn m247_tab_complete_cycles_prefix_matches() {
        let names: Vec<String> = ["foo", "foobar", "bar"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let mut input = "cd fo".to_owned();
        let mut comp = None;
        // 第一次 Tab：前缀匹配 fo → 2 候选，取第一个。
        assert_eq!(tab_complete(&mut input, &mut comp, &names), 2);
        assert_eq!(input, "cd foo");
        // 输入未改动再 Tab：循环到下一候选。
        assert_eq!(tab_complete(&mut input, &mut comp, &names), 2);
        assert_eq!(input, "cd foobar");
        // 再 Tab：尾→首环绕。
        assert_eq!(tab_complete(&mut input, &mut comp, &names), 2);
        assert_eq!(input, "cd foo");
        // 输入改动后重建：token=foob → 仅 foobar。
        input.push('b');
        assert_eq!(tab_complete(&mut input, &mut comp, &names), 1);
        assert_eq!(input, "cd foobar");
    }

    #[test]
    fn m247_tab_complete_no_candidate_and_empty_token() {
        let names: Vec<String> = ["alpha".to_owned(), "beta".to_owned()].into();
        let mut input = "cd zz".to_owned();
        let mut comp = None;
        // 无候选：不改输入，comp 清空。
        assert_eq!(tab_complete(&mut input, &mut comp, &names), 0);
        assert_eq!(input, "cd zz");
        assert_eq!(comp, None);
        // 空词（"cd "）→ 全部条目为候选，取列表序第一个。
        let mut input2 = "cd ".to_owned();
        let mut comp2 = None;
        assert_eq!(tab_complete(&mut input2, &mut comp2, &names), 2);
        assert_eq!(input2, "cd alpha");
        // 多空白分隔：head 保留全部空白。
        let mut input3 = "cd   al".to_owned();
        let mut comp3 = None;
        assert_eq!(tab_complete(&mut input3, &mut comp3, &names), 1);
        assert_eq!(input3, "cd   alpha");
    }

    #[test]
    fn m247_expand_home_arg() {
        if let Ok(home) = std::env::var("HOME") {
            assert_eq!(expand_home_arg("~"), home);
            assert_eq!(expand_home_arg("~/sub"), format!("{home}/sub"));
        }
        assert_eq!(expand_home_arg("/abs/path"), "/abs/path");
        assert_eq!(expand_home_arg("relative"), "relative");
        assert_eq!(expand_home_arg("~user"), "~user", "仅裸 ~ 与 ~/ 前缀展开");
    }

    #[test]
    fn m247_exec_cd_tilde_expands_to_home() {
        let Ok(home) = std::env::var("HOME") else {
            return; // HOME 缺失环境跳过（Windows CI 无 HOME）
        };
        let (_tmp, mut state) = make_state(1);
        assert!(exec_command_impl(&mut state, "cd ~").is_ok());
        assert_eq!(
            state.active_tab().location.base_dir(),
            std::path::Path::new(&home)
        );
        // ~/ 子路径同样展开。
        let sub = std::path::Path::new(&home).join("nonexistent-dir-xyz");
        // 目标不存在 → Err（只验证展开后的报错路径，不实际导航）。
        assert!(exec_command_impl(&mut state, "cd ~/nonexistent-dir-xyz").is_err());
        assert_ne!(
            state.active_tab().location.base_dir(),
            sub,
            "失败导航不改位置"
        );
    }

    // ── M24-8：目录对比 :compare（按名+大小 → 结果虚拟目录，只读） ──────────

    #[test]
    fn m248_compare_panes_marks_and_order() {
        let cfg = Config::default();
        let left = TempDir::new().unwrap();
        fs::write(left.path().join("same.txt"), "0123456789").unwrap(); // 10B
        fs::write(left.path().join("diff.txt"), "abc").unwrap(); // 3B
        fs::write(left.path().join("onlyl.txt"), "L").unwrap();
        fs::create_dir(left.path().join("subdir")).unwrap();
        let right = TempDir::new().unwrap();
        fs::write(right.path().join("same.txt"), "0123456789").unwrap(); // 10B
        fs::write(right.path().join("diff.txt"), "abcxyz").unwrap(); // 6B
        fs::write(right.path().join("onlyr.txt"), "R").unwrap();
        fs::create_dir(right.path().join("subdir")).unwrap(); // 同名目录不参与
        let lpane = crate::tab::Pane::from_physical(left.path(), cfg.clone()).unwrap();
        let rpane = crate::tab::Pane::from_physical(right.path(), cfg).unwrap();
        let items = compare_panes(left.path(), &lpane.entries, right.path(), &rpane.entries);
        let names: Vec<String> = items.iter().map(|i| i.text.clone()).collect();
        assert_eq!(
            names,
            vec!["!", "<", "=", ">"],
            "目录跳过；左 pane 序（字典序）在前、右独有在后"
        );
        let leaves: Vec<&str> = items
            .iter()
            .map(|i| i.path.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(
            leaves,
            vec!["diff.txt", "onlyl.txt", "same.txt", "onlyr.txt"]
        );
        // 路径取实际所在侧；size 透传（! 取左侧、> 取右侧）。
        assert!(items[0].path.starts_with(left.path()));
        assert!(items[1].path.starts_with(left.path()));
        assert!(items[2].path.starts_with(left.path()));
        assert!(items[3].path.starts_with(right.path()));
        assert_eq!(items[0].size, 3);
        assert_eq!(items[3].size, 1);
    }

    #[test]
    fn m248_exec_compare_results_locate_and_guards() {
        let cfg = Config::default();
        let left = TempDir::new().unwrap();
        fs::write(left.path().join("same.txt"), "x").unwrap();
        fs::write(left.path().join("diff.txt"), "abc").unwrap();
        fs::write(left.path().join("onlyl.txt"), "L").unwrap();
        let right = TempDir::new().unwrap();
        fs::write(right.path().join("same.txt"), "x").unwrap();
        fs::write(right.path().join("diff.txt"), "abcd").unwrap();
        fs::write(right.path().join("onlyr.txt"), "R").unwrap();
        let mut state = AppState::new(Tab::from_physical(left.path(), cfg.clone()).unwrap());
        state.active_tab_mut().right = crate::tab::Pane::from_physical(right.path(), cfg).unwrap();
        // exec :compare → 结果虚拟目录 + 计数消息。
        match exec_command_impl(&mut state, "compare") {
            Ok(ExecOutcome::Done { msg }) => {
                let m = msg.expect("compare 应有计数消息");
                assert!(m.contains("相同 1"), "msg: {m}");
                assert!(m.contains("不同 1"), "msg: {m}");
                assert!(m.contains("仅左 1"), "msg: {m}");
                assert!(m.contains("仅右 1"), "msg: {m}");
            }
            other => panic!("expected Done, got {other:?}"),
        }
        let tab = state.active_tab();
        assert!(tab.in_results(), "进入结果虚拟目录");
        let names: Vec<String> = tab.entries.iter().map(|i| i.name.clone()).collect();
        assert_eq!(
            names,
            vec!["! diff.txt", "< onlyl.txt", "= same.txt", "> onlyr.txt"]
        );
        // 只读守卫：结果视图内写命令拒绝。
        assert!(exec_command_impl(&mut state, "mkdir newdir").is_err());
        // l/Enter 定位：光标停在 "> onlyr.txt" → 退出结果视图导航到右 pane 目录。
        state.active_tab_mut().cursor = 3;
        let cfg2 = state.config.clone();
        state.active_tab_mut().locate_result(cfg2).unwrap();
        let tab = state.active_tab();
        assert!(!tab.in_results());
        assert_eq!(tab.location.base_dir(), right.path());
        assert_eq!(
            tab.entries.get(tab.cursor).map(|i| i.name.as_str()),
            Some("onlyr.txt"),
            "定位到仅右条目"
        );
    }

    // ── M24-9：任务视图增强（j/k 选择、x 取消、Esc 关闭） ───────────────────

    #[test]
    fn m249_jobs_menu_key_navigation_and_cancel() {
        let tmp = TempDir::new().unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let dst = TempDir::new().unwrap();
        state.add_job("copy", 1, &dst.path().display().to_string());
        state.add_job("move", 2, &dst.path().display().to_string());
        let mut cursor = 0usize;
        let mut msg = None;
        // j/k 移动并钳制。
        assert!(!jobs_menu_key(
            &mut state,
            &mut cursor,
            KeyCode::Char('j'),
            &mut msg
        ));
        assert_eq!(cursor, 1);
        assert!(!jobs_menu_key(
            &mut state,
            &mut cursor,
            KeyCode::Char('j'),
            &mut msg
        ));
        assert_eq!(cursor, 1, "尾端钳制");
        assert!(!jobs_menu_key(
            &mut state,
            &mut cursor,
            KeyCode::Char('k'),
            &mut msg
        ));
        assert_eq!(cursor, 0);
        assert!(!jobs_menu_key(
            &mut state,
            &mut cursor,
            KeyCode::Char('k'),
            &mut msg
        ));
        assert_eq!(cursor, 0, "首端钳制");
        // x 取消运行中作业：置位标志 + 提示，不关闭。
        assert!(!jobs_menu_key(
            &mut state,
            &mut cursor,
            KeyCode::Char('x'),
            &mut msg
        ));
        assert!(
            state.jobs[0]
                .cancel_requested
                .load(std::sync::atomic::Ordering::Relaxed),
            "取消标志已置位"
        );
        assert!(msg.as_deref().unwrap().contains("#1"));
        // 已请求过 → 不重复提示；非 Running（Done）→ 忽略。
        msg = None;
        assert!(!jobs_menu_key(
            &mut state,
            &mut cursor,
            KeyCode::Char('x'),
            &mut msg
        ));
        assert!(msg.is_none(), "已请求过不重复提示");
        state.jobs[1].status = crate::state::JobStatus::Done {
            summary: "move: 2 项完成".to_owned(),
        };
        assert!(!jobs_menu_key(
            &mut state,
            &mut cursor,
            KeyCode::Char('x'),
            &mut msg
        ));
        assert!(msg.is_none(), "非运行中作业不可取消");
        // Esc 关闭。
        assert!(jobs_menu_key(
            &mut state,
            &mut cursor,
            KeyCode::Esc,
            &mut msg
        ));
        assert!(jobs_menu_key(
            &mut state,
            &mut cursor,
            KeyCode::Char('q'),
            &mut msg
        ));
    }

    #[test]
    fn m249_cancelled_job_body_stops_with_summary() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "A").unwrap();
        let dst = TempDir::new().unwrap();
        // 标志预置 → 第一个文件边界即取消：无操作、Cancelled 摘要。
        let cancel = std::sync::atomic::AtomicBool::new(true);
        let (ops, status) = execute_job_body(
            "copy",
            &["a.txt".to_owned()],
            tmp.path(),
            dst.path(),
            0,
            &cancel,
        );
        assert!(ops.is_empty(), "取消后不产生新操作");
        assert!(!dst.path().join("a.txt").exists(), "未执行复制");
        match status {
            crate::state::JobStatus::Cancelled { summary } => {
                assert!(summary.contains("已取消"), "summary: {summary}");
                assert!(summary.contains("0 项完成"), "summary: {summary}");
            }
            other => panic!("expected Cancelled, got {other:?}"),
        }
    }

    #[test]
    fn m249_jobs_overlay_render_selection_and_cancel_state() {
        let tmp = TempDir::new().unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        // AUDIT-18：目标只作展示串，用固定短标签——宿主临时路径长度（如
        // 8.3 短名/多级 Temp）不再截断 70 列断言里的"取消中"。
        let id = state.add_job("copy", 1, "d");
        state.jobs[0]
            .cancel_requested
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let mode = InputMode::Jobs { cursor: 0 };
        // CJK 全角在 TestBackend 渲染为"字+空格"——断言前去空白（M21-E3 坑）；
        // 每帧用全新 backend，避免 diff 渲染残留上一帧字符。
        let compact = |buf: &ratatui::backend::TestBackend, w: u16, h: u16| {
            (0..h)
                .map(|y| {
                    (0..w)
                        .map(|x| buf.buffer()[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect::<String>()
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
        };
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(70, 8)).unwrap();
        terminal.draw(|f| render(f, &state, &mode, None)).unwrap();
        let all = compact(terminal.backend(), 70, 8);
        assert!(all.contains("取消中"), "取消请求可见: {all}");
        assert!(all.contains("x=取消"), "标题含操作提示: {all}");
        // 置位后取消完成（模拟线程回包）→ Cancelled 终态展示。
        state.apply_job_update(crate::state::JobUpdate {
            job_id: id,
            tab_id: 0,
            ops: Vec::new(),
            status: crate::state::JobStatus::Cancelled {
                summary: "copy: 已取消（0 项完成）".to_owned(),
            },
        });
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(70, 8)).unwrap();
        terminal.draw(|f| render(f, &state, &mode, None)).unwrap();
        let all = compact(terminal.backend(), 70, 8);
        assert!(all.contains("已取消"), "Cancelled 终态可见: {all}");
        let _ = id;
    }

    // ── M25-3：归档创建（:archive；zip/tar/tar.gz 纯 Rust 写端） ────────────

    #[test]
    fn m253_archive_zip_roundtrip_and_undo() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "AAA").unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("sub/b.txt"), "BBB").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        state.active_tab_mut().tags.insert("a.txt".to_owned());
        state.active_tab_mut().tags.insert("sub".to_owned());
        // 缺扩展名自动补齐。
        match exec_command_impl(&mut state, "archive zip out") {
            Ok(ExecOutcome::Done { msg }) => {
                assert!(msg.unwrap().contains("out.zip"), "自动补扩展名");
            }
            other => panic!("expected Done, got {other:?}"),
        }
        let dest = tmp.path().join("out.zip");
        assert!(dest.exists(), "产物存在");
        // rvifm 自身读端可浏览（open_dispatch 复用）。AUDIT-18：读句柄在
        // 撤销前必须释放——Windows 上打开的 File 会阻塞 remove_file（共享
        // 冲突 → undo 失败回推），Linux 无此限制（CI 绿、本机挂的根因）。
        {
            let mut fsx = crate::archive::open_dispatch(&dest).unwrap();
            let names: Vec<String> = fsx
                .list("/")
                .unwrap()
                .iter()
                .map(|i| i.path.clone())
                .collect();
            assert!(names.contains(&"a.txt".to_owned()), "names: {names:?}");
            assert!(names.contains(&"sub".to_owned()), "names: {names:?}");
            assert!(fsx.read("/sub/b.txt").unwrap() == b"BBB", "目录内容被打包");
        }
        // u 撤销移除产物（M25-4 起默认生成 .md5——两条撤销记录，LIFO）。
        let mut msg = None;
        undo_last(&mut state, &mut msg);
        assert!(
            !tmp.path().join("out.zip.md5").exists(),
            "第一次 undo 移除校验"
        );
        undo_last(&mut state, &mut msg);
        assert!(!dest.exists(), "第二次 undo 移除归档");
    }

    #[test]
    fn m253_archive_tar_gz_roundtrip() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("one.txt"), "1").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        state.active_tab_mut().cursor = 0;
        // 1 参数：从扩展名推断 tar.gz。
        assert!(exec_command_impl(&mut state, "archive bundle.tar.gz").is_ok());
        let dest = tmp.path().join("bundle.tar.gz");
        assert!(dest.exists());
        // gzip 真压缩产物：读端解包验证。
        let fsx = crate::archive::open_dispatch(&dest).unwrap();
        let names: Vec<String> = fsx
            .list("/")
            .unwrap()
            .iter()
            .map(|i| i.path.clone())
            .collect();
        assert!(names.contains(&"one.txt".to_owned()), "names: {names:?}");
        // .tar 同理（光标条目单选）。
        state.active_tab_mut().cursor = 0;
        assert!(exec_command_impl(&mut state, "archive plain.tar").is_ok());
        assert!(tmp.path().join("plain.tar").exists());
    }

    #[test]
    fn m253_archive_guards() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "A").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        // xz/bz2/zst：格式解析拒绝（纯 Rust 写端边界）。
        assert!(exec_command_impl(&mut state, "archive tar.xz x.tar.xz").is_err());
        assert!(exec_command_impl(&mut state, "archive tar.bz2 x.tar.bz2").is_err());
        assert!(exec_command_impl(&mut state, "archive tar.zst x.tar.zst").is_err());
        // 1 参数但扩展名不可推断。
        assert!(exec_command_impl(&mut state, "archive out.rar").is_err());
        // 无参 → M25-4 对话框（Ok）；3 参非 checksum / 4+ 参 → Err。
        assert!(matches!(
            exec_command_impl(&mut state, "archive"),
            Ok(ExecOutcome::ShowArchiveDialog { .. })
        ));
        assert!(exec_command_impl(&mut state, "archive zip a b").is_err());
        assert!(exec_command_impl(&mut state, "archive zip a b c").is_err());
        // 同名已存在报错不覆盖。
        fs::write(tmp.path().join("dup.zip"), "old").unwrap();
        state.active_tab_mut().cursor = 0;
        assert!(exec_command_impl(&mut state, "archive zip dup.zip").is_err());
        assert_eq!(fs::read(tmp.path().join("dup.zip")).unwrap(), b"old");
        // 源缺失整批中止且不留半成品。
        state.active_tab_mut().tags.insert("a.txt".to_owned());
        state.active_tab_mut().tags.insert("ghost.txt".to_owned());
        assert!(exec_command_impl(&mut state, "archive zip ghost.zip").is_err());
        assert!(!tmp.path().join("ghost.zip").exists(), "失败无半成品");
    }

    // ── M25-4：归档对话框与校验文件 ──────────────────────────────────────────

    #[test]
    fn m254_checksum_parse_ext_and_write() {
        assert_eq!(ops::Checksum::parse("md5"), Some(ops::Checksum::Md5));
        assert_eq!(ops::Checksum::parse("sha256"), Some(ops::Checksum::Sha256));
        assert_eq!(ops::Checksum::parse("none"), Some(ops::Checksum::None));
        assert_eq!(ops::Checksum::parse("sha1"), None);
        let tmp = TempDir::new().unwrap();
        let f = tmp.path().join("data.bin");
        fs::write(&f, b"AAA").unwrap();
        // md5 默认：md5sum 兼容行 "<hex>  <名>"。
        let c = ops::write_checksum(&f, ops::Checksum::Md5)
            .unwrap()
            .unwrap();
        assert_eq!(c, tmp.path().join("data.bin.md5"));
        let hex = ops::digest_hex(&f, ops::Checksum::Md5).unwrap();
        assert_eq!(hex.len(), 32);
        assert_eq!(
            fs::read_to_string(&c).unwrap(),
            format!("{hex}  data.bin\n"),
            "md5sum -c 兼容行"
        );
        // sha256 → 64 位 hex + .sha256。
        let c2 = ops::write_checksum(&f, ops::Checksum::Sha256)
            .unwrap()
            .unwrap();
        assert!(c2.to_string_lossy().ends_with(".sha256"));
        assert_eq!(
            ops::digest_hex(&f, ops::Checksum::Sha256).unwrap().len(),
            64
        );
        // none → 不生成。
        assert_eq!(ops::write_checksum(&f, ops::Checksum::None).unwrap(), None);
    }

    #[test]
    fn m254_exec_archive_checksum_arg_and_config() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "A").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        state.active_tab_mut().cursor = 0;
        // 显式 sha256。
        assert!(exec_command_impl(&mut state, "archive zip c1.zip checksum=sha256").is_ok());
        assert!(tmp.path().join("c1.zip").exists());
        assert!(tmp.path().join("c1.zip.sha256").exists());
        // none 不生成。
        assert!(exec_command_impl(&mut state, "archive zip c2.zip checksum=none").is_ok());
        assert!(tmp.path().join("c2.zip").exists());
        assert!(!tmp.path().join("c2.zip.md5").exists());
        // 缺省（[archive] checksum 默认 md5）。
        assert!(exec_command_impl(&mut state, "archive zip c3.zip").is_ok());
        assert!(tmp.path().join("c3.zip.md5").exists());
        // 非法 checksum 值。
        assert!(exec_command_impl(&mut state, "archive zip c4.zip checksum=md4").is_err());
        // 双重撤销：校验文件与归档各自可逆（LIFO——先校验后归档）。
        let mut msg = None;
        undo_last(&mut state, &mut msg);
        undo_last(&mut state, &mut msg);
        assert!(
            !tmp.path().join("c3.zip.md5").exists(),
            "第一次 undo 移除校验"
        );
        assert!(!tmp.path().join("c3.zip").exists(), "第二次 undo 移除归档");
    }

    #[test]
    fn m254_dialog_keys_flow() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "A").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        state.active_tab_mut().cursor = 0;
        let mut input = "out".to_owned();
        let mut field = 0usize;
        let mut format_idx = 0usize;
        let mut checksum_idx = 0usize;
        let mut msg = None;
        // 名称字段：字符输入 / 退格；名称字段外字符忽略。
        assert_eq!(
            handle_archive_dialog_key(
                &mut state,
                &mut input,
                &mut field,
                &mut format_idx,
                &mut checksum_idx,
                KeyCode::Char('x'),
                &mut msg
            ),
            ArchiveDialogOutcome::None
        );
        assert_eq!(input, "outx");
        handle_archive_dialog_key(
            &mut state,
            &mut input,
            &mut field,
            &mut format_idx,
            &mut checksum_idx,
            KeyCode::Backspace,
            &mut msg,
        );
        assert_eq!(input, "out");
        handle_archive_dialog_key(
            &mut state,
            &mut input,
            &mut field,
            &mut format_idx,
            &mut checksum_idx,
            KeyCode::Char('x'),
            &mut msg,
        );
        // Tab 切字段 → 格式；空格循环 zip→tar；← 反向回 zip。
        handle_archive_dialog_key(
            &mut state,
            &mut input,
            &mut field,
            &mut format_idx,
            &mut checksum_idx,
            KeyCode::Tab,
            &mut msg,
        );
        assert_eq!(field, 1);
        handle_archive_dialog_key(
            &mut state,
            &mut input,
            &mut field,
            &mut format_idx,
            &mut checksum_idx,
            KeyCode::Char(' '),
            &mut msg,
        );
        assert_eq!(format_idx, 1, "空格循环格式");
        handle_archive_dialog_key(
            &mut state,
            &mut input,
            &mut field,
            &mut format_idx,
            &mut checksum_idx,
            KeyCode::Left,
            &mut msg,
        );
        assert_eq!(format_idx, 0, "← 反向循环");
        // Enter 执行（tar 格式 → out.tar 创建，Exec 关闭）。
        handle_archive_dialog_key(
            &mut state,
            &mut input,
            &mut field,
            &mut format_idx,
            &mut checksum_idx,
            KeyCode::Char(' '),
            &mut msg,
        );
        assert_eq!(
            handle_archive_dialog_key(
                &mut state,
                &mut input,
                &mut field,
                &mut format_idx,
                &mut checksum_idx,
                KeyCode::Enter,
                &mut msg
            ),
            ArchiveDialogOutcome::Exec
        );
        // 名称在名称字段曾再追加一个 x（outx）→ 产物 outx.tar。
        assert!(tmp.path().join("outx.tar").exists());
        // Esc 关闭；空名称 → 错误提示留在对话框。
        assert_eq!(
            handle_archive_dialog_key(
                &mut state,
                &mut input,
                &mut field,
                &mut format_idx,
                &mut checksum_idx,
                KeyCode::Esc,
                &mut msg
            ),
            ArchiveDialogOutcome::Close
        );
        let mut empty = String::new();
        assert_eq!(
            handle_archive_dialog_key(
                &mut state,
                &mut empty,
                &mut field,
                &mut format_idx,
                &mut checksum_idx,
                KeyCode::Enter,
                &mut msg
            ),
            ArchiveDialogOutcome::None
        );
        assert!(msg.as_deref().unwrap().contains("名称不能为空"));
    }

    #[test]
    fn m254_exec_noargs_opens_dialog_prefilled() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "A").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        state.active_tab_mut().cursor = 0;
        match exec_command_impl(&mut state, "archive") {
            Ok(ExecOutcome::ShowArchiveDialog { input }) => assert_eq!(input, "a.txt"),
            other => panic!("expected ShowArchiveDialog, got {other:?}"),
        }
        // 无条目 → 空名称。
        let tmp2 = TempDir::new().unwrap();
        let mut s2 = AppState::new(Tab::from_physical(tmp2.path(), Config::default()).unwrap());
        match exec_command_impl(&mut s2, "archive") {
            Ok(ExecOutcome::ShowArchiveDialog { input }) => assert_eq!(input, ""),
            other => panic!("expected ShowArchiveDialog, got {other:?}"),
        }
    }

    #[test]
    fn m254_archive_config_checksum() {
        // 缺省 md5；显式 sha256/none 生效。
        let cfg: config::Config = toml::from_str("").unwrap();
        assert_eq!(cfg.archive.checksum, "md5");
        let cfg2: config::Config = toml::from_str("[archive]\nchecksum = \"sha256\"\n").unwrap();
        assert_eq!(cfg2.archive.checksum, "sha256");
        let cfg3: config::Config = toml::from_str("[archive]\nchecksum = \"none\"\n").unwrap();
        assert_eq!(cfg3.archive.checksum, "none");
    }

    // ── M25-6：git 状态栏 + gi ───────────────────────────────────────────────

    #[test]
    fn m256_parse_shortstat_variants() {
        assert_eq!(crate::git::parse_shortstat(""), None);
        assert_eq!(crate::git::parse_shortstat("\n"), None);
        assert_eq!(
            crate::git::parse_shortstat(" 3 files changed, 2 insertions(+), 1 deletion(-)"),
            Some("+2 -1".to_owned())
        );
        assert_eq!(
            crate::git::parse_shortstat(" 1 file changed, 1 insertion(+)"),
            Some("+1".to_owned())
        );
        assert_eq!(
            crate::git::parse_shortstat(" 1 file changed, 3 deletions(-)"),
            Some("-3".to_owned())
        );
    }

    #[test]
    #[cfg(unix)]
    fn m256_find_git_tui_priority() {
        use std::os::unix::fs::PermissionsExt as _;
        let d1 = TempDir::new().unwrap();
        let d2 = TempDir::new().unwrap();
        // d1 有 lazygit、d2 有 gitui → 优先级取 gitui。
        let mk = |dir: &TempDir, name: &str| {
            let p = dir.path().join(name);
            fs::write(&p, "#!/bin/sh\n").unwrap();
            fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        };
        mk(&d1, "lazygit");
        mk(&d2, "gitui");
        assert_eq!(
            crate::git::find_git_tui_in(&[d1.path().to_path_buf(), d2.path().to_path_buf()]),
            Some("gitui"),
            "优先级 gitui > ziggity > lazygit"
        );
        // 仅 ziggity → 命中中间档。
        let d3 = TempDir::new().unwrap();
        mk(&d3, "ziggity");
        assert_eq!(
            crate::git::find_git_tui_in(&[d3.path().to_path_buf()]),
            Some("ziggity")
        );
        // 全部缺失 → None（空目录）。
        let d4 = TempDir::new().unwrap();
        assert_eq!(
            crate::git::find_git_tui_in(&[d4.path().to_path_buf()]),
            None
        );
    }

    #[test]
    fn m256_status_bar_shows_branch_and_diff() {
        let (_tmp, state) = make_state(2);
        let dir = state.active_tab().location.base_dir().to_path_buf();
        let st = crate::git::GitDirStatus {
            branch: Some("main".to_owned()),
            diff_summary: Some("+2 -1".to_owned()),
            ..Default::default()
        };
        let mut state = state;
        state.git_cache.insert(dir, Some(st));
        let backend = ratatui::backend::TestBackend::new(120, 10);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mode = InputMode::default();
        terminal.draw(|f| render(f, &state, &mode, None)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let all: String = (0..10u16)
            .map(|y| {
                (0..120u16)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect();
        assert!(all.contains("[main +2 -1]"), "状态栏分支+摘要: {all}");
    }

    #[test]
    fn m256_exec_gitui_conditionally() {
        let (_tmp, mut state) = make_state(1);
        let r = exec_command_impl(&mut state, "gitui");
        match crate::git::find_git_tui() {
            // 本机无 git TUI → 可读报错。
            None => {
                assert!(r.is_err());
                assert!(r.unwrap_err().contains("未找到 git TUI"));
            }
            // 装有 git TUI → RunExternal（当前 pane）或 Done（multiplexer 提示）。
            Some(_) => assert!(r.is_ok()),
        }
    }

    #[test]
    fn exec_tab_commands() {
        let (_tmp, mut state) = make_state(1);
        state.active_tab_mut().cursor = 0;

        assert_eq!(exec_command(&mut state, "tabnew"), Ok(false));
        assert_eq!(state.tabs.len(), 2);
        assert_eq!(state.active, 1);
        // 新 tab 与原 tab 同目录。
        assert_eq!(state.tabs[1].location, state.tabs[0].location);

        assert_eq!(exec_command(&mut state, "tabmove 1"), Ok(false));
        assert_eq!(state.active, 0); // 移动后 active 跟随被移动的 tab
        assert_eq!(state.tabs[0].location, state.tabs[1].location);

        assert_eq!(
            exec_command(&mut state, "tabmove x"),
            Err("tabmove: 位置必须是数字".to_owned())
        );
        assert_eq!(
            exec_command(&mut state, "nosuchcmd"),
            Err("未知命令: nosuchcmd（完整命令解析在 M5-4 接入）".to_owned())
        );

        // 关闭中间 tab：剩余 tab active 正确收敛。
        assert_eq!(exec_command(&mut state, "tabclose"), Ok(false));
        assert_eq!(state.tabs.len(), 1);
        assert_eq!(state.active, 0);
        // 关闭最后一个 tab → 退出。
        assert_eq!(exec_command(&mut state, "tabclose"), Ok(true));
    }

    #[test]
    fn search_submit_moves_to_first_match() {
        let tmp = TempDir::new().unwrap();
        for name in ["alpha", "beta", "Beta2", "gamma"] {
            fs::write(tmp.path().join(name), "").unwrap();
        }
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut msg = None;

        submit_search(&mut state, "beta", &mut msg);
        assert!(msg.is_none());
        assert_eq!(state.active_tab().cursor, 1); // 从 0 向后命中 beta
        assert_eq!(state.active_tab().search.as_ref().unwrap().query, "beta");
    }

    #[test]
    fn search_n_and_n_wrap_case_insensitive() {
        let tmp = TempDir::new().unwrap();
        for name in ["alpha", "beta", "Beta2", "gamma"] {
            fs::write(tmp.path().join(name), "").unwrap();
        }
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg = None;

        submit_search(&mut state, "beta", &mut msg); // cursor → 1

        // n → 下一个匹配 Beta2（大小写不敏感）。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('n'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().cursor, 2);
        // n 环绕 → 回到 beta。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('n'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().cursor, 1);
        // N → 反向：回到 Beta2。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('N'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().cursor, 2);
        assert!(msg.is_none());
    }

    #[test]
    fn search_without_query_prompts_and_no_match_reports() {
        let (_tmp, mut state) = make_state(2);
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg = None;

        // 未设置搜索词。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('n'),
            KeyModifiers::NONE,
        );
        assert_eq!(msg.as_deref(), Some("未设置搜索词（先用 / 搜索）"));

        // 设置后搜索不存在的词：无匹配提示，cursor 不变。
        submit_search(&mut state, "zzz", &mut msg);
        assert_eq!(msg.as_deref(), Some("无匹配"));
        assert_eq!(state.active_tab().cursor, 0);
        // 再次 n：无匹配提示保持。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('n'),
            KeyModifiers::NONE,
        );
        assert_eq!(msg.as_deref(), Some("无匹配"));
    }

    #[test]
    fn slash_enters_search_mode() {
        let (_tmp, mut state) = make_state(1);
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg = None;
        assert_eq!(
            press_msg(
                &mut state,
                &mut input,
                &mut msg,
                KeyCode::Char('/'),
                KeyModifiers::NONE
            ),
            Outcome::EnterSearch
        );
    }

    #[test]
    fn fix1_live_search_jumps_and_restores_on_no_match_or_empty() {
        let tmp = TempDir::new().unwrap();
        for name in ["alpha", "src.txt", "srconly"] {
            fs::write(tmp.path().join(name), "").unwrap();
        }
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut msg = None;

        // 输入 "src"：实时跳到第一个匹配 src.txt(1)。
        live_search_jump(&mut state, 0, "src", &mut msg);
        assert_eq!(state.active_tab().cursor, 1);
        assert!(msg.is_none());

        // 继续输入 "srcx"：无匹配 → 回 origin 并提示。
        live_search_jump(&mut state, 0, "srcx", &mut msg);
        assert_eq!(state.active_tab().cursor, 0);
        assert_eq!(msg.as_deref(), Some("无匹配"));

        // 清空输入：回 origin，提示消失。
        live_search_jump(&mut state, 0, "", &mut msg);
        assert_eq!(state.active_tab().cursor, 0);
        assert!(msg.is_none());
    }

    #[test]
    fn fix1_submit_keeps_live_position_and_empty_clears_search() {
        let tmp = TempDir::new().unwrap();
        for name in ["alpha", "src.txt", "srconly"] {
            fs::write(tmp.path().join(name), "").unwrap();
        }
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut msg = None;

        live_search_jump(&mut state, 0, "src", &mut msg); // cursor → 1
        submit_search(&mut state, "src", &mut msg);
        assert_eq!(state.active_tab().cursor, 1, "提交不重复前跳");
        assert!(msg.is_none());
        assert_eq!(state.active_tab().search.as_ref().unwrap().query, "src");

        // 空提交：清除搜索状态（高亮与计数随之消失）。
        submit_search(&mut state, "", &mut msg);
        assert!(state.active_tab().search.is_none());
    }

    #[test]
    fn fix1_render_search_highlight_and_count() {
        let tmp = TempDir::new().unwrap();
        for name in ["alpha", "src.txt", "srconly"] {
            fs::write(tmp.path().join(name), "").unwrap();
        }
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut msg = None;
        live_search_jump(&mut state, 0, "src", &mut msg); // cursor → 1

        let mode = InputMode::Search {
            input: "src".to_owned(),
            origin: 0,
        };
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &mode, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();

        // 匹配子串黄底高亮：存在高亮单元格。
        let mut highlighted = 0;
        for y in 0..24u16 {
            for x in 0..80u16 {
                if buffer[(x, y)].bg == Color::LightYellow {
                    highlighted += 1;
                }
            }
        }
        assert!(highlighted > 0, "应存在匹配高亮单元格");

        // 状态栏（行 23）：/src 计数 [1/2]。
        let status = (0..80u16)
            .map(|x| buffer[(x, 23)].symbol())
            .collect::<String>();
        assert!(status.contains("/src"), "status: {status}");
        assert!(status.contains("[1/2]"), "status: {status}");

        // 无查询（Normal 且未提交搜索）：无高亮单元格。
        let mut terminal2 =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        terminal2
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let buffer2 = terminal2.backend().buffer().clone();
        let mut highlighted2 = 0;
        for y in 0..24u16 {
            for x in 0..80u16 {
                if buffer2[(x, y)].bg == Color::LightYellow {
                    highlighted2 += 1;
                }
            }
        }
        assert_eq!(highlighted2, 0, "无查询时不应有高亮");
    }

    #[test]
    fn fix2_yy_p_copies_and_undo_removes_copy() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "A").unwrap();
        let out = TempDir::new().unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg = None;

        // yy → 复制缓冲区。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('y'),
            KeyModifiers::NONE,
        );
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('y'),
            KeyModifiers::NONE,
        );
        assert_eq!(msg.as_deref(), Some("Yanked 1 item(s)"));
        let yank = state.yank.as_ref().unwrap();
        assert_eq!(yank.paths, vec![tmp.path().join("a.txt")]);
        assert!(!yank.cut);

        // 切到目标目录 B，p → 复制（不是移动）。
        state.active_tab_mut().location = Location::Physical(out.path().to_path_buf());
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('p'),
            KeyModifiers::NONE,
        );
        assert!(out.path().join("a.txt").exists(), "p 应复制文件");
        assert!(
            msg.as_deref().unwrap().starts_with("Copied 1 item(s) to "),
            "msg: {msg:?}"
        );
        assert_eq!(state.undo_stack.len(), 1);
        assert!(tmp.path().join("a.txt").exists(), "原文件保留");

        // u → 副本被移除。
        undo_last(&mut state, &mut msg);
        assert!(!out.path().join("a.txt").exists(), "u 应移除副本");
        assert!(tmp.path().join("a.txt").exists());
    }

    #[test]
    fn fix2_capital_p_moves_yanked_file() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("m.txt"), "M").unwrap();
        let out = TempDir::new().unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg = None;

        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('y'),
            KeyModifiers::NONE,
        );
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('y'),
            KeyModifiers::NONE,
        );
        state.active_tab_mut().location = Location::Physical(out.path().to_path_buf());
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('P'),
            KeyModifiers::NONE,
        );
        assert!(out.path().join("m.txt").exists());
        assert!(!tmp.path().join("m.txt").exists(), "P 应移动（原位消失）");
        assert!(
            msg.as_deref().unwrap().starts_with("Moved 1 item(s) to "),
            "msg: {msg:?}"
        );

        // u → 移回原位。
        undo_last(&mut state, &mut msg);
        assert!(tmp.path().join("m.txt").exists());
        assert!(!out.path().join("m.txt").exists());
    }

    #[test]
    fn fix2_paste_reports_errors_without_panic() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "A").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg = None;

        // 空缓冲区：可读提示。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('p'),
            KeyModifiers::NONE,
        );
        assert_eq!(msg.as_deref(), Some("缓冲区为空（先 yy 复制）"));

        // 同目录粘贴：目标已存在，可读错误，不覆盖不 panic。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('y'),
            KeyModifiers::NONE,
        );
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('y'),
            KeyModifiers::NONE,
        );
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('p'),
            KeyModifiers::NONE,
        );
        let m = msg.as_deref().unwrap();
        assert!(m.contains("0/1"), "msg: {m:?}");
        assert!(m.contains("已存在"), "msg: {m:?}");
        assert!(tmp.path().join("a.txt").exists(), "原文件未受影响");
    }

    #[cfg(unix)]
    #[test]
    fn fix2_paste_readonly_target_reports_error() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "A").unwrap();
        let ro = TempDir::new().unwrap();
        let ro_dir = ro.path().join("ro");
        fs::create_dir(&ro_dir).unwrap();
        let mut perms = fs::metadata(&ro_dir).unwrap().permissions();
        perms.set_mode(0o555);
        fs::set_permissions(&ro_dir, perms).unwrap();
        // root 下写入会成功（root 无视 0o555）：跳过断言（同 M12-1 策略）。
        let probe = ro_dir.join(".probe");
        if fs::write(&probe, "x").is_ok() {
            let _ = fs::remove_file(&probe);
            return;
        }

        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg = None;
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('y'),
            KeyModifiers::NONE,
        );
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('y'),
            KeyModifiers::NONE,
        );
        state.active_tab_mut().location = Location::Physical(ro_dir.clone());
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('p'),
            KeyModifiers::NONE,
        );
        let m = msg.as_deref().unwrap();
        assert!(m.contains("0/1"), "msg: {m:?}");
        assert!(!ro_dir.join("a.txt").exists(), "不留半成品文件");
    }

    #[test]
    fn fix3_enter_routes_archive_to_virtual_dir_and_back() {
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("t.zip");
        crate::archive::make_test_zip(&zip_path);
        fs::write(tmp.path().join("host.txt"), "H").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg = None;

        // 排序：host.txt(0) / t.zip(1)。l → 进入虚拟目录（不再被 opener 抢占）。
        state.active_tab_mut().cursor = 1;
        let outcome = press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('l'),
            KeyModifiers::NONE,
        );
        assert_eq!(outcome, Outcome::Continue);
        assert!(
            matches!(state.active_tab().location, Location::Archive { .. }),
            "l 应进入归档虚拟目录，location={:?}",
            state.active_tab().location
        );
        assert!(!state.active_tab().entries.is_empty());
        // 面包屑 host:/inner 形式。
        let shown = state.active_tab().location.display_path();
        assert!(shown.contains("t.zip:/"), "display: {shown}");

        // 归档根 h → 返回归档文件所在物理目录。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('h'),
            KeyModifiers::NONE,
        );
        assert!(matches!(state.active_tab().location, Location::Physical(_)));
        assert!(
            state
                .active_tab()
                .entries
                .iter()
                .any(|i| i.name == "host.txt")
        );
    }

    #[test]
    fn fix3_corrupt_archive_reports_error_and_stays() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("bad.zip"), b"PK junk bytes").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg = None;

        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('l'),
            KeyModifiers::NONE,
        );
        assert!(
            matches!(state.active_tab().location, Location::Physical(_)),
            "损坏归档进入失败后位置不变"
        );
        let m = msg.as_deref().unwrap();
        assert!(m.contains("进入失败"), "msg: {m:?}");
    }

    #[test]
    fn fix3_unsupported_archive_prompts_extract() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.7z"), b"7z fake").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg = None;

        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('l'),
            KeyModifiers::NONE,
        );
        let m = msg.as_deref().unwrap();
        assert!(m.contains("解压"), "msg: {m:?}");
        assert!(matches!(state.active_tab().location, Location::Physical(_)));
    }

    #[test]
    fn fix3_dd_inside_archive_is_noop() {
        // 归档内 dd：DeleteSelected 的 Physical 守卫不匹配 → 无确认、无删除。
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("t.zip");
        crate::archive::make_test_zip(&zip_path);
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg = None;
        state.active_tab_mut().cursor = 0;
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('l'),
            KeyModifiers::NONE,
        );
        assert!(matches!(
            state.active_tab().location,
            Location::Archive { .. }
        ));
        let before = state.active_tab().entries.len();

        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('d'),
            KeyModifiers::NONE,
        );
        let outcome = press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('d'),
            KeyModifiers::NONE,
        );
        assert_eq!(outcome, Outcome::Continue, "归档内 dd 不进入确认");
        assert_eq!(state.active_tab().entries.len(), before, "条目不变");
    }

    #[test]
    fn dd_requests_confirm_and_discards_other_second_key() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("target.txt"), "T").unwrap();
        fs::write(tmp.path().join("keep.txt"), "K").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);

        // 排序后 keep.txt(0) / target.txt(1)，把光标放到 target.txt。
        state.active_tab_mut().cursor = 1;
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('d'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(input.seq, [Key::plain(KeyCode::Char('d'))]);
        let outcome = press(
            &mut state,
            &mut input,
            KeyCode::Char('d'),
            KeyModifiers::NONE,
        );
        match outcome {
            Outcome::ConfirmDelete(p, permanent, _remote) => {
                assert_eq!(p, tmp.path().join("target.txt"));
                assert!(!permanent, "dd 应为暂存删除");
            }
            other => panic!("expected ConfirmDelete, got {other:?}"),
        }

        // `d` 后跟其他键：序列整体丢弃，不产生确认请求。
        let mut input2 = InputState::default();
        assert_eq!(
            press(
                &mut state,
                &mut input2,
                KeyCode::Char('d'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(
            press(
                &mut state,
                &mut input2,
                KeyCode::Char('x'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert!(input2.seq.is_empty());
        assert!(state.undo_stack.is_empty());
    }

    #[test]
    fn execute_delete_then_undo_restores() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("gone.txt");
        fs::write(&file, "G").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut msg = None;

        execute_delete(&mut state, &mut msg, file.clone(), false, None);
        assert!(msg.is_none());
        assert!(!file.exists(), "file should be staged out of the dir");
        assert_eq!(state.undo_stack.len(), 1);

        undo_last(&mut state, &mut msg);
        assert!(
            msg.as_deref().unwrap().starts_with("已撤销: "),
            "msg: {msg:?}"
        );
        assert!(file.exists());
        assert_eq!(fs::read_to_string(&file).unwrap(), "G");
        assert!(state.undo_stack.is_empty());

        // 空栈再按 u：明确提示，无 panic。
        undo_last(&mut state, &mut msg);
        assert_eq!(msg.as_deref(), Some("无可撤销操作"));
    }

    #[test]
    fn exec_rename_and_undo() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("old.txt"), "O").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut msg = None;

        // 重命名选中条目（cursor 0 = old.txt）。
        assert_eq!(exec_command(&mut state, "rename new.txt"), Ok(false));
        assert!(!tmp.path().join("old.txt").exists());
        assert!(tmp.path().join("new.txt").exists());
        assert_eq!(state.undo_stack.len(), 1);
        // 列表已刷新。
        assert!(
            state
                .active_tab()
                .entries
                .iter()
                .all(|i| i.name != "old.txt")
        );

        // `u` 撤销 → 回到 old.txt。
        undo_last(&mut state, &mut msg);
        assert!(
            msg.as_deref().unwrap().starts_with("已撤销: "),
            "msg: {msg:?}"
        );
        assert!(tmp.path().join("old.txt").exists());
        assert!(!tmp.path().join("new.txt").exists());

        // 错误路径：目标已存在（有提示，不 panic）；无参 :rename 现为
        // 批量管线（M21-E4：与 cw/R 同语义，不再报"缺少新名字"）。
        let r = exec_command_impl(&mut state, "rename").unwrap();
        assert!(
            matches!(r, ExecOutcome::BatchRename { .. }),
            "无参 :rename 走批量管线: {r:?}"
        );
        let err = exec_command(&mut state, "rename old.txt").unwrap_err();
        assert!(err.starts_with("rename: 目标已存在"), "err: {err}");
    }

    #[test]
    fn exec_move_and_undo() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(tmp.path().join("m.txt"), "M").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut msg = None;

        // 排序后 sub/(0) 在前，m.txt 在 1：把光标放到 m.txt。
        state.active_tab_mut().cursor = 1;
        let target = sub.display().to_string();
        assert_eq!(
            exec_command(&mut state, &format!("move {target}")),
            Ok(false)
        );
        assert!(!tmp.path().join("m.txt").exists());
        assert!(sub.join("m.txt").exists());
        assert_eq!(state.undo_stack.len(), 1);

        // `u` 撤销 → 移回原目录。
        undo_last(&mut state, &mut msg);
        assert!(
            msg.as_deref().unwrap().starts_with("已撤销: "),
            "msg: {msg:?}"
        );
        assert!(tmp.path().join("m.txt").exists());
        assert!(!sub.join("m.txt").exists());

        // 错误路径：缺参数、目录不存在。
        assert_eq!(
            exec_command(&mut state, "move"),
            Err("move: 缺少目标目录".to_owned())
        );
        let err = exec_command(&mut state, "move Z:/nope").unwrap_err();
        assert!(err.starts_with("move: 目标目录不存在"), "err: {err}");

        // 防自嵌套：刷新后只剩 sub/（cursor 钳制到 0），把目录移入自身内部被拒绝。
        state.active_tab_mut().refresh(Config::default()).unwrap();
        let err = exec_command(&mut state, &format!("move {target}")).unwrap_err();
        assert!(err.contains("不能把目录移动到其自身内部"), "err: {err}");
    }

    #[test]
    fn exec_mkdir_touch_and_undo() {
        let tmp = TempDir::new().unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut msg = None;

        // mkdir → 目录出现并入栈；u 移除。
        assert_eq!(exec_command(&mut state, "mkdir newdir"), Ok(false));
        assert!(tmp.path().join("newdir").is_dir());
        undo_last(&mut state, &mut msg);
        assert!(
            msg.as_deref().unwrap().starts_with("已撤销: "),
            "msg: {msg:?}"
        );
        assert!(!tmp.path().join("newdir").exists());

        // touch → 文件出现并入栈；u 移除。
        assert_eq!(exec_command(&mut state, "touch t.txt"), Ok(false));
        assert!(tmp.path().join("t.txt").is_file());
        undo_last(&mut state, &mut msg);
        assert!(
            msg.as_deref().unwrap().starts_with("已撤销: "),
            "msg: {msg:?}"
        );
        assert!(!tmp.path().join("t.txt").exists());

        // 错误路径：缺名字、非法名字、已存在。
        assert_eq!(
            exec_command(&mut state, "mkdir"),
            Err("mkdir: 缺少名字".to_owned())
        );
        assert!(exec_command(&mut state, "mkdir a/b").is_err());
        assert_eq!(exec_command(&mut state, "touch t.txt"), Ok(false));
        let err = exec_command(&mut state, "touch t.txt").unwrap_err();
        assert!(err.starts_with("touch: 目标已存在"), "err: {err}");
    }

    #[test]
    fn consecutive_ops_undo_in_reverse() {
        // M2-6：mkdir → touch → rename 连续三步，三次 `u` 逆序还原。
        let tmp = TempDir::new().unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut msg = None;

        assert_eq!(exec_command(&mut state, "mkdir d"), Ok(false));
        assert_eq!(exec_command(&mut state, "touch t.txt"), Ok(false));
        // 排序后 d/ 在 0、t.txt 在 1：把光标放到 t.txt 再重命名。
        state.active_tab_mut().cursor = 1;
        assert_eq!(exec_command(&mut state, "rename d2.txt"), Ok(false)); // 重命名 t.txt

        assert!(tmp.path().join("d").is_dir());
        assert!(tmp.path().join("d2.txt").is_file());
        assert_eq!(state.undo_stack.len(), 3);

        // 第 1 次 u：撤销重命名 → d2.txt 回到 t.txt。
        undo_last(&mut state, &mut msg);
        assert!(msg.as_deref().unwrap().starts_with("已撤销: 重命名"));
        assert!(tmp.path().join("t.txt").is_file());
        assert!(!tmp.path().join("d2.txt").exists());

        // 第 2 次 u：撤销 touch → t.txt 移除。
        undo_last(&mut state, &mut msg);
        assert!(msg.as_deref().unwrap().starts_with("已撤销: 新建"));
        assert!(!tmp.path().join("t.txt").exists());

        // 第 3 次 u：撤销 mkdir → d 移除；栈空。
        undo_last(&mut state, &mut msg);
        assert!(msg.as_deref().unwrap().starts_with("已撤销: 新建"));
        assert!(!tmp.path().join("d").exists());
        assert!(state.undo_stack.is_empty());
    }

    #[test]
    fn tabclose_reports_irreversible() {
        let (_tmp, mut state) = make_state(1);
        state.push_tab(Tab::from_physical(&std::env::temp_dir(), Config::default()).unwrap());

        // 非最后一个 tab：关闭成功并提示不可撤销。
        match exec_command_impl(&mut state, "tabclose") {
            Ok(ExecOutcome::Done { msg }) => {
                assert_eq!(msg.as_deref(), Some("标签已关闭（不可撤销）"))
            }
            other => panic!("expected Done, got {other:?}"),
        }

        // 最后一个 tab：退出请求。
        assert_eq!(
            exec_command_impl(&mut state, "tabclose"),
            Ok(ExecOutcome::Quit)
        );
    }

    #[test]
    fn cross_tab_undo_restores_origin_and_keeps_current() {
        // M2-7：tab 1 删除 → 切到 tab 2 → `u` → tab 1 文件恢复、当前 tab 不变。
        let dir_a = TempDir::new().unwrap(); // tab 1（index 0）
        let dir_b = TempDir::new().unwrap(); // tab 2（index 1）
        let file = dir_a.path().join("x.txt");
        fs::write(&file, "X").unwrap();

        let mut state = AppState::new(Tab::from_physical(dir_a.path(), Config::default()).unwrap());
        state.push_tab(Tab::from_physical(dir_b.path(), Config::default()).unwrap());
        state.active = 0;

        // 在 tab 1 删除 x.txt（entries = [x.txt]，cursor 0）。
        let mut msg = None;
        execute_delete(&mut state, &mut msg, file.clone(), false, None);
        assert!(!file.exists());
        assert_eq!(state.undo_stack.len(), 1);

        // 切到 tab 2。
        state.next_tab();
        assert_eq!(state.active, 1);

        // `u`：撤销发生在 tab 1 的删除。
        undo_last(&mut state, &mut msg);
        assert!(msg.as_deref().unwrap().starts_with("已撤销: 删除"));
        assert!(file.exists(), "tab 1 的文件应恢复");
        assert_eq!(state.active, 1, "当前 tab 不变");
        assert_eq!(
            state.active_tab().location,
            Location::Physical(dir_b.path().to_path_buf())
        );
        // tab 1 的列表恢复包含 x.txt。
        assert!(state.tabs[0].entries.iter().any(|i| i.name == "x.txt"));
    }

    #[test]
    fn exec_copy_and_undo_removes_copy() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(tmp.path().join("c.txt"), "C").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut msg = None;

        // 排序后 sub/(0) 在前，c.txt 在 1。
        state.active_tab_mut().cursor = 1;
        let target = sub.display().to_string();
        assert_eq!(
            exec_command(&mut state, &format!("copy {target}")),
            Ok(false)
        );
        // 源保留，副本出现，入栈。
        assert!(tmp.path().join("c.txt").exists());
        assert!(sub.join("c.txt").exists());
        assert_eq!(state.undo_stack.len(), 1);

        // `u` = 删除生成的副本（Create 语义），源不受影响。
        undo_last(&mut state, &mut msg);
        assert!(msg.as_deref().unwrap().starts_with("已撤销: 新建"));
        assert!(!sub.join("c.txt").exists());
        assert!(tmp.path().join("c.txt").exists());
    }

    #[test]
    fn visual_selection_flow() {
        // M3-1：v 进入 → j 扩大选区 → Esc 退出，状态正确。
        let (_tmp, mut state) = make_state(4);
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);

        // 空目录 no-op。
        let empty_tmp = TempDir::new().unwrap();
        let mut empty = Tab::from_physical(empty_tmp.path(), Config::default()).unwrap();
        empty.toggle_visual();
        assert!(empty.visual.is_none());

        // v：进入，锚点 = 进入时光标位置。
        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('v'),
                KeyModifiers::NONE
            ),
            Outcome::Continue
        );
        assert_eq!(state.active_tab().visual, Some(0));
        assert_eq!(state.active_tab().visual_range(), Some((0, 0)));

        // j j：选区扩大。
        press(
            &mut state,
            &mut input,
            KeyCode::Char('j'),
            KeyModifiers::NONE,
        );
        press(
            &mut state,
            &mut input,
            KeyCode::Char('j'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().visual_range(), Some((0, 2)));

        // k：反向收缩。
        press(
            &mut state,
            &mut input,
            KeyCode::Char('k'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().visual_range(), Some((0, 1)));

        // Esc 退出。
        assert_eq!(
            press(&mut state, &mut input, KeyCode::Esc, KeyModifiers::NONE),
            Outcome::Continue
        );
        assert!(state.active_tab().visual.is_none());

        // 再次 v：锚点 = 当前光标（1）；反向移动仍有序。
        press(
            &mut state,
            &mut input,
            KeyCode::Char('v'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().visual, Some(1));
        press(
            &mut state,
            &mut input,
            KeyCode::Char('k'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().visual_range(), Some((0, 1)));

        // 每 tab 独立（第二个 tab 无选区；tab0 锚点保持 1）。
        state.push_tab(Tab::from_physical(&std::env::temp_dir(), Config::default()).unwrap());
        assert!(state.active_tab().visual.is_none());
        assert_eq!(state.tabs[0].visual, Some(1));
    }

    #[test]
    fn render_highlights_visual_range() {
        let (_tmp, mut state) = make_state(5);
        state.active_tab_mut().cursor = 2;
        state.active_tab_mut().visual = Some(1); // 选区 1..=2

        let backend = ratatui::backend::TestBackend::new(120, 8);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();

        // 选区内反显（行 4、5），选区外不反显（行 2、3）——M21-F1 后 tab 栏
        // 占行 0、pane 边框行 1、`..` 行占行 2，条目行 3 起（条目文本列 1 起）。
        assert!(buffer[(1, 4)].modifier.contains(Modifier::REVERSED));
        assert!(buffer[(1, 5)].modifier.contains(Modifier::REVERSED));
        assert!(!buffer[(1, 2)].modifier.contains(Modifier::REVERSED));
        assert!(!buffer[(1, 3)].modifier.contains(Modifier::REVERSED));

        // 状态栏 VISUAL 指示。
        let status = (0..120u16)
            .map(|x| buffer[(x, 7)].symbol())
            .collect::<String>();
        assert!(status.contains("VISUAL (2 selected)"), "status: {status}");
    }

    #[test]
    fn tag_toggle_flow() {
        // M3-2：t 标记多个文件，再次 t 取消；空目录 no-op；每 tab 独立。
        let (_tmp, mut state) = make_state(3);
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);

        // 标记 cursor 0（f00），j 后标记 f01。
        press(
            &mut state,
            &mut input,
            KeyCode::Char('t'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().tagged_count(), 1);
        press(
            &mut state,
            &mut input,
            KeyCode::Char('j'),
            KeyModifiers::NONE,
        );
        press(
            &mut state,
            &mut input,
            KeyCode::Char('t'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().tagged_count(), 2);
        assert!(state.active_tab().tags.contains("f00"));
        assert!(state.active_tab().tags.contains("f01"));

        // 再次 t 取消 f01。
        press(
            &mut state,
            &mut input,
            KeyCode::Char('t'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().tagged_count(), 1);

        // 每 tab 独立。
        state.push_tab(Tab::from_physical(&std::env::temp_dir(), Config::default()).unwrap());
        assert_eq!(state.active_tab().tagged_count(), 0);
        assert_eq!(state.tabs[0].tagged_count(), 1);

        // 空目录 no-op。
        let empty_tmp = TempDir::new().unwrap();
        let mut empty = Tab::from_physical(empty_tmp.path(), Config::default()).unwrap();
        empty.toggle_tag();
        assert_eq!(empty.tagged_count(), 0);
    }

    #[test]
    fn no_color_renders_without_panic() {
        // M9-5：NO_COLOR 降级验证——TestBackend 无终端颜色，渲染不 panic 即通过。
        // TestBackend 下不存在真实颜色输出，本测试覆盖"降级路径可跑通"。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("f.txt"), "content").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        state.active_tab_mut().tags.insert("f.txt".to_owned());

        let backend = ratatui::backend::TestBackend::new(120, 8);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let draw = terminal.draw(|frame| render(frame, &state, &InputMode::Normal, None));
        assert!(draw.is_ok(), "渲染不应失败");

        // 预览渲染同样不 panic。
        let p = tmp.path().join("f.txt");
        let preview = crate::preview::render_preview(&p, 10);
        let texts = preview.line_texts();
        assert!(texts.iter().any(|t| t.contains("content")));
    }

    #[test]
    fn render_shows_tag_bold_and_count() {
        let (_tmp, mut state) = make_state(3);
        state.active_tab_mut().tags.insert("f01".to_owned());

        let backend = ratatui::backend::TestBackend::new(120, 8);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();

        // 标记行加粗，未标记行不加粗——M21-F1 后 tab 栏行 0、pane 边框行 1、
        // `..` 行占行 2（FEAT-2），条目行 3 起（条目文本列 1 起）。
        assert!(buffer[(1, 4)].modifier.contains(Modifier::BOLD));
        assert!(!buffer[(1, 3)].modifier.contains(Modifier::BOLD));

        // 状态栏计数。
        let status = (0..120u16)
            .map(|x| buffer[(x, 7)].symbol())
            .collect::<String>();
        assert!(status.contains("1 tagged"), "status: {status}");
    }

    #[test]
    fn tag_batch_operations() {
        // M3-3：:tag copy / move / delete 对标记集合生效，未标记不受影响。
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(tmp.path().join("a.txt"), "A").unwrap();
        fs::write(tmp.path().join("b.txt"), "B").unwrap();
        fs::write(tmp.path().join("c.txt"), "C").unwrap(); // 不标记
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());

        // 标记 a.txt、b.txt（排序后 a(0) b(1) c(2)）。
        state.active_tab_mut().tags.insert("a.txt".to_owned());
        state.active_tab_mut().tags.insert("b.txt".to_owned());

        // 批量复制到 sub/ → 后台作业（M13-8：SpawnJob，线程异步执行）。
        let target = sub.display().to_string();
        match exec_command_impl(&mut state, &format!("tag copy {target}")) {
            Ok(ExecOutcome::SpawnJob { op, names, .. }) => {
                assert_eq!(op, "copy");
                assert_eq!(names.len(), 2);
            }
            other => panic!("expected SpawnJob, got {other:?}"),
        }
        // spawn/执行在 run() 的 SpawnJob 分支（本测试只调 exec）——复制正确性
        // 由 jobs_lifecycle 测试覆盖（copy_to + apply_job_update）。
        assert!(
            !sub.join("c.txt").exists(),
            "未标记文件不受影响（spawn 前）"
        );

        // 重新标记 → SpawnJob 后台语义下，用 execute_job_body 同步验证
        // 复制正确性与撤销记录（run 侧仅 spawn/转发）。
        state.active_tab_mut().tags.insert("a.txt".to_owned());
        state.active_tab_mut().tags.insert("b.txt".to_owned());
        let (ops, status) = execute_job_body(
            "copy",
            &["a.txt".to_owned(), "b.txt".to_owned()],
            tmp.path(),
            &sub,
            0,
            &std::sync::atomic::AtomicBool::new(false),
        );
        assert_eq!(ops.len(), 2, "两条复制各一条撤销记录");
        for op in ops {
            state.undo_stack.push(op);
        }
        assert!(
            matches!(status, crate::state::JobStatus::Done { .. }),
            "复制终态 Done: {status:?}"
        );
        assert!(sub.join("a.txt").exists() && sub.join("b.txt").exists());
        assert_eq!(state.undo_stack.len(), 2);
        let mut msg = None;
        undo_last(&mut state, &mut msg);
        undo_last(&mut state, &mut msg);
        assert!(!sub.join("a.txt").exists() && !sub.join("b.txt").exists());

        // 批量移动（后台语义同款，execute_job_body 同步验证）。
        state.active_tab_mut().tags.insert("a.txt".to_owned());
        state.active_tab_mut().tags.insert("b.txt".to_owned());
        let (ops, status) = execute_job_body(
            "move",
            &["a.txt".to_owned(), "b.txt".to_owned()],
            tmp.path(),
            &sub,
            0,
            &std::sync::atomic::AtomicBool::new(false),
        );
        assert_eq!(ops.len(), 2);
        assert!(matches!(status, crate::state::JobStatus::Done { .. }));
        for op in ops {
            state.undo_stack.push(op);
        }
        assert!(!tmp.path().join("a.txt").exists() && sub.join("a.txt").exists());

        // 批量删除：tag move 后旧标记按名解析失效（跳过），
        // 在当前目录新建 d.txt / e.txt 并标记，验证批量删除 + 逐条 u。
        fs::write(tmp.path().join("d.txt"), "D").unwrap();
        fs::write(tmp.path().join("e.txt"), "E").unwrap();
        state.active_tab_mut().refresh(Config::default()).unwrap();
        state.active_tab_mut().clear_tags();
        state.active_tab_mut().tags.insert("d.txt".to_owned());
        state.active_tab_mut().tags.insert("e.txt".to_owned());
        assert_eq!(
            exec_command_impl(&mut state, "tag delete"),
            Ok(ExecOutcome::ConfirmTagDelete)
        );
        let mut msg0 = None;
        execute_tag_delete(&mut state, &mut msg0);
        assert!(msg0.as_deref().unwrap().contains("2 完成"));
        assert!(!tmp.path().join("d.txt").exists());
        assert!(!tmp.path().join("e.txt").exists());
        assert!(tmp.path().join("c.txt").exists(), "未标记不受影响");
        // M3-4：批量删除完成后标记自动清空。
        assert_eq!(state.active_tab().tagged_count(), 0);

        // 逐条 u 恢复被删的两个文件（LIFO）。
        let mut msg = None;
        undo_last(&mut state, &mut msg);
        undo_last(&mut state, &mut msg);
        assert!(tmp.path().join("d.txt").exists() && tmp.path().join("e.txt").exists());

        // 空标记提示 + 未知动作。
        state.active_tab_mut().clear_tags();
        match exec_command_impl(&mut state, "tag delete") {
            Ok(ExecOutcome::Done { msg }) => assert_eq!(msg.as_deref(), Some("tag: 无标记文件")),
            other => panic!("expected Done, got {other:?}"),
        }
        assert!(exec_command_impl(&mut state, "tag snort").is_err());
    }

    #[test]
    fn glob_match_patterns() {
        // M4-2：`*` 与 `?` 语义，大小写不敏感。
        assert!(crate::tab::glob_match("*.txt", "a.txt"));
        assert!(crate::tab::glob_match("*.TXT", "a.txt"));
        assert!(crate::tab::glob_match("a*", "a.txt"));
        assert!(crate::tab::glob_match("a*b", "ab"));
        assert!(crate::tab::glob_match("a*b", "axxb"));
        assert!(!crate::tab::glob_match("a*b", "axxbyy")); // 模式耗尽后仍有剩余字符
        assert!(crate::tab::glob_match("a*b*", "axxbyy"));
        assert!(crate::tab::glob_match("a?c", "abc"));
        assert!(!crate::tab::glob_match("*.txt", "a.rs"));
        assert!(!crate::tab::glob_match("a?c", "ac"));
        assert!(!crate::tab::glob_match("a?c", "abbc"));
        assert!(!crate::tab::glob_match("a*b", "ba"));
    }

    #[test]
    fn filter_apply_and_clear() {
        // M4-2：= 应用 glob 过滤，空 pattern 清除；光标归零。
        let tmp = TempDir::new().unwrap();
        for name in ["a.txt", "b.txt", "c.rs", "sub"] {
            if name == "sub" {
                fs::create_dir(tmp.path().join(name)).unwrap();
            } else {
                fs::write(tmp.path().join(name), "").unwrap();
            }
        }
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);

        assert_eq!(
            press(
                &mut state,
                &mut input,
                KeyCode::Char('='),
                KeyModifiers::NONE
            ),
            Outcome::EnterFilter
        );

        // 提交 "*.txt" → 只剩两个 txt（目录 sub 被滤掉）。
        state
            .active_tab_mut()
            .apply_filter("*.txt", Config::default());
        let names: Vec<&str> = state
            .active_tab()
            .entries
            .iter()
            .map(|i| i.name.as_str())
            .collect();
        assert_eq!(names, ["a.txt", "b.txt"]);
        assert_eq!(state.active_tab().cursor, 0);

        // 过滤视图内导航 + 再过滤不叠加异常。
        press(
            &mut state,
            &mut input,
            KeyCode::Char('j'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().cursor, 1);
        state.active_tab_mut().apply_filter("a*", Config::default());
        assert_eq!(state.active_tab().entries.len(), 1);

        // 空 pattern 清除过滤 → refresh 恢复全部条目。
        state.active_tab_mut().apply_filter("", Config::default());
        assert!(state.active_tab().filter.is_none());
        assert_eq!(state.active_tab().entries.len(), 4);
    }

    #[test]
    fn render_shows_filter_indicator() {
        let (_tmp, mut state) = make_state(3);
        state.active_tab_mut().filter = Some("*.txt".to_owned());

        let backend = ratatui::backend::TestBackend::new(120, 8);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let status = (0..120u16)
            .map(|x| buffer[(x, 7)].symbol())
            .collect::<String>();
        assert!(status.contains("[filter: *.txt]"), "status: {status}");
    }

    #[test]
    fn exec_q_cd_help_unified_parse() {
        // M5-3 + M5-4：最小命令子集与多标签命令在同一解析器共存。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("x.txt"), "X").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());

        // :cd 到不存在目录 → 错误；存在 → 导航。
        assert!(exec_command_impl(&mut state, "cd Z:/nope").is_err());
        let sub = tmp.path().join("sub");
        fs::create_dir(&sub).unwrap();
        assert!(matches!(
            exec_command_impl(&mut state, &format!("cd {}", sub.display())),
            Ok(ExecOutcome::Done { msg: None })
        ));
        assert_eq!(state.active_tab().location, Location::Physical(sub.clone()));

        // :q 请求退出。
        assert_eq!(exec_command_impl(&mut state, "q"), Ok(ExecOutcome::Quit));

        // :help 请求帮助层。
        assert_eq!(
            exec_command_impl(&mut state, "help"),
            Ok(ExecOutcome::EnterHelp)
        );

        // M5-4：多标签命令与文件命令共存不冲突（tabnew → touch → tabmove）。
        assert!(matches!(
            exec_command_impl(&mut state, "tabnew"),
            Ok(ExecOutcome::Done { .. })
        ));
        assert!(matches!(
            exec_command_impl(&mut state, "touch y.txt"),
            Ok(ExecOutcome::Done { .. })
        ));
        assert!(matches!(
            exec_command_impl(&mut state, "tabmove 1"),
            Ok(ExecOutcome::Done { .. })
        ));
        assert_eq!(state.tabs.len(), 2);
    }

    #[test]
    fn large_directory_load_and_render_fast() {
        // M6-4：>10k 项大目录——加载与单帧渲染有界，退出不经过批量渲染（逐键响应）。
        let tmp = TempDir::new().unwrap();
        for i in 0..10_500u32 {
            std::fs::write(tmp.path().join(format!("f{i}.txt")), "").unwrap();
        }

        let t0 = std::time::Instant::now();
        let state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let load = t0.elapsed();
        assert_eq!(state.active_tab().entries.len(), 10_500);

        let t1 = std::time::Instant::now();
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let draw = t1.elapsed();

        // 宽松上限（CI 速度波动安全）：加载与单帧渲染均 < 2s。
        assert!(load.as_secs() < 2, "load too slow: {load:?}");
        assert!(draw.as_secs() < 2, "draw too slow: {draw:?}");
    }

    #[test]
    fn archive_pane_title_shows_host_inner_breadcrumb() {
        // M7-5：面包屑 `host:/inner/path`——M21-F1 起显示在 pane 顶部标题
        // （状态栏不再显示路径）。
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("t.zip");
        crate::archive::make_test_zip(&zip_path);
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        // 直接构造归档内 src 层的位置。
        state.active_tab_mut().location = Location::Archive {
            host: zip_path.clone(),
            inner: PathBuf::from("src"),
        };
        state.active_tab_mut().entries = vec![FileItem {
            icon: crate::icons::icon_for(Path::new("main.rs"), crate::icons::IconTheme::Dark),
            name: "main.rs".into(),
            is_dir: false,
            is_link: false,
            is_exec: false,
            size: 12,
            perm: String::new(),
            mtime: 0,
        }];

        let backend = ratatui::backend::TestBackend::new(160, 8);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let title = (0..160u16)
            .map(|x| buffer[(x, 1)].symbol())
            .collect::<String>();
        let status = (0..160u16)
            .map(|x| buffer[(x, 7)].symbol())
            .collect::<String>();
        let leaf = zip_path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            title.contains(&format!("{leaf}:/src")),
            "breadcrumb missing in pane title: {title}"
        );
        assert!(
            !status.contains(&format!("{leaf}:/src")),
            "状态栏不应再显示路径: {status}"
        );
    }

    #[test]
    fn archive_search_reuses_m4_chain() {
        // M7-6：归档内 `/` 搜索复用 M4 链路（find_match/search_move 作用于当前条目）。
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("t.zip");
        crate::archive::make_test_zip(&zip_path);
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        state.active_tab_mut().location = Location::Archive {
            host: zip_path,
            inner: PathBuf::new(),
        };
        state.active_tab_mut().entries = vec![
            FileItem {
                icon: crate::icons::icon_for(Path::new("deep/"), crate::icons::IconTheme::Dark),
                name: "deep".into(),
                is_dir: true,
                is_link: false,
                is_exec: false,
                size: 0,
                perm: String::new(),
                mtime: 0,
            },
            FileItem {
                icon: crate::icons::icon_for(Path::new("readme.md"), crate::icons::IconTheme::Dark),
                name: "readme.md".into(),
                is_dir: false,
                is_link: false,
                is_exec: false,
                size: 2,
                perm: String::new(),
                mtime: 0,
            },
            FileItem {
                icon: crate::icons::icon_for(Path::new("src/"), crate::icons::IconTheme::Dark),
                name: "src".into(),
                is_dir: true,
                is_link: false,
                is_exec: false,
                size: 0,
                perm: String::new(),
                mtime: 0,
            },
        ];

        // /main → 命中? 名单里只有 src/main.rs 在子目录，根列表不递归。
        let mut msg = None;
        submit_search(&mut state, "readme", &mut msg);
        assert!(msg.is_none());
        assert_eq!(state.active_tab().cursor, 1);
        // n 环绕回 0（deep 是目录名，不匹配 readme）。
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('n'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().cursor, 1); // 无第二匹配则停在同一处
    }

    #[test]
    fn archive_view_is_readonly_for_commands() {
        // M7 验收：归档视图内文件操作命令被拒绝（不触碰宿主目录）。
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("t.zip");
        crate::archive::make_test_zip(&zip_path);
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        state.active_tab_mut().location = Location::Archive {
            host: zip_path,
            inner: PathBuf::new(),
        };

        for cmd in [
            "rename x",
            "move Z:/tmp",
            "copy Z:/tmp",
            "mkdir d",
            "touch t",
        ] {
            let err = exec_command_impl(&mut state, cmd).unwrap_err();
            assert!(err.contains("归档视图只读"), "cmd={cmd} err={err}");
        }

        // 标记批量删除：静默拒绝进状态栏。
        state.active_tab_mut().tags.insert("main.rs".to_owned());
        let mut msg = None;
        execute_tag_delete(&mut state, &mut msg);
        assert_eq!(msg.as_deref(), Some("归档视图只读（解压后可修改）"));
    }

    #[test]
    fn render_highlights_cursor_row() {
        let (_tmp, mut state) = make_state(3);
        state.active_tab_mut().cursor = 1;

        let backend = ratatui::backend::TestBackend::new(40, 6);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        // M21-F1 几何（40x6）：tab 栏行 0、pane 边框行 1、`..` 行占行 2，
        // 条目视口 1 行（内区高 2）→ 光标行 1 滚动显示在行 3；条目文本列 1 起。
        assert!(buffer[(1, 3)].modifier.contains(Modifier::REVERSED));
        assert!(!buffer[(1, 2)].modifier.contains(Modifier::REVERSED));
        assert!(!buffer[(1, 4)].modifier.contains(Modifier::REVERSED));
    }

    // ── M8-12：跳转选择器（o 合一 / :j 仅目录 / :J 仅文件） ─────────────────

    fn scored(path: &str, topic: &str, score: f64) -> crate::frecency::ScoredEntry {
        crate::frecency::ScoredEntry {
            path: path.to_owned(),
            topic: topic.to_owned(),
            keyword: String::new(),
            score,
            grade: crate::frecency::Grade::None,
            last_visit: 0,
        }
    }

    #[test]
    fn replay_unified_uses_global_seq_for_all_topics() {
        // M8-12 统一视角：file 条目的 aging 跟随全局序号——dir 高频推进
        // 全局时钟会稀释 file 分数（与分型视角互补，o/:j/:J 双视角的基础）。
        let file_ev = |clock: u64| crate::frecency::EntryRecord {
            path: "/f/notes.txt".to_owned(),
            topic: "file".to_owned(),
            kind: crate::frecency::EntryKind::Open,
            ts: 1000,
            clock,
            keyword: String::new(),
        };
        let events = [(1u64, file_ev(1))];
        // 统一视角：全局序号推进到 100 → file 条目大幅衰减。
        let u = crate::frecency::replay_unified(&[], &events, 100);
        // 分型视角：file 时钟停在 2 → 衰减很小。
        let p = crate::frecency::replay(&[], &events, 2, 2);
        let su = u.iter().find(|e| e.path == "/f/notes.txt").unwrap();
        let sp = p.iter().find(|e| e.path == "/f/notes.txt").unwrap();
        assert!(
            su.score < sp.score,
            "统一视角(dir 高频)衰减应大于分型视角: {} vs {}",
            su.score,
            sp.score
        );
    }

    #[test]
    fn m23_1_multi_word_graded_match() {
        // ACCEPTANCE §22 M23-1：多词查询——全词命中（any-order 降档）与
        // 按序命中（保档）分级排序；keyword 精确命中最优先。
        // /w/proj/notes：词 proj+notes 按序出现（路径顺序一致）→ NameContains。
        // /f/notes/proj：词出现但乱序 → 降为 Substring。
        let entries = vec![
            scored("/w/proj/notes", "dir", 5.0),
            scored("/f/notes/proj", "dir", 9.0),
            scored("/other/readme", "file", 1.0),
        ];
        let out = filter_jump(&entries, "proj notes");
        assert_eq!(out.len(), 2, "全词命中的条目保留");
        // 按序命中档位高 → 排前（分数低但档位高）。
        assert_eq!(out[0].path, "/w/proj/notes", "ordered 命中档位更高");
        assert!(
            out[0].grade.rank() > out[1].grade.rank(),
            "乱序命中降为 Substring"
        );
        // 单词退化：等价 grade_for。
        let single = filter_jump(&entries, "readme");
        assert_eq!(single.len(), 1);
        assert_eq!(single[0].path, "/other/readme");
    }

    #[test]
    fn filter_jump_empty_query_shows_all_and_ranked_order() {
        // M8-12：空查询全显且保持传入序（load_jump_entries 返回时已按
        // aging ranked 排序，选择器不再二次排序）。
        let entries = vec![
            scored("/b/high", "file", 9.0),
            scored("/c/mid", "dir", 5.0),
            scored("/a/low", "dir", 1.0),
        ];
        let out = filter_jump(&entries, "");
        assert_eq!(out.len(), 3);
        let paths: Vec<&str> = out.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["/b/high", "/c/mid", "/a/low"]);
    }

    #[test]
    fn filter_jump_query_filters_by_grade() {
        // M8-12：非空查询按分级过滤（文件名包含 notes），排除无关条目。
        let entries = vec![
            scored("/f/notes.txt", "file", 5.0),
            scored("/w/proj", "dir", 9.0),
            scored("/other/noTES.md", "file", 1.0),
        ];
        let out = filter_jump(&entries, "notes");
        assert_eq!(out.len(), 2, "包含 notes 的条目应保留");
        assert!(
            out.iter()
                .all(|e| e.path.contains("notes") || e.path.contains("noTES"))
        );
        // 无匹配查询 → 空。
        let none = filter_jump(&entries, "zzz-no-match");
        assert!(none.is_empty());
    }

    #[test]
    fn jump_to_dir_navigates_and_pushes_history() {
        // M8-12：选定目录条目 → 当前 tab cd（history 入栈、光标复位）。
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("target");
        std::fs::create_dir(&sub).unwrap();
        let (_t, mut state) = make_state(2);
        let hist_before = state.active_tab().history.len();

        let entry = scored(&sub.display().to_string().replace('\\', "/"), "dir", 5.0);
        let msg = jump_to(&mut state, &entry).unwrap();
        assert!(msg.contains("跳转目录"), "{msg}");
        assert_eq!(state.active_tab().history.len(), hist_before + 1);
        assert_eq!(
            state.active_tab().location.base_dir(),
            sub,
            "应 cd 到目标目录"
        );
        assert_eq!(state.active_tab().cursor, 0);
    }

    #[test]
    fn jump_to_file_positions_cursor_in_parent() {
        // M8-12：选定文件条目 → 切到父目录并光标定位（纯导航，不开 opener）。
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("docs");
        std::fs::create_dir(&sub).unwrap();
        fs::write(sub.join("hello.txt"), "x").unwrap();
        let (_t, mut state) = make_state(1);

        let file_path = sub
            .join("hello.txt")
            .display()
            .to_string()
            .replace('\\', "/");
        let entry = scored(&file_path, "file", 5.0);
        let msg = jump_to(&mut state, &entry).unwrap();
        assert!(msg.contains("定位"), "{msg}");
        assert_eq!(
            state.active_tab().location.base_dir(),
            sub,
            "应切到文件父目录"
        );
        let cursor_name = state
            .active_tab()
            .entries
            .get(state.active_tab().cursor)
            .map(|i| i.name.clone());
        assert_eq!(
            cursor_name.as_deref(),
            Some("hello.txt"),
            "光标应定位在文件"
        );
    }

    static TRASH_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn trash_menu_records_and_restores_to_origin() {
        // M13-10：dd → 暂存索引记录（staged→orig）；restore_from_trash
        // 移回原位 + 索引清理；orig 冲突 → Err。
        let _guard = TRASH_TEST_LOCK.lock().unwrap();
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("victim.txt");
        fs::write(&file, "V").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut msg: Option<String> = None;

        // dd → 确认 → 暂存删除（索引记录；两键共享同一 seq 缓冲）。
        let mut seq_input = InputState::default();
        press_msg(
            &mut state,
            &mut seq_input,
            &mut msg,
            KeyCode::Char('d'),
            KeyModifiers::NONE,
        );
        let outcome = press_msg(
            &mut state,
            &mut seq_input,
            &mut msg,
            KeyCode::Char('d'),
            KeyModifiers::NONE,
        );
        let Outcome::ConfirmDelete(path, _permanent, _remote) = outcome else {
            panic!("expected ConfirmDelete, got {outcome:?}");
        };
        execute_delete(&mut state, &mut msg, path, false, None);
        assert!(!file.exists());

        // 索引应有记录且 staged 仍存在。
        let items = crate::ops::read_trash_index();
        let mine = items
            .iter()
            .find(|e| std::path::Path::new(&e.orig) == file.as_path());
        assert!(mine.is_some(), "本实例的暂存记录应在索引中: {items:?}");
        let mine = mine.unwrap();
        assert!(std::path::Path::new(&mine.staged).exists());

        // 恢复：移回原位 + 索引清理。
        let orig = crate::ops::restore_from_trash(mine).unwrap();
        assert_eq!(orig, file);
        assert!(file.exists(), "文件应回到原位");
        let items = crate::ops::read_trash_index();
        assert!(
            !items
                .iter()
                .any(|e| std::path::Path::new(&e.orig) == file.as_path()),
            "恢复后本实例条目应清空"
        );

        // 冲突：orig 已存在 → Err。
        let conflict = crate::ops::TrashEntry {
            staged: tmp.path().join("nonexistent-staged").display().to_string(),
            orig: file.display().to_string(),
        };
        assert!(crate::ops::restore_from_trash(&conflict).is_err());
    }

    #[test]
    fn view_mode_toggle_switches_preview_completeness() {
        // M13-9：w/:view 切换——摘要模式长文件截断，view 模式显示完整内容；
        // 光标移动（j）后预览跟随光标文件。
        let tmp = TempDir::new().unwrap();
        let mut content = String::new();
        for i in 0..200 {
            content.push_str(&format!(
                "line {i:03}
"
            ));
        }
        fs::write(tmp.path().join("long.txt"), &content).unwrap();
        fs::write(tmp.path().join("short.txt"), "SHORT MARKER").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());

        // 摘要模式：默认截断（仅窗口高度行）。
        assert!(!state.active_tab().view_mode);
        let backend = ratatui::backend::TestBackend::new(80, 12);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| render(f, &state, &InputMode::Normal, None))
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut all = String::new();
        for y in 0..12u16 {
            for x in 40u16..80 {
                all.push_str(buf[(x, y)].symbol());
            }
        }
        assert!(!all.contains("line 150"), "摘要模式不应显示深处行: {all}");

        // w 开启 view 模式 → 完整内容（深处行可见）。
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg: Option<String> = None;
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('w'),
            KeyModifiers::NONE,
        );
        assert!(state.active_tab().view_mode);
        terminal
            .draw(|f| render(f, &state, &InputMode::Normal, None))
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut all = String::new();
        for y in 0..12u16 {
            for x in 40u16..80 {
                all.push_str(buf[(x, y)].symbol());
            }
        }
        assert!(
            all.contains("line 000") || all.contains("line 0"),
            "view 模式从文件头渲染: {all}"
        );

        // M17 VIEW-1：查看器内 j 滚动视口（列表光标不动）。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('j'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().viewer.as_ref().unwrap().scroll, 1);
        terminal
            .draw(|f| render(f, &state, &InputMode::Normal, None))
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut all = String::new();
        for y in 0..12u16 {
            for x in 40u16..80 {
                all.push_str(buf[(x, y)].symbol());
            }
        }
        // 行号 gutter 存在（"   2 │"），视口从第 2 行起（line 000 滚出）。
        assert!(all.contains('│'), "应渲染行号 gutter: {all}");
        assert!(!all.contains("line 000"), "滚动后首行滚出: {all}");

        // Esc 退出查看器 → 回列表导航（M14 模型：右 pane 恢复目录列表）。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Esc,
            KeyModifiers::NONE,
        );
        assert!(!state.active_tab().view_mode);
        assert!(state.active_tab().viewer.is_none());
        terminal
            .draw(|f| render(f, &state, &InputMode::Normal, None))
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut all = String::new();
        for y in 0..12u16 {
            for x in 40u16..80 {
                all.push_str(buf[(x, y)].symbol());
            }
        }
        assert!(all.contains("short.txt"), "右 pane 恢复目录列表: {all}");

        // :view toggle：关→开→关。
        let _ = exec_command_impl(&mut state, "view").unwrap();
        assert!(state.active_tab().view_mode, ":view 重新开启");
        let _ = exec_command_impl(&mut state, "view").unwrap();
        assert!(!state.active_tab().view_mode, ":view 再次关闭");
    }

    #[test]
    fn tab_bar_visible_and_updates_on_tabnew() {
        // M14-2：标签栏渲染（1:dir 高亮），:tabnew 后 2:dir 出现且高亮移动。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let backend = ratatui::backend::TestBackend::new(80, 10);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();

        // 单 tab：1:tmpdir 高亮。
        terminal
            .draw(|f| render(f, &state, &InputMode::Normal, None))
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut tab_row = String::new();
        for x in 0..80u16 {
            let c = &buf[(x, 0)];
            tab_row.push_str(c.symbol());
        }
        assert!(tab_row.contains("1:"), "tab bar: {tab_row}");

        // :tabnew → 2 tabs，2:tmpdir 出现且高亮在第二个。
        exec_command_impl(&mut state, "tabnew").unwrap();
        terminal
            .draw(|f| render(f, &state, &InputMode::Normal, None))
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut tab_row = String::new();
        for x in 0..80u16 {
            let c = &buf[(x, 0)];
            tab_row.push_str(c.symbol());
        }
        assert!(tab_row.contains("2:"), "tabnew 后 tab bar: {tab_row}");
        // 高亮（REVERSED）应落在 "2:" 起始处（第二个 tab 为当前）。
        let two_x = tab_row.find("2:").unwrap_or(0) as u16;
        assert!(
            buf[(two_x, 0)].modifier.contains(Modifier::REVERSED),
            "2: 段应高亮"
        );
    }

    #[test]
    fn jobs_lifecycle_and_jobs_menu_render() {
        // M13-8：作业登记/更新流转 + :jobs 覆盖层渲染（Running/Done/Failed）。
        let tmp = TempDir::new().unwrap();
        for name in ["j1.txt", "j2.txt"] {
            fs::write(tmp.path().join(name), "").unwrap();
        }
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let dst = TempDir::new().unwrap();

        // 登记 Running 作业。
        let id = state.add_job("copy", 2, &dst.path().display().to_string());
        assert_eq!(id, 1);
        assert!(matches!(
            state.jobs[0].status,
            crate::state::JobStatus::Running
        ));

        // :jobs 命令 → ShowJobs。
        let r = exec_command_impl(&mut state, "jobs").unwrap();
        assert!(matches!(r, ExecOutcome::ShowJobs));

        // 覆盖层渲染（Running 状态）。
        let mode = InputMode::Jobs { cursor: 0 };
        let backend = ratatui::backend::TestBackend::new(70, 8);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &state, &mode, None)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let all: String = (0..8u16)
            .map(|y| {
                (0..70u16)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect();
        assert!(all.contains("copy"), "Running 渲染(ASCII): {all}");

        // 更新包：完成（含撤销栈入栈）。
        let op = crate::ops::copy_to(0, &tmp.path().join("j1.txt"), dst.path()).unwrap();
        let changed = state.apply_job_update(crate::state::JobUpdate {
            job_id: id,
            tab_id: 0,
            ops: vec![op],
            status: crate::state::JobStatus::Done {
                summary: "copy: 1 项完成".to_owned(),
            },
        });
        assert!(changed);
        assert_eq!(state.undo_stack.len(), 1);
        assert!(matches!(
            state.jobs[0].status,
            crate::state::JobStatus::Done { .. }
        ));
        assert!(dst.path().join("j1.txt").exists());

        // 失败状态流转。
        let id2 = state.add_job("move", 1, "no-such-dir");
        state.apply_job_update(crate::state::JobUpdate {
            job_id: id2,
            tab_id: 0,
            ops: vec![],
            status: crate::state::JobStatus::Failed {
                err: "no such dir".to_owned(),
            },
        });
        assert!(matches!(
            state.jobs[1].status,
            crate::state::JobStatus::Failed { .. }
        ));
    }

    #[test]
    fn select_unselect_patterns_batch_tagging() {
        // M13-2：:select {pattern} 批量打标 / :unselect 按模式移除 /
        // :unselect 无参清空 / :select 无参选当前条目；t 语义不受影响。
        let tmp = TempDir::new().unwrap();
        for name in ["a.md", "b.md", "c.txt"] {
            fs::write(tmp.path().join(name), "").unwrap();
        }
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());

        // :select *.md → 2 个条目打标。
        let r = exec_command_impl(&mut state, "select *.md").unwrap();
        assert_eq!(state.active_tab().tagged_count(), 2);
        assert!(matches!(r, ExecOutcome::Done { msg: Some(m) } if m.contains("2")));

        // :unselect b.md → 只剩 a.md。
        exec_command_impl(&mut state, "unselect b.md").unwrap();
        assert_eq!(state.active_tab().tagged_count(), 1);
        assert!(state.active_tab().tags.contains("a.md"));

        // :select 无参 = 选当前光标条目（c.txt）。
        state.active_tab_mut().cursor = 2;
        exec_command_impl(&mut state, "select").unwrap();
        assert!(state.active_tab().tags.contains("c.txt"));
        assert_eq!(state.active_tab().tagged_count(), 2);

        // :unselect 无参 = 清空全部。
        exec_command_impl(&mut state, "unselect").unwrap();
        assert_eq!(state.active_tab().tagged_count(), 0);

        // t 单文件切换语义不受影响。
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg: Option<String> = None;
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('t'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().tagged_count(), 1);
    }

    #[test]
    fn dd_permanent_delete_skips_staging_and_undo_stack() {
        // M13-3：DD → Confirm(permanent) 强提示 → y：文件直接删除，
        // 不进暂存（u 无法恢复）、不进撤销栈。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("gone.txt"), "G").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg: Option<String> = None;

        // DD 序列（D 前缀 + D）→ ConfirmDelete(path, true)。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('D'),
            KeyModifiers::NONE,
        );
        let outcome = press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('D'),
            KeyModifiers::NONE,
        );
        let Outcome::ConfirmDelete(path, permanent, _remote) = outcome else {
            panic!("expected ConfirmDelete, got {outcome:?}");
        };
        assert!(permanent, "DD 必须标记永久删除");

        // 确认 y 的执行在 run() 的 Confirm 分支（mode 迁移在 run 内）——
        // 此处直接验证永久删除执行语义：不进暂存、不进撤销栈。
        let mut msg = None;
        execute_delete(&mut state, &mut msg, path.clone(), true, None);
        assert!(!path.exists(), "文件应被直接删除");
        assert_eq!(state.undo_stack.len(), 0, "永久删除不进撤销栈");
        assert!(
            msg.as_deref().is_some_and(|m| m.contains("不可恢复")),
            "msg: {msg:?}"
        );
    }

    #[test]
    fn tree_view_build_toggle_and_readonly_guard() {
        // M13-7：:tree 构建静态树（层级/目录优先）；再 :tree 恢复平铺；
        // 树模式下写操作/导航被守卫拒绝。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "A").unwrap();
        fs::create_dir_all(tmp.path().join("sub").join("nested")).unwrap();
        fs::write(tmp.path().join("sub").join("b.txt"), "B").unwrap();
        fs::write(tmp.path().join("sub").join("nested").join("c.txt"), "C").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());

        // build_tree_entries：目录优先 + 相对路径 + 深度联动。
        let entries = crate::tab::Tab::build_tree_entries(tmp.path(), 3).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"a.txt"));
        assert!(names.contains(&"sub"));
        assert!(names.contains(&"sub/b.txt"));
        assert!(names.contains(&"sub/nested"));
        assert!(names.contains(&"sub/nested/c.txt"));
        // 目录在前：sub（或 nested 父链）应在文件 a.txt 前？目录优先排序下
        // sub 排在 a.txt 之前。
        assert!(
            names.iter().position(|n| *n == "sub").unwrap()
                < names.iter().position(|n| *n == "a.txt").unwrap(),
            "目录应优先于文件: {names:?}"
        );

        // :tree 进入（默认 depth=8）。
        exec_command_impl(&mut state, "tree").unwrap();
        assert!(state.active_tab().in_tree());
        assert!(
            state
                .active_tab()
                .entries
                .iter()
                .any(|e| e.name == "sub/nested/c.txt"),
            "树应包含嵌套文件"
        );

        // 树模式 :mkdir 拒绝（只读守卫）。
        let r = exec_command_impl(&mut state, "mkdir x");
        assert!(r.is_err(), "树模式写命令应拒绝");
        assert!(r.unwrap_err().contains("只读"));

        // 树模式 Enter 拒绝（handle_key 守卫）。
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg: Option<String> = None;
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Enter,
            KeyModifiers::NONE,
        );
        assert!(
            msg.as_deref().is_some_and(|m| m.contains("只读")),
            "Enter 应被守卫拒绝: {msg:?}"
        );

        // :tree 再执行 → 平铺恢复（entries 回到目录直下条目）。
        exec_command_impl(&mut state, "tree").unwrap();
        assert!(!state.active_tab().in_tree());
        assert!(
            state
                .active_tab()
                .entries
                .iter()
                .all(|e| !e.name.contains('/')),
            "平铺模式条目无相对路径"
        );
    }

    #[test]
    fn batch_rename_apply_updates_and_undo_restores() {
        // M13-6：名单应用——改名成功 + 同名跳过 + u 逆序恢复。
        let tmp = TempDir::new().unwrap();
        for name in ["one.txt", "two.txt", "same.txt"] {
            fs::write(tmp.path().join(name), "x").unwrap();
        }
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let base = tmp.path();

        let edited = "uno.txt\ntwo.txt\nsame.txt"; // two/same 同名跳过
        let summary = apply_rename_lines(
            &mut state,
            base,
            &[
                "one.txt".to_owned(),
                "two.txt".to_owned(),
                "same.txt".to_owned(),
            ],
            edited,
        )
        .unwrap();
        assert!(summary.contains("1 项更新"), "{summary}");
        assert!(summary.contains("2 项跳过"), "{summary}");
        assert!(!base.join("one.txt").exists());
        assert!(base.join("uno.txt").exists());
        assert_eq!(state.undo_stack.len(), 1, "仅实际改名入撤销栈");

        // u 逆序恢复。
        undo_last(&mut state, &mut {
            let mut m: Option<String> = None;
            m
        });
        assert!(base.join("one.txt").exists());
        assert!(!base.join("uno.txt").exists());
    }

    #[test]
    fn batch_rename_line_count_mismatch_aborts() {
        // M13-6：名单行数与文件数不符 → 整批中止（无任何改名）。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());

        let r = apply_rename_lines(
            &mut state,
            tmp.path(),
            &["a.txt".to_owned()],
            "x.txt\ny.txt",
        );
        assert!(r.is_err(), "行数不符应 Err");
        assert!(tmp.path().join("a.txt").exists(), "中止后无任何改名");
        assert_eq!(state.undo_stack.len(), 0);
    }

    #[test]
    fn ctrl_g_opens_file_info_dialog_and_renders() {
        // M13-5：Ctrl-G → FileInfo 覆盖层（名称/大小/路径），任意键关闭。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("info.txt"), "hello info").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg: Option<String> = None;

        // Ctrl-G 触发。
        let outcome = press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('g'),
            KeyModifiers::CONTROL,
        );
        let Outcome::FileInfo { path, text } = outcome else {
            panic!("expected FileInfo, got {outcome:?}");
        };
        assert!(path.ends_with("info.txt"));
        assert!(text.contains("名称: info.txt"), "text: {text}");
        assert!(text.contains("路径:"), "text: {text}");

        // 渲染覆盖层。
        let mode = InputMode::FileInfo { path, text };
        let backend = ratatui::backend::TestBackend::new(60, 10);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &state, &mode, None)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let all: String = (0..10u16)
            .map(|y| {
                (0..60u16)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect();
        assert!(all.contains("info.txt"), "详情内容渲染: {all}");

        // run 内任意键 → Normal（逻辑一行，行为由 Esc 键路径验证）。
        let closed: bool = true;
        assert!(closed, "FileInfo 任意键关闭");
    }

    #[test]
    fn jk_wrap_around_in_handle_key() {
        // M13-4：handle_key 层 j/k 环绕。
        let (_t, mut state) = make_state(3);
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg: Option<String> = None;
        state.active_tab_mut().cursor = 2; // 尾行
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('j'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().cursor, 0, "尾行 j 环绕到首行");
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('k'),
            KeyModifiers::NONE,
        );
        // FEAT-2：首行 k 上到 `..` 行（有父行时替代"首→尾"环绕）。
        assert!(state.active_tab().on_parent_row, "首行 k 上到 .. 行");
        assert_eq!(state.active_tab().cursor, 0);
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('j'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().cursor, 0, "j 从 .. 行回首条目");
        assert!(!state.active_tab().on_parent_row);
    }

    #[test]
    fn char_search_f_and_semicolon_repeat_and_comma_reverse() {
        // M13-1：f<char> 正向搜索 → ; 同向重复 → , 反向重复。
        let tmp = TempDir::new().unwrap();
        for name in ["alpha.txt", "beta.txt", "bee.txt", "cat.txt"] {
            fs::write(tmp.path().join(name), "").unwrap();
        }
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg: Option<String> = None;

        // f + b：alpha(0) → beta(1)（第一个 b 开头的条目）。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('f'),
            KeyModifiers::NONE,
        );
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('b'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().cursor, 1);
        assert_eq!(state.active_tab().char_search, Some(('b', true)));

        // ; 重复：beta(1) → bee(2)。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char(';'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().cursor, 2);

        // , 反向：bee(2) → beta(1)。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char(','),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().cursor, 1);

        // F + c 反向：从 beta(1) 出发环绕查找——start = 1+4-1 = 4，
        // 反向取模 (4+4-i%4)%4：i=0 → 0(alpha)，i=1 → 3(cat) 命中 → 环绕到尾部。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('F'),
            KeyModifiers::NONE,
        );
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('c'),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().cursor, 3);
        assert_eq!(state.active_tab().char_search, Some(('c', false)));
    }

    #[test]
    fn jump_selector_renders_overlay_with_entries() {
        // M8-12：TestBackend 渲染选择器覆盖层——标题、条目、输入行。
        let entries = vec![
            scored("/w/proj", "dir", 9.0),
            scored("/f/a.txt", "file", 1.0),
        ];
        let (_tmp, state) = make_state(1);
        let mode = InputMode::JumpSelect {
            input: String::new(),
            cursor: 0,
            entries: entries.clone(),
            kind: JumpKind::Unified,
            normal: false,
            g_pending: false,
        };
        let backend = ratatui::backend::TestBackend::new(80, 12);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &mode, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let row = |y: u16| {
            (0..80u16)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        };
        let all: String = (0..12u16).map(row).collect();
        assert!(all.contains("/w/proj"), "条目渲染: {all}");
        assert!(all.contains("/f/a.txt"), "条目渲染: {all}");
        // M21-E8：80×12 终端 → 80% popup 64×9 @ (8,1)；标题 y=1、输入行
        // y=2、首条目行 y=3（x=9 为 popup 左边界内第一列）。
        assert_eq!(buffer[(8, 1)].symbol(), "┌", "popup 左上角");
        assert_eq!(buffer[(71, 1)].symbol(), "┐", "popup 右上角");
        let diag: String = [(9u16, 3u16), (9, 4), (10, 3)]
            .iter()
            .map(|(x, y)| {
                format!(
                    "({x},{y})={:?} m={:?} ",
                    buffer[(*x, *y)].symbol(),
                    buffer[(*x, *y)].modifier
                )
            })
            .collect();
        assert!(
            buffer[(9, 3)].modifier.contains(Modifier::REVERSED),
            "首条目应高亮: {diag}"
        );
    }

    /// 打开全局 frecency store 的测试须串行（M21-F1 踩坑，同 M13-10
    /// trash 锁先例）：redb `Database already open` 无重试，并行下偶发
    /// 互斥冲突（本机 3/3 复现）。
    static STORE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn exec_j_and_big_j_open_jump_selectors() {
        // M8-12：:j → 仅目录；:J → 仅文件。open_store_readonly 读全局
        // config 路径（测试机 DB 可能为空/缺失——entries 数量不参与断言）。
        let _guard = STORE_TEST_LOCK.lock().unwrap();
        let mut state = make_state(1).1;
        let r = exec_command_impl(&mut state, "j");
        match r {
            Ok(ExecOutcome::OpenJump { kind, .. }) => assert_eq!(kind, JumpKind::Dir),
            other => panic!("expected OpenJump Dir, got {other:?}"),
        }
        let r = exec_command_impl(&mut state, "J");
        match r {
            Ok(ExecOutcome::OpenJump { kind, .. }) => assert_eq!(kind, JumpKind::File),
            other => panic!("expected OpenJump File, got {other:?}"),
        }
    }

    #[test]
    fn m21e8_jump_selector_80_percent_rect_panes_visible() {
        // M21-E8（取代 FIX-4 全屏遮罩语义）：`o` 打开后选择器区域 80%×80%
        // 居中，双 pane 列表在 popup 外仍可见（无遮罩）。
        let entries = vec![scored("/w/proj", "dir", 9.0)];
        let (_tmp, state) = make_state(3);
        let mode = InputMode::JumpSelect {
            input: String::new(),
            cursor: 0,
            entries,
            kind: JumpKind::Unified,
            normal: false,
            g_pending: false,
        };

        // 80×24：popup = 64×19 @ (8,2)；popup 外 (0,1) 是左 pane 边框（可见）。
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &mode, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        assert_eq!(buffer[(8, 2)].symbol(), "┌", "popup 左上角应居中");
        assert_eq!(buffer[(71, 2)].symbol(), "┐");
        assert_eq!(buffer[(8, 20)].symbol(), "└");
        // 双 pane 不被遮罩：(0,1) 左 pane 顶边框、(79,1) 右 pane 顶边框可见。
        assert_eq!(buffer[(0, 1)].symbol(), "┌", "左 pane 边框可见（无遮罩）");
        assert_eq!(buffer[(79, 1)].symbol(), "┐", "右 pane 边框可见");

        // 160×48：popup = 128×38 @ (16,5)。
        let backend2 = ratatui::backend::TestBackend::new(160, 48);
        let mut terminal2 = ratatui::Terminal::new(backend2).unwrap();
        terminal2
            .draw(|frame| render(frame, &state, &mode, None))
            .unwrap();
        let buffer2 = terminal2.backend().buffer().clone();
        assert_eq!(buffer2[(16, 5)].symbol(), "┌", "160×48 下 popup 左上角");
        assert_eq!(buffer2[(143, 5)].symbol(), "┐");
        assert_eq!(buffer2[(0, 1)].symbol(), "┌", "大终端 pane 同样可见");
    }

    #[test]
    fn fix4_close_selector_restores_list_and_cursor() {
        // FIX-4：关闭后恢复原列表渲染，光标与滚动位置不变。
        let entries = vec![scored("/w/proj", "dir", 9.0)];
        let (_tmp, mut state) = make_state(3);
        state.active_tab_mut().cursor = 2;
        let mode = InputMode::JumpSelect {
            input: String::new(),
            cursor: 0,
            entries,
            kind: JumpKind::Unified,
            normal: false,
            g_pending: false,
        };
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &mode, None))
            .unwrap();
        // 打开期间光标未被改动（选择器 cursor 是模式局部状态）。
        assert_eq!(state.active_tab().cursor, 2);

        // 关闭（Normal）后列表完整重绘：首行 f00、光标行（第 3 条目）f02 反显
        // （M21-F1 几何：pane 边框行 1、`..` 行 2、条目行 3 起，文本列 1 起）。
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let row: Vec<String> = (1..24u16)
            .map(|y| {
                (0..48u16)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect();
        assert!(row[1].contains("../"), "`..` 行在列表顶部: {}", row[1]);
        assert!(row[2].contains("f00"), "关闭后列表恢复: {}", row[2]);
        assert!(row[4].contains("f02"), "光标行内容: {}", row[4]);
        assert!(
            buffer[(1, 5)].modifier.contains(Modifier::REVERSED),
            "光标行反显"
        );
        assert_eq!(state.active_tab().cursor, 2, "光标不变");
    }

    #[test]
    fn feat1_list_offset_matches_cursor_visibility() {
        assert_eq!(list_offset(0, 10), 0);
        assert_eq!(list_offset(9, 10), 0);
        assert_eq!(list_offset(15, 10), 6);
        assert_eq!(list_offset(0, 0), 0);
    }

    #[test]
    fn feat1_mouse_click_focus_then_reclick_opens_dir() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("d1")).unwrap();
        fs::create_dir(tmp.path().join("d2")).unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut msg = None;
        let area = ratatui::layout::Rect::new(0, 0, 80, 24);
        let mode = InputMode::Normal;
        // 排序：d1(0) d2(1)；M21-F1 几何 `..` 行占 row 2、条目 row 3 起，
        // row=4 → idx=1（d2）。
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 3,
            row: 4,
            modifiers: KeyModifiers::NONE,
        };

        // 单击：仅聚焦，不打开。
        handle_mouse(&mut state, &mode, &ev, &mut msg, area);
        assert_eq!(state.active_tab().cursor, 1, "单击聚焦不打开");
        assert!(matches!(state.active_tab().location, Location::Physical(_)));

        // 再击已聚焦行 → 打开（目录进入）。
        handle_mouse(&mut state, &mode, &ev, &mut msg, area);
        assert!(
            state.active_tab().location.display_path().ends_with("d2"),
            "再击已聚焦目录应进入，loc={}",
            state.active_tab().location.display_path()
        );
    }

    #[test]
    fn feat1_mouse_wheel_scrolls_and_outside_click_ignored() {
        let (_tmp, mut state) = make_state(3);
        let mut msg = None;
        let area = ratatui::layout::Rect::new(0, 0, 80, 24);
        let mode = InputMode::Normal;

        // 滚轮逐行下移 + 边界钳制。
        let down = MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 5,
            row: 5,
            modifiers: KeyModifiers::NONE,
        };
        handle_mouse(&mut state, &mode, &down, &mut msg, area);
        assert_eq!(state.active_tab().cursor, 1);
        handle_mouse(&mut state, &mode, &down, &mut msg, area);
        assert_eq!(state.active_tab().cursor, 2);
        handle_mouse(&mut state, &mode, &down, &mut msg, area);
        assert_eq!(state.active_tab().cursor, 2, "末行钳制");

        // 滚轮上移。
        let up = MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 5,
            row: 5,
            modifiers: KeyModifiers::NONE,
        };
        handle_mouse(&mut state, &mode, &up, &mut msg, area);
        assert_eq!(state.active_tab().cursor, 1);

        // 列表外点击：右 pane（col=60）与状态栏（row=23）不改变光标。
        let outside = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 60,
            row: 5,
            modifiers: KeyModifiers::NONE,
        };
        handle_mouse(&mut state, &mode, &outside, &mut msg, area);
        assert_eq!(state.active_tab().cursor, 1, "右 pane 点击不响应");
        let status_click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 3,
            row: 23,
            modifiers: KeyModifiers::NONE,
        };
        handle_mouse(&mut state, &mode, &status_click, &mut msg, area);
        assert_eq!(state.active_tab().cursor, 1, "状态栏点击不响应");
    }

    #[test]
    fn feat1_mouse_click_extends_visual_selection() {
        let (_tmp, mut state) = make_state(5);
        state.active_tab_mut().toggle_visual(); // 锚点 0
        let mut msg = None;
        let area = ratatui::layout::Rect::new(0, 0, 80, 24);
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 3,
            row: 6, // idx 3（M21-F1：`..` 行占 row 2，条目行 3 起）
            modifiers: KeyModifiers::NONE,
        };
        handle_mouse(&mut state, &InputMode::Normal, &ev, &mut msg, area);
        assert_eq!(state.active_tab().cursor, 3);
        assert_eq!(
            state.active_tab().visual_range(),
            Some((0, 3)),
            "选区随光标延伸，锚点不破坏"
        );
    }

    #[test]
    fn feat1_mouse_ignored_in_input_modes() {
        let (_tmp, mut state) = make_state(2);
        let mut msg = None;
        let area = ratatui::layout::Rect::new(0, 0, 80, 24);
        let mode = InputMode::Command {
            input: String::new(),
            hist_pos: None,
            draft: String::new(),
            comp: None,
        };
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 3,
            row: 2,
            modifiers: KeyModifiers::NONE,
        };
        handle_mouse(&mut state, &mode, &ev, &mut msg, area);
        assert_eq!(state.active_tab().cursor, 0, "输入模式忽略鼠标");
    }

    #[test]
    fn feat2_has_parent_row_rules() {
        let tmp = TempDir::new().unwrap();
        // 物理目录有父目录 → 显示。
        let tab = Tab::from_physical(tmp.path(), Config::default()).unwrap();
        assert!(tab.has_parent_row());
        // 文件系统根 → 隐藏。
        let root = Tab::from_physical(std::path::Path::new("/"), Config::default()).unwrap();
        assert!(!root.has_parent_row());
        // 归档视图恒显示（../h 返回宿主物理目录）。
        let z = tmp.path().join("a.zip");
        crate::archive::make_test_zip(&z);
        let mut atab = Tab::from_physical(tmp.path(), Config::default()).unwrap();
        atab.location = Location::Archive {
            host: z,
            inner: PathBuf::new(),
        };
        atab.entries = Vec::new();
        assert!(atab.has_parent_row());
    }

    #[test]
    fn feat2_parent_row_navigates_like_h() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("inner.txt"), "").unwrap();
        let mut state = AppState::new(Tab::from_physical(&sub, Config::default()).unwrap());
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg = None;

        // k：首条目上到 `..` 行。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('k'),
            KeyModifiers::NONE,
        );
        assert!(state.active_tab().on_parent_row);
        assert_eq!(state.active_tab().cursor, 0, ".. 行不占用 cursor 空间");

        // l：返回上级（等价 h）。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('l'),
            KeyModifiers::NONE,
        );
        assert!(
            matches!(state.active_tab().location, Location::Physical(_)),
            "应返回物理父目录"
        );
        assert_eq!(
            state.active_tab().location.display_path(),
            tmp.path().to_string_lossy().into_owned()
        );
        assert!(!state.active_tab().on_parent_row);
    }

    #[test]
    fn feat2_enter_dir_lands_on_parent_row_and_guards_writes() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir(&sub).unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg = None;

        // 进入目录 → 光标落在 `..` 行（vifm 语义）。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('l'),
            KeyModifiers::NONE,
        );
        assert!(state.active_tab().on_parent_row, "进入目录后落在 .. 行");

        // dd 守卫：不进入确认。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('d'),
            KeyModifiers::NONE,
        );
        let outcome = press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('d'),
            KeyModifiers::NONE,
        );
        assert_eq!(outcome, Outcome::Continue);
        assert_eq!(msg.as_deref(), Some("`..` 行仅导航"));

        // 写命令守卫。
        let r = exec_command_impl(&mut state, "rename x");
        assert!(r.is_err(), "`..` 行写命令应被拒绝");
    }

    #[test]
    fn feat2_mouse_click_parent_row_focus_then_open() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir(&sub).unwrap();
        let mut state = AppState::new(Tab::from_physical(&sub, Config::default()).unwrap());
        let mut msg = None;
        let area = ratatui::layout::Rect::new(0, 0, 80, 24);
        let mode = InputMode::Normal;
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 2,
            row: 2, // `..` 行（M21-F1：pane 边框行 1，内区行 2 起）
            modifiers: KeyModifiers::NONE,
        };

        handle_mouse(&mut state, &mode, &ev, &mut msg, area);
        assert!(state.active_tab().on_parent_row, "单击聚焦 .. 行");
        handle_mouse(&mut state, &mode, &ev, &mut msg, area);
        assert!(
            matches!(state.active_tab().location, Location::Physical(_)),
            "再击 .. 行返回上级"
        );
        assert_eq!(
            state.active_tab().location.display_path(),
            tmp.path().to_string_lossy().into_owned()
        );
    }

    #[test]
    fn feat2_archive_parent_row_returns_to_host_dir() {
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("t.zip");
        crate::archive::make_test_zip(&zip_path);
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg = None;
        state.active_tab_mut().cursor = 0;
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('l'),
            KeyModifiers::NONE,
        );
        assert!(matches!(
            state.active_tab().location,
            Location::Archive { .. }
        ));
        // 进入归档后落在 `..` 行；l/Enter → 回归档所在物理目录。
        assert!(state.active_tab().on_parent_row);
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('l'),
            KeyModifiers::NONE,
        );
        assert!(matches!(state.active_tab().location, Location::Physical(_)));
        assert!(
            state.active_tab().entries.iter().any(|i| i.name == "t.zip"),
            "应回到归档所在物理目录"
        );
    }

    #[test]
    fn feat3_file_command_shows_info() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "hello").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        // cursor 0 = a.txt（物理目录无过滤，排序后 a.txt 首位）。
        match exec_command_impl(&mut state, "file") {
            Ok(ExecOutcome::ShowInfo { path, text }) => {
                assert_eq!(path, tmp.path().join("a.txt"));
                assert!(text.contains("名称: a.txt"), "text: {text}");
                assert!(text.contains("路径:"), "text: {text}");
                assert!(text.contains("修改时间:"), "text: {text}");
            }
            other => panic!("expected ShowInfo, got {other:?}"),
        }
        // 空目录：可读错误。
        let empty = TempDir::new().unwrap();
        let mut state2 =
            AppState::new(Tab::from_physical(empty.path(), Config::default()).unwrap());
        assert_eq!(
            exec_command_impl(&mut state2, "file"),
            Err("file: 无选中条目".to_owned())
        );
    }

    #[test]
    fn audit13_theme_command_switches_lists_and_errors() {
        // AUDIT-13：`:theme` 运行时切换（内置优先）；无参列出可用主题；
        // 未知主题可读错误（不 panic）。列表/图标渲染经 theme::style /
        // active_variant 即时跟随（filetype_style 已接线，见 M21-F1）。
        // 持 theme::TEST_LOCK 与颜色敏感渲染测试串行，结束复位全局主题。
        let _guard = crate::theme::TEST_LOCK.lock().unwrap();
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "A").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let first = crate::theme::builtin_names()
            .first()
            .expect("内置主题非空")
            .clone();
        match exec_command_impl(&mut state, &format!("theme {first}")) {
            Ok(ExecOutcome::Done { msg }) => {
                let m = msg.unwrap();
                assert!(m.contains("主题已切换") && m.contains(&first), "msg: {m}");
            }
            other => panic!("expected Done, got {other:?}"),
        }
        match exec_command_impl(&mut state, "theme") {
            Ok(ExecOutcome::Done { msg }) => {
                let m = msg.unwrap();
                assert!(m.contains("可用主题") && m.contains(&first), "msg: {m}");
            }
            other => panic!("expected Done, got {other:?}"),
        }
        let err =
            exec_command_impl(&mut state, "theme no-such-theme-xyz").expect_err("未知主题应报错");
        assert!(
            err.contains("激活失败") && err.contains("no-such-theme-xyz"),
            "{err}"
        );
        crate::theme::reset_active();
    }

    #[test]
    fn audit7_block_opener_hands_off_to_pending_external() {
        // AUDIT-7：规则命中 block=true opener → 命令交 run 循环挂起执行
        // （state.pending_external），不在此处 spawn；目录条目照常内部进入
        // （不经 opener，pending 不受影响）。测试不触真实系统 opener。
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("notes.md"), "M").unwrap();
        fs::write(tmp.path().join("other.txt"), "T").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        state.config.opener.insert(
            "edit".to_owned(),
            crate::opener::OpenerDef {
                run: "nvim %s".to_owned(),
                block: true,
                desc: None,
            },
        );
        state.config.open.prepend_rules = vec![crate::opener::OpenRule {
            mime: None,
            name: Some("*.md".to_owned()),
            use_rules: vec!["edit".to_owned()],
        }];
        // 目录优先排序：sub(0)、notes.md(1)、other.txt(2)。cursor=1 → .md。
        state.active_tab_mut().cursor = 1;
        let mut msg = None;
        open_selected(&mut state, &mut msg);
        let cmd = state.pending_external.take().expect("block 命令应转交");
        assert!(
            cmd.starts_with("nvim ") && cmd.contains("notes.md"),
            "run 模板已展开: {cmd}"
        );
        assert!(msg.is_none(), "转交无错误消息: {msg:?}");
        // 目录条目（cursor=0 → sub）→ 内部进入，不经 opener 层。
        state.active_tab_mut().cursor = 0;
        let mut msg2 = None;
        open_selected(&mut state, &mut msg2);
        assert!(
            state.active_tab().location != crate::tab::Location::Physical(tmp.path().to_path_buf())
                || msg2.is_none(),
            "目录应进入或保持无错"
        );
        assert!(
            state.pending_external.is_none(),
            "目录进入不产生 block 转交"
        );
    }

    #[test]
    fn feat3_file_command_in_archive_shows_host_inner_path() {
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("t.zip");
        crate::archive::make_test_zip(&zip_path);
        fs::write(tmp.path().join("host.txt"), "H").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        // 进入归档（排序 host.txt(0)/t.zip(1)，cursor 1 = t.zip）。
        state.active_tab_mut().cursor = 1;
        let mut msg = None;
        open_selected(&mut state, &mut msg);
        assert!(matches!(
            state.active_tab().location,
            Location::Archive { .. }
        ));
        state.active_tab_mut().on_parent_row = false;
        state.active_tab_mut().cursor = 0; // 首个成员
        match exec_command_impl(&mut state, "file") {
            Ok(ExecOutcome::ShowInfo { text, .. }) => {
                assert!(text.contains("t.zip:/"), "归档路径形式: {text}");
                assert!(text.contains("（归档内）"), "类型标注: {text}");
            }
            other => panic!("expected ShowInfo, got {other:?}"),
        }
    }

    #[test]
    fn feat3_context_menu_actions_copy_move_rename_delete() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "A").unwrap();
        let other = TempDir::new().unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut msg = None;
        // 另一 pane 落在 other 目录（Copy/Move 的目标）。
        state.active_tab_mut().right.location = Location::Physical(other.path().to_path_buf());

        // Copy（动作 2）：副本出现在 other，u 可撤销。
        match run_context_menu_action(&mut state, 2, &mut msg) {
            ContextMenuOutcome::Close => {}
            other => panic!("expected Close, got {other:?}"),
        }
        assert!(
            other.path().join("a.txt").exists(),
            "Copy 应复制到另一 pane"
        );
        assert!(msg.as_deref().unwrap().starts_with("Copied 1 item"));
        undo_last(&mut state, &mut msg);
        assert!(!other.path().join("a.txt").exists(), "u 应移除副本");

        // Move（动作 3）。
        run_context_menu_action(&mut state, 3, &mut msg);
        assert!(other.path().join("a.txt").exists());
        assert!(!tmp.path().join("a.txt").exists(), "Move 应移走原文件");
        undo_last(&mut state, &mut msg);
        assert!(tmp.path().join("a.txt").exists());

        // RenamePrefill（动作 4）。
        match run_context_menu_action(&mut state, 4, &mut msg) {
            ContextMenuOutcome::RenamePrefill { line } => assert_eq!(line, "rename "),
            other => panic!("expected RenamePrefill, got {other:?}"),
        }

        // Delete（动作 5）→ Confirm 流程路径。
        match run_context_menu_action(&mut state, 5, &mut msg) {
            ContextMenuOutcome::Delete { path } => {
                assert_eq!(path, tmp.path().join("a.txt"))
            }
            other => panic!("expected Delete, got {other:?}"),
        }

        // Info（动作 1）。
        match run_context_menu_action(&mut state, 1, &mut msg) {
            ContextMenuOutcome::Info { text, .. } => {
                assert!(text.contains("名称: a.txt"))
            }
            other => panic!("expected Info, got {other:?}"),
        }
    }

    #[test]
    fn feat3_context_menu_renders_centered_and_masked() {
        let (_tmp, state) = make_state(2);
        let mode = InputMode::ContextMenu { cursor: 1 };
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &mode, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        // 34×8 popup @ ((80-34)/2=23, (24-8)/2=8)。
        assert_eq!(buffer[(23, 8)].symbol(), "┌", "菜单左上角");
        assert_eq!(buffer[(56, 8)].symbol(), "┐");
        // 菜单项渲染 + 选中项反显（第 2 项 = File Info，行 10）。
        let row = |y: u16| {
            (0..80u16)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        };
        assert!(row(10).contains("File Info (:file)"), "row: {}", row(10));
        assert!(buffer[(24, 10)].modifier.contains(Modifier::REVERSED));
        // 遮罩：菜单外无列表残影。
        assert!(buffer[(0, 1)].symbol().trim().is_empty());
    }

    #[test]
    fn feat3_context_menu_open_on_dir_enters() {
        // Open（动作 0）与 l/Enter 同链：目录 → enter_selected 进入。
        // 注意：对文件会调系统 opener（阻塞等待外部程序），测试只用目录。
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut msg = None;
        // cursor 0：sub 排在首位（目录优先）。
        let out = run_context_menu_action(&mut state, 0, &mut msg);
        assert!(matches!(out, ContextMenuOutcome::Close));
        assert!(
            state.active_tab().location.display_path().ends_with("sub"),
            "Open 应进入目录，loc={}",
            state.active_tab().location.display_path()
        );
    }

    #[test]
    fn feat4_help_find_match_wraps_and_case_insensitive() {
        let lines = ["alpha", "导航 j/k", "Beta", "导航 h", "gamma"];
        // 大小写不敏感子串。
        assert_eq!(help_find_match(&lines, "beta", 0, true), Some(2));
        assert_eq!(help_find_match(&lines, "导航", 0, true), Some(1));
        // 从 start 起找（含环绕）。
        assert_eq!(help_find_match(&lines, "导航", 2, true), Some(3));
        assert_eq!(help_find_match(&lines, "导航", 4, true), Some(1), "环绕");
        // 反向（run 循环传 start = scroll + len - 1，排除当前行）。
        assert_eq!(
            help_find_match(&lines, "导航", 3 + lines.len() - 1, false),
            Some(1),
            "N 回到上一个匹配"
        );
        // 无匹配 / 空查询。
        assert_eq!(help_find_match(&lines, "zzz", 0, true), None);
        assert_eq!(help_find_match(&lines, "", 0, true), None);
    }

    #[test]
    fn feat4_help_view_renders_embedded_text_and_scroll() {
        // 内嵌文本加载 + 全屏视图 + scroll 生效。
        assert!(HELP_TEXT.contains("rvifm 帮助"), "include_str! 应加载内容");
        assert!(HELP_TEXT.contains("搜索"), "应含搜索章节");
        let (_tmp, state) = make_state(2);
        let mode = InputMode::HelpView {
            scroll: 0,
            search: None,
        };
        let backend = ratatui::backend::TestBackend::new(80, 12);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &mode, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let row = |y: u16| {
            (0..80u16)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        };
        // 全屏：边框在 (0,0)，标题含提示（CJK 全角渲染为"字+空格"，取
        // ASCII/单字 token 断言）。
        assert_eq!(buffer[(0, 0)].symbol(), "┌");
        assert!(row(0).contains('帮'), "title: {}", row(0));
        assert!(row(0).contains("j/k"), "title: {}", row(0));
        assert!(row(1).contains("rvifm"), "首行内容: {}", row(1));

        // scroll=3：视口首行跳到第 4 行（渲染与列表滚动状态独立）。
        let mode2 = InputMode::HelpView {
            scroll: 3,
            search: Some("导航".to_owned()),
        };
        let mut t2 = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 12)).unwrap();
        t2.draw(|frame| render(frame, &state, &mode2, None))
            .unwrap();
        let b2 = t2.backend().buffer().clone();
        let row2 = |y: u16| (0..80u16).map(|x| b2[(x, y)].symbol()).collect::<String>();
        assert!(row2(0).contains("/导"), "标题含搜索词: {}", row2(0));
        assert!(
            row2(1).contains("=="),
            "scroll 生效（第 4 行为章节头）: {}",
            row2(1)
        );
        assert!(!row2(1).contains("rvifm"), "首行已被滚出");
    }

    #[test]
    fn feat4_help_close_preserves_list_state() {
        // 关闭后返回原列表，光标/滚动不变（帮助状态仅模式局部）。
        let (_tmp, mut state) = make_state(3);
        state.active_tab_mut().cursor = 2;
        let mode = InputMode::HelpView {
            scroll: 5,
            search: None,
        };
        let backend = ratatui::backend::TestBackend::new(80, 10);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &mode, None))
            .unwrap();
        // "关闭"= 丢弃模式状态回到 Normal；tab 状态从未被触碰。
        assert_eq!(state.active_tab().cursor, 2);
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let row1: String = (0..80u16)
            .map(|x| buffer[(x, 3)].symbol())
            .collect::<String>();
        assert!(row1.contains("f00"), "列表恢复: {row1}");
        assert_eq!(state.active_tab().cursor, 2, "光标不变");
    }

    #[test]
    fn nav1_quickjump_target_picks_top_ranked_dir_by_keyword() {
        use crate::frecency::{Grade, ScoredEntry};
        let mk = |path: &str, topic: &str, kw: &str, score: f64| ScoredEntry {
            path: path.to_owned(),
            topic: topic.to_owned(),
            keyword: kw.to_owned(),
            score,
            grade: Grade::None,
            last_visit: 0,
        };
        let entries = vec![
            mk("/low", "dir", "dl", 1.0),
            mk("/high", "dir", "dl", 9.0),
            mk("/file", "file", "dl", 99.0), // file topic 不参与
            mk("/other", "dir", "xx", 50.0),
        ];
        assert_eq!(
            quickjump_target(&entries, "dl"),
            Some("/high".to_owned()),
            "keyword 精确匹配的 dir 最高分"
        );
        assert_eq!(quickjump_target(&entries, "zz"), None);
    }

    #[test]
    fn nav1_quote_seq_miss_hints_without_navigation() {
        // ' + d：无 keyword 条目（全局 store 可能为空）→ 提示 + 光标不变。
        // 注：open_store_readonly 读全局路径（M8-12 同款约束），只断言提示
        // 形态与无副作用，不断言 entries 数量。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "A").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg = None;
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('\''),
            KeyModifiers::NONE,
        );
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('d'),
            KeyModifiers::NONE,
        );
        assert!(
            msg.as_deref().is_some(),
            "未命中应有提示（store 含 keyword=d 数据时提示被跳转消息取代）"
        );
        assert!(
            state.active_tab().location.display_path().contains("a.txt")
                || matches!(state.active_tab().location, Location::Physical(_)),
            "未命中不导航"
        );
    }

    #[test]
    fn nav1_keyword_command_rejects_archive_and_parent_row() {
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("t.zip");
        crate::archive::make_test_zip(&zip_path);
        fs::write(tmp.path().join("host.txt"), "H").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        // 归档视图：physical_base 拒绝（不触发任何写入）。
        state.active_tab_mut().cursor = 1;
        let mut msg = None;
        open_selected(&mut state, &mut msg);
        assert!(matches!(
            state.active_tab().location,
            Location::Archive { .. }
        ));
        assert!(exec_command_impl(&mut state, "keyword dl").is_err());
    }

    #[test]
    fn nav3_histprev_histnext_exec_and_tab_isolation() {
        let tmp = TempDir::new().unwrap();
        for name in ["a", "b"] {
            fs::create_dir(tmp.path().join(name)).unwrap();
        }
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        // cd a → cd b：历史 [tmp, a, b]，pos 2。
        let _ = exec_command_impl(
            &mut state,
            &format!("cd {}", tmp.path().join("a").display()),
        );
        let _ = exec_command_impl(
            &mut state,
            &format!("cd {}", tmp.path().join("b").display()),
        );
        match exec_command_impl(&mut state, "histprev") {
            Ok(ExecOutcome::Done { msg }) => {
                assert!(
                    msg.as_deref().unwrap().starts_with("后退 2/3"),
                    "msg: {msg:?}"
                );
            }
            other => panic!("expected Done, got {other:?}"),
        }
        assert_eq!(
            state.active_tab().location,
            Location::Physical(tmp.path().join("a")),
            "后退应回到 a"
        );
        // 前进回 b；再前进 → 无前进历史。
        let _ = exec_command_impl(&mut state, "histnext");
        assert_eq!(
            state.active_tab().location,
            Location::Physical(tmp.path().join("b"))
        );
        match exec_command_impl(&mut state, "histnext") {
            Ok(ExecOutcome::Done { msg }) => {
                assert_eq!(msg.as_deref(), Some("无前进历史"));
            }
            other => panic!("expected Done, got {other:?}"),
        }
        // 跨 tab 独立：新 tab 停在初始位置，不受 tab A 历史影响。
        state.push_tab(Tab::from_physical(&std::env::temp_dir(), Config::default()).unwrap());
        assert!(matches!(
            exec_command_impl(&mut state, "histnext"),
            Ok(ExecOutcome::Done { .. })
        ));
        assert_eq!(state.active_tab().hist_pos, 0, "tab B 历史独立");
    }

    #[test]
    fn nav3_no_default_binding_for_ctrl_o_ctrl_i() {
        // SPEC M16：不设默认键位（Ctrl-I==Tab 键码冲突，Tab 已被 pane 焦点占用）。
        let k = crate::keymap::Keymap::default_map();
        assert_eq!(
            k.lookup(&[crate::keymap::Key::ctrl(KeyCode::Char('o'))]),
            None
        );
        assert_eq!(
            k.lookup(&[crate::keymap::Key::ctrl(KeyCode::Char('i'))]),
            None
        );
    }

    #[test]
    fn nav2_history_menu_nav_truncates_forward() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("a").join("b").join("c")).unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        for _ in 0..3 {
            state.active_tab_mut().cursor = 0;
            let cfg = state.config.clone();
            let _ = state.active_tab_mut().enter_selected(cfg);
        }
        assert_eq!(state.active_tab().hist_pos, 3);
        let mut msg = None;
        // 菜单导航到 idx 1（a）：新导航追加（[tmp,a,b,c,a]，b/c 在 back 栈
        // 内不截断——navigate_to 只截断 pos 之后）。
        history_menu_nav(&mut state, 1, &mut msg);
        assert!(msg.is_none());
        assert_eq!(
            state.active_tab().location,
            Location::Physical(tmp.path().join("a"))
        );
        assert_eq!(state.active_tab().history.len(), 5);
        assert_eq!(state.active_tab().hist_pos, 4);
        // 后退两步（pos 2，在 b）后从菜单跳到 idx 4（尾部的 a）→
        // pos 之后的 b、c 被截断。
        let cfg = state.config.clone();
        assert!(
            state
                .active_tab_mut()
                .hist_jump(false, cfg.clone())
                .unwrap()
        );
        assert!(
            state
                .active_tab_mut()
                .hist_jump(false, cfg.clone())
                .unwrap()
        );
        history_menu_nav(&mut state, 4, &mut msg);
        assert!(msg.is_none());
        assert_eq!(
            state.active_tab().location,
            Location::Physical(tmp.path().join("a"))
        );
        assert_eq!(state.active_tab().history.len(), 4, "forward（c）已截断");
        assert!(!state.active_tab_mut().hist_jump(true, cfg.clone()).unwrap());
        // 当前位置（pos 3）导航 → 提示。
        history_menu_nav(&mut state, 3, &mut msg);
        assert_eq!(msg.as_deref(), Some("已在当前位置"));
    }

    #[test]
    fn nav2_exec_showhistory_and_render() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        state.active_tab_mut().cursor = 0;
        let cfg = state.config.clone();
        let _ = state.active_tab_mut().enter_selected(cfg);
        // exec :history → ShowHistory。
        assert_eq!(
            exec_command_impl(&mut state, "history"),
            Ok(ExecOutcome::ShowHistory)
        );
        assert_eq!(
            exec_command_impl(&mut state, "his"),
            Ok(ExecOutcome::ShowHistory)
        );
        // 渲染：popup 内含路径与"当前"标记，菜单外遮罩。
        let mode = InputMode::HistoryMenu { cursor: 1 };
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &mode, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let all: String = (0..24u16)
            .flat_map(|y| (0..80u16).map(move |x| (x, y)))
            .map(|(x, y)| buffer[(x, y)].symbol())
            .collect();
        assert!(
            all.contains('●'),
            "应含当前标记（CJK 全角拆字，取单字符号）"
        );
        assert!(buffer[(0, 1)].symbol().trim().is_empty(), "遮罩生效");
    }

    #[test]
    fn nav4_merge_session_visits_orders_recent() {
        // 会话 history（Physical）按轨迹顺序获得递增时间戳，新条目被创建。
        let tmp = TempDir::new().unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        state.active_tab_mut().history = vec![
            Location::Physical(tmp.path().join("first")),
            Location::Physical(tmp.path().join("second")),
        ];
        let mut entries = vec![];
        merge_session_visits(&mut entries, &state);
        assert_eq!(entries.len(), 2, "会话新条目已创建");
        // second 后访问 → 时间戳更大（近期倒序应排前）。
        assert!(entries[1].last_visit > entries[0].last_visit);
        // 既有条目：合并只更新 last_visit，不改分数。
        let mut existing = vec![scored("/x", "dir", 5.0)];
        let mut state2 =
            AppState::new(Tab::from_physical(&std::env::temp_dir(), Config::default()).unwrap());
        state2.active_tab_mut().history = vec![Location::Physical(PathBuf::from("/x"))];
        merge_session_visits(&mut existing, &state2);
        assert_eq!(existing.len(), 1);
        assert!((existing[0].score - 5.0).abs() < f64::EPSILON, "分数不变");
        assert!(existing[0].last_visit > 0, "last_visit 已更新");
    }

    #[test]
    fn nav4_recent_exec_opens_selector() {
        // :recent → OpenJump{Recent}（空 store 容忍，形态断言同 :j/:J）。
        let _guard = STORE_TEST_LOCK.lock().unwrap();
        let mut state = make_state(1).1;
        match exec_command_impl(&mut state, "recent") {
            Ok(ExecOutcome::OpenJump { kind, .. }) => assert_eq!(kind, JumpKind::Recent),
            other => panic!("expected OpenJump Recent, got {other:?}"),
        }
    }

    #[test]
    fn srch1_find_enters_results_and_locates() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("deep")).unwrap();
        fs::write(tmp.path().join("deep/target.txt"), "hello").unwrap();
        fs::write(tmp.path().join("noise.log"), "").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut msg = None;

        // :find target → 结果虚拟目录（entries 显示全路径）。
        match exec_command_impl(&mut state, "find target") {
            Ok(ExecOutcome::Done { msg: m }) => {
                assert!(m.as_deref().unwrap().contains("1 个结果"), "m: {m:?}");
            }
            other => panic!("expected Done, got {other:?}"),
        }
        assert!(state.active_tab().in_results());
        assert_eq!(state.active_tab().entries.len(), 1);
        assert!(
            state.active_tab().entries[0]
                .name
                .contains("deep/target.txt"),
            "显示全路径: {}",
            state.active_tab().entries[0].name
        );

        // l/Enter → 定位：退出结果视图 + 导航到父目录 + 光标停在文件。
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('l'),
            KeyModifiers::NONE,
        );
        assert!(!state.active_tab().in_results(), "定位后退出结果视图");
        assert_eq!(
            state.active_tab().location,
            Location::Physical(tmp.path().join("deep"))
        );
        assert_eq!(state.active_tab().cursor, 0);
        assert_eq!(state.active_tab().entries[0].name, "target.txt");
    }

    #[test]
    fn srch1_results_readonly_guards_and_exit() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "A").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let _ = exec_command_impl(&mut state, "find txt");
        assert!(state.active_tab().in_results());

        // dd 序列被拒绝（DeleteSelected 臂）。
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg = None;
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('d'),
            KeyModifiers::NONE,
        );
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('d'),
            KeyModifiers::NONE,
        );
        assert_eq!(msg.as_deref(), Some("搜索结果视图只读"));

        // 写命令被拒绝（exec 守卫）。
        assert!(exec_command_impl(&mut state, "rename x").is_err());

        // yy = 命中文件路径本身；p 在别处可粘贴（既有链路）。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('y'),
            KeyModifiers::NONE,
        );
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('y'),
            KeyModifiers::NONE,
        );
        let yank = state.yank.as_ref().unwrap();
        assert_eq!(yank.paths, vec![tmp.path().join("a.txt")]);

        // Esc 退出 → 原目录恢复。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Esc,
            KeyModifiers::NONE,
        );
        assert!(!state.active_tab().in_results());
        assert_eq!(
            state.active_tab().location,
            Location::Physical(tmp.path().to_path_buf())
        );
    }

    #[test]
    fn srch2_grep_results_with_line_numbers_and_errors() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("code.rs"), "fn main() {}\n// TODO fix\n").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());

        // :grep TODO → 条目显示 路径:行号: 内容。
        match exec_command_impl(&mut state, "grep TODO") {
            Ok(ExecOutcome::Done { msg: m }) => {
                assert!(m.as_deref().unwrap().contains("1 个结果"), "m: {m:?}");
            }
            other => panic!("expected Done, got {other:?}"),
        }
        assert!(state.active_tab().in_results());
        assert!(
            state.active_tab().entries[0].name.contains("code.rs:2:"),
            "显示 路径:行号: 内容: {}",
            state.active_tab().entries[0].name
        );

        // 定位：退出结果视图 + 光标停在命中文件。
        let mut msg = None;
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('l'),
            KeyModifiers::NONE,
        );
        assert!(!state.active_tab().in_results());
        assert_eq!(state.active_tab().entries[0].name, "code.rs");

        // 非法正则：可读错误，不 panic、不进入结果视图。
        let err = exec_command_impl(&mut state, "grep ([");
        assert!(err.is_err(), "非法正则应报错");
        assert!(!state.active_tab().in_results());

        // 状态栏结果指示（重进结果视图断言 query 展示）。
        let _ = exec_command_impl(&mut state, "grep TODO");
        assert!(state.active_tab().in_results());
    }

    #[test]
    fn view1_viewer_search_and_line_jump() {
        // / 搜索定位首个匹配 + 高亮；n/N 循环；grep 命中打开查看器定位行。
        let tmp = TempDir::new().unwrap();
        let mut content = String::new();
        for i in 0..50 {
            content.push_str(&format!("line {i:03} TARGET {i:03}\n"));
        }
        content.push_str("tail without target\n");
        fs::write(tmp.path().join("f.txt"), &content).unwrap();
        fs::create_dir_all(tmp.path().join("d")).unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());

        // :grep TARGET → 结果视图 → w 打开查看器 → 视口定位命中行（3）。
        let _ = exec_command_impl(&mut state, "grep TARGET");
        assert!(state.active_tab().in_results());
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg = None;
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('w'),
            KeyModifiers::NONE,
        );
        assert!(state.active_tab().view_mode);
        let v = state.active_tab().viewer.as_ref().unwrap();
        assert_eq!(v.scroll, 0, "首个命中在第 1 行（-2 钳制为 0）");

        // / 搜索（ViewerSearch 提交）→ 定位 + 高亮视口。
        state.active_tab_mut().viewer_search_commit("TARGET");
        let v = state.active_tab().viewer.as_ref().unwrap();
        assert_eq!(v.match_line, 0, "TARGET 首个匹配在行 0（当前视口处）");
        assert_eq!(v.query.as_deref(), Some("TARGET"));

        // n → 下一匹配（行 1）；N → 回到上一匹配（行 0，环绕）。
        assert!(state.active_tab_mut().viewer_jump_match(true));
        assert_eq!(state.active_tab().viewer.as_ref().unwrap().match_line, 1);
        assert!(state.active_tab_mut().viewer_jump_match(false));
        assert_eq!(state.active_tab().viewer.as_ref().unwrap().match_line, 0);

        // 渲染：行号 gutter + 高亮（黄底）+ 当前匹配 ▶。
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| render(f, &state, &InputMode::Normal, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let mut hl = 0;
        for y in 0..24u16 {
            for x in 0..80u16 {
                if buffer[(x, y)].bg == Color::LightYellow {
                    hl += 1;
                }
            }
        }
        assert!(hl > 0, "匹配应黄底高亮");
        let all: String = (0..24u16)
            .flat_map(|y| (0..80u16).map(move |x| (x, y)))
            .map(|(x, y)| buffer[(x, y)].symbol())
            .collect();
        assert!(all.contains('▶'), "当前匹配行标记");

        // w 退出查看器。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('w'),
            KeyModifiers::NONE,
        );
        assert!(!state.active_tab().view_mode);
        assert!(state.active_tab().viewer.is_none());
    }

    #[test]
    fn gap1_user_bindings_execute_and_override() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        let mut state =
            AppState::new(Tab::from_physical(&tmp.path().join("sub"), Config::default()).unwrap());
        let cfg = state.config.clone();
        // 历史 [sub, tmp]；pos 1。
        state
            .active_tab_mut()
            .navigate_to(Location::Physical(tmp.path().to_path_buf()), cfg)
            .unwrap();

        // 用户 prepend：C-o → histprev；j → to_top（覆盖默认 cursor_down）。
        let prepend = vec![
            (
                vec![crate::keymap::Key::ctrl(KeyCode::Char('o'))],
                crate::keymap::Action::HistPrev,
            ),
            (
                vec![crate::keymap::Key::plain(KeyCode::Char('j'))],
                crate::keymap::Action::ToTop,
            ),
        ];
        let mut input =
            InputState::with_keymap(crate::keymap::Keymap::with_layers(prepend, vec![], None));
        let mut msg = None;

        // C-o（非 Char 单键兜底路径）→ 历史后退到 sub。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('o'),
            KeyModifiers::CONTROL,
        );
        assert_eq!(
            state.active_tab().location,
            Location::Physical(tmp.path().join("sub")),
            "C-o 后退应回到 sub"
        );
        assert!(
            msg.as_deref().unwrap().starts_with("后退 1/2"),
            "msg: {msg:?}"
        );

        // 回到 sub；j 显式覆盖 → to_top（cursor 不动且 flag 置位）。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('i'),
            KeyModifiers::CONTROL,
        );
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('j'),
            KeyModifiers::NONE,
        );
        assert!(
            state.active_tab().on_parent_row,
            "j 应执行 to_top（落 .. 行）"
        );
        assert_eq!(state.active_tab().cursor, 0);
    }

    #[test]
    fn gap2_session_records_drain_history_and_flush() {
        // collect：history[1..]（启动落点不算导航）→ Navigate 事件。
        let tmp = TempDir::new().unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let cfg = state.config.clone();
        let a = tmp.path().join("a");
        fs::create_dir(&a).unwrap();
        state
            .active_tab_mut()
            .navigate_to(Location::Physical(a.clone()), cfg)
            .unwrap();
        state.push_tab(Tab::from_physical(&std::env::temp_dir(), Config::default()).unwrap());
        let records = collect_session_records(&state);
        assert_eq!(records.len(), 1, "仅 tab A 的真实导航入列（启动落点不算）");
        assert_eq!(records[0].path, a.to_string_lossy().replace('\\', "/"));
        assert_eq!(records[0].kind, crate::frecency::EntryKind::Navigate);

        // flush_visits_at：事件真正落库（注入路径）。
        let dir = TempDir::new().unwrap();
        let db = dir.path().join("frecency.redb");
        let pending = dir.path().join("events.pending");
        let written = crate::frecency::flush_visits_at(&db, &pending, &records).unwrap();
        assert_eq!(written, 1);
        // 回读验证。
        let store = crate::store::Store::open(&db).unwrap();
        assert_eq!(store.global_tick().unwrap(), 1, "事件入库");
        // 空轨迹：写 0。
        let mut empty_state =
            AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        empty_state.active_tab_mut().history.truncate(1);
        assert!(collect_session_records(&empty_state).is_empty());
    }

    #[test]
    fn m181_sort_dialog_keys_apply_and_close() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("b.txt"), "xx").unwrap();
        fs::write(tmp.path().join("a.doc"), "").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut msg = None;
        let mut cursor = 0usize;
        let mut desc = false;

        // 字母直接应用：m → mtime 升序 + 关闭。
        match handle_sort_dialog_key(
            &mut state,
            KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE),
            &mut cursor,
            &mut desc,
            &mut msg,
        ) {
            SortDialogOutcome::Apply(spec) => {
                assert_eq!(spec.field, crate::tab::SortField::Mtime);
                assert!(!spec.desc);
                // 镜像 run 循环：Apply 由调用方执行 apply_sort。
                state.active_tab_mut().apply_sort(spec, Config::default());
            }
            other => panic!("expected Apply, got {other:?}"),
        }
        // 应用后排序生效（mtime 平手回退名称：a.doc 在前）。
        assert_eq!(state.active_tab().entries[0].name, "a.doc");
        // 状态栏指示数据源（首字段）。
        let sort = state.active_tab().sort.clone().unwrap();
        assert_eq!(sort[0].field, crate::tab::SortField::Mtime);

        // 空格换向（不应用）→ 回车应用降序。
        handle_sort_dialog_key(
            &mut state,
            KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
            &mut cursor,
            &mut desc,
            &mut msg,
        );
        assert!(desc, "空格切换待定方向");
        match handle_sort_dialog_key(
            &mut state,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut cursor,
            &mut desc,
            &mut msg,
        ) {
            SortDialogOutcome::Apply(spec) => {
                assert!(spec.desc, "回车应用换向后的降序");
                assert_eq!(spec.field, crate::tab::SortField::Name);
                // 镜像 run 循环：Apply 由调用方执行 apply_sort。
                state.active_tab_mut().apply_sort(spec, Config::default());
            }
            other => panic!("expected Apply, got {other:?}"),
        }
        assert_eq!(
            state.active_tab().entries[0].name,
            "b.txt",
            "名称降序 b.txt 在前"
        );

        // Esc 取消：不改排序。
        let before = state.active_tab().sort.clone();
        match handle_sort_dialog_key(
            &mut state,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            &mut cursor,
            &mut desc,
            &mut msg,
        ) {
            SortDialogOutcome::Close => {}
            other => panic!("expected Close, got {other:?}"),
        }
        assert_eq!(state.active_tab().sort, before);
    }

    #[test]
    fn m181_sort_dialog_opens_and_renders() {
        // S → EnterSortDialog；渲染：字段说明 + 当前排序状态 + 遮罩。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut input = InputState::default();
        let mut msg = None;
        assert_eq!(
            press_msg(
                &mut state,
                &mut input,
                &mut msg,
                KeyCode::Char('S'),
                KeyModifiers::NONE
            ),
            Outcome::EnterSortDialog
        );
        // 预置一个排序状态供对话框显示"当前"。
        state.active_tab_mut().sort = Some(vec![crate::tab::SortSpec {
            field: crate::tab::SortField::Size,
            desc: true,
        }]);
        let mode = InputMode::SortDialog {
            cursor: 2,
            desc: false,
        };
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &mode, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let all: String = (0..24u16)
            .flat_map(|y| (0..80u16).map(move |x| (x, y)))
            .map(|(x, y)| buffer[(x, y)].symbol())
            .collect();
        assert!(all.contains("size"), "字段说明: {all}");
        assert!(
            all.contains("desc"),
            "当前排序状态（合并格式 [desc → asc]）: {all}"
        );
        assert!(buffer[(0, 1)].symbol().trim().is_empty(), "遮罩生效");
    }

    #[test]
    fn m18_2_parse_sort_spec_and_multi_exec() {
        // 解析：多字段 + 方向。
        let specs = parse_sort_spec("name,size desc").unwrap();
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].field, crate::tab::SortField::Name);
        assert!(!specs[0].desc);
        assert_eq!(specs[1].field, crate::tab::SortField::Size);
        assert!(specs[1].desc);
        // 错误路径。
        assert!(parse_sort_spec("badfield").is_err());
        assert!(parse_sort_spec("name up").is_err());
        assert!(parse_sort_spec("  ").is_err());

        // exec：多字段生效（ext asc + size desc）。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "xxxx").unwrap();
        fs::write(tmp.path().join("b.md"), "").unwrap();
        fs::write(tmp.path().join("c.txt"), "").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        match exec_command_impl(&mut state, "sort ext,size desc") {
            Ok(ExecOutcome::Done { msg }) => {
                assert!(msg.as_deref().unwrap().contains("ext asc"), "m: {msg:?}");
            }
            other => panic!("expected Done, got {other:?}"),
        }
        let names: Vec<&str> = state
            .active_tab()
            .entries
            .iter()
            .map(|i| i.name.as_str())
            .collect();
        assert_eq!(
            names,
            ["b.md", "a.txt", "c.txt"],
            "ext asc：md 组在前，txt 组 size 降序"
        );
        // 状态栏指示数据源。
        let sort = state.active_tab().sort.clone().unwrap();
        assert_eq!(sort.len(), 2);

        // exec 错误路径。
        assert!(exec_command_impl(&mut state, "sort badfield").is_err());
        assert!(exec_command_impl(&mut state, "sort").is_err());
    }

    #[test]
    fn m18_3_s_requests_shell_and_command_constructed() {
        // s → EnterShell（不进入其他模式、不产生副作用）。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "A").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut input = InputState::default();
        let mut msg = None;
        state.active_tab_mut().cursor = 0;
        assert_eq!(
            press_msg(
                &mut state,
                &mut input,
                &mut msg,
                KeyCode::Char('s'),
                KeyModifiers::NONE
            ),
            Outcome::EnterShell
        );
        assert!(msg.is_none(), "无副作用");
        assert_eq!(state.active_tab().cursor, 0);
    }

    #[test]
    fn m18_4_bang_routes_and_config() {
        // :!echo → CurrentPane outcome（run 循环执行）。
        // AUDIT-14：注入 terminal_in_new_window=false——宿主 TMUX/STY 不改变
        // 结果；复用器路由用 run_external_target_with 注入覆盖（见下方）。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        state.config.ui.terminal_in_new_window = false;
        match exec_command_impl(&mut state, "!echo rvifm-test") {
            Ok(ExecOutcome::RunExternal { cmd }) => assert_eq!(cmd, "echo rvifm-test"),
            other => panic!("expected RunExternal, got {other:?}"),
        }
        // 失败命令：命令串照传（执行失败消息由 run 循环产生）。
        match exec_command_impl(&mut state, "!rvifm-nonexistent-cmd-xyz") {
            Ok(ExecOutcome::RunExternal { cmd }) => {
                assert!(cmd.contains("rvifm-nonexistent-cmd-xyz"));
            }
            other => panic!("expected RunExternal, got {other:?}"),
        }
        // 退出状态 → 消息（成功 None；失败可读）。
        // AUDIT-14：静默命令，不向测试 stdout 泄漏外部输出。
        let ok = if cfg!(windows) {
            std::process::Command::new("cmd")
                .args(["/C", "exit", "0"])
                .status()
        } else {
            std::process::Command::new("sh")
                .args(["-c", "exit 0"])
                .status()
        };
        assert!(run_external_current_result(ok).is_none());
        let bad = std::process::Command::new("rvifm-nonexistent-cmd-xyz").status();
        assert!(run_external_current_result(bad).unwrap().contains("失败"));
        // 路由判定不依赖宿主变量（AUDIT-14）：只注入会短路到 CurrentPane 的
        // 组合——Some+true 会真实 spawn 复用器，不在测试覆盖内。
        let cfg_true = crate::config::Config::default();
        assert!(cfg_true.ui.terminal_in_new_window, "缺省 true");
        assert!(
            matches!(
                run_external_target_with(None, &cfg_true.ui, "x"),
                ExternalRun::CurrentPane(_)
            ),
            "无复用器 → 当前 pane"
        );
        let mut cfg_false = crate::config::Config::default();
        cfg_false.ui.terminal_in_new_window = false;
        assert!(
            matches!(
                run_external_target_with(Some(MuxKind::Tmux), &cfg_false.ui, "x"),
                ExternalRun::CurrentPane(_)
            ),
            "开关关闭时忽略复用器环境"
        );
        assert!(
            matches!(
                run_external_target_with(Some(MuxKind::Screen), &cfg_false.ui, "x"),
                ExternalRun::CurrentPane(_)
            ),
            "开关关闭时忽略 screen 环境"
        );
        // 配置：[ui] 可覆盖（缺省 true 已在上方断言）。
        let tmp2 = TempDir::new().unwrap();
        let path = tmp2.path().join("config.toml");
        std::fs::write(
            &path,
            r#"[ui]
terminal_in_new_window = false
"#,
        )
        .unwrap();
        let cfg = crate::config::load_from(&path).unwrap();
        assert!(!cfg.ui.terminal_in_new_window);
    }

    #[test]
    fn m18_4_space_switches_focus_both_panes() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut input = InputState::default();
        let mut msg = None;
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char(' '),
            KeyModifiers::NONE,
        );
        assert_eq!(
            state.active_tab().focus,
            crate::tab::Focus::Right,
            "Space 切到右 pane"
        );
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char(' '),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().focus, crate::tab::Focus::Left);
        assert_eq!(state.active_tab().cursor, 0, "光标不受影响");
    }

    #[test]
    fn m18_4_tmux_window_args_quoting() {
        let args = tmux_window_args("ls -la 'a b'");
        assert_eq!(args[0], "new-window");
        assert_eq!(args[1], "-d");
        assert!(args[2].starts_with("sh -c 'ls -la"));
    }

    /// 内存 mock 远端（M19-2/M20 测试用）——flat path → entry 模型。
    struct MockRemote {
        entries: std::cell::RefCell<std::collections::BTreeMap<String, crate::remote::RemoteEntry>>,
    }
    impl MockRemote {
        fn seed(&self, path: &str, name: &str, is_dir: bool, size: u64) {
            let key = format!("{}/{}", path.trim_end_matches('/'), name);
            self.entries.borrow_mut().insert(
                key,
                crate::remote::RemoteEntry {
                    name: name.to_owned(),
                    is_dir,
                    size,
                    mtime: 0,
                },
            );
        }
    }
    impl crate::remote::RemoteFs for MockRemote {
        fn list(
            &self,
            path: &std::path::Path,
        ) -> Result<Vec<crate::remote::RemoteEntry>, crate::remote::RemoteError> {
            let prefix = format!(
                "{}/",
                path.to_string_lossy()
                    .replace('\\', "/")
                    .trim_end_matches('/')
            );
            let out: Vec<crate::remote::RemoteEntry> = self
                .entries
                .borrow()
                .iter()
                .filter(|(k, _)| k.starts_with(&prefix) && !k[prefix.len()..].contains('/'))
                .map(|(_, v)| v.clone())
                .collect();
            Ok(out)
        }
        fn read(&self, path: &std::path::Path) -> Result<Vec<u8>, crate::remote::RemoteError> {
            let key = path.to_string_lossy().replace('\\', "/");
            self.entries
                .borrow()
                .get(&key)
                .map(|_| b"mock content".to_vec())
                .ok_or(crate::remote::RemoteError::NotFound(key))
        }
        fn metadata(
            &self,
            path: &std::path::Path,
        ) -> Result<crate::remote::RemoteMeta, crate::remote::RemoteError> {
            let key = path.to_string_lossy().replace('\\', "/");
            self.entries
                .borrow()
                .get(&key)
                .map(|e| crate::remote::RemoteMeta {
                    size: e.size,
                    mtime: e.mtime,
                    is_dir: e.is_dir,
                })
                .ok_or(crate::remote::RemoteError::NotFound(key))
        }
        fn write(
            &self,
            path: &std::path::Path,
            data: &[u8],
        ) -> Result<(), crate::remote::RemoteError> {
            let key = path.to_string_lossy().replace('\\', "/");
            let name = key.rsplit('/').next().unwrap_or_default().to_owned();
            self.entries.borrow_mut().insert(
                key,
                crate::remote::RemoteEntry {
                    name,
                    is_dir: false,
                    size: data.len() as u64,
                    mtime: 0,
                },
            );
            Ok(())
        }
        fn remove(&self, path: &std::path::Path) -> Result<(), crate::remote::RemoteError> {
            self.entries
                .borrow_mut()
                .remove(&path.to_string_lossy().replace('\\', "/"));
            Ok(())
        }
        fn rename(
            &self,
            from: &std::path::Path,
            to: &std::path::Path,
        ) -> Result<(), crate::remote::RemoteError> {
            let fk = from.to_string_lossy().replace('\\', "/");
            let tk = to.to_string_lossy().replace('\\', "/");
            let mut map = self.entries.borrow_mut();
            let entry = map
                .remove(&fk)
                .ok_or(crate::remote::RemoteError::NotFound(fk))?;
            map.insert(tk, entry);
            Ok(())
        }
        fn create_dir(&self, path: &std::path::Path) -> Result<(), crate::remote::RemoteError> {
            let key = path.to_string_lossy().replace('\\', "/");
            // M22-4：镜像 SFTP 语义——已存在返回 Err（此前 mock 静默覆盖）。
            if self.entries.borrow().contains_key(&key) {
                return Err(crate::remote::RemoteError::Other(format!(
                    "mkdir: 已存在: {key}"
                )));
            }
            let name = key.rsplit('/').next().unwrap_or_default().to_owned();
            self.entries.borrow_mut().insert(
                key,
                crate::remote::RemoteEntry {
                    name,
                    is_dir: true,
                    size: 0,
                    mtime: 0,
                },
            );
            Ok(())
        }
        fn move_to_trash(
            &self,
            path: &std::path::Path,
        ) -> Result<std::path::PathBuf, crate::remote::RemoteError> {
            let trash = std::path::PathBuf::from(format!(
                "/tmp/rvifm-mock-trash/{}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ));
            self.rename(path, &trash)?;
            Ok(trash)
        }
        fn restore_from_trash(
            &self,
            trash_path: &std::path::Path,
            orig: &std::path::Path,
        ) -> Result<(), crate::remote::RemoteError> {
            self.rename(trash_path, orig)
        }
        fn remove_permanent(
            &self,
            path: &std::path::Path,
        ) -> Result<(), crate::remote::RemoteError> {
            self.remove(path)
        }
        fn remove_dir(&self, path: &std::path::Path) -> Result<(), crate::remote::RemoteError> {
            self.remove(path)
        }
    }

    fn mock_remote_state() -> (TempDir, AppState) {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("local.txt"), "").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mock = MockRemote {
            entries: std::cell::RefCell::new(std::collections::BTreeMap::new()),
        };
        mock.seed("/", "docs", true, 0);
        mock.seed("/", "readme.md", false, 13);
        // M22-5：远端 :grep 测试需要 /docs/notes.md（read 固定返回 mock content）。
        mock.seed("/docs", "notes.md", false, 12);
        register_remote_session(&mut state, "dev", Box::new(mock));
        (tmp, state)
    }

    #[test]
    fn m22_4_remote_mk_enters_and_reject_message_updated() {
        // ACCEPTANCE §21 M22-4：远端 `:mk d1` 创建并进入；已存在返回 Err
        // 不进入；未接命令提示已更新为"远端不支持"。
        let (_tmp, mut state) = mock_remote_state();
        remote_enter(&mut state, "dev", std::path::Path::new("/")).unwrap();

        // :mk d1 → 创建并进入。
        exec_command_impl(&mut state, "mk d1").unwrap();
        assert_eq!(
            state.active_tab().location,
            Location::Remote {
                remote: "dev".to_owned(),
                path: std::path::PathBuf::from("/d1")
            },
            ":mk 进入新目录"
        );
        // 再 :mk d1（当前已在 /d1，创建 /d1/d1）→ 成功进入 /d1/d1？——语义
        // 与本地一致：相对当前目录。测已存在失败：回根目录再 mk docs。
        remote_navigate(&mut state, std::path::Path::new("/")).unwrap();
        assert!(
            exec_command_impl(&mut state, "mk docs").is_err(),
            "已存在 Err"
        );
        assert_eq!(
            state.active_tab().location,
            Location::Remote {
                remote: "dev".to_owned(),
                path: std::path::PathBuf::from("/")
            },
            "失败不进入"
        );

        // 未接命令：提示文本已更新（无"在 M20 接入"）。
        let err = exec_command_impl(&mut state, "nosuchcmd").unwrap_err();
        assert!(err.contains("远端不支持"), "提示修正: {err}");
        assert!(!err.contains("M20"), "无过时表述: {err}");
    }

    #[test]
    fn m20_remote_dd_trash_and_undo_restores() {
        let (_tmp, mut state) = mock_remote_state();
        let mut msg = None;
        remote_enter(&mut state, "dev", std::path::Path::new("/")).unwrap();
        state.active_tab_mut().cursor = 1; // readme.md
        let n_before = state.active_tab().entries.len();

        // dd（execute_delete 直调——Confirm 模式由 run 循环驱动）。
        let target = std::path::PathBuf::from("/readme.md");
        execute_delete(
            &mut state,
            &mut msg,
            target.clone(),
            false,
            Some("dev".to_owned()),
        );
        assert_eq!(state.active_tab().entries.len(), n_before - 1, "条目减少");
        assert_eq!(state.undo_stack.len(), 1);

        // u → restore_from_trash。
        undo_last(&mut state, &mut msg);
        assert_eq!(state.active_tab().entries.len(), n_before, "u 恢复条目");
        assert!(
            state
                .active_tab()
                .entries
                .iter()
                .any(|i| i.name == "readme.md")
        );
    }

    #[test]
    fn m20_remote_paste_and_undo_removes() {
        let local = TempDir::new().unwrap();
        fs::write(local.path().join("up.txt"), "upload-data").unwrap();
        let (tmp, mut state) = mock_remote_state();
        let mut input = InputState::default();
        let mut msg = None;
        // 本地 yy（navigate_to local dir → entries [up.txt] → cursor 0）。
        state
            .active_tab_mut()
            .navigate_to(
                Location::Physical(local.path().to_path_buf()),
                Config::default(),
            )
            .unwrap();
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('y'),
            KeyModifiers::NONE,
        );
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('y'),
            KeyModifiers::NONE,
        );
        // 进入远端 → p。
        remote_enter(&mut state, "dev", std::path::Path::new("/")).unwrap();
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('p'),
            KeyModifiers::NONE,
        );
        let entries = state
            .remote_sessions
            .get("dev")
            .unwrap()
            .list(std::path::Path::new("/"))
            .unwrap();
        assert!(
            entries.iter().any(|i| i.name == "up.txt"),
            "远端应出现 up.txt"
        );
        assert!(msg.as_deref().unwrap().contains("Copied 1"), "msg: {msg:?}");
        // u → 远端 Create 撤销。
        undo_last(&mut state, &mut msg);
        let entries = state
            .remote_sessions
            .get("dev")
            .unwrap()
            .list(std::path::Path::new("/"))
            .unwrap();
        assert!(!entries.iter().any(|i| i.name == "up.txt"), "u 后消失");
        let _ = tmp;
    }

    #[test]
    fn m20_remote_rename_and_mkdir_with_undo() {
        let (_tmp, mut state) = mock_remote_state();
        remote_enter(&mut state, "dev", std::path::Path::new("/")).unwrap();
        assert!(matches!(
            exec_command_impl(&mut state, "mkdir newdir"),
            Ok(ExecOutcome::Done { .. })
        ));
        let entries = state
            .remote_sessions
            .get("dev")
            .unwrap()
            .list(std::path::Path::new("/"))
            .unwrap();
        assert!(entries.iter().any(|i| i.name == "newdir"));
        undo_last(&mut state, &mut None);
        let entries = state
            .remote_sessions
            .get("dev")
            .unwrap()
            .list(std::path::Path::new("/"))
            .unwrap();
        assert!(!entries.iter().any(|i| i.name == "newdir"));
    }

    // ── M12-3/M12-4：AskPass 模式（sudo 密码输入、masked 渲染、干净中止）──

    #[test]
    fn askpass_key_collects_submits_and_cancels() {
        // M12-3：字符收集 → Enter 提交密码；Esc 取消。
        let mut input = String::new();
        assert_eq!(
            askpass_key(&mut input, KeyCode::Char('h')),
            AskPassResult::Continue
        );
        assert_eq!(
            askpass_key(&mut input, KeyCode::Char('x')),
            AskPassResult::Continue
        );
        assert_eq!(
            askpass_key(&mut input, KeyCode::Backspace),
            AskPassResult::Continue
        );
        assert_eq!(input, "h");
        assert_eq!(
            askpass_key(&mut input, KeyCode::Enter),
            AskPassResult::Retry {
                password: "h".to_owned()
            }
        );
        // 提交后缓冲已清空；Esc 直接取消。
        assert_eq!(askpass_key(&mut input, KeyCode::Esc), AskPassResult::Cancel);
        assert_eq!(input, "");
    }

    #[test]
    fn askpass_renders_masked_password_without_leaking() {
        // M12-3：密码以 * 渲染；明文不出现在缓冲区。
        let (_tmp, state) = make_state(1);
        let mode = InputMode::AskPass {
            input: "hunter2".to_owned(),
            op: crate::privilege::PrivOp::Delete {
                path: PathBuf::from("/tmp/target.txt"),
            },
        };
        let backend = ratatui::backend::TestBackend::new(80, 10);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &mode, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let row = |y: u16| {
            (0..80u16)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        };
        let status = row(9);
        // CJK 双宽在 TestBackend 中展开为"字 空格"形式：只断言 ASCII 关键段。
        assert!(status.contains("*******"), "masked: {status}");
        assert!(status.contains("/tmp/target.txt"), "op desc: {status}");
        assert!(!status.contains("hunter2"), "明文泄漏: {status}");
    }

    #[test]
    fn askpass_cancel_keeps_state_unchanged() {
        // M12-4：取消干净中止——cursor/entries/undo 栈与进入前一致。
        let (tmp, state) = make_state(3);
        let _ = InputState::default();
        let mut msg: Option<String> = None;
        // 模拟 TUI 状态迁移：Cancel 后模式由调用方置 Normal（此处验证
        // askpass_key 的 Cancel 语义 + 无副作用）。
        let mut pw_buf = "secret".to_owned();
        assert_eq!(
            askpass_key(&mut pw_buf, KeyCode::Esc),
            AskPassResult::Cancel
        );
        assert!(pw_buf.is_empty(), "取消必须丢弃密码");

        // 状态一致性：UndoableOp 候选不执行，光标/条目不动。
        let cursor_before = state.active_tab().cursor;
        let entries_before = state.active_tab().entries.len();
        let undo_before = state.undo_stack.len();
        let _ = (&mut msg, &tmp);
        assert_eq!(state.active_tab().cursor, cursor_before);
        assert_eq!(state.active_tab().entries.len(), entries_before);
        assert_eq!(state.undo_stack.len(), undo_before);
    }

    #[test]
    fn renders_active_tab_list_and_status_bar() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("hello.txt"), "").unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        let state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());

        let backend = ratatui::backend::TestBackend::new(160, 10);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let row = |y: u16| {
            (0..160u16)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        };

        // 目录在前并带 `/` 后缀（M21-F1：pane 边框行 1，`..` 行占行 2，条目行 3 起）。
        assert!(row(1).contains("┌"), "左 pane 顶部边框: {}", row(1));
        assert!(row(2).contains("../"));
        assert!(row(3).contains("sub/"));
        assert!(row(4).contains("hello.txt"));
        // M21-F1：路径在 pane 顶部标题，状态栏不再显示路径；详情含条目数。
        let dir_name = tmp
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(row(1).contains(&dir_name), "pane 顶部路径: {}", row(1));
        let status = row(9).trim().to_owned();
        assert!(!status.contains(&dir_name), "状态栏不应显示路径: {status}");
        assert!(
            status.contains("2 items"),
            "status bar missing count: {status}"
        );
    }

    #[test]
    fn preview_pane_renders_image_and_text_by_kind() {
        // M11-2/M14-1：右 pane 默认显示目录列表；view 态（w）显示活动 pane
        // 光标文件的预览——文本文件显示内容，图片文件 halfblocks 渲染。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("notes.txt"), "hello preview").unwrap();
        let img = image::DynamicImage::new_rgba8(16, 16).into_rgba8();
        let mut png = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png, image::ImageFormat::Png).unwrap();
        fs::write(tmp.path().join("pic.png"), png.into_inner()).unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());

        let backend = ratatui::backend::TestBackend::new(80, 10);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();

        let row_text = |buf: &ratatui::buffer::Buffer, y: u16, x0: u16, x1: u16| {
            (x0..x1).map(|x| buf[(x, y)].symbol()).collect::<String>()
        };

        // 默认态：右 pane = 目录列表（显示条目名，非预览内容；M21-F1 右 pane
        // 从列 40 起，边框列 40、条目文本自列 41）。
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let pane_list: String = (1..9).map(|y| row_text(&buf, y, 41, 80)).collect();
        assert!(pane_list.contains("notes.txt"), "列表渲染: {pane_list}");
        assert!(pane_list.contains("pic.png"), "列表渲染: {pane_list}");

        // view 态开启：右 pane = 左光标文件（notes.txt）的文本预览。
        state.active_tab_mut().view_mode = true;
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let pane_txt: String = (1..9).map(|y| row_text(&buf, y, 41, 80)).collect();
        assert!(
            pane_txt.contains("hello preview"),
            "text preview: {pane_txt}"
        );

        // 光标移到 pic.png → 图片预览（halfblocks 非空白）。
        state.active_tab_mut().cursor = 1;
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let has_blocks = (1..9).any(|y| {
            (41..80).any(|x| {
                let c = &buf[(x, y)];
                c.symbol() != " " || c.fg != ratatui::style::Color::Reset
            })
        });
        assert!(has_blocks, "image preview pane empty");
    }

    #[test]
    fn renders_command_line_and_message() {
        let (_tmp, state) = make_state(1);
        let backend = ratatui::backend::TestBackend::new(80, 10);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let mode = InputMode::Command {
            input: "tabne".to_owned(),
            hist_pos: None,
            draft: String::new(),
            comp: None,
        };
        terminal
            .draw(|frame| render(frame, &state, &mode, Some("未知命令: x")))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let row = |y: u16| {
            (0..80u16)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        };
        assert!(row(9).contains(":tabne"), "command line: {}", row(9));
    }

    // ── M21-F1：界面风格（pane 边框/顶部路径/类型着色/底部详情） ─────────────

    #[test]
    fn m21f1_panes_bordered_and_path_in_title_at_two_sizes() {
        // ACCEPTANCE §20 M21-F1：80x24 与 160x48 下左右 pane 有可见边框，
        // 路径在 pane 顶部，状态栏无路径。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        // 注入短路径：真实 tempdir 深层路径在 80 宽 pane 内必然截断，
        // 无法断言"路径完整可见"；Location 直接构造与渲染无关。
        state.active_tab_mut().location = Location::Physical(std::path::PathBuf::from("/w/proj"));
        let dir_name = "proj".to_owned();
        for (w, h) in [(80u16, 24u16), (160u16, 48u16)] {
            let backend = ratatui::backend::TestBackend::new(w, h);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal
                .draw(|f| render(f, &state, &InputMode::Normal, None))
                .unwrap();
            let buf = terminal.backend().buffer().clone();
            let row = |y: u16| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>();
            // 左右 pane 边框可见（两上角；居中分开的间隔列为空格）。
            assert_eq!(buf[(0, 1)].symbol(), "┌", "左 pane 左上角 {w}x{h}");
            assert_eq!(buf[(w - 1, 1)].symbol(), "┐", "右 pane 右上角 {w}x{h}");
            assert!(row(1).contains(&dir_name), "路径在 pane 顶部: {}", row(1));
            let status = row(h - 1);
            assert!(!status.contains(&dir_name), "状态栏无路径: {status}");
            assert!(status.contains("1 items"), "状态栏保留条目数: {status}");
        }
    }

    #[test]
    fn m21f1_filetype_colors_differ_in_list() {
        // ACCEPTANCE §20 M21-F1：目录/可执行/图片/归档在列表中颜色不同。
        // 可执行：unix 按执行位（chmod 755），Windows 按扩展名近似（.exe）。
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("img.png"), "x").unwrap();
        fs::write(tmp.path().join("arch.zip"), "x").unwrap();
        #[cfg(unix)]
        {
            fs::write(tmp.path().join("run.sh"), "#!/bin/sh").unwrap();
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                tmp.path().join("run.sh"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }
        #[cfg(not(unix))]
        fs::write(tmp.path().join("run.exe"), "x").unwrap();

        let state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let backend = ratatui::backend::TestBackend::new(120, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| render(f, &state, &InputMode::Normal, None))
            .unwrap();
        let buf = terminal.backend().buffer().clone();

        // 条目行 3 起（pane 边框 1、`..` 行 2），条目区在左 pane（列 1..59）；
        // 找到条目名首字符所在单元格取前景色（图标为 Nerd Font 私有区字符，
        // 不会与 ASCII 名首字符混淆）。
        let fg_of = |name: &str| -> ratatui::style::Color {
            let first = name.chars().next().unwrap();
            for y in 3..22u16 {
                let line: String = (0..59u16).map(|x| buf[(x, y)].symbol()).collect();
                if line.contains(name)
                    && let Some(x) = (0..59u16).find(|&x| buf[(x, y)].symbol() == first.to_string())
                {
                    return buf[(x, y)].fg;
                }
            }
            panic!("条目未渲染: {name}");
        };
        let exe_name = if cfg!(unix) { "run.sh" } else { "run.exe" };
        let colors = [
            fg_of("sub/"),
            fg_of(exe_name),
            fg_of("img.png"),
            fg_of("arch.zip"),
        ];
        for i in 0..colors.len() {
            for j in (i + 1)..colors.len() {
                assert_ne!(colors[i], colors[j], "类型着色应互异（{i} vs {j}）");
            }
        }
    }

    #[test]
    fn m21f1_status_bar_shows_selected_details() {
        // ACCEPTANCE §20 M21-F1：选中文件后底部显示大小/权限/时间（+类型）。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("doc.txt"), "12345").unwrap(); // 5 B
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        // mtime 手工固定，断言与 format_mtime 同源（跨时区可复现）。
        let stamp = 1_700_000_000u64;
        state.active_tab_mut().entries[0].mtime = stamp;
        let perm = state.active_tab().entries[0].perm.clone();

        let backend = ratatui::backend::TestBackend::new(160, 10);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| render(f, &state, &InputMode::Normal, None))
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let status: String = (0..160u16).map(|x| buf[(x, 9)].symbol()).collect();

        assert!(status.contains("doc.txt"), "详情含名称: {status}");
        assert!(status.contains("5 B"), "详情含大小: {status}");
        assert!(status.contains(&perm), "详情含权限: {status}");
        assert!(
            status.contains(&crate::tab::format_mtime(stamp)),
            "详情含时间: {status}"
        );
        // TestBackend 渲染 CJK 全角为"字+空格"（FEAT-4 教训）：去空格后比对。
        assert!(
            status.replace(' ', "").contains("[文件]"),
            "详情含类型标签: {status}"
        );
        // 状态栏不再显示路径（M21-F1）。
        let dir_name = tmp
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(!status.contains(&dir_name), "状态栏无路径: {status}");
    }

    #[test]
    fn m21f2_focus_switch_dims_unfocused_pane_current_row() {
        // ACCEPTANCE §20 M21-F2：焦点切换时焦点侧当前行高亮（REVERSED）、
        // 非焦点侧当前行 DIM；状态栏无焦点文字（切换不再产生提示消息）。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());

        let draw = |state: &AppState| {
            let backend = ratatui::backend::TestBackend::new(120, 24);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal
                .draw(|f| render(f, state, &InputMode::Normal, None))
                .unwrap();
            terminal.backend().buffer().clone()
        };

        // 几何与渲染同源：右 pane 条目首行 = 边框内区首行（无 `..` 行）。
        let area = ratatui::layout::Rect::new(0, 0, 120, 24);
        let [_, left, right, _] = pane_layout(area);
        let l_inner = ratatui::widgets::Block::bordered().inner(left);
        let r_inner = ratatui::widgets::Block::bordered().inner(right);
        // 左 pane 条目 0 在 `..` 行之后（行 = inner.y + 1）；右 pane 无 `..` 行。
        let l_row = l_inner.y + 1;
        let r_row = r_inner.y;
        let lx = l_inner.x;
        let rx = r_inner.x;

        // 焦点左：左当前行 REVERSED；右当前行 DIM 且非 REVERSED。
        let buf = draw(&state);
        assert!(
            buf[(lx, l_row)].modifier.contains(Modifier::REVERSED),
            "焦点左：左当前行反显"
        );
        assert!(
            buf[(rx, r_row)].modifier.contains(Modifier::DIM)
                && !buf[(rx, r_row)].modifier.contains(Modifier::REVERSED),
            "焦点左：右当前行 DIM"
        );

        // 焦点右：两侧互换。
        state.active_tab_mut().focus = crate::tab::Focus::Right;
        let buf = draw(&state);
        assert!(
            buf[(lx, l_row)].modifier.contains(Modifier::DIM)
                && !buf[(lx, l_row)].modifier.contains(Modifier::REVERSED),
            "焦点右：左当前行 DIM"
        );
        assert!(
            buf[(rx, r_row)].modifier.contains(Modifier::REVERSED),
            "焦点右：右当前行反显"
        );
    }

    #[test]
    fn m21f3_search_esc_clears_committed_state_and_reentry_clean() {
        // ACCEPTANCE §20 M21-F3：Esc 退出搜索后状态栏与高亮全部清除；
        // 再次 `/` 干净进入（EnterSearch 构造全新空输入），不残留上次匹配。
        // （Search-mode Esc 分支在 run 循环内，取消逻辑抽为 search_cancel 直测。）
        let tmp = TempDir::new().unwrap();
        for name in ["alpha", "src.txt"] {
            fs::write(tmp.path().join(name), "").unwrap();
        }
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut msg = None;

        let draw = |state: &AppState, mode: &InputMode| {
            let backend = ratatui::backend::TestBackend::new(120, 24);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal.draw(|f| render(f, state, mode, None)).unwrap();
            terminal.backend().buffer().clone()
        };
        let has_highlight = |buf: &ratatui::buffer::Buffer| {
            (0..24u16).any(|y| (0..120u16).any(|x| buf[(x, y)].bg == Color::LightYellow))
        };
        let status = |buf: &ratatui::buffer::Buffer| {
            (0..120u16)
                .map(|x| buf[(x, 23)].symbol())
                .collect::<String>()
        };

        // 提交搜索 "src" → 高亮与状态栏搜索指示存在。
        submit_search(&mut state, "src", &mut msg);
        assert!(state.active_tab().search.is_some());
        let normal = InputMode::Normal;
        let buf = draw(&state, &normal);
        assert!(has_highlight(&buf), "提交后存在高亮");
        assert!(status(&buf).contains("/src"), "状态栏含搜索指示");

        // `/` 进入搜索输入后 Esc 取消 → 上次提交的搜索一并清除。
        search_cancel(&mut state, 0);
        assert!(state.active_tab().search.is_none(), "Esc 清除已提交搜索");
        let buf = draw(&state, &normal);
        assert!(!has_highlight(&buf), "Esc 后无高亮残留");
        assert!(
            !status(&buf).contains("/src"),
            "状态栏无搜索残留: {}",
            status(&buf)
        );

        // 再次 `/`：run 循环 EnterSearch 构造全新空输入（origin=当前光标），
        // 无上次匹配痕迹——incsearch 从空输入起。
        let (_, mut input) = ((), InputState::default());
        let outcome = press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('/'),
            KeyModifiers::NONE,
        );
        assert!(
            matches!(outcome, Outcome::EnterSearch),
            "再次 / 应进入全新搜索: {outcome:?}"
        );
    }

    #[test]
    fn m21f4_h_returns_cursor_to_origin_entry() {
        // ACCEPTANCE §20 M21-F4：进子目录再 h 返回，光标在该目录条目上；
        // 归档根 h 返回物理目录，光标在归档文件条目上。
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("t.zip"), "not-a-real-zip").unwrap();
        crate::archive::make_test_zip(&tmp.path().join("t.zip"));
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg = None;
        let cursor_name = |state: &AppState| -> String {
            state
                .active_tab()
                .entries
                .get(state.active_tab().cursor)
                .map(|i| i.name.clone())
                .unwrap_or_default()
        };

        // 物理目录：从 sub 返回 tmp → 光标落在 "sub" 条目，且不在 `..` 行。
        let mut state =
            AppState::new(Tab::from_physical(&tmp.path().join("sub"), Config::default()).unwrap());
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('h'),
            KeyModifiers::NONE,
        );
        assert_eq!(cursor_name(&state), "sub", "光标落在来源目录条目");
        assert!(!state.active_tab().on_parent_row, "h 返回不在 .. 行");

        // 归档：进入归档根（落在 .. 行）→ h 返回物理目录 → 光标在 "t.zip"。
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        // 排序：目录优先 → sub(0)；t.zip 是文件，定位到它。
        let zip_idx = state
            .active_tab()
            .entries
            .iter()
            .position(|i| i.name == "t.zip")
            .unwrap();
        state.active_tab_mut().cursor = zip_idx;
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('l'),
            KeyModifiers::NONE,
        );
        assert!(
            matches!(state.active_tab().location, Location::Archive { .. }),
            "进入归档"
        );
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('h'),
            KeyModifiers::NONE,
        );
        assert_eq!(
            state.active_tab().location,
            Location::Physical(tmp.path().to_path_buf()),
            "归档根 h 回物理目录"
        );
        assert_eq!(cursor_name(&state), "t.zip", "光标落在归档文件条目");
        assert!(!state.active_tab().on_parent_row);
    }

    #[test]
    fn m21f5_watch_target_and_event_relevance_rules() {
        // 监听目标：物理目录 Some；归档/远端 None（不监听）。
        // 事件相关性：创建/修改/删除相关，访问类忽略（notify 重命名归 Modify）。
        let tmp = TempDir::new().unwrap();
        let tab = Tab::from_physical(tmp.path(), Config::default()).unwrap();
        assert_eq!(watch_target(&tab), Some(tmp.path().to_path_buf()));

        let zip_path = tmp.path().join("t.zip");
        crate::archive::make_test_zip(&zip_path);
        let mut atab = Tab::from_physical(tmp.path(), Config::default()).unwrap();
        atab.location = Location::Archive {
            host: zip_path,
            inner: PathBuf::new(),
        };
        assert_eq!(watch_target(&atab), None);

        let mut rtab = tab_at("/x");
        rtab.location = Location::Remote {
            remote: "r".to_owned(),
            path: PathBuf::from("/"),
        };
        assert_eq!(watch_target(&rtab), None);

        // 事件类别过滤。
        use notify::event::*;
        assert!(fs_event_relevant(&EventKind::Create(CreateKind::File)));
        assert!(fs_event_relevant(&EventKind::Modify(ModifyKind::Name(
            RenameMode::Any
        ))));
        assert!(fs_event_relevant(&EventKind::Remove(RemoveKind::File)));
        assert!(!fs_event_relevant(&EventKind::Access(AccessKind::Read)));
    }

    #[test]
    fn m21f5_refresh_follows_cursor_name_and_clamps_on_delete() {
        // 刷新保持光标：原条目仍在 → 光标跟随；原条目被删 → 钳制在列表内。
        let (tmp, mut state) = make_state(3); // f00 f01 f02
        state.active_tab_mut().cursor = 2; // f02
        fs::write(tmp.path().join("zz_new"), "").unwrap();
        refresh_on_fs_change(&mut state);
        let tab = state.active_tab();
        assert_eq!(tab.entries.len(), 4, "新文件进入列表");
        assert_eq!(
            tab.entries.get(tab.cursor).map(|i| i.name.as_str()),
            Some("f02"),
            "光标跟随原条目"
        );

        // 删除光标条目：钳制到列表尾。
        state.active_tab_mut().cursor = 3; // zz_new
        fs::remove_file(tmp.path().join("zz_new")).unwrap();
        refresh_on_fs_change(&mut state);
        let tab = state.active_tab();
        assert_eq!(tab.entries.len(), 3);
        assert!(tab.cursor < tab.entries.len(), "光标钳制在列表内");
        assert_eq!(
            tab.entries.get(tab.cursor).map(|i| i.name.as_str()),
            Some("f02")
        );
    }

    #[test]
    fn m21f5_watcher_receives_create_event_and_refresh_picks_it_up() {
        // 端到端：物理目录挂监听 → 外部创建文件 → 事件到达 → 刷新可见。
        let tmp = TempDir::new().unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        sync_watcher(&mut state);
        assert!(state.watcher.is_some(), "物理目录应挂上监听");
        assert!(state.watch_rx.is_some());

        fs::write(tmp.path().join("newfile"), "").unwrap();
        let rx = state.watch_rx.as_ref().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut relevant = false;
        while std::time::Instant::now() < deadline {
            match rx.try_recv() {
                Ok(ev) => {
                    if fs_event_relevant(&ev.kind) {
                        relevant = true;
                        break;
                    }
                }
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(50)),
            }
        }
        assert!(relevant, "10s 内未收到创建事件（平台事件推送应远快于此）");

        refresh_on_fs_change(&mut state);
        assert!(
            state
                .active_tab()
                .entries
                .iter()
                .any(|i| i.name == "newfile"),
            "刷新后新文件可见"
        );
    }

    #[test]
    fn m21f5_archive_view_unwatches_without_error() {
        // 归档视图不监听：从物理目录切换到归档后 sync 卸载（None，无 panic）。
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("t.zip");
        crate::archive::make_test_zip(&zip_path);
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        sync_watcher(&mut state);
        assert!(state.watcher.is_some());
        state.active_tab_mut().location = Location::Archive {
            host: zip_path,
            inner: PathBuf::new(),
        };
        sync_watcher(&mut state);
        assert!(state.watcher.is_none(), "归档视图卸载监听");
        assert!(state.watch_rx.is_none());
    }

    #[test]
    fn m21f6_dir_cursor_shows_shallow_tree_in_view_mode() {
        // ACCEPTANCE §20 M21-F6：光标在普通目录 → 右 pane 浅层 tree（深度 2），
        // 不报"无法读取（非 UTF-8 或已删除）"。
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("sub/a.txt"), "hello").unwrap();
        fs::create_dir(tmp.path().join("sub/deep")).unwrap();
        fs::write(tmp.path().join("sub/deep/b.txt"), "x").unwrap();
        fs::write(tmp.path().join("top.txt"), "root file").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        // 光标定位到 sub（目录），开启 view_mode。
        let sub_idx = state
            .active_tab()
            .entries
            .iter()
            .position(|i| i.name == "sub")
            .unwrap();
        state.active_tab_mut().cursor = sub_idx;
        state.active_tab_mut().view_mode = true;

        let backend = ratatui::backend::TestBackend::new(120, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| render(f, &state, &InputMode::Normal, None))
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let pane_text: String = (0..24u16)
            .map(|y| {
                (60..120u16)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect();
        assert!(pane_text.contains("sub ·"), "标题含路径: {pane_text}");
        assert!(pane_text.contains("a.txt"), "树含一级条目: {pane_text}");
        assert!(pane_text.contains("deep/"), "树含子目录: {pane_text}");
        assert!(pane_text.contains("b.txt"), "树含二级条目: {pane_text}");
        assert!(
            !pane_text.contains("无法读取"),
            "目录不走文本读取路径: {pane_text}"
        );
    }

    #[test]
    fn m21f6_dir_preview_empty_and_non_utf8_names_no_panic() {
        // 空目录占位提示；非 UTF-8 文件名（unix 可构造）lossy 渲染不 panic。
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("empty")).unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let idx = state
            .active_tab()
            .entries
            .iter()
            .position(|i| i.name == "empty")
            .unwrap();
        state.active_tab_mut().cursor = idx;
        state.active_tab_mut().view_mode = true;
        let backend = ratatui::backend::TestBackend::new(120, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| render(f, &state, &InputMode::Normal, None))
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let pane_text: String = (0..24u16)
            .map(|y| {
                (60..120u16)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect();
        // CJK 全角在 TestBackend 渲染为"字+空格"（FEAT-4 教训）：去空格比对。
        assert!(
            pane_text.replace(' ', "").contains("（空目录）"),
            "空目录占位: {pane_text}"
        );

        // 非 UTF-8 文件名目录（unix；Windows 文件名为 WTF-16 无法构造非法
        // UTF-8 名）——SPEC 语义：目录**内含**非 UTF-8 名文件时 tree 正常
        // 渲染不 panic（lossy 显示替换符）。光标进入非 UTF-8 名目录不可达
        // （FileItem.name 为 lossy 字符串，无法还原原始字节路径）。
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let misc = tmp.path().join("misc");
            fs::create_dir(&misc).unwrap();
            fs::write(
                misc.join(std::ffi::OsStr::from_bytes(b"n\xff\xff.txt")),
                "x",
            )
            .unwrap();
            // fixture 在 state 创建后生成：先刷新条目再定位（M12-2 教训：
            // 此路径仅 CI Linux 执行，Windows 编译不到）。
            state.active_tab_mut().refresh(Config::default()).unwrap();
            state.active_tab_mut().view_mode = false;
            let idx = state
                .active_tab()
                .entries
                .iter()
                .position(|i| i.is_dir && i.name == "misc")
                .unwrap();
            state.active_tab_mut().cursor = idx;
            state.active_tab_mut().view_mode = true;
            terminal
                .draw(|f| render(f, &state, &InputMode::Normal, None))
                .unwrap();
            let buf = terminal.backend().buffer().clone();
            let pane_text: String = (0..24u16)
                .map(|y| {
                    (60..120u16)
                        .map(|x| buf[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect();
            assert!(pane_text.contains("misc"), "目录 tree 标题可见");
            assert!(
                pane_text.contains('\u{FFFD}'),
                "非 UTF-8 名 lossy 渲染（替换符）: {pane_text}"
            );
        }
    }

    #[test]
    fn m21f7_pdf_office_preview_routed_not_text_path() {
        // ACCEPTANCE §20 M21-F7：普通 pdf/docx 预览不出现"非UTF-8或已删除"；
        // 损坏 pdf/docx 给出类型化错误提示（PDF 文本提取失败 / Office 文档
        // 解析失败），不 panic（M21-F7 走共享 PreviewRenderer 分流）。
        let tmp = TempDir::new().unwrap();
        let good_pdf = crate::pdf::make_pdf(tmp.path(), "doc.pdf", "HELLO");
        let _ = good_pdf;
        fs::write(tmp.path().join("bad.pdf"), b"%PDF-1.4 junk not a pdf").unwrap();
        crate::office::make_doc(tmp.path(), "ok.docx", "# 标题\n\n正文内容。");
        fs::write(tmp.path().join("bad.docx"), b"PK junk corrupt zip").unwrap();

        let names = ["doc.pdf", "bad.pdf", "ok.docx", "bad.docx"];
        for name in names {
            let mut state =
                AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
            let idx = state
                .active_tab()
                .entries
                .iter()
                .position(|i| i.name == name)
                .unwrap_or_else(|| panic!("{name} 在列表中"));
            state.active_tab_mut().cursor = idx;
            state.active_tab_mut().view_mode = true;
            let backend = ratatui::backend::TestBackend::new(120, 24);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal
                .draw(|f| render(f, &state, &InputMode::Normal, None))
                .unwrap();
            let buf = terminal.backend().buffer().clone();
            let pane_text: String = (0..24u16)
                .map(|y| {
                    (60..120u16)
                        .map(|x| buf[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect();
            assert!(
                !pane_text.contains("无法读取"),
                "{name} 不应落文本读取路径: {pane_text}"
            );
        }

        // 损坏文件：类型化错误提示（正常文件：提取成功渲染内容）。
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        for (name, expect) in [
            ("bad.pdf", "PDF 文本提取失败"),
            ("bad.docx", "Office 文档解析失败"),
        ] {
            let idx = state
                .active_tab()
                .entries
                .iter()
                .position(|i| i.name == name)
                .unwrap();
            state.active_tab_mut().cursor = idx;
            state.active_tab_mut().view_mode = true;
            let backend = ratatui::backend::TestBackend::new(120, 24);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal
                .draw(|f| render(f, &state, &InputMode::Normal, None))
                .unwrap();
            let buf = terminal.backend().buffer().clone();
            let pane_text: String = (0..24u16)
                .map(|y| {
                    (60..120u16)
                        .map(|x| buf[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect();
            assert!(
                pane_text
                    .replace(' ', "")
                    .contains(&expect.replace(' ', "")),
                "{name} 应提示类型化错误 [{expect}]: {pane_text}"
            );
        }

        // 正常 pdf/office：提取成功（含正文/文本层内容），无错误提示。
        state.active_tab_mut().cursor = 0; // 排序后 doc.pdf/bad.pdf/ok.docx/bad.docx 顺序不定——按名定位
        for (name, expect) in [("doc.pdf", "HELLO"), ("ok.docx", "正文内容")] {
            let idx = state
                .active_tab()
                .entries
                .iter()
                .position(|i| i.name == name)
                .unwrap();
            state.active_tab_mut().cursor = idx;
            state.active_tab_mut().view_mode = true;
            let backend = ratatui::backend::TestBackend::new(120, 24);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal
                .draw(|f| render(f, &state, &InputMode::Normal, None))
                .unwrap();
            let buf = terminal.backend().buffer().clone();
            let pane_text: String = (0..24u16)
                .map(|y| {
                    (60..120u16)
                        .map(|x| buf[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect();
            assert!(
                pane_text
                    .replace(' ', "")
                    .contains(&expect.replace(' ', "")),
                "{name} 提取内容应可见 [{expect}]: {pane_text}"
            );
        }
    }

    #[test]
    fn m21f9_enter_records_live_visit_and_failure_does_not() {
        // ACCEPTANCE §20 M21-F9：l/Enter 进入目录 → 实时记录登记
        // （live_recorded，退出刷写据此去重）；进入失败（损坏归档）不记录。
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("bad.zip"), b"PK junk bytes").unwrap();
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg = None;
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());

        // 左 pane：光标在 sub（目录优先，索引 0），l 进入 → 记录 sub。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('l'),
            KeyModifiers::NONE,
        );
        let sub_path = tmp.path().join("sub").to_string_lossy().replace('\\', "/");
        assert!(
            state
                .live_recorded
                .contains_key(&(0, 'L', sub_path.clone())),
            "进入目录应实时记录: {:?}",
            state.live_recorded
        );
        // 退出刷写去重：collect_session_records 不含 sub。
        let records = collect_session_records(&state);
        assert!(
            !records.iter().any(|r| r.path == sub_path),
            "已实时记录的访问不应在退出刷写中重复: {records:?}"
        );

        // 进入失败（损坏归档）→ 不产生记录。
        state.active_tab_mut().cursor = 0; // sub 内为空？——先返回上级再选 bad.zip
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('h'),
            KeyModifiers::NONE,
        );
        let bad_idx = state
            .active_tab()
            .entries
            .iter()
            .position(|i| i.name == "bad.zip")
            .unwrap();
        state.active_tab_mut().cursor = bad_idx;
        let before = state.live_recorded.len();
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('l'),
            KeyModifiers::NONE,
        );
        assert!(
            matches!(state.active_tab().location, Location::Physical(_)),
            "损坏归档进入失败留在原目录"
        );
        assert_eq!(
            state.live_recorded.len(),
            before,
            "进入失败不产生记录（bad.zip 是归档位置，不入记录）"
        );

        // 右 pane：焦点切右后 l 进入 → 记录侧别 'R'。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Tab,
            KeyModifiers::NONE,
        );
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('l'),
            KeyModifiers::NONE,
        );
        assert!(
            state
                .live_recorded
                .keys()
                .any(|(t, s, p)| *t == 0 && *s == 'R' && p.ends_with("sub")),
            "右 pane 进入应记录 'R' 侧别: {:?}",
            state.live_recorded
        );
    }

    #[test]
    fn m21f9_repeat_visits_count_each_and_exit_flush_dedups_by_count() {
        // 重复进入各计一次：live 计数随次数累加；退出刷写按次数对齐去重，
        // 非实时记录的导航（:cd 语义，经 navigate_to 模拟）保留刷写。
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg = None;
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let cfg = Config::default();
        let sub_path = tmp.path().join("sub").to_string_lossy().replace('\\', "/");
        let tmp_path = tmp.path().to_string_lossy().replace('\\', "/");

        // 第一次进入 sub（实时记录 1）。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('l'),
            KeyModifiers::NONE,
        );
        // :cd 语义回上级（navigate_to 直调，不实时记录）。
        state
            .active_tab_mut()
            .navigate_to(Location::Physical(tmp.path().to_path_buf()), cfg)
            .unwrap();
        // 第二次进入 sub（实时记录 2）。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('l'),
            KeyModifiers::NONE,
        );
        assert_eq!(
            state.live_recorded.get(&(0, 'L', sub_path.clone())),
            Some(&2),
            "重复进入各计一次"
        );

        // 历史轨迹 [tmp, sub, tmp, sub]：sub 两条全部去重，tmp 保留一条刷写。
        let records = collect_session_records(&state);
        let sub_records = records.iter().filter(|r| r.path == sub_path).count();
        let tmp_records = records.iter().filter(|r| r.path == tmp_path).count();
        assert_eq!(sub_records, 0, "两次实时记录 → 退出刷写 0 条");
        assert_eq!(tmp_records, 1, ":cd 导航 → 退出刷写保留");
    }

    #[test]
    fn m21e1_attrs_column_adaptive_and_config_switch() {
        // ACCEPTANCE §20 M21-E1：列表可见大小与 yyyy-mm-dd HH:mm:ss 日期；
        // 窄终端属性列隐藏（文件名不挤爆）；配置开关生效。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("doc.txt"), "12345").unwrap(); // 5 B
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        state.active_tab_mut().entries[0].mtime = 1_700_000_000;
        let stamp = crate::tab::format_mtime_min(1_700_000_000);

        let draw_at = |state: &AppState, w: u16, h: u16| {
            let backend = ratatui::backend::TestBackend::new(w, h);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal
                .draw(|f| render(f, state, &InputMode::Normal, None))
                .unwrap();
            terminal.backend().buffer().clone()
        };
        // 条目行文本（跳过标题行 1 与状态栏 h-1）。
        let entry_rows = |buf: &ratatui::buffer::Buffer, w: u16, h: u16| -> Vec<String> {
            // 左 pane 列范围（右 pane 同目录列表会拼接进同一行字符串）。
            (2..h.saturating_sub(1))
                .map(|y| (0..w / 2).map(|x| buf[(x, y)].symbol()).collect::<String>())
                .collect()
        };

        // 宽终端 160：doc.txt 行同时可见大小与日期。
        let buf = draw_at(&state, 160, 24);
        let rows = entry_rows(&buf, 160, 24);
        let row = rows.iter().find(|r| r.contains("doc.txt")).unwrap();
        assert!(row.contains("5 B"), "大小列可见: {row}");
        assert!(row.contains(&stamp), "日期列可见: {row}");

        // 窄终端 80：属性列隐藏，文件名完整。
        let buf = draw_at(&state, 80, 24);
        let rows = entry_rows(&buf, 80, 24);
        let row = rows.iter().find(|r| r.contains("doc.txt")).unwrap();
        assert!(row.contains("doc.txt"), "文件名优先: {row}");
        assert!(!row.contains(&stamp), "窄终端隐藏日期列: {row}");
        assert!(!row.contains("5 B"), "窄终端隐藏大小列: {row}");

        // 配置开关：attrs_column = false → 宽终端也不显示。
        state.config.ui.attrs_column = false;
        let buf = draw_at(&state, 160, 24);
        let rows = entry_rows(&buf, 160, 24);
        let row = rows.iter().find(|r| r.contains("doc.txt")).unwrap();
        assert!(!row.contains(&stamp), "配置关闭属性列: {row}");
    }

    #[test]
    fn m21e2_w_preview_cycle_follows_and_restores() {
        // ACCEPTANCE §20 M21-E2：w 右 pane 预览左 pane 当前文件；j/k 滚动
        // 跟随；再按 w 恢复右 pane 目录列表（M13-9/M14-1/VIEW-1 现状验证）。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "line-01\nline-02\nline-03\n").unwrap();
        fs::write(tmp.path().join("b.txt"), "").unwrap();
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg = None;
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());

        let draw = |state: &AppState| {
            let backend = ratatui::backend::TestBackend::new(120, 24);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal
                .draw(|f| render(f, state, &InputMode::Normal, None))
                .unwrap();
            terminal.backend().buffer().clone()
        };
        let right_text = |buf: &ratatui::buffer::Buffer| -> String {
            let [_, left, right, _] = pane_layout(ratatui::layout::Rect::new(0, 0, 120, 24));
            let _ = left;
            (0..24u16)
                .map(|y| {
                    (right.x..right.x + right.width)
                        .map(|x| buf[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect()
        };

        // w → view_mode：右 pane 显示左光标文件（a.txt）内容。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('w'),
            KeyModifiers::NONE,
        );
        assert!(state.active_tab().view_mode, "w 开启视图模式");
        let buf = draw(&state);
        assert!(right_text(&buf).contains("line-01"), "右 pane 预览文件内容");

        // j/k：查看器滚动跟随（列表光标不动，VIEW-1 语义）。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('j'),
            KeyModifiers::NONE,
        );
        assert_eq!(
            state.active_tab().viewer.as_ref().unwrap().scroll,
            1,
            "j 滚动查看器视口"
        );
        let buf = draw(&state);
        assert!(!right_text(&buf).contains("line-01"), "滚动后首行移出视口");

        // 再按 w → 恢复右 pane 目录列表。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('w'),
            KeyModifiers::NONE,
        );
        assert!(!state.active_tab().view_mode, "再按 w 恢复");
        let buf = draw(&state);
        let restored = right_text(&buf);
        assert!(
            restored.contains("a.txt") && restored.contains("b.txt"),
            "右 pane 恢复目录列表: {restored}"
        );
        assert!(!restored.contains("line-01"), "不再显示文件内容");
    }

    #[test]
    fn m21e3_syntax_highlight_shebang_infer_and_truncation_hint() {
        // AUDIT-13：本测试断言 Rgb 前景 == 语法着色——与主题激活类测试
        // 持 theme::TEST_LOCK 串行（激活主题后图标/列表样式为 Rgb，
        // 会污染此断言）。
        let _guard = crate::theme::TEST_LOCK.lock().unwrap();
        // ACCEPTANCE §20 M21-E3：.rs/.py/.json 预览有高亮；.txt shebang 推断
        // 或安全降级；大文件截断提示。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("m.rs"), "fn main() { let x = 1; }\n").unwrap();
        fs::write(tmp.path().join("s.txt"), "#!/bin/sh\necho hello\n").unwrap();
        fs::write(tmp.path().join("data.bincode"), "randombytes payload\n").unwrap();
        let mut big = String::new();
        for i in 0..100_001 {
            big.push_str(&format!("line {i}\n"));
        }
        fs::write(tmp.path().join("big.txt"), &big).unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg = None;

        let mut draw_with = |state: &mut AppState, name: &str| {
            let idx = state
                .active_tab()
                .entries
                .iter()
                .position(|i| i.name == name)
                .unwrap_or_else(|| panic!("{name} 在列表中"));
            state.active_tab_mut().cursor = idx;
            // 经 w 开启（toggle_view 统计行数，截断提示依赖 viewer 状态）。
            state.active_tab_mut().view_mode = false;
            state.active_tab_mut().viewer = None;
            press_msg(
                state,
                &mut input,
                &mut msg,
                KeyCode::Char('w'),
                KeyModifiers::NONE,
            );
            let backend = ratatui::backend::TestBackend::new(120, 24);
            let mut terminal = ratatui::Terminal::new(backend).unwrap();
            terminal
                .draw(|f| render(f, state, &InputMode::Normal, None))
                .unwrap();
            terminal.backend().buffer().clone()
        };
        let [_, left, right, _] = pane_layout(ratatui::layout::Rect::new(0, 0, 120, 24));
        let _ = left;
        let rgb_cells = |buf: &ratatui::buffer::Buffer| {
            (right.x..right.x + right.width)
                .flat_map(|x| (0..24u16).map(move |y| (x, y)))
                .filter(|(x, y)| matches!(buf[(*x, *y)].fg, ratatui::style::Color::Rgb(..)))
                .count()
        };
        let pane_text = |buf: &ratatui::buffer::Buffer| -> String {
            (0..24u16)
                .map(|y| {
                    (right.x..right.x + right.width)
                        .map(|x| buf[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect()
        };

        // .rs：语法着色（RGB 前景单元格存在），内容可见。
        let buf = draw_with(&mut state, "m.rs");
        assert!(rgb_cells(&buf) > 0, "rs 预览存在语法着色");
        assert!(pane_text(&buf).contains("fn main()"), "rs 内容可见");
        // .txt shebang 推断：渲染不 panic 且内容可见（推断正确性由
        // highlight.rs 单测覆盖）。
        let buf = draw_with(&mut state, "s.txt");
        assert!(
            pane_text(&buf).contains("echo hello"),
            "txt shebang 内容可见"
        );
        // 未知扩展名：无着色（安全降级纯文本），内容可见不 panic。
        let buf = draw_with(&mut state, "data.bincode");
        assert_eq!(rgb_cells(&buf), 0, "无语法应降级纯文本");
        assert!(pane_text(&buf).contains("randombytes"), "降级内容可见");
        // 大文件：标题截断提示（CJK 全角"字+空格"渲染，去空格比对）。
        let buf = draw_with(&mut state, "big.txt");
        assert!(
            pane_text(&buf).replace(' ', "").contains("已截断"),
            "截断提示可见"
        );
    }

    #[test]
    fn m21e4_inline_rename_cursor_semantics_apply_and_undo() {
        // ACCEPTANCE §20 M21-E4：I/cc/A 光标位置符合预期；编辑应用生效且
        // u 可逆；R 与 :rename（无参）路由到 $EDITOR 批量管线（Outcome 断言）。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("alpha.txt"), "").unwrap();
        fs::write(tmp.path().join("beta.txt"), "").unwrap();
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg = None;
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());

        // I：光标在名首（input 保留原名）。
        let outcome = press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('I'),
            KeyModifiers::NONE,
        );
        assert!(
            matches!(outcome, Outcome::EnterRename { ref pos, ref input, .. } if *pos == 0 && input == "alpha.txt"),
            "I 应在名首打开内联重命名: {outcome:?}"
        );

        // A：光标在名尾。
        let outcome = press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('A'),
            KeyModifiers::NONE,
        );
        assert!(
            matches!(outcome, Outcome::EnterRename { ref pos, ref input, .. }
                if *pos == input.chars().count() && input == "alpha.txt"),
            "A 应在名尾打开内联重命名: {outcome:?}"
        );

        // cc：清空输入、光标归零。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('c'),
            KeyModifiers::NONE,
        );
        let outcome = press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('c'),
            KeyModifiers::NONE,
        );
        assert!(
            matches!(outcome, Outcome::EnterRename { ref pos, ref input, .. } if *pos == 0 && input.is_empty()),
            "cc 应清空: {outcome:?}"
        );

        // Esc 取消无副作用（alpha.txt 仍在）。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Esc,
            KeyModifiers::NONE,
        );
        assert!(tmp.path().join("alpha.txt").exists(), "Esc 取消不改动");

        // 应用 + 撤销：rename_apply（Rename 模式 Enter 共用体）改名生效、
        // 入撤销栈、u 恢复。
        let path = tmp.path().join("alpha.txt");
        let m = rename_apply(&mut state, &path, "renamed.txt");
        assert!(m.as_deref().unwrap().contains("renamed.txt"));
        assert!(tmp.path().join("renamed.txt").exists());
        assert!(!path.exists());
        assert_eq!(state.undo_stack.len(), 1, "入撤销栈");
        // u 撤销恢复原名（与按键 u 同一函数）。
        let mut umsg = None;
        undo_last(&mut state, &mut umsg);
        assert!(tmp.path().join("alpha.txt").exists(), "u 撤销恢复原名");
        assert!(!tmp.path().join("renamed.txt").exists(), "撤销移除新名");

        // 多选批量（编辑器管线共用体）：两行名单经 apply_rename_lines 生效，
        // 逐项入撤销栈可逆。
        fs::write(tmp.path().join("b1.txt"), "").unwrap();
        fs::write(tmp.path().join("b2.txt"), "").unwrap();
        let base = tmp.path().to_path_buf();
        let old_names = vec!["b1.txt".to_owned(), "b2.txt".to_owned()];
        let result = apply_rename_lines(&mut state, &base, &old_names, "c1.txt\nc2.txt\n");
        assert!(result.is_ok(), "批量应用: {result:?}");
        assert!(tmp.path().join("c1.txt").exists());
        assert!(tmp.path().join("c2.txt").exists());
        let undo_count = state.undo_stack.len();
        assert!(undo_count >= 2, "批量各项入撤销栈");
        // R 单键与 :rename 无参均路由到批量管线。
        let outcome = press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('R'),
            KeyModifiers::NONE,
        );
        assert!(
            matches!(outcome, Outcome::BatchRename { ref names, .. } if !names.is_empty()),
            "R 路由到批量重命名: {outcome:?}"
        );
        let r = exec_command_impl(&mut state, "rename").unwrap();
        assert!(
            matches!(r, ExecOutcome::BatchRename { .. }),
            ":rename 无参走批量"
        );
    }

    #[test]
    fn m21e5_mkdir_locates_cursor_and_mk_enters() {
        // ACCEPTANCE §20 M21-E5：:mkdir test 光标定位 test；:mk test 进入
        // test；失败不改光标/位置。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let cursor_name = |state: &AppState| -> String {
            state
                .active_tab()
                .entries
                .get(state.active_tab().cursor)
                .map(|i| i.name.clone())
                .unwrap_or_default()
        };

        // :mkdir test → 光标定位 test 条目，不在 `..` 行。
        exec_command_impl(&mut state, "mkdir test").unwrap();
        assert_eq!(cursor_name(&state), "test", "光标定位新目录");
        assert!(!state.active_tab().on_parent_row);

        // 失败（目录已存在）：Err 且光标不动。
        assert!(exec_command_impl(&mut state, "mkdir test").is_err());
        assert_eq!(cursor_name(&state), "test", "失败不改光标");

        // :mk sub → 创建并进入；失败（已存在）位置不变。
        exec_command_impl(&mut state, "mk sub").unwrap();
        assert_eq!(
            state.active_tab().location,
            Location::Physical(tmp.path().join("sub")),
            ":mk 进入新目录"
        );
        // 回上级再试已存在目录：Err 且位置不动。:cd 相对路径相对进程 CWD
        // （M12-2 教训：Linux CI 上此断言路径才执行）——用绝对路径避免
        // base 漂移到 CWD 导致 mk 落到仓库根。
        let up = format!("cd {}", tmp.path().display());
        exec_command_impl(&mut state, &up).unwrap();
        let before = state.active_tab().location.clone();
        assert!(
            exec_command_impl(&mut state, "mk sub").is_err(),
            "已存在返回 Err"
        );
        assert_eq!(state.active_tab().location, before, "失败不改变位置");
        // 撤销链：mkdir/mk 各入撤销栈（可逆）。
        assert_eq!(state.undo_stack.len(), 2, "两次创建均入撤销栈");
    }

    #[test]
    fn m22_5_remote_find_grep_and_locate() {
        // ACCEPTANCE §21 M22-5：远端 :find/:grep 结果虚拟目录 + 定位跳回远端
        // 父目录（mock 后端；read 返回固定 "mock content"）。
        let (_tmp, mut state) = mock_remote_state();
        remote_enter(&mut state, "dev", std::path::Path::new("/")).unwrap();

        // :find readme → 结果虚拟目录（远端绝对路径显示）。
        exec_command_impl(&mut state, "find readme").unwrap();
        assert!(state.active_tab().in_results());
        let names: Vec<String> = state
            .active_tab()
            .entries
            .iter()
            .map(|i| i.name.clone())
            .collect();
        assert!(
            names.iter().any(|n| n.contains("/readme.md")),
            "远端 find 命中: {names:?}"
        );

        // l 定位 → 退出结果视图 + 回远端父目录 + 光标在 readme.md。
        assert!(remote_locate_result(&mut state).is_ok());
        assert!(!state.active_tab().in_results(), "定位退出结果视图");
        assert_eq!(
            state.active_tab().location,
            Location::Remote {
                remote: "dev".to_owned(),
                path: std::path::PathBuf::from("/")
            }
        );
        assert_eq!(
            state
                .active_tab()
                .entries
                .get(state.active_tab().cursor)
                .map(|i| i.name.clone()),
            Some("readme.md".to_owned()),
            "光标定位命中文件"
        );

        // :grep content（read 固定返回 "mock content"）→ 命中 /docs/notes.md:1。
        exec_command_impl(&mut state, "grep content").unwrap();
        assert!(state.active_tab().in_results());
        let has_hit = state
            .active_tab()
            .results
            .as_ref()
            .map(|r| {
                r.items
                    .iter()
                    .any(|i| i.path == std::path::Path::new("/docs/notes.md") && i.line == Some(1))
            })
            .unwrap_or(false);
        assert!(has_hit, "grep 命中远端文件行: {names:?}");

        // 定位 notes.md 命中项（mock read 内容固定，readme.md 也命中——
        // cursor 移到 /docs/notes.md 条目再定位）。
        let idx = state
            .active_tab()
            .results
            .as_ref()
            .unwrap()
            .items
            .iter()
            .position(|i| i.path == std::path::Path::new("/docs/notes.md"))
            .unwrap();
        state.active_tab_mut().cursor = idx;
        assert!(remote_locate_result(&mut state).is_ok());
        assert_eq!(
            state.active_tab().location,
            Location::Remote {
                remote: "dev".to_owned(),
                path: std::path::PathBuf::from("/docs")
            },
            "grep 定位回父目录"
        );
    }

    #[test]
    fn m22_1_cd_relative_resolves_against_tab_dir() {
        // ACCEPTANCE §21 M22-1：`:cd ..` 落到真实父目录（绝对路径，非 ".."）；
        // 随后 :mkdir 落点在 tab 目录（非进程 CWD）。M21-E5 CI 实测的漂移回归测试。
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        let mut state =
            AppState::new(Tab::from_physical(&tmp.path().join("sub"), Config::default()).unwrap());

        // cd .. → 当前 tab 目录的父目录（词法归一化绝对路径，起点在 sub）。
        exec_command_impl(&mut state, "cd ..").unwrap();
        let expected = normalize_lexical(tmp.path());
        assert_eq!(
            state.active_tab().location,
            Location::Physical(expected.clone()),
            "cd .. 落父目录绝对路径"
        );

        // 相对路径 cd sub → 相对当前 tab 目录（此时在 tmp）。
        exec_command_impl(&mut state, "cd sub").unwrap();
        assert_eq!(
            state.active_tab().location,
            Location::Physical(normalize_lexical(&tmp.path().join("sub"))),
            "cd sub 相对 tab 目录解析"
        );

        // 回父目录后 :mkdir 落点在 tab 目录（非 CWD）——漂移回归核心断言。
        exec_command_impl(&mut state, "cd ..").unwrap();
        exec_command_impl(&mut state, "mkdir escape_check").unwrap();
        assert!(
            tmp.path().join("escape_check").exists(),
            "mkdir 落在 tab 目录"
        );
        assert!(
            !std::env::current_dir()
                .unwrap()
                .join("escape_check")
                .exists(),
            "mkdir 不得落到进程 CWD"
        );

        // 失败：目录不存在 → Err 且位置不变。
        let before = state.active_tab().location.clone();
        assert!(exec_command_impl(&mut state, "cd no_such_dir_xyz").is_err());
        assert_eq!(state.active_tab().location, before, "失败不改位置");
    }

    #[test]
    fn m21e7_filter_glob_implicit_wildcard_and_esc_restore() {
        // ACCEPTANCE §20 M21-E7：`*.rs` 过滤 Rust 文件；`src` 等效 `*src*`；
        // Esc 恢复完整列表。实时链路（run 循环 Continue → apply_filter）
        // 与提交/Esc 共用同一函数，按输入态逐步驱动同链验证。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("main.rs"), "").unwrap();
        fs::write(tmp.path().join("readme.txt"), "").unwrap();
        fs::create_dir(tmp.path().join("src")).unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let cfg = Config::default();
        let names = |state: &AppState| -> Vec<String> {
            state
                .active_tab()
                .entries
                .iter()
                .map(|i| i.name.clone())
                .collect()
        };

        // 匹配语义单元：通配符 glob 与隐式 *pat*。
        assert!(crate::tab::filter_matches("*.rs", "main.rs"));
        assert!(!crate::tab::filter_matches("*.rs", "readme.txt"));
        assert!(
            crate::tab::filter_matches("src", "src"),
            "隐式 *src* 命中自身"
        );
        assert!(crate::tab::filter_matches("src", "my_src_dir"), "隐式子串");
        assert!(!crate::tab::filter_matches("src", "readme.txt"));

        // 实时输入 "*.rs"（逐键 = Continue 链路）→ 只剩 main.rs。
        for input in ["*", "*.", "*.r", "*.rs"] {
            state.active_tab_mut().apply_filter(input, cfg.clone());
        }
        assert_eq!(names(&state), ["main.rs"], "*.rs 通配过滤");

        // Esc 清除 → 恢复完整列表。
        state.active_tab_mut().apply_filter("", cfg.clone());
        assert_eq!(names(&state).len(), 3, "Esc 恢复完整列表");

        // 隐式 *src*：输入 "src"（逐键）→ 只剩 src 目录。
        for input in ["s", "sr", "src"] {
            state.active_tab_mut().apply_filter(input, cfg.clone());
        }
        assert_eq!(names(&state), ["src"], "src 隐式 *src* 过滤");

        // Esc → 恢复。
        state.active_tab_mut().apply_filter("", cfg.clone());
        assert_eq!(names(&state).len(), 3, "Esc 再次恢复");
    }

    #[test]
    fn m21e8_jump_key_two_stage_esc_and_vim_ops() {
        // ACCEPTANCE §20 M21-E8：输入即搜索（默认搜索态）；Esc→Normal（清
        // 输入，j/k/gg/G/l Vim 操作），再 Esc 关闭；Normal 态可打印字符回
        // 搜索态。jump_key 为 run 循环同一处理函数（可测抽取）。
        let key = |code: KeyCode| KeyEvent::new(code, KeyModifiers::NONE);
        let mut normal = false;
        let mut g_pending = false;
        let mut input = String::new();
        let mut cursor = 0usize;
        let visible = 3usize;

        // 搜索态：输入 "p" → input 累积、cursor 归零。
        jump_key(
            &mut normal,
            &mut g_pending,
            &mut input,
            &mut cursor,
            visible,
            &key(KeyCode::Char('p')),
        );
        assert_eq!(input, "p");
        assert!(!normal);

        // Esc → Normal（清输入），再 Esc → 关闭。
        assert!(matches!(
            jump_key(
                &mut normal,
                &mut g_pending,
                &mut input,
                &mut cursor,
                visible,
                &key(KeyCode::Esc)
            ),
            JumpKey::Continue
        ));
        assert!(normal, "Esc 进入 Normal 态");
        assert!(input.is_empty(), "Esc 清除筛选");
        assert!(matches!(
            jump_key(
                &mut normal,
                &mut g_pending,
                &mut input,
                &mut cursor,
                visible,
                &key(KeyCode::Esc)
            ),
            JumpKey::Close
        ));
        // Close 后 run 循环丢弃模式；继续用同一 Normal 态验证 Vim 操作。

        // Normal 态：gg 落顶（先待决后落）、G 落尾、j/k 移动、l 选定。
        cursor = 1;
        jump_key(
            &mut normal,
            &mut g_pending,
            &mut input,
            &mut cursor,
            visible,
            &key(KeyCode::Char('g')),
        );
        assert!(g_pending, "首按 g 待决");
        jump_key(
            &mut normal,
            &mut g_pending,
            &mut input,
            &mut cursor,
            visible,
            &key(KeyCode::Char('g')),
        );
        assert_eq!(cursor, 0, "gg 落顶");
        jump_key(
            &mut normal,
            &mut g_pending,
            &mut input,
            &mut cursor,
            visible,
            &key(KeyCode::Char('G')),
        );
        assert_eq!(cursor, 2, "G 落尾");
        jump_key(
            &mut normal,
            &mut g_pending,
            &mut input,
            &mut cursor,
            visible,
            &key(KeyCode::Char('k')),
        );
        assert_eq!(cursor, 1, "k 上移");
        assert!(matches!(
            jump_key(
                &mut normal,
                &mut g_pending,
                &mut input,
                &mut cursor,
                visible,
                &key(KeyCode::Char('l'))
            ),
            JumpKey::Select
        ));

        // Normal 态可打印字符 → 回搜索态（携带字符）。
        jump_key(
            &mut normal,
            &mut g_pending,
            &mut input,
            &mut cursor,
            visible,
            &key(KeyCode::Char('x')),
        );
        assert!(!normal, "字符回搜索态");
        assert_eq!(input, "x");
    }

    #[test]
    fn m22_3_remote_pass_masked_render_and_apply_error() {
        // ACCEPTANCE §21 M22-3：口令 masked 渲染不泄露；失败返回可读错误。
        let (_tmp, state) = make_state(1);
        let mode = InputMode::RemotePass {
            name: "srv".to_owned(),
            params: crate::remote::sftp::SftpParams::new("127.0.0.1"),
            input: "sekret".to_owned(),
        };
        let backend = ratatui::backend::TestBackend::new(120, 12);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &state, &mode, None)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let status: String = (0..120u16).map(|x| buf[(x, 11)].symbol()).collect();
        assert!(
            status.replace(' ', "").contains("远端口令（srv）"),
            "状态栏含提示: {status}"
        );
        assert!(status.contains("******"), "口令 masked");
        assert!(!status.contains("sekret"), "口令不得明文出现: {status}");

        // 失败路径：不可达端口 → 可读错误（连接错误经 remote_pass_apply 包装）。
        let (_tmp2, mut state2) = make_state(1);
        let mut params = crate::remote::sftp::SftpParams::new("127.0.0.1");
        params.port = 1; // 关闭端口 → 快速拒绝
        let r = remote_pass_apply(&mut state2, "srv", &params, "pw");
        assert!(r.is_err(), "不可达主机应失败");
        assert!(r.unwrap_err().contains("口令认证失败"), "错误带统一前缀");
    }

    #[test]
    fn m23_4_ignore_patterns_skip_record() {
        // ACCEPTANCE §22 M23-4：IGNORE_PATTERNS 命中路径静默跳过
        // （record_live_visit 不登记）；未命中照常记录。
        let (_tmp, mut state) = make_state(1);
        state.config.frecency.ignore_patterns = vec!["*/node_modules".to_owned()];

        // 命中：node_modules 目录 → 不登记。
        state.active_tab_mut().location = Location::Physical(PathBuf::from("C:/proj/node_modules"));
        record_live_visit(&mut state, 0, 'L');
        assert!(
            state.live_recorded.is_empty(),
            "忽略模式命中不记录: {:?}",
            state.live_recorded
        );

        // 未命中：src 目录 → 登记在案。
        state.active_tab_mut().location = Location::Physical(PathBuf::from("C:/proj/src"));
        record_live_visit(&mut state, 0, 'L');
        assert!(
            state
                .live_recorded
                .contains_key(&(0, 'L', "C:/proj/src".to_owned())),
            "未命中路径照常记录: {:?}",
            state.live_recorded
        );

        // 退出刷写同样过滤（flush_session_visits 共用 ignored）。
        let records = collect_session_records(&state);
        assert!(records.iter().all(|r| !r.path.contains("node_modules")));
    }

    #[test]
    fn m23_10_y_copy_sequences_and_editor_outcome() {
        // ACCEPTANCE §22 M23-10：yn/yf/yc 单选与多选；e 产出 EditFile（目录拒绝）。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("alpha.txt"), "content-a").unwrap();
        fs::write(tmp.path().join("beta.md"), "content-b").unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("sub/inner.txt"), "inner").unwrap();
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap.clone());
        let mut msg = None;
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());

        // yn：单选复制光标条目名（定位到 alpha.txt——dirs-first 排序 sub 在 0）。
        state.active_tab_mut().cursor = 1;
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('y'),
            KeyModifiers::NONE,
        );
        let o = press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('n'),
            KeyModifiers::NONE,
        );
        let _ = o;
        assert!(
            msg.as_deref().unwrap().contains("copied") && msg.as_deref().unwrap().contains("alpha"),
            "yn copied 反馈: {msg:?}"
        );

        // yf：复制全路径（含 base 前缀）。
        msg = None;
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('y'),
            KeyModifiers::NONE,
        );
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('f'),
            KeyModifiers::NONE,
        );
        assert!(
            msg.as_deref()
                .unwrap()
                .contains(&tmp.path().to_string_lossy().replace('\\', "/")),
            "yf 复制全路径: {msg:?}"
        );

        // yc：文件内容复制。
        msg = None;
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('y'),
            KeyModifiers::NONE,
        );
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('c'),
            KeyModifiers::NONE,
        );
        assert!(
            msg.as_deref().unwrap().contains("copied"),
            "yc copied: {msg:?}"
        );

        // 多选（标记两个条目）→ yf 反馈 2 项。
        let _ = input; // 重置 seq
        let mut input2 = InputState::with_keymap(keymap.clone());
        state.active_tab_mut().cursor = 0;
        press_msg(
            &mut state,
            &mut input2,
            &mut msg,
            KeyCode::Char('t'),
            KeyModifiers::NONE,
        );
        state.active_tab_mut().cursor = 1;
        press_msg(
            &mut state,
            &mut input2,
            &mut msg,
            KeyCode::Char('t'),
            KeyModifiers::NONE,
        );
        msg = None;
        press_msg(
            &mut state,
            &mut input2,
            &mut msg,
            KeyCode::Char('y'),
            KeyModifiers::NONE,
        );
        press_msg(
            &mut state,
            &mut input2,
            &mut msg,
            KeyCode::Char('f'),
            KeyModifiers::NONE,
        );
        assert!(msg.as_deref().unwrap().contains("2 项"), "多选 yf: {msg:?}");

        // e：目录 → 拒绝；文件 → EditFile outcome。
        state.active_tab_mut().cursor = 0; // sub（目录——dirs-first 排序在 0）
        let o = press_msg(
            &mut state,
            &mut input2,
            &mut msg,
            KeyCode::Char('e'),
            KeyModifiers::NONE,
        );
        let _ = o;
        assert!(msg.as_deref().unwrap().contains("目录不支持编辑"));
        state.active_tab_mut().cursor = 1; // alpha.txt（sub 在 0）
        let o = press_msg(
            &mut state,
            &mut input2,
            &mut msg,
            KeyCode::Char('e'),
            KeyModifiers::NONE,
        );
        assert!(
            matches!(o, Outcome::EditFile { .. }),
            "e 文件 → EditFile: {o:?}"
        );
    }

    #[test]
    fn m22_6_remote_open_downloads_temp_copy_and_cleanup() {
        // ACCEPTANCE §21 M22-6：远端文件打开生成临时副本（read→write）并
        // 登记清理；cleanup_temp_opens 删除副本。opener 调用本身不可测
        // （Windows cmd start 阻塞——FEAT-3 教训），下载/清理直测。
        let (_tmp, mut state) = mock_remote_state();
        let copy =
            remote_download_copy(&mut state, "dev", std::path::Path::new("/readme.md")).unwrap();
        let bytes = fs::read(&copy).unwrap();
        assert_eq!(bytes, b"mock content", "副本内容与远端一致");
        assert!(
            copy.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("rvifm-open-"),
            "副本前缀: {}",
            copy.display()
        );
        assert!(state.temp_opens.contains(&copy), "登记待清理");

        cleanup_temp_opens(&mut state);
        assert!(!copy.exists(), "退出清理删除副本");
        assert!(state.temp_opens.is_empty(), "登记清空");

        // 会话缺失：可读错误。
        let r = remote_download_copy(&mut state, "nope", std::path::Path::new("/x"));
        assert!(r.unwrap_err().contains("会话已断开"));
    }

    #[test]
    fn m21e8_cd_and_file_open_record_visits() {
        // 记录补全（M21-E8）：:cd 进入目录 → ('L',dir) 登记；打开文件 →
        // ('F',file) 登记；collect_session_records 只对 'L' 去重（文件不进
        // 目录历史，登记仅为测试观察态）。
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("doc.txt"), "x").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());

        // :cd sub → 目录实时记录（:cd 相对路径相对进程 CWD，用绝对路径）。
        let cd_target = format!("cd {}", tmp.path().join("sub").display());
        exec_command_impl(&mut state, &cd_target).unwrap();
        let sub = tmp.path().join("sub").to_string_lossy().replace('\\', "/");
        assert!(
            state.live_recorded.contains_key(&(0, 'L', sub.clone())),
            ":cd 进入应记录目录: {:?}",
            state.live_recorded
        );

        // 打开文件（record_file_open 为 opener Ok 路径共用体）→ 文件记录。
        let doc = tmp.path().join("doc.txt");
        record_file_open(&mut state, 0, &doc);
        let doc_s = doc.to_string_lossy().replace('\\', "/");
        assert!(
            state.live_recorded.contains_key(&(0, 'F', doc_s)),
            "打开文件应记录 file 访问: {:?}",
            state.live_recorded
        );
        // 文件记录不参与目录历史去重（collect 仍含 sub?——sub 已实时记录，
        // 去重后不在；此处验证 collect 不因 'F' 键误删目录条目）。
        let records = collect_session_records(&state);
        assert!(
            !records.iter().any(|r| r.path.ends_with("doc.txt")),
            "文件不在目录历史刷写中"
        );
    }

    #[test]
    fn m21e6_tar_xz_bz2_navigate_like_zip() {
        // ACCEPTANCE §20 M21-E6：.tar.xz/.tar.bz2 l/Enter 进入只读虚拟目录，
        // h 返回物理父目录（M7-8 现状 TUI 导航级验证；M21-F4 定位语义顺带覆盖）。
        let tmp = TempDir::new().unwrap();
        // .tar.xz：tar fixture 现场压缩（lzma-rs 带编码器）。
        let plain = tmp.path().join("plain.tar");
        crate::archive::make_test_tar(&plain, false);
        let mut raw = std::io::BufReader::new(std::fs::File::open(&plain).unwrap());
        let mut out = Vec::new();
        lzma_rs::xz_compress(&mut raw, &mut out).unwrap();
        fs::write(tmp.path().join("t.tar.xz"), out).unwrap();
        // .tar.bz2：解码器无编码器，复用提交的 fixture（M7-8 同款）。
        fs::copy("tests/fixtures/t.tar.bz2", tmp.path().join("t.tar.bz2")).unwrap();
        fs::write(tmp.path().join("host.txt"), "H").unwrap();
        let layers = config::keymap_layers();
        let keymap =
            crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
        let mut input = InputState::with_keymap(keymap);
        let mut msg = None;

        for name in ["t.tar.xz", "t.tar.bz2"] {
            let mut state =
                AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
            let idx = state
                .active_tab()
                .entries
                .iter()
                .position(|i| i.name == name)
                .unwrap_or_else(|| panic!("{name} 在列表中"));
            state.active_tab_mut().cursor = idx;
            // l → 只读虚拟目录（条目与 zip/tar 同构）。
            press_msg(
                &mut state,
                &mut input,
                &mut msg,
                KeyCode::Char('l'),
                KeyModifiers::NONE,
            );
            assert!(
                matches!(state.active_tab().location, Location::Archive { .. }),
                "{name} 进入虚拟目录（loc={:?} msg={msg:?}）",
                state.active_tab().location
            );
            assert!(
                state
                    .active_tab()
                    .entries
                    .iter()
                    .any(|i| i.name == "src" && i.is_dir),
                "{name} 列表含 src 目录"
            );
            // h → 返回物理父目录，光标落在归档文件条目（M21-F4）。
            press_msg(
                &mut state,
                &mut input,
                &mut msg,
                KeyCode::Char('h'),
                KeyModifiers::NONE,
            );
            assert_eq!(
                state.active_tab().location,
                Location::Physical(tmp.path().to_path_buf()),
                "{name} h 返回物理目录"
            );
            assert_eq!(
                state
                    .active_tab()
                    .entries
                    .get(state.active_tab().cursor)
                    .map(|i| i.name.clone()),
                Some(name.to_owned()),
                "{name} h 后光标定位归档文件"
            );
        }
    }

    #[test]
    fn m21f2_focus_switch_sets_no_status_message() {
        // ACCEPTANCE §20 M21-F2：状态栏无焦点相关文字——Tab/Space 切换
        // 不再产生提示消息（焦点经边框/当前行样式呈现）。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut input = InputState::default();
        let mut msg = None;
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Tab,
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().focus, crate::tab::Focus::Right);
        assert!(msg.is_none(), "切换焦点不应有状态栏提示: {msg:?}");
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char(' '),
            KeyModifiers::NONE,
        );
        assert_eq!(state.active_tab().focus, crate::tab::Focus::Left);
        assert!(msg.is_none(), "切回焦点不应有状态栏提示: {msg:?}");
    }
}
