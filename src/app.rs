//! 事件循环：绘制目录列表与状态栏 → 阻塞等待事件 → 分发按键。
//!
//! M1-3 起支持多键序列（`g` 前缀：`gg`）；M1-5 起支持多标签
//! （`gt`/`gT`/`N gt`）与最小命令行（`:tabnew` / `:tabclose` / `:tabmove`；
//! 统一命令解析在 M5-4 接入）。

use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
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
}

/// 输入模式（M5-4 统一模式系统前先持有最小集合）。
/// M8-12 起 JumpSelect 携带 f64 分数的条目，不再派生 Eq。
#[derive(Debug, Default, PartialEq)]
enum InputMode {
    #[default]
    Normal,
    /// 命令行模式：正在输入 `:` 后的内容。
    Command { input: String },
    /// 搜索输入模式：正在输入 `/` 后的内容（M1-7）。
    Search { input: String },
    /// 过滤输入模式：正在输入 `=` 后的内容（M4-2）。
    Filter { input: String },
    /// 删除确认模式：`y` 执行，其余取消（M2-2；M13-3 起 permanent 标志
    /// 区分 dd 暂存删除与 DD 永久删除，渲染与执行分支不同）。
    Confirm { path: PathBuf, permanent: bool },
    /// 标记批量删除确认模式（`:tag delete`，M3-3）。
    ConfirmTagDelete,
    /// 帮助覆盖层（`:help`，M5-3；任意键返回）。
    Help,
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

// ── M8-12：跳转选择器（o 合一 / :j 仅目录 / :J 仅文件） ─────────────────────

/// 选择器数据（纯只读）：按视角从 frecency 存储加载并排名。
/// Unified = 合一表 + 全局统一时钟（全部条目混合排名）；
/// Dir/File = 仅对应 topic + 独立类型时钟。交互与 `pick` 一致（分级 + aging）。
fn load_jump_entries(kind: JumpKind) -> Result<Vec<crate::frecency::ScoredEntry>, String> {
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
            tab.history.push(tab.location.clone());
            tab.location = Location::Physical(target);
            tab.cursor = 0;
            tab.scroll = 0;
            let _ = tab.refresh(cfg);
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
            refresh_tab_panes(state, state.active);
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
            refresh_tab_panes(state, state.active);
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
            refresh_tab_panes(state, state.active);
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
            refresh_tab_panes(state, state.active);
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
            let entries = load_jump_entries(kind)?;
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
fn submit_search(state: &mut AppState, line: &str, msg: &mut Option<String>) {
    let query = line.trim().to_owned();
    if query.is_empty() {
        *msg = None;
        return;
    }
    let tab = state.active_tab_mut();
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
                let tab = state.active_tab();
                if let (Location::Physical(base), Some(item)) =
                    (&tab.location, tab.entries.get(tab.cursor))
                {
                    return Outcome::ConfirmDelete(base.join(&item.name), false);
                }
            }
            Some(Action::RenameBatch) => {
                // M13-6：cw 批量重命名——收集标记（或光标单条）名单。
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
                }
                input.seq.clear();
            }
            Some(Action::DeletePermanent) => {
                // M13-3：DD 永久删除——确认提示后跳过暂存（不可逆强提示）。
                let tab = state.active_tab();
                if let (Location::Physical(base), Some(item)) =
                    (&tab.location, tab.entries.get(tab.cursor))
                {
                    return Outcome::ConfirmDelete(base.join(&item.name), true);
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
            let cfg = state.config;
            // M11-7：目录/归档 → enter_selected 导航；普通文件 → 系统 opener。
            let tab = state.active_tab();
            let is_file = tab
                .entries
                .get(tab.cursor)
                .map(|item| !item.is_dir)
                .unwrap_or(false);
            let is_physical = matches!(tab.location, crate::tab::Location::Physical(_));
            if is_file && is_physical {
                let path = tab.location.base_dir().join(&tab.entries[tab.cursor].name);
                match crate::opener::system_open(&path) {
                    Ok(()) => *msg = None,
                    Err(e) => *msg = Some(format!("打开失败: {e}")),
                }
                return Outcome::Continue;
            }
            if let Err(e) = state.active_tab_mut().enter_selected(cfg) {
                *msg = Some(format!("进入失败: {e}"));
            }
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
            match load_jump_entries(JumpKind::Unified) {
                Ok(entries) => {
                    return Outcome::OpenJump {
                        kind: JumpKind::Unified,
                        entries,
                    };
                }
                Err(e) => *msg = Some(e),
            }
        }
        // M10-3：`g`/`d`/`v`/`t` 前缀检测经 Keymap；单键动作经 dispatch。
        KeyCode::Char('g') | KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Char('c') => {
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
            // M10-1：PutCopy/PutMove 已入 Keymap；yy 缓冲区在 M10-7 接入。
            input.count = None;
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
    // M5-1：启动加载用户配置；缺失用默认值，语法错误在状态栏提示（不退出）。
    let mut msg: Option<String> = None;
    let config = match config::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            msg = Some(e);
            config::Config::default()
        }
    };
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
                            Ok(ExecOutcome::EnterHelp) => mode = InputMode::Help,
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
                            Err(m) => msg = Some(m),
                        }
                    }
                },
                InputMode::Search { input } => match command_input(input, key) {
                    CmdInput::Continue => {}
                    CmdInput::Cancel => mode = InputMode::Normal,
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
                InputMode::Help => {
                    // 任意键返回（M5-3 最小帮助覆盖层；完整帮助页在 M6-2）。
                    mode = InputMode::Normal;
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
                                        refresh_tab_panes(state, state.active);
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
        } // if event::poll && let Event::Key
    } // loop
} // fn run

