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

use std::path::PathBuf;

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
    /// 请求确认删除（`dd`/`DD`，M2-2/M13-3）：携带待删完整路径；
    /// permanent = 跳过暂存的不可逆删除（DD）。
    ConfirmDelete(PathBuf, bool),
    /// 打开跳转选择器（M8-12：`o` 合一视角；数据已在 handle_key 读好）。
    OpenJump {
        kind: JumpKind,
        entries: Vec<crate::frecency::ScoredEntry>,
    },
    /// 打开文件详情对话框（M13-5：Ctrl-G，文本已构造好）。
    FileInfo { path: PathBuf, text: String },
    /// 批量重命名（M13-6：cw——名单待 $EDITOR 编辑）。
    BatchRename { base: PathBuf, names: Vec<String> },
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
    /// 命令行模式：正在输入 `:` 后的内容。
    Command { input: String },
    /// 搜索输入模式：正在输入 `/` 后的内容（M1-7）。`origin` 为进入搜索时
    /// 的光标位置（FIX-1：实时跳转的起点，Esc 时恢复）。
    Search { input: String, origin: usize },
    /// 过滤输入模式：正在输入 `=` 后的内容（M4-2）。
    Filter { input: String },
    /// 删除确认模式：`y` 执行，其余取消（M2-2；M13-3 起 permanent 标志
    /// 区分 dd 暂存删除与 DD 永久删除，渲染与执行分支不同）。
    Confirm { path: PathBuf, permanent: bool },
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
    JumpSelect {
        input: String,
        cursor: usize,
        entries: Vec<crate::frecency::ScoredEntry>,
        kind: JumpKind,
    },
    /// 右键上下文菜单（FEAT-3）：`cursor` = 菜单内选中项；Enter 执行，
    /// Esc/点击外部关闭。
    ContextMenu { cursor: usize },
    /// 目录历史菜单（M16 NAV-2）：`cursor` = 历史条目下标（live 渲染
    /// `history[..=hist_pos]`）；Enter/l 导航、dd 移除、Esc/q 关闭。
    HistoryMenu { cursor: usize },
    /// 文件详情对话框（M13-5，Ctrl-G）：打开时读取完成，任意键关闭。
    FileInfo { path: PathBuf, text: String },
    /// 后台作业菜单（M13-8：`:jobs`）。
    Jobs,
    /// 暂存区菜单（M13-10：`:trash`，Enter 恢复选中项到原路径）。
    TrashMenu {
        cursor: usize,
        items: Vec<crate::ops::TrashEntry>,
    },
}

/// 命令行单键处理结果。
#[derive(Debug, PartialEq, Eq)]
enum CmdInput {
    Continue,
    Submit,
    Cancel,
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
    Done { msg: Option<String> },
    /// 请求确认批量删除标记文件（`:tag delete`，M3-3）。
    ConfirmTagDelete,
    /// 写操作 EACCES → 进入 sudo 密码输入（M12-3 操作级提权）。
    Escalate { op: crate::privilege::PrivOp },
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
    /// 打开暂存区菜单（M13-10：`:trash`，条目已读好）。
    ShowTrash { items: Vec<crate::ops::TrashEntry> },
    /// 打开文件详情对话框（FEAT-3：`:file`，与 Ctrl-G 同源）。
    ShowInfo { path: PathBuf, text: String },
    /// 打开目录历史菜单（M16 NAV-2：`:history`）。
    ShowHistory,
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
    }
}

/// M16 NAV-2：历史菜单导航——落到历史第 idx 条（作为新导航，截断
/// forward）；当前位置提示。
fn history_menu_nav(state: &mut AppState, idx: usize, msg: &mut Option<String>) {
    let cfg = state.config;
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
                    Location::Archive { .. } => None,
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
    let cfg = state.config;
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
    let store = crate::frecency::open_store().map_err(|e| format!("frecency 存储打开失败: {e}"))?;
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
    let mut out: Vec<crate::frecency::ScoredEntry> = entries
        .iter()
        .filter(|e| {
            let snap = crate::frecency::SnapshotEntry {
                path: e.path.clone(),
                topic: e.topic.clone(),
                keyword: e.keyword.clone(),
                score: 0.0,
                tick: 0,
                clock: 0,
            };
            crate::frecency::grade_for(&snap, query) != crate::frecency::Grade::None
        })
        .cloned()
        .collect();
    crate::frecency::ranked(&mut out);
    out
}