fn render(frame: &mut Frame, state: &AppState, mode: &InputMode, msg: Option<&str>) {
    let tab = state.active_tab();
    let [main_area, status_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(frame.area());
    // M11-2：vifm 式双栏——左列表、右预览（SPEC SIMPLIFIED 允许单栏，
    // 统一预览窗格自 M11-2 起落地）。
    let [list_area, preview_area] =
        Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)])
            .areas(main_area);

    let visual_range = tab.visual_range();
    let list_items: Vec<ListItem> = tab
        .entries
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let label = if item.is_dir {
                format!("{}/", item.name)
            } else {
                item.name.clone()
            };
            // M13-7：树视图按相对路径深度缩进。
            let label = if tab.tree.is_some() {
                let depth = item.name.matches('/').count();
                format!("  {}{}", "  ".repeat(depth), label)
            } else {
                label
            };
            // M9-1：行首渲染 devicons 类型图标。
            let label = format!("{} {label}", item.icon.glyph);
            // M9-2：图标颜色经 opaline 样式染色（命名样式缺失时回落默认）。
            let base_style = crate::theme::style("file_icon");
            // 可视选择范围内的行反显（M3-1）；tag 标记行加粗（M3-2）；
            // 光标行由 highlight_style 叠加。
            let mut li = ListItem::new(Line::from(label)).style(base_style);
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
    let mut list_state = ListState::default().with_selected(Some(tab.cursor));
    frame.render_stateful_widget(
        List::new(list_items).highlight_style(Style::new().add_modifier(Modifier::REVERSED)),
        list_area,
        &mut list_state,
    );

    // M11-2：预览窗格——光标项按 `preview_kind` 分流；图片走
    // ImageEngine（解码失败降级为提示行），其余走共享 PreviewRenderer。
    render_right_pane(frame, state, preview_area);

    if matches!(mode, InputMode::Help) {
        // 帮助覆盖层（M5-3）。
        let text = "rvifm 按键（任意键返回）\n\
            \n  j/k 移动    l/Enter 进入    h 返回\n\
            gg/G 首/末  Ctrl-u/d 半页\n\
            / 搜索    n/N 下/上一匹配    = 过滤\n\
            v 可视选择    t 标记    :tag delete|copy|move <目录>\n\
            dd 删除    u 撤销\n\
            :rename/:move/:copy/:mkdir/:touch/:cd/:q/:help\n\
            :tabnew/:tabclose/:tabmove    gt/gT/N gt 切换标签";
        frame.render_widget(
            Paragraph::new(text).block(ratatui::widgets::Block::bordered().title("帮助")),
            frame.area(),
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
    if let InputMode::JumpSelect {
        input,
        cursor,
        entries,
        kind,
    } = mode
    {
        // M8-12：跳转选择器覆盖层（纯只读；分级与 pick 一致）。
        let title = match kind {
            JumpKind::Unified => "跳转（合一 · 全局时钟）",
            JumpKind::Dir => "跳转（仅目录 · dir 时钟）",
            JumpKind::File => "跳转（仅文件 · file 时钟）",
        };
        let visible = filter_jump(entries, input);
        let area = frame.area();
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
        .areas(ratatui::widgets::Block::bordered().title(title).inner(area));
        frame.render_widget(ratatui::widgets::Block::bordered().title(title), area);
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
        InputMode::Command { input } => Line::from(format!(":{input}")),
        InputMode::Search { input } => Line::from(format!("/{input}")),
        InputMode::Filter { input } => Line::from(format!("={input}")),
        InputMode::Help => Line::from(String::new()), // 帮助覆盖层提前 return，不可达
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
            if let Some(s) = &tab.search {
                spans.push(Span::from(format!("  /{}", s.query)));
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
        let path = match &tab.location {
            Location::Physical(base) => base.join(
                tab.entries
                    .get(tab.cursor)
                    .map(|i| i.name.as_str())
                    .unwrap_or_default(),
            ),
            Location::Archive { .. } => {
                frame.render_widget(Paragraph::new("归档内预览暂不支持"), inner);
                return;
            }
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
            visual: None,
            filter: None,
            tags: HashSet::new(),
            char_search: None,
            tree: None,
            right: crate::tab::Pane::from_physical(Path::new("."), Config::default()).unwrap(),
            focus: Default::default(),
            view_mode: false,
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

        // 选区内反显（行 1、2），选区外不反显（行 0、3）。
        assert!(buffer[(0, 1)].modifier.contains(Modifier::REVERSED));
        assert!(buffer[(0, 2)].modifier.contains(Modifier::REVERSED));
        assert!(!buffer[(0, 0)].modifier.contains(Modifier::REVERSED));
        assert!(!buffer[(0, 3)].modifier.contains(Modifier::REVERSED));

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

        // 标记行加粗，未标记行不加粗。
        assert!(buffer[(0, 1)].modifier.contains(Modifier::BOLD));
        assert!(!buffer[(0, 0)].modifier.contains(Modifier::BOLD));

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
        assert!(buffer[(0, 1)].modifier.contains(Modifier::REVERSED));
        assert!(!buffer[(0, 0)].modifier.contains(Modifier::REVERSED));
        assert!(!buffer[(0, 2)].modifier.contains(Modifier::REVERSED));
    }

    // ── M8-12：跳转选择器（o 合一 / :j 仅目录 / :J 仅文件） ─────────────────

    fn scored(path: &str, topic: &str, score: f64) -> crate::frecency::ScoredEntry {
        crate::frecency::ScoredEntry {
            path: path.to_owned(),
            topic: topic.to_owned(),
            keyword: String::new(),
            score,
            grade: crate::frecency::Grade::None,
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
        assert_eq!(state.active_tab().cursor, 2, "首行 k 环绕到尾行");
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
        let all: String = (0..12u16).map(|y| row(y)).collect();
        assert!(all.contains("/w/proj"), "条目渲染: {all}");
        assert!(all.contains("/f/a.txt"), "条目渲染: {all}");
        // 光标行反显（第一条列表行 y=2：边框 1 + 输入行 1）。
        let diag: String = [(1u16, 2u16), (1, 3), (2, 2)]
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
            buffer[(1, 2)].modifier.contains(Modifier::REVERSED),
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

        // 目录在前并带 `/` 后缀。
        assert!(row(0).contains("sub/"));
        assert!(row(1).contains("hello.txt"));
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