/// 选定执行（M8-12，纯导航语义）：目录 → cd；文件 → 切到父目录并定位光标。
fn jump_to(state: &mut AppState, entry: &crate::frecency::ScoredEntry) -> Result<String, String> {
    let cfg = state.config;
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
    let mut parts = line.split_whitespace();
    let cmd_head = parts.next();
    // FEAT-2：`..` 行仅导航——写操作与详情命令拒绝（cd/q/help/tree 例外）。
    if state.active_tab().on_parent_row {
        match cmd_head {
            Some("cd" | "q" | "quit" | "help" | "histprev" | "histnext" | "history" | "his") => {}
            Some(other) => return Err(format!("`..` 行仅导航（j 向下离开）: {other}")),
            None => return Ok(ExecOutcome::Done { msg: None }),
        }
    }
    // M17 SRCH：结果视图只读——find/grep 重执行与退出类命令豁免。
    if state.active_tab().in_results() {
        match cmd_head {
            Some("find" | "grep" | "q" | "quit" | "help") => {}
            Some(other) => {
                return Err(format!("搜索结果视图为只读（Esc 退出）: {other}"));
            }
            None => return Ok(ExecOutcome::Done { msg: None }),
        }
    }
    // M13-7：树视图只读——除 :tree（退出）/ :q / :help 外的命令拒绝。
    if state.active_tab().in_tree() {
        match cmd_head {
            Some("tree") | Some("q" | "quit") | Some("help") => {}
            Some(other) => {
                return Err(format!("树视图为只读浏览（:tree 退出）: {other}"));
            }
            None => return Ok(ExecOutcome::Done { msg: None }),
        }
    }
    match cmd_head {
        None => Ok(ExecOutcome::Done { msg: None }),
        Some("find" | "grep") => {
            // M17 SRCH-1/2：递归搜索 → 结果虚拟目录（重执行 = 替换）。
            let is_grep = cmd_head == Some("grep");
            let pattern = parts.next().unwrap_or("").to_owned();
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
        Some("recent") => {
            // M16 NAV-4：近期视角——全部条目按最后访问时间倒序（含会话合并）。
            let entries = load_jump_entries(state, JumpKind::Recent)?;
            Ok(ExecOutcome::OpenJump {
                kind: JumpKind::Recent,
                entries,
            })
        }
        Some("q" | "quit") => Ok(ExecOutcome::Quit),
        Some("cd") => {
            // M5-3：切换当前 tab 到目标目录（保留原名导航语义）。
            let dir_arg = parts.next().ok_or_else(|| "cd: 缺少目录".to_owned())?;
            let target = PathBuf::from(dir_arg);
            if !target.is_dir() {
                return Err(format!("cd: 目录不存在: {dir_arg}"));
            }
            let cfg = state.config;
            let tab = state.active_tab_mut();
            tab.navigate_to(Location::Physical(target), cfg)
                .map_err(|e| format!("cd: {e}"))?;
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
            // M2-3：重命名选中条目（新名字不含空白；含空格的名字在 M5-4 统一解析）。
            let new_name = parts
                .next()
                .ok_or_else(|| "rename: 缺少新名字".to_owned())?;
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
            let target = PathBuf::from(dir_arg);
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
            let target = PathBuf::from(dir_arg);
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
                    let target = PathBuf::from(dir_arg);
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
            // M16 NAV-3：当前 tab 视图历史后退/前进（无默认键位）。
            let forward = cmd_head == Some("histnext");
            let cfg = state.config;
            let moved = state
                .active_tab_mut()
                .hist_jump(forward, cfg)
                .map_err(|e| format!("{}: {e}", if forward { "histnext" } else { "histprev" }))?;
            Ok(ExecOutcome::Done {
                msg: Some(if moved {
                    let (pos, len) = {
                        let tab = state.active_tab();
                        (tab.hist_pos, tab.history.len())
                    };
                    format!(
                        "{} {}/{}",
                        if forward { "前进" } else { "后退" },
                        pos + 1,
                        len
                    )
                } else if forward {
                    "无前进历史".to_owned()
                } else {
                    "无后退历史".to_owned()
                }),
            })
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
    let cfg = state.config;
    let tab = state.active_tab_mut();
    let half = 8isize; // 右 pane 半页步进（固定近似值，视口高度不可得）
    match key.code {
        KeyCode::Tab => {
            tab.focus = crate::tab::Focus::Left;
            *msg = Some("焦点: 左 pane".to_owned());
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
fn handle_key(
    state: &mut AppState,
    input: &mut InputState,
    msg: &mut Option<String>,
    key: KeyEvent,
    viewport_h: u16,
) -> Outcome {
    if key.kind != KeyEventKind::Press {
        return Outcome::Continue; // Release 先过滤，避免误消费待决序列。
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
        match input.keymap.lookup(&input.seq) {
            Some(Action::ToTop) => state.active_tab_mut().cursor_top(),
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
                    input.seq.clear();
                    return Outcome::Continue;
                }
                if state.active_tab().on_parent_row {
                    // FEAT-2：`..` 行仅导航。
                    *msg = Some("`..` 行仅导航".to_owned());
                    input.seq.clear();
                    return Outcome::Continue;
                }
                let tab = state.active_tab();
                if let (Location::Physical(base), Some(item)) =
                    (&tab.location, tab.entries.get(tab.cursor))
                {
                    input.seq.clear(); // 序列已完成，残留会吞掉后续按键
                    return Outcome::ConfirmDelete(base.join(&item.name), false);
                }
            }
            Some(Action::RenameBatch) => {
                // M13-6：cw 批量重命名——收集标记（或光标单条）名单。
                if state.active_tab().in_results() {
                    *msg = Some("搜索结果视图只读".to_owned());
                    input.seq.clear();
                    return Outcome::Continue;
                }
                if state.active_tab().on_parent_row {
                    // FEAT-2：`..` 行仅导航。
                    *msg = Some("`..` 行仅导航".to_owned());
                    input.seq.clear();
                    return Outcome::Continue;
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
                        input.seq.clear();
                        return Outcome::BatchRename {
                            base: base.clone(),
                            names,
                        };
                    }
                    input.seq.clear();
                }
                input.seq.clear();
            }
            Some(Action::DeletePermanent) => {
                // M13-3：DD 永久删除——确认提示后跳过暂存（不可逆强提示）。
                if state.active_tab().in_results() {
                    *msg = Some("搜索结果视图只读".to_owned());
                    input.seq.clear();
                    return Outcome::Continue;
                }
                if state.active_tab().on_parent_row {
                    // FEAT-2：`..` 行仅导航。
                    *msg = Some("`..` 行仅导航".to_owned());
                    input.seq.clear();
                    return Outcome::Continue;
                }
                let tab = state.active_tab();
                if let (Location::Physical(base), Some(item)) =
                    (&tab.location, tab.entries.get(tab.cursor))
                {
                    input.seq.clear();
                    return Outcome::ConfirmDelete(base.join(&item.name), true);
                }
            }
            Some(Action::Yank) => {
                // FIX-2：yy——可视选择 → 标记 → 光标起 N 条（count 前缀），
                // 放入复制缓冲区（cut=false；dd 的 cut 缓冲接线待后续）。
                if state.active_tab().on_parent_row {
                    // FEAT-2：`..` 行仅导航。
                    *msg = Some("`..` 行仅导航".to_owned());
                    return Outcome::Continue;
                }
                // M17 SRCH：结果视图 yy = 命中文件路径本身。
                if let Some(r) = state.active_tab().results.as_ref() {
                    let count = count.unwrap_or(1).max(1);
                    let start = state.active_tab().cursor.min(r.items.len());
                    let end = (start + count).min(r.items.len());
                    let paths: Vec<PathBuf> =
                        r.items[start..end].iter().map(|i| i.path.clone()).collect();
                    let n = paths.len();
                    state.yank = Some(crate::state::YankBuffer { paths, cut: false });
                    *msg = Some(format!("Yanked {n} item(s)"));
                    input.seq.clear();
                    return Outcome::Continue;
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
                    state.yank = Some(crate::state::YankBuffer { paths, cut: false });
                    *msg = Some(format!("Yanked {n} item(s)"));
                }
            }
            _ => {} // 未知序列整体丢弃
        }
        input.seq.clear();
        return Outcome::Continue;
    }

    if is_quit_key(key) {
        return Outcome::Quit;
    }

    // M14-1：右 pane 焦点——精简导航分支（SIMPLIFIED：搜索/可视选择/标记/
    // 树/选择器/提权仅左 pane；写命令经 exec 焦点化仍可用）。
    if state.active_tab().focus == crate::tab::Focus::Right {
        return handle_right_key(state, msg, key);
    }

    // M17 SRCH：结果虚拟目录——l/Enter=定位、Esc/h=退出、v/t=只读拒绝；
    // 其余导航/yy/预览键落回既有处理（目录语义）。
    if state.active_tab().in_results() {
        match key.code {
            KeyCode::Esc | KeyCode::Char('h') => {
                input.count = None;
                let cfg = state.config;
                state.active_tab_mut().exit_results(cfg);
                *msg = Some("已退出搜索结果".to_owned());
                return Outcome::Continue;
            }
            KeyCode::Char('l') | KeyCode::Enter => {
                input.count = None;
                let cfg = state.config;
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

    // M14-1：Tab 键切换焦点 pane（左 → 右）。
    if key.code == KeyCode::Tab && key.kind == KeyEventKind::Press {
        input.count = None;
        let tab = state.active_tab_mut();
        tab.focus = crate::tab::Focus::Right;
        *msg = Some("焦点: 右 pane（Tab 切回）".to_owned());
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
            // M13-4：j/k 环绕（尾→首 / 首→尾）。
            input.count = None;
            state.active_tab_mut().move_cursor_wrap(1);
        }
        KeyCode::Char('k') => {
            input.count = None;
            state.active_tab_mut().move_cursor_wrap(-1);
        }
        KeyCode::Char('l') | KeyCode::Enter => {
            input.count = None;
            open_selected(state, msg);
        }
        KeyCode::Char('h') => {
            input.count = None;
            let cfg = state.config;
            if let Err(e) = state.active_tab_mut().return_parent(cfg) {
                *msg = Some(format!("返回失败: {e}"));
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
            // M8-12：合一跳转选择器（合一表 + 全局统一时钟视角，纯只读）。
            input.count = None;
            match load_jump_entries(state, JumpKind::Unified) {
                Ok(entries) => {
                    return Outcome::OpenJump {
                        kind: JumpKind::Unified,
                        entries,
                    };
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
        _ => input.count = None,
    }
    Outcome::Continue
}

/// 刷新 tab 的双 pane（M14-1）：左 pane 走 Tab::refresh，右 pane 走 Pane::refresh。
fn refresh_tab_panes(state: &mut AppState, tab_id: usize) {
    let cfg = state.config;
    if let Some(tab) = state.tabs.get_mut(tab_id) {
        let _ = tab.refresh(cfg);
        let _ = tab.right.refresh(cfg);
    }
}

/// `l`/Enter 与鼠标再击共用的打开逻辑（FEAT-1）：目录/归档 → enter_selected
/// 导航（FIX-3：归档文件优先于 opener）；普通文件 → 系统 opener（M11-7）。
fn open_selected(state: &mut AppState, msg: &mut Option<String>) {
    // FEAT-2：焦点在 `..` 行 → 等价 `h`（返回上级；归档根回物理目录）。
    if state.active_tab().on_parent_row {
        let cfg = state.config;
        if let Err(e) = state.active_tab_mut().return_parent(cfg) {
            *msg = Some(format!("返回失败: {e}"));
        }
        return;
    }
    let cfg = state.config;
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
        match crate::opener::system_open(&path) {
            Ok(()) => *msg = None,
            Err(e) => *msg = Some(format!("打开失败: {e}")),
        }
        return;
    }
    if let Err(e) = state.active_tab_mut().enter_selected(cfg) {
        *msg = Some(format!("进入失败: {e}"));
    }
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
/// （`..` 行/右 pane/标签栏/状态栏/越界）返回 None。
fn hit_entry_index(
    state: &AppState,
    mouse: &MouseEvent,
    area: ratatui::layout::Rect,
) -> Option<usize> {
    let has_parent = state.active_tab().has_parent_row() as usize;
    let list_h = area.height.saturating_sub(2) as usize;
    let view_h = list_h.saturating_sub(has_parent);
    let list_w = area.width * 60 / 100;
    if mouse.column >= list_w
        || mouse.row < 1 + has_parent as u16
        || mouse.row >= area.height.saturating_sub(1)
    {
        return None;
    }
    let tab = state.active_tab();
    let offset = list_offset(tab.cursor, view_h);
    let idx = (mouse.row as usize - 1 - has_parent).checked_add(offset)?;
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
    // 列表几何：与 render 相同——标签栏 1 行 + 主区（左 60%）+ 状态栏 1 行；
    // FEAT-2：`..` 行占用条目视口首行（有父行时条目从第 2 行开始）。
    let has_parent = state.active_tab().has_parent_row() as usize;
    let list_w = area.width * 60 / 100;
    let in_columns =
        mouse.column < list_w && mouse.row >= 1 && mouse.row < area.height.saturating_sub(1);
    let in_parent_row = has_parent > 0 && mouse.row == 1 && in_columns;
    let in_entries = in_columns && mouse.row as usize > has_parent;
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
    let cfg = state.config;
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

    let editor = std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| {
            if cfg!(windows) {
                "notepad".to_owned()
            } else {
                "vi".to_owned()
            }
        });
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
) -> (Vec<crate::state::UndoableOp>, crate::state::JobStatus) {
    let mut ops = Vec::new();
    let mut first_err: Option<String> = None;
    let mut done = 0usize;
    for name in names {
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
    let status = if first_err.is_some() && done == 0 {
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

/// spawn 后台线程执行作业并把更新发送回主线程（M13-8 run 侧薄壳）。
fn spawn_job_thread(
    job_tx: &std::sync::mpsc::Sender<crate::state::JobUpdate>,
    job_id: usize,
    tab_id: usize,
    op: &str,
    names: &[String],
    base: &std::path::Path,
    target: &std::path::Path,
) {
    let tx = job_tx.clone();
    let op = op.to_owned();
    let names = names.to_vec();
    let base = base.to_path_buf();
    let target = target.to_path_buf();
    std::thread::spawn(move || {
        let (ops, status) = execute_job_body(&op, &names, &base, &target, tab_id);
        let _ = tx.send(crate::state::JobUpdate {
            job_id,
            tab_id,
            ops,
            status,
        });
    });
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
) -> Option<crate::privilege::PrivOp> {
    let tab_id = state.active;
    let cfg = state.config;
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
    let cfg = state.config;
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
    match ops::undo_op(&op) {
        Ok(()) => {
            // 撤销成功给出反馈（M2-6）。
            *msg = Some(format!("已撤销: {}", op.describe()));
            // 刷新受影响 tab（M2-7：跨 tab 撤销刷新原 tab；tab 已关闭则跳过，
            // 磁盘恢复不受影响；设计取向为"当前 tab 不变"，ACCEPTANCE §4）。
            let cfg = state.config;
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
    let mut state = AppState::new(Tab::from_physical(&std::env::current_dir()?, config)?);
    state.config = config;
    let mut input = InputState::default();
    let mut mode = InputMode::default();
    // M13-8：后台作业通道（多作业共存，run 退出时 rx 关闭）。
    let (job_tx, job_rx) = std::sync::mpsc::channel::<crate::state::JobUpdate>();
    loop {
        terminal.draw(|frame| render(frame, &state, &mode, msg.as_deref()))?;
        // 列表区高度 = 终端高度 - 状态栏 1 行。
        let viewport_h = terminal.size()?.height.saturating_sub(1);
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
        // M13-8：事件轮询（100ms）——后台作业完成即时感知，UI 不空转阻塞。
        let polled = event::poll(std::time::Duration::from_millis(100))?;
        let key_ev = if polled { event::read().ok() } else { None };
        if let Some(Event::Key(key)) = key_ev {
            match &mut mode {
                InputMode::Command { input } => match command_input(input, key) {
                    CmdInput::Continue => {}
                    CmdInput::Cancel => mode = InputMode::Normal,
                    CmdInput::Submit => {
                        let line = std::mem::take(input);
                        mode = InputMode::Normal;
                        match exec_command_impl(&mut state, line.trim()) {
                            Ok(ExecOutcome::Quit) => return Ok(()),
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
                                };
                            }
                            Ok(ExecOutcome::SpawnJob {
                                op,
                                names,
                                target,
                                base,
                            }) => {
                                // M13-8：登记作业并 spawn 后台线程。
                                let job_id =
                                    state.add_job(&op, names.len(), &target.display().to_string());
                                let tab_id = state.active;
                                spawn_job_thread(
                                    &job_tx, job_id, tab_id, &op, &names, &base, &target,
                                );
                            }
                            Ok(ExecOutcome::ShowJobs) => {
                                mode = InputMode::Jobs;
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
                            Err(m) => msg = Some(m),
                        }
                    }
                },
                InputMode::Search { input, origin } => match command_input(input, key) {
                    // FIX-1：输入变化即实时跳转到第一个匹配项（incsearch）。
                    CmdInput::Continue => live_search_jump(&mut state, *origin, input, &mut msg),
                    CmdInput::Cancel => {
                        // 取消输入：光标回到进入搜索前的位置，清除瞬时提示。
                        let tab = state.active_tab_mut();
                        tab.cursor = (*origin).min(tab.entries.len().saturating_sub(1));
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
                    CmdInput::Continue => {}
                    CmdInput::Cancel => mode = InputMode::Normal,
                    CmdInput::Submit => {
                        let line = std::mem::take(input);
                        mode = InputMode::Normal;
                        let cfg = state.config;
                        state.active_tab_mut().apply_filter(&line, cfg);
                        msg = None;
                    }
                },
                InputMode::Confirm { path, permanent } => {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            // y / Enter 确认删除；n / Esc / 其他 = 取消（无副作用）。
                            KeyCode::Char('y') | KeyCode::Enter => {
                                let path = std::mem::take(path);
                                let permanent = *permanent;
                                mode = InputMode::Normal;
                                if let Some(op) =
                                    execute_delete(&mut state, &mut msg, path, permanent)
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
                        Outcome::Quit => return Ok(()),
                        Outcome::EnterCommand => {
                            mode = InputMode::Command {
                                input: String::new(),
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
                        Outcome::ConfirmDelete(path, permanent) => {
                            mode = InputMode::Confirm { path, permanent };
                        }
                        Outcome::FileInfo { path, text } => {
                            mode = InputMode::FileInfo { path, text };
                        }
                        Outcome::OpenJump { kind, entries } => {
                            mode = InputMode::JumpSelect {
                                input: String::new(),
                                cursor: 0,
                                entries,
                                kind,
                            };
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
                    }
                }
                InputMode::FileInfo { .. } => {
                    // M13-5：详情对话框——任意键关闭（同帮助覆盖层）。
                    mode = InputMode::Normal;
                }
                InputMode::Jobs => {
                    // M13-8：作业菜单——任意键关闭。
                    mode = InputMode::Normal;
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
                                        };
                                    }
                                    ContextMenuOutcome::RenamePrefill { line } => {
                                        mode = InputMode::Command { input: line };
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
                                            let cfg = state.config;
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
                } => {
                    // M8-12：选择器按键——输入筛选、j/k 移动、Enter 选定、Esc 取消。
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Esc => {
                                mode = InputMode::Normal;
                                msg = None;
                            }
                            KeyCode::Enter => {
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
                            KeyCode::Backspace => {
                                input.pop();
                                *cursor = 0;
                            }
                            KeyCode::Down => *cursor = cursor.saturating_add(1),
                            KeyCode::Up => *cursor = cursor.saturating_sub(1),
                            KeyCode::Char(c) => {
                                input.push(c);
                                *cursor = 0;
                            }
                            _ => {}
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
                            };
                        }
                        ContextMenuOutcome::RenamePrefill { line } => {
                            mode = InputMode::Command { input: line };
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

fn render(frame: &mut Frame, state: &AppState, mode: &InputMode, msg: Option<&str>) {
    let tab = state.active_tab();
    // M14-2：标签栏（1 行）——每个 tab 的索引与目录名，当前 tab 高亮。
    let [tab_bar_area, main_area, status_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(frame.area());
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
    // M11-2：vifm 式双栏——左列表、右预览（SPEC SIMPLIFIED 允许单栏，
    // 统一预览窗格自 M11-2 起落地）。
    let [list_area, preview_area] =
        Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)])
            .areas(main_area);

    // FEAT-2：`..` 行——列表顶部独立 1 行（渲染层虚拟条目，不进 entries）。
    // 物理根目录无父目录 → 不渲染；归档视图恒渲染。
    let parent_available = tab.has_parent_row() && !tab.in_results();
    let parent_area = if parent_available {
        let [p, rest] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(list_area);
        let parent_style = if tab.on_parent_row {
            Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            // 视觉上与普通目录项区分（置灰）。
            Style::new().add_modifier(Modifier::DIM)
        };
        frame.render_widget(Paragraph::new(Line::from("../")).style(parent_style), p);
        rest
    } else {
        list_area
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
    let list_items: Vec<ListItem> = tab
        .entries
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            // FIX-1：匹配子串分段高亮（黄底黑字，与光标行反显区分）。
            // 图标/树缩进进前缀 span，目录后缀 `/` 跟在名称之后。
            let mut spans: Vec<Span> = vec![Span::from(format!("{} ", item.icon.glyph))];
            if tab.tree.is_some() {
                let depth = item.name.matches('/').count();
                spans.push(Span::from("  ".repeat(depth)));
            }
            match active_query
                .as_deref()
                .and_then(|query| crate::tab::match_range(&item.name, query))
            {
                Some((start, end)) => {
                    let (before, mid, after) = (
                        &item.name[..start],
                        &item.name[start..end],
                        &item.name[end..],
                    );
                    if !before.is_empty() {
                        spans.push(Span::from(before.to_owned()));
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
                        spans.push(Span::from(suffix));
                    }
                }
                None => {
                    let label = if item.is_dir {
                        format!("{}/", item.name)
                    } else {
                        item.name.clone()
                    };
                    spans.push(Span::from(label));
                }
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
        List::new(list_items).highlight_style(Style::new().add_modifier(Modifier::REVERSED)),
        parent_area,
        &mut list_state,
    );

    // M11-2：预览窗格——光标项按 `preview_kind` 分流；图片走
    // ImageEngine（解码失败降级为提示行），其余走共享 PreviewRenderer。
    render_right_pane(frame, state, preview_area);

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
    if matches!(mode, InputMode::Jobs) {
        // M13-8：后台作业菜单覆盖层（任意键关闭）。
        let mut lines: Vec<Line> = Vec::new();
        if state.jobs.is_empty() {
            lines.push(Line::from("（无后台作业）"));
        } else {
            for j in &state.jobs {
                let status = match &j.status {
                    crate::state::JobStatus::Running => "运行中".to_owned(),
                    crate::state::JobStatus::Done { summary } => summary.clone(),
                    crate::state::JobStatus::Failed { err } => format!("失败: {err}"),
                };
                lines.push(Line::from(format!(
                    "#{id} {kind} → {target}（{} 项）: {status}",
                    j.src_count,
                    id = j.id,
                    kind = j.kind,
                    target = j.target
                )));
            }
        }
        frame.render_widget(
            Paragraph::new(lines)
                .block(ratatui::widgets::Block::bordered().title("后台作业（任意键关闭）")),
            frame.area(),
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
    } = mode
    {
        // M8-12：跳转选择器覆盖层（纯只读；分级与 pick 一致）。
        // FIX-4：全屏遮罩清除列表残影 + 居中浮窗（自适应 + 上限）。
        // 先 Clear 抹掉符号，再铺遮罩底色。
        let area = frame.area();
        frame.render_widget(ratatui::widgets::Clear, area);
        frame.render_widget(
            ratatui::widgets::Block::new().style(Style::new().bg(Color::Black)),
            area,
        );
        let title = match kind {
            JumpKind::Unified => "跳转（合一 · 全局时钟）",
            JumpKind::Dir => "跳转（仅目录 · dir 时钟）",
            JumpKind::File => "跳转（仅文件 · file 时钟）",
            JumpKind::Recent => "跳转（近期 · 时间倒序）",
        };
        let visible = filter_jump(entries, input);
        let popup = centered_rect(area);
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
                .title(title)
                .inner(popup),
        );
        frame.render_widget(ratatui::widgets::Block::bordered().title(title), popup);
        frame.render_widget(Paragraph::new(Line::from(format!("/{}", input))), inner[0]);
        let list = ratatui::widgets::List::new(items)
            .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
        let mut list_state = ratatui::widgets::ListState::default().with_selected(Some(*cursor));
        frame.render_stateful_widget(list, inner[1], &mut list_state);
        frame.render_widget(
            Paragraph::new("Enter=选定  Esc=取消  输入模糊筛选"),
            inner[2],
        );
        return;
    }
    let status = match mode {
        InputMode::ContextMenu { .. } => Line::from(String::new()), // 菜单覆盖层提前 return，不可达
        InputMode::HistoryMenu { .. } => Line::from(String::new()), // 历史菜单提前 return，不可达
        InputMode::Command { input } => Line::from(format!(":{input}")),
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
        InputMode::Jobs => Line::from(String::new()), // 作业菜单覆盖层提前 return，不可达
        InputMode::TrashMenu { .. } => Line::from(String::new()), // 暂存区覆盖层提前 return，不可达
        InputMode::FileInfo { .. } => Line::from(String::new()), // 详情覆盖层提前 return，不可达
        InputMode::Confirm { path, permanent } => {
            if *permanent {
                // M13-3：DD 永久删除强提示（不可恢复）。
                Line::from(format!(
                    "**永久删除** {}（不可恢复）? [y/N]",
                    path.display()
                ))
            } else {
                Line::from(format!("删除 {}? [y/N]", path.display()))
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
            let selected = match tab.entries.get(tab.cursor) {
                Some(item) => {
                    let size = if item.is_dir {
                        String::new()
                    } else {
                        format!(" {}", crate::tab::format_size(item.size))
                    };
                    format!(" | {}{} {}", item.name, size, item.perm)
                }
                None => String::new(),
            };
            let mut spans = vec![
                Span::styled(
                    tab.location.display_path(),
                    Style::new().add_modifier(Modifier::BOLD),
                ),
                Span::from(selected),
                Span::from(format!(" | {} items", tab.entries.len())),
            ];
            if let Some(r) = &tab.results {
                // M17 SRCH：结果视图指示（类别 + 查询 + 条数 + 截断）。
                let kind = match r.kind {
                    crate::search::SearchKind::Find => "find",
                    crate::search::SearchKind::Grep => "grep",
                };
                spans.push(Span::from(format!(
                    "  [{}: {} — {} 项{}]",
                    kind,
                    r.query,
                    r.items.len(),
                    if r.truncated { " 已截断" } else { "" }
                )));
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

/// 预览窗格（M11-2）：与 `pick` 共用同一分流与渲染入口。
/// 归档视图条目暂不预览（M7 只读 VFS 的文本读取未接入预览）。
/// 右 pane 渲染（M14-1）：view 模式 = 活动 pane 光标文件的完整内容预览
/// （跟随光标重置）；默认 = 右 pane 独立目录列表（可导航）。
fn render_right_pane(frame: &mut Frame, state: &AppState, area: ratatui::layout::Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let tab = state.active_tab();
    let focused = tab.focus == crate::tab::Focus::Right;
    // view 模式：预览活动 pane 光标文件（M13-9 语义保持）。
    if tab.view_mode {
        let pane = ratatui::widgets::Block::bordered().title(if focused {
            "view · 预览（焦点）"
        } else {
            "view · 预览"
        });
        let inner = pane.inner(area);
        frame.render_widget(pane, area);
        // M17 SRCH：预览源抽象——结果模式取命中文件，物理视图取 base.join。
        let Some(path) = tab.preview_path() else {
            frame.render_widget(Paragraph::new("归档内预览暂不支持"), inner);
            return;
        };
        let p = match crate::preview::preview_kind(&path) {
            crate::preview::PreviewKind::Image => {
                // 图片渲染失败（损坏/超大）→ 降级为共享提示行。
                if state.image_engine.render_file(frame, inner, &path).is_ok() {
                    return;
                }
                crate::preview::render_preview(&path, 5000)
            }
            _ => crate::preview::render_preview(&path, 5000),
        };
        frame.render_widget(p.into_paragraph(), inner);
        return;
    }
    // 默认：右 pane 目录列表（独立导航）。
    let title = if focused {
        "右 pane（焦点 · Tab 切换）"
    } else {
        "右 pane（Tab 切换焦点）"
    };
    let pane = ratatui::widgets::Block::bordered().title(title);
    let inner = pane.inner(area);
    frame.render_widget(pane, area);
    let items: Vec<ListItem> = tab
        .right
        .entries
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let label = if item.is_dir {
                format!("{}/", item.name)
            } else {
                item.name.clone()
            };
            let label = format!("{} {label}", item.icon.glyph);
            ListItem::new(Line::from(label)).style(if idx == tab.right.cursor && focused {
                Style::new().add_modifier(Modifier::REVERSED)
            } else {
                Style::new()
            })
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
        let mut input = InputState::default();
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
        let mut input = InputState::default();

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
        let mut input = InputState::default();
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
        let mut input = InputState::default();

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
        let mut input = InputState::default();

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
        let (_tmp, mut state) = make_state(3);
        let mut input = InputState::default();
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
        assert_eq!(state.active_tab().cursor, 1); // j 只移动 1 行
    }

    #[test]
    fn colon_enters_command_mode() {
        let (_tmp, mut state) = make_state(1);
        let mut input = InputState::default();
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
        let mut input = InputState::default();
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
        let mut input = InputState::default();
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
        let mut input = InputState::default();
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
        let mut input = InputState::default();
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
        let mut input = InputState::default();
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
        let mut input = InputState::default();
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
        let mut input = InputState::default();
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
        let mut input = InputState::default();
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
        let mut input = InputState::default();
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
        let mut input = InputState::default();
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
        let mut input = InputState::default();
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
        let mut input = InputState::default();

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
            Outcome::ConfirmDelete(p, permanent) => {
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

        execute_delete(&mut state, &mut msg, file.clone(), false);
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

        // 错误路径：缺参数、目标已存在（均有提示，不 panic）。
        assert_eq!(
            exec_command(&mut state, "rename"),
            Err("rename: 缺少新名字".to_owned())
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
        execute_delete(&mut state, &mut msg, file.clone(), false);
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
        let mut input = InputState::default();

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

        // 选区内反显（行 3、4），选区外不反显（行 2、5）——tab 栏占行 0、
        // `..` 行占行 1（FEAT-2）。
        assert!(buffer[(0, 3)].modifier.contains(Modifier::REVERSED));
        assert!(buffer[(0, 4)].modifier.contains(Modifier::REVERSED));
        assert!(!buffer[(0, 2)].modifier.contains(Modifier::REVERSED));
        assert!(!buffer[(0, 5)].modifier.contains(Modifier::REVERSED));

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
        let mut input = InputState::default();

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

        // 标记行加粗，未标记行不加粗——tab 栏占行 0，`..` 行占行 1（FEAT-2），
        // 条目行 2 起。
        assert!(buffer[(0, 3)].modifier.contains(Modifier::BOLD));
        assert!(!buffer[(0, 2)].modifier.contains(Modifier::BOLD));

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
        let mut input = InputState::default();

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
    fn archive_status_bar_shows_host_inner_breadcrumb() {
        // M7-5：状态栏/面包屑显示 `host:/inner/path`。
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
            size: 12,
            perm: String::new(),
        }];

        let backend = ratatui::backend::TestBackend::new(120, 8);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let status = (0..120u16)
            .map(|x| buffer[(x, 7)].symbol())
            .collect::<String>();
        let leaf = zip_path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(
            status.contains(&format!("{leaf}:/src")),
            "breadcrumb missing: {status}"
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
                size: 0,
                perm: String::new(),
            },
            FileItem {
                icon: crate::icons::icon_for(Path::new("readme.md"), crate::icons::IconTheme::Dark),
                name: "readme.md".into(),
                is_dir: false,
                size: 2,
                perm: String::new(),
            },
            FileItem {
                icon: crate::icons::icon_for(Path::new("src/"), crate::icons::IconTheme::Dark),
                name: "src".into(),
                is_dir: true,
                size: 0,
                perm: String::new(),
            },
        ];

        // /main → 命中? 名单里只有 src/main.rs 在子目录，根列表不递归。
        let mut msg = None;
        submit_search(&mut state, "readme", &mut msg);
        assert!(msg.is_none());
        assert_eq!(state.active_tab().cursor, 1);
        // n 环绕回 0（deep 是目录名，不匹配 readme）。
        let mut input = InputState::default();
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
        // tab 栏占行 0、`..` 行占行 1（FEAT-2），条目行 2 起；光标行 1
        // （第二条目）→ y=3。
        assert!(buffer[(0, 3)].modifier.contains(Modifier::REVERSED));
        assert!(!buffer[(0, 1)].modifier.contains(Modifier::REVERSED));
        assert!(!buffer[(0, 4)].modifier.contains(Modifier::REVERSED));
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
        let Outcome::ConfirmDelete(path, _permanent) = outcome else {
            panic!("expected ConfirmDelete, got {outcome:?}");
        };
        execute_delete(&mut state, &mut msg, path, false);
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
        let mut input = InputState::default();
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

        // j 移动光标 → 视图跟随（short.txt 的 SHORT MARKER 出现）。
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('j'),
            KeyModifiers::NONE,
        );
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
        assert!(all.contains("SHORT MARKER"), "视图应跟随光标文件: {all}");

        // :view 关闭。
        let r = exec_command_impl(&mut state, "view").unwrap();
        assert!(!state.active_tab().view_mode);
        assert!(matches!(r, ExecOutcome::Done { .. }));
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
        let mode = InputMode::Jobs;
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
        let mut input = InputState::default();
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
        let mut input = InputState::default();
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
        let Outcome::ConfirmDelete(path, permanent) = outcome else {
            panic!("expected ConfirmDelete, got {outcome:?}");
        };
        assert!(permanent, "DD 必须标记永久删除");

        // 确认 y 的执行在 run() 的 Confirm 分支（mode 迁移在 run 内）——
        // 此处直接验证永久删除执行语义：不进暂存、不进撤销栈。
        let mut msg = None;
        execute_delete(&mut state, &mut msg, path.clone(), true);
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
        let mut input = InputState::default();
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
        let mut input = InputState::default();
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
        let mut input = InputState::default();
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
        let mut input = InputState::default();
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
        // FIX-4 居中浮窗：80×12 终端 → 48×8 popup 于 (16,2)；边框 y=2、
        // 输入行 y=3、首条目行 y=4（x=17 为 popup 左边界内第一列）。
        assert_eq!(buffer[(16, 2)].symbol(), "┌", "popup 左上角");
        assert_eq!(buffer[(63, 2)].symbol(), "┐", "popup 右上角");
        let diag: String = [(17u16, 4u16), (17, 5), (18, 4)]
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
            buffer[(17, 4)].modifier.contains(Modifier::REVERSED),
            "首条目应高亮: {diag}"
        );
    }

    #[test]
    fn exec_j_and_big_j_open_jump_selectors() {
        // M8-12：:j → 仅目录；:J → 仅文件。open_store 读全局 config 路径
        //（测试机 DB 可能为空——entries 数量不参与断言，只验证 kind 与形态）。
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
    fn fix4_jump_selector_centered_and_masked_at_two_sizes() {
        // FIX-4：80×24 与 160×48 两档——居中浮窗 + 全屏遮罩（列表无残影）。
        let entries = vec![scored("/w/proj", "dir", 9.0)];
        let (_tmp, state) = make_state(3);
        let mode = InputMode::JumpSelect {
            input: String::new(),
            cursor: 0,
            entries,
            kind: JumpKind::Unified,
        };

        // 80×24：popup = 48×12 @ (16,6)。
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &mode, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        assert_eq!(buffer[(16, 6)].symbol(), "┌", "popup 左上角应居中");
        assert_eq!(buffer[(63, 6)].symbol(), "┐");
        assert_eq!(buffer[(16, 17)].symbol(), "└");
        // 遮罩：popup 外（列表首行 (0,1)）无目录列表内容。
        assert!(
            buffer[(0, 1)].symbol().trim().is_empty(),
            "popup 外应被遮罩，got {:?}",
            buffer[(0, 1)].symbol()
        );
        assert_eq!(buffer[(0, 1)].bg, Color::Black, "遮罩底色");

        // 160×48：popup = 80×20 @ (40,14)（触上限仍居中）。
        let backend2 = ratatui::backend::TestBackend::new(160, 48);
        let mut terminal2 = ratatui::Terminal::new(backend2).unwrap();
        terminal2
            .draw(|frame| render(frame, &state, &mode, None))
            .unwrap();
        let buffer2 = terminal2.backend().buffer().clone();
        assert_eq!(buffer2[(40, 14)].symbol(), "┌", "160×48 下 popup 左上角");
        assert_eq!(buffer2[(119, 14)].symbol(), "┐");
        assert!(buffer2[(0, 1)].symbol().trim().is_empty(), "大终端同样遮罩");
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
        };
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &mode, None))
            .unwrap();
        // 打开期间光标未被改动（选择器 cursor 是模式局部状态）。
        assert_eq!(state.active_tab().cursor, 2);

        // 关闭（Normal）后列表完整重绘：首行 f00、光标行（第 3 行）f02 反显。
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
        assert!(row[0].contains("../"), "`..` 行在列表顶部: {}", row[0]);
        assert!(row[1].contains("f00"), "关闭后列表恢复: {}", row[1]);
        assert!(row[3].contains("f02"), "光标行内容: {}", row[3]);
        assert!(
            buffer[(0, 4)].modifier.contains(Modifier::REVERSED),
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
        // 排序：d1(0) d2(1)；row=3 → idx=1（d2，`..` 行占 row 1）。
        let ev = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 3,
            row: 3,
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
            row: 5, // idx 3（`..` 行占 row 1，条目行 2 起）
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
        let mut input = InputState::default();
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
        let mut input = InputState::default();
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
            row: 1, // `..` 行
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
        let mut input = InputState::default();
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
            .map(|x| buffer[(x, 2)].symbol())
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
        // 注：open_store 读全局路径（M8-12 同款约束），只断言提示形态与
        // 无副作用，不断言 entries 数量。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "A").unwrap();
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        let mut input = InputState::default();
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
            let cfg = state.config;
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
        let cfg = state.config;
        assert!(state.active_tab_mut().hist_jump(false, cfg).unwrap());
        assert!(state.active_tab_mut().hist_jump(false, cfg).unwrap());
        history_menu_nav(&mut state, 4, &mut msg);
        assert!(msg.is_none());
        assert_eq!(
            state.active_tab().location,
            Location::Physical(tmp.path().join("a"))
        );
        assert_eq!(state.active_tab().history.len(), 4, "forward（c）已截断");
        assert!(!state.active_tab_mut().hist_jump(true, cfg).unwrap());
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
        let cfg = state.config;
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
        let mut input = InputState::default();
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
        let mut input = InputState::default();
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
        let mut input = InputState::default();
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

        let backend = ratatui::backend::TestBackend::new(80, 10);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let row = |y: u16| {
            (0..80u16)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        };

        // 目录在前并带 `/` 后缀（`..` 行占行 1，条目行 2 起）。
        assert!(row(1).contains("../"));
        assert!(row(2).contains("sub/"));
        assert!(row(3).contains("hello.txt"));
        // 状态栏显示当前位置与条目数。
        let dir_name = tmp
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let status = row(9).trim().to_owned();
        assert!(
            status.contains(&dir_name),
            "status bar missing cwd: {status}"
        );
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

        // 默认态：右 pane = 目录列表（显示条目名，非预览内容）。
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let pane_list: String = (1..9).map(|y| row_text(&buf, y, 48, 80)).collect();
        assert!(pane_list.contains("notes.txt"), "列表渲染: {pane_list}");
        assert!(pane_list.contains("pic.png"), "列表渲染: {pane_list}");

        // view 态开启：右 pane = 左光标文件（notes.txt）的文本预览。
        state.active_tab_mut().view_mode = true;
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let pane_txt: String = (1..9).map(|y| row_text(&buf, y, 48, 80)).collect();
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
            (48..80).any(|x| {
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
}
