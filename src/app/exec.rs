use super::*;

/// M18-2：解析 `:sort` 参数——逗号分隔字段，字段后可跟 asc/desc。
/// 例：`name,size desc`。空参数/未知字段/坏方向返回可读错误。
pub(crate) fn parse_sort_spec(args: &str) -> Result<Vec<crate::tab::SortSpec>, String> {
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
pub(crate) fn tmux_window_args(cmd: &str) -> Vec<String> {
    vec![
        "new-window".to_owned(),
        "-d".to_owned(),
        format!("sh -c {}", shell_quote(cmd)),
    ]
}

// AUDIT-7：shell_quote 迁往 opener::shell_quote（tmux 窗口命令与 opener
// `%s` 展开共用同一 POSIX 转义），此处 use 引入保持原调用点不变。
use crate::opener::shell_quote;

/// 复用器环境探测（生产路径）。AUDIT-14：测试一律走 `run_external_target_with`
/// 注入，宿主的 `TMUX`/`STY` 不得改变测试结果。
pub(crate) fn detect_mux() -> Option<MuxKind> {
    if std::env::var_os("TMUX").is_some() {
        Some(MuxKind::Tmux)
    } else if std::env::var_os("STY").is_some() {
        Some(MuxKind::Screen)
    } else {
        None
    }
}

pub(crate) fn run_external_target(cfg: &config::UiConfig, cmd: &str) -> ExternalRun {
    run_external_target_with(detect_mux(), cfg, cmd)
}

/// AUDIT-14：注入复用器形态的判定（`run_external_target` 与测试共用）。
/// 注意：`cfg.terminal_in_new_window=true` 且 mux 有值时会真实 spawn 复用器
/// （输出已静音）；测试只注入会短路到 `CurrentPane` 的组合。
pub(crate) fn run_external_target_with(
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
pub(crate) fn run_external_current_result(
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
pub(crate) fn suspend_execute(cmd: &str) -> (ratatui::DefaultTerminal, Option<String>) {
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

/// FEAT-3：当前焦点条目的详情（物理 → fs 元数据；归档成员 →
/// `host:/inner/name` 路径 + 成员索引元数据）。`:file` 与右键菜单同源。
pub(crate) fn current_item_info(state: &AppState) -> Option<(PathBuf, String)> {
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
pub(crate) fn history_menu_nav(state: &mut AppState, idx: usize, msg: &mut Option<String>) {
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
pub(crate) fn run_context_menu_action(
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
pub(crate) fn exec_command(state: &mut AppState, line: &str) -> Result<bool, String> {
    exec_command_impl(state, line).map(|o| matches!(o, ExecOutcome::Quit))
}

// ── M19-2：远端会话导航 ──────────────────────────────────────────────

/// M23-11：git 仓库根目录探测——从 dir 逐级向上找 .git（目录或文件，
/// worktree 的 .git 是文件）；非仓库返回 None。
pub(crate) fn git_root_of(dir: &std::path::Path) -> Option<std::path::PathBuf> {
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
pub(crate) fn substitute_vars(template: &str, state: &AppState) -> Result<String, String> {
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
pub(crate) fn run_user_command(
    state: &mut AppState,
    index: usize,
    msg: &mut Option<String>,
) -> Outcome {
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
pub(crate) fn osc52_sequence(text: &str) -> String {
    use base64ct::Encoding as _;
    format!(
        "\x1b]52;c;{}\x07",
        base64ct::Base64::encode_string(text.as_bytes())
    )
}

/// M23-10：写入剪贴板（OSC52 写 stdout；失败静默——状态栏反馈由调用方）。
/// 测试下不写 stdout（OSC52 序列噪声）；序列本身由 osc52_sequence 覆盖。
pub(crate) fn copy_to_clipboard(text: &str) {
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

/// M37-1：符号模式解析（[ugoa]*[+-=][rwxXst] 简易单段形态）——
/// 返回结果八进制 mode。基础 `cur` 为当前文件 mode。
/// POSIX only（调用点均在 cfg(unix) 块内；Windows 下不编译避免 dead-code）。
#[cfg(unix)]
fn parse_symbolic_mode(spec: &str, cur: u32) -> Option<u32> {
    // 解析 [ugoa]* 前缀（缺省 = a）。
    let (targets, rest) = {
        let mut t = 0u32;
        let mut rest = spec;
        loop {
            match rest.chars().next() {
                Some('u') => {
                    t |= 0o400 | 0o200 | 0o100;
                    rest = &rest[1..];
                }
                Some('g') => {
                    t |= 0o40 | 0o20 | 0o10;
                    rest = &rest[1..];
                }
                Some('o') => {
                    t |= 0o4 | 0o2 | 0o1;
                    rest = &rest[1..];
                }
                Some('a') => {
                    t |= 0o777;
                    rest = &rest[1..];
                }
                _ => break,
            }
        }
        if t == 0 {
            t = 0o777;
        }
        (t, rest)
    };
    let (op, perms_str) = match rest.chars().next()? {
        c @ ('+' | '-' | '=') => (c, &rest[1..]),
        _ => return None,
    };
    let mut perms = 0u32;
    for c in perms_str.chars() {
        match c {
            'r' => perms |= 0o444,
            'w' => perms |= 0o222,
            'x' | 'X' => perms |= 0o111,
            _ => return None,
        }
    }
    Some(match op {
        '+' => cur | (perms & targets),
        '-' => cur & !(perms & targets),
        '=' => (cur & !targets) | (perms & targets),
        _ => cur,
    })
}

/// M37-1：chmod 目标路径（光标条目 base 拼接）。POSIX only（同上）。
#[cfg(unix)]
fn base_dir_for_chmod(state: &AppState, name: &str) -> PathBuf {
    state.active_tab().location.base_dir().join(name)
}

/// M24-8/M29-1：执行左右 pane 目录对比并进入结果视图。
/// `by_contents` = 内容判定（SHA-256，M29-1）；返回状态栏消息。
pub(crate) fn run_compare(state: &mut AppState, by_contents: bool) -> Result<String, String> {
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
    let items = compare_panes(
        &left_base,
        &left_items,
        &right_base,
        &right_items,
        by_contents,
    );
    let (same, diff, only_l, only_r) = (
        items.iter().filter(|i| i.text == "=").count(),
        items.iter().filter(|i| i.text == "!").count(),
        items.iter().filter(|i| i.text == "<").count(),
        items.iter().filter(|i| i.text == ">").count(),
    );
    let total = items.len();
    state.active_tab_mut().enter_results(
        crate::search::SearchKind::Compare,
        if by_contents {
            "bycontents".to_owned()
        } else {
            String::new()
        },
        left_base,
        items,
        false,
    );
    Ok(format!(
        "compare{mode_note}: 相同 {same} · 不同 {diff} · 仅左 {only_l} · 仅右 {only_r}（共 {total}）",
        mode_note = if by_contents { " bycontents" } else { "" }
    ))
}

/// M30-3：`:` Tab 补全候选加权排序——候选仍来自焦点 pane 条目；
/// 有 frecency 分数（dir/file Navigate/Open 事件，只读快照）的条目
/// 分数降序在前，无记录条目字典序兜底；store 缺失/空 → 全量字典序
/// （零成本路径，与 M24-7 行为一致）。
pub(crate) fn completion_candidates(
    base: &Path,
    items: &[crate::tab::FileItem],
    scores: &std::collections::HashMap<(String, String), f64>,
) -> Vec<String> {
    let key_of = |name: &str, is_dir: bool| -> (String, String) {
        (
            if is_dir { "dir" } else { "file" }.to_owned(),
            base.join(name).to_string_lossy().replace('\\', "/"),
        )
    };
    let mut weighted: Vec<(String, f64)> = items
        .iter()
        .map(|i| {
            let s = scores
                .get(&key_of(&i.name, i.is_dir))
                .copied()
                .unwrap_or(0.0);
            (i.name.clone(), s)
        })
        .collect();
    weighted.sort_by(|a, b| {
        let (ah, bh) = (a.1 > 0.0, b.1 > 0.0);
        bh.cmp(&ah).then_with(|| match (ah, bh) {
            (true, true) => b.1.total_cmp(&a.1),
            _ => a.0.cmp(&b.0),
        })
    });
    weighted.into_iter().map(|(n, _)| n).collect()
}

/// M29-2：对比结果视图同步操作（dp/do）。`put` = dp（取回另一侧条目到
/// 当前侧，仅 `>` 条目）；否则 do（删除当前侧多出条目，仅 `<`，可恢复
/// 路径 + 撤销栈）。`!`/`=` 条目可读拒绝（copy 链路无覆盖确认流——
/// SIMPLIFIED）。操作后按原模式重建对比结果（前缀变化正确）。
pub(crate) fn compare_sync(state: &mut AppState, msg: &mut Option<String>, put: bool) {
    let kind_ok = state
        .active_tab()
        .results
        .as_ref()
        .map(|r| r.kind == crate::search::SearchKind::Compare)
        .unwrap_or(false);
    if !kind_ok {
        *msg = Some("dp/do 仅在 :compare 对比结果视图可用".to_owned());
        return;
    }
    let (by_contents, left_base, item) = {
        let tab = state.active_tab();
        let Some(r) = tab.results.as_ref() else {
            return;
        };
        let by_contents = r.query == "bycontents";
        let Location::Physical(left_base) = tab.location.clone() else {
            *msg = Some("compare: 需要 pane 为物理目录视图".to_owned());
            return;
        };
        let Some(item) = r.items.get(tab.cursor).cloned() else {
            *msg = Some("compare: 无选中对比条目".to_owned());
            return;
        };
        (by_contents, left_base, item)
    };
    let tab_id = state.active;
    let marker = item.text.as_str();
    let name = item
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if put {
        // dp：取回另一侧（`>` 条目 path 指向右侧）复制到当前侧（左）。
        if marker != ">" {
            *msg = Some(match marker {
                "<" => "dp: 该条目仅当前侧存在（另一侧无内容可取回；删除当前侧用 do）".to_owned(),
                "!" => "dp: 两侧内容不同——不支持覆盖同步（SIMPLIFIED），请手动处理".to_owned(),
                _ => "dp: 两侧相同，无需同步".to_owned(),
            });
            return;
        }
        match crate::ops::copy_to(tab_id, &item.path, &left_base) {
            Ok(op) => {
                state.undo_stack.push(op);
                *msg = Some(format!("已取回 {name} 到当前侧（u 可撤销）"));
            }
            Err(e) => {
                *msg = Some(format!("dp 失败: {e}"));
                return;
            }
        }
    } else {
        // do：删除当前侧多出条目（`<` 条目 path 指向左侧当前侧；
        // 可恢复路径 = 应用内暂存 + 撤销栈）。
        if marker != "<" {
            *msg = Some(match marker {
                ">" => "do: 该条目仅另一侧存在（当前侧无此文件）".to_owned(),
                "!" => "do: 两侧内容不同——不支持覆盖同步（SIMPLIFIED），请手动处理".to_owned(),
                _ => "do: 两侧相同，无需删除".to_owned(),
            });
            return;
        }
        let mut m = None;
        let escalate = execute_delete(state, &mut m, item.path.clone(), false, None);
        if escalate.is_some() {
            *msg = Some("do: 权限不足（可 l 定位后在普通视图删除）".to_owned());
            return;
        }
        if let Some(e) = m {
            *msg = Some(format!("do 失败: {e}"));
            return;
        }
        *msg = Some(format!("已删除当前侧 {name}（暂存可恢复，u 可撤销）"));
    }
    // 操作后按原模式重建对比结果（左 pane 列表先刷新）；
    // 操作消息与刷新摘要合并展示。
    let op_msg = msg.clone().unwrap_or_default();
    refresh_tab_panes(state, tab_id);
    match run_compare(state, by_contents) {
        Ok(m) => *msg = Some(format!("{op_msg}；{m}")),
        Err(e) => *msg = Some(format!("{op_msg}；重建失败: {e}")),
    }
}

/// M28-2：结构化重命名变换形态。
#[derive(Debug, Clone)]
enum RenameTransform {
    /// `s/<pat>/<repl>/[i]`（regex 替换，i = 大小写不敏感）。
    Regex(regex::Regex, String),
    /// `prefix=<str>`：前缀插入。
    Prefix(String),
    /// `suffix=<str>`：后缀追加。
    Suffix(String),
    /// `seq=<start>,<step>,<width>`：序号替换主干、保留扩展名。
    Seq { start: i64, step: i64, width: usize },
}

/// M28-2：参数是否为结构化形态（判定保守——`s-x` 这类 plain 名字
/// 不误判；`s` 形态需分隔符切出 >=2 段；已知键 = 前缀才算）。
fn looks_structured_rename(arg: &str) -> bool {
    if arg.starts_with("prefix=") || arg.starts_with("suffix=") || arg.starts_with("seq=") {
        return true;
    }
    arg.strip_prefix('s').is_some_and(|rest| {
        rest.chars()
            .next()
            .is_some_and(|d| !d.is_alphanumeric() && !d.is_whitespace())
            && rest[rest.chars().next().unwrap().len_utf8()..]
                .split(rest.chars().next().unwrap())
                .count()
                >= 2
    })
}

/// M28-2：解析结构化 `:rename` 参数（非 plain 名字形态）。
/// 形态判定：`s` + 非字母数字分隔符、`prefix=`、`suffix=`、`seq=`。
fn parse_structured_rename(arg: &str) -> Result<RenameTransform, String> {
    // s 形态判定先行：`s` + 非字母数字分隔符才是 s///（否则 seq= 等以 s
    // 开头的形态落后续分支，不能在此误报"未知形态"）。
    let s_form = arg
        .strip_prefix('s')
        .and_then(|rest| rest.chars().next())
        .map(|d| !d.is_alphanumeric() && !d.is_whitespace())
        .unwrap_or(false);
    if s_form {
        let rest = &arg[1..];
        let delim = rest.chars().next().unwrap();
        let segs: Vec<&str> = rest[delim.len_utf8()..].split(delim).collect();
        match segs.len() {
            2 => {
                let re =
                    regex::Regex::new(segs[0]).map_err(|e| format!("rename: 非法正则: {e}"))?;
                Ok(RenameTransform::Regex(re, segs[1].to_owned()))
            }
            3 => {
                if !segs[2].is_empty() && segs[2] != "i" {
                    return Err(format!("rename: 未知标志 {}（仅支持 i）", segs[2]));
                }
                let pattern = if segs[2] == "i" {
                    format!("(?i){}", segs[0])
                } else {
                    segs[0].to_owned()
                };
                let re =
                    regex::Regex::new(&pattern).map_err(|e| format!("rename: 非法正则: {e}"))?;
                Ok(RenameTransform::Regex(re, segs[1].to_owned()))
            }
            _ => Err(format!(
                "rename: s 形态段数错误（:rename s/<pat>/<repl>/[i]）: {arg}"
            )),
        }
    } else if let Some(v) = arg.strip_prefix("prefix=") {
        Ok(RenameTransform::Prefix(v.to_owned()))
    } else if let Some(v) = arg.strip_prefix("suffix=") {
        Ok(RenameTransform::Suffix(v.to_owned()))
    } else if let Some(v) = arg.strip_prefix("seq=") {
        let parts: Vec<&str> = v.split(',').collect();
        if parts.len() != 3 {
            return Err("rename: seq 形态需 <start>,<step>,<width>".to_owned());
        }
        let start: i64 = parts[0]
            .trim()
            .parse()
            .map_err(|_| "rename: seq start 非法整数".to_owned())?;
        let step: i64 = parts[1]
            .trim()
            .parse()
            .map_err(|_| "rename: seq step 非法整数".to_owned())?;
        let width: usize = parts[2]
            .trim()
            .parse()
            .map_err(|_| "rename: seq width 非法整数".to_owned())?;
        if start < 0 || step < 1 || width == 0 || width > 10 {
            return Err("rename: seq 需 start>=0、step>=1、1<=width<=10".to_owned());
        }
        Ok(RenameTransform::Seq { start, step, width })
    } else {
        Err(format!(
            "rename: 未知参数形态（支持 s/<pat>/<repl>/[i]、prefix=、suffix=、seq=）: {arg}"
        ))
    }
}

/// M28-2：应用变换到单个名字（`index` 为选集内序号，seq 用）。
fn apply_rename_transform(t: &RenameTransform, name: &str, index: usize) -> String {
    match t {
        RenameTransform::Regex(re, repl) => re.replace_all(name, repl.as_str()).into_owned(),
        RenameTransform::Prefix(p) => format!("{p}{name}"),
        RenameTransform::Suffix(s) => format!("{name}{s}"),
        RenameTransform::Seq { start, step, width } => {
            // 序号替换主干、保留扩展名（photo.jpg -> 001.jpg；无扩展名 -> 001）。
            let num = start + (index as i64) * step;
            match name.rsplit_once('.') {
                Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => {
                    let _ = stem;
                    format!("{num:0width$}.{ext}")
                }
                _ => format!("{num:0width$}"),
            }
        }
    }
}

/// M28-2：预览计划条目——`skip` 为 None 表示将重命名。
pub(crate) struct RenamePlanItem {
    pub old: PathBuf,
    pub new: String,
    pub skip: Option<&'static str>,
}

/// M28-2：构建结构化重命名计划（选集顺序；跳过 = 无变化/冲突/非法）。
fn rename_preview_plan(state: &AppState, arg: &str) -> Result<Vec<RenamePlanItem>, String> {
    let transform = parse_structured_rename(arg)?;
    let base = physical_base(state)?;
    let names = selected_names(state);
    if names.is_empty() {
        return Err("rename: 无选中条目".to_owned());
    }
    let mut plan = Vec::new();
    let mut actionable = 0usize;
    for (idx, name) in names.iter().enumerate() {
        let new = apply_rename_transform(&transform, name, idx);
        let (new, skip) = if new == *name {
            (name.to_owned(), Some("无变化，跳过"))
        } else if new.contains('/') || new.contains('\\') || new.is_empty() {
            (new, Some("非法名字，跳过"))
        } else if base.join(&new).exists() {
            (new, Some("冲突：目标已存在，跳过"))
        } else {
            actionable += 1;
            (new, None)
        };
        plan.push(RenamePlanItem {
            old: base.join(name),
            new,
            skip,
        });
    }
    if actionable == 0 {
        return Err("rename: 预览无可重命名条目（全部跳过）".to_owned());
    }
    Ok(plan)
}

/// M28-2：应用预览计划（y 键）——逐项 rename_to 入撤销栈（`u` 逐项可逆，
/// M13-6 口径），失败项跳过并计数；应用后退出预览并刷新。
pub(crate) fn apply_rename_preview(state: &mut AppState, msg: &mut Option<String>) {
    let Some(plan) = state.active_tab().pending_rename.clone() else {
        *msg = Some("rename: 无待应用的预览计划".to_owned());
        return;
    };
    let tab_id = state.active;
    let mut applied = 0usize;
    let mut failed: Vec<String> = Vec::new();
    for (old, new) in &plan {
        match ops::rename_to(tab_id, old, new) {
            Ok(op) => {
                state.undo_stack.push(op);
                applied += 1;
            }
            Err(e) => failed.push(format!("{}: {e}", old.display())),
        }
    }
    let cfg = state.config.clone();
    state.active_tab_mut().pending_rename = None;
    state.active_tab_mut().exit_results(cfg);
    refresh_tab_panes(state, tab_id);
    *msg = if failed.is_empty() {
        Some(format!("已重命名 {applied} 项"))
    } else {
        Some(format!(
            "已重命名 {applied} 项，失败 {} 项: {}",
            failed.len(),
            failed.join("; ")
        ))
    };
}

/// M23-10：复制目标的选集——标记优先（按列表序），其次可视范围，最后
/// 光标单条目（与 cw/R 的选集语义一致）。
pub(crate) fn selected_names(state: &AppState) -> Vec<String> {
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
pub(crate) fn editor_name() -> String {
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
pub(crate) fn remote_locate_result(state: &mut AppState) -> Result<(), String> {
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
pub(crate) fn remote_open_file(
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
pub(crate) fn spawn_upload_watcher(
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
pub(crate) fn remote_download_copy(
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
pub(crate) fn cleanup_temp_opens(state: &mut AppState) {
    for p in state.temp_opens.drain(..) {
        let _ = std::fs::remove_file(p);
    }
}

/// M22-3：远端口令连接应用（RemotePass 模式 Enter 共用体）——连接成功
/// 注册会话并进入；失败返回可读错误（重试经 :remote 重新发起）。
pub(crate) fn remote_pass_apply(
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
pub(crate) fn current_remote_path(state: &AppState) -> Option<PathBuf> {
    match &state.active_tab().location {
        Location::Remote { path, .. } => Some(path.clone()),
        _ => None,
    }
}

/// M32-1：远端操作连接类错误自动重连——`f` 以当前会话执行；返回连接
/// 类错误（Connect/Timeout）时按会话原参数重连一次，成功则替换会话并
/// 以新后端重试 `f`；重连失败/非连接类错误原样上浮。会话不在表（已
/// 断开无参数可解析）→ "会话已断开"。
pub(crate) fn remote_with_reconnect<R>(
    state: &mut AppState,
    name: &str,
    f: impl Fn(&dyn crate::remote::RemoteFs) -> Result<R, crate::remote::RemoteError>,
) -> Result<R, String> {
    // 第一遍：现有会话。
    let first = match state.remote_sessions.get(name) {
        Some(b) => f(b.as_ref()),
        None => return Err("remote: 会话已断开".to_owned()),
    };
    match first {
        Ok(v) => Ok(v),
        Err(e) if e.is_connection_class() => {
            // downcast 拿 SftpBackend（含 params/password 副本）重连；
            // Mock 测试后端无重连语义——原样放回并上浮错误。
            let any = state.remote_sessions.remove(name);
            let Some(b) = any else {
                return Err(format!("remote: {e}"));
            };
            match b
                .as_any()
                .and_then(|a| a.downcast_ref::<crate::remote::sftp::SftpBackend>())
            {
                Some(sb) => match sb.reconnect_from_params() {
                    Ok(nb) => {
                        state.remote_sessions.insert(name.to_owned(), Box::new(nb));
                    }
                    Err(e2) => {
                        state.remote_sessions.insert(name.to_owned(), b);
                        return Err(format!("remote: 重连失败: {e2}（原错误: {e}）"));
                    }
                },
                None => {
                    state.remote_sessions.insert(name.to_owned(), b);
                    return Err(format!("remote: {e}（该会话类型不支持自动重连）"));
                }
            }
            // 重连成功 → 替换会话并重试操作。
            let retry = match state.remote_sessions.get(name) {
                Some(b) => f(b.as_ref()),
                None => return Err("remote: 会话已断开".to_owned()),
            };
            retry.map_err(|e2| format!("remote: 重连后仍失败: {e2}"))
        }
        Err(e) => Err(format!("remote: {e}")),
    }
}

/// M32-3：远端条目加载（会话恢复用；remote_navigate 的数据段同源）。
/// 失败返回可读 Err（恢复调用方跳过该 tab）。
pub(crate) fn remote_load_entries(
    state: &mut AppState,
    name: &str,
    path: &std::path::Path,
) -> Result<Vec<crate::tab::FileItem>, String> {
    let items = remote_with_reconnect(state, name, |b| b.list(path))?;
    Ok(items
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
        .collect())
}

pub(crate) fn remote_navigate(state: &mut AppState, path: &std::path::Path) -> Result<(), String> {
    // M32-1：list 经重连包装（连接类错误自动重连一次后重试）。
    let (name, items) = {
        let tab = state.active_tab();
        let Location::Remote { remote, .. } = &tab.location else {
            return Err("remote: 非远端视图".to_owned());
        };
        let name = remote.clone();
        let path_buf = path.to_path_buf();
        let items = remote_with_reconnect(state, &name, |b| b.list(&path_buf))?;
        (name, items)
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
pub(crate) fn hist_move(
    state: &mut AppState,
    forward: bool,
    msg: &mut Option<String>,
) -> Result<(), String> {
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
pub(crate) fn physical_base(state: &AppState) -> Result<PathBuf, String> {
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

// ── M16 NAV-1 / M25-2：' 快速跳转与 marks/bookmark（frecency keyword） ──────

/// NAV-1/M25-2 纯函数：从重放条目中取 keyword 精确匹配的**最高排名条目**
/// （目录与文件都参与——跳转语义由 `jump_to` 按 topic 分派：目录 cd、
/// 文件父目录定位光标）。候选 keyword 相同 → 档位相同（Grade 6 书签）
/// → 分数即唯一区分；平分按路径字典序取前（确定性）。注意 cmp_ranked
/// 是降序比较器，不能直接喂 max_by。
pub(crate) fn quickjump_target<'a>(
    entries: &'a [crate::frecency::ScoredEntry],
    keyword: &str,
) -> Option<&'a crate::frecency::ScoredEntry> {
    entries
        .iter()
        .filter(|e| e.keyword == keyword)
        .max_by(|a, b| a.score.total_cmp(&b.score).then(b.path.cmp(&a.path)))
}

/// `'`+字符：keyword 直达——配置别名优先，缺省即字符本身。
/// 跳转语义由 `jump_to` 分派：目录 cd（等价 :cd）、文件父目录定位光标
/// （M8-12 语义）；未命中提示不静默。
pub(crate) fn quickjump(state: &mut AppState, c: char, msg: &mut Option<String>) {
    let target_kw = config::quickjump_alias(c).unwrap_or_else(|| c.to_string());
    let entries = match load_jump_entries(state, JumpKind::Unified) {
        Ok(entries) => entries,
        Err(e) => {
            *msg = Some(e);
            return;
        }
    };
    let Some(entry) = quickjump_target(&entries, &target_kw) else {
        *msg = Some(format!(
            "无 keyword {target_kw} 的条目（m<char> / :keyword / :bmark 设置）"
        ));
        return;
    };
    let entry = entry.clone();
    match jump_to(state, &entry) {
        Ok(m) => *msg = Some(m),
        Err(e) => *msg = Some(e),
    }
}

// ── M25-2：marks / bookmark（frecency keyword 数据的命令视图） ───────────────

/// marks 合法字符：字母与数字（vifm marks 口径；多字符 tag 亦限字母数字，
/// 多字符 keyword 在 `o`/`:j`/`:J` 搜索中享有高优先级匹配）。
pub(crate) fn valid_mark_keyword(kw: &str) -> bool {
    !kw.is_empty() && kw.chars().all(|c| c.is_ascii_alphanumeric())
}

/// M25-2：`m<char>`——当前光标条目以 keyword=<char> 写入 frecency redb
/// （与 bookmark/keyword 共用 `EntryRecord.keyword`；`..` 行 = 当前目录）。
/// 仅物理视图（归档/远端/结果/树视图不记录）。
pub(crate) fn mark_set(state: &mut AppState, c: char, msg: &mut Option<String>) {
    if !c.is_ascii_alphanumeric() {
        *msg = Some(format!("marks 字符限字母与数字（收到 {c}）"));
        return;
    }
    let tab = state.active_tab();
    if !matches!(tab.location, crate::tab::Location::Physical(_)) {
        *msg = Some("marks 仅物理视图可用（归档/远端/结果/树视图不记录）".to_owned());
        return;
    }
    let path = if tab.on_parent_row {
        tab.location.base_dir().to_path_buf()
    } else if let Some(item) = tab.entries.get(tab.cursor) {
        tab.location.base_dir().join(&item.name)
    } else {
        *msg = Some("无选中条目".to_owned());
        return;
    };
    let is_dir = tab.on_parent_row
        || tab
            .entries
            .get(tab.cursor)
            .map(|i| i.is_dir)
            .unwrap_or(true);
    let topic = if is_dir { "dir" } else { "file" };
    let path_str = path.to_string_lossy().replace('\\', "/");
    match crate::frecency::set_keyword_topic_now(&path_str, topic, &c.to_string()) {
        Ok(()) => *msg = Some(format!("已标记 {c}: {path_str}")),
        Err(e) => *msg = Some(format!("标记失败: {e}")),
    }
}

/// M25-2：`:bmark <tag> [tag2...]`——为当前目录设置书签 tag（物理视图限定）。
pub(crate) fn bmark_set(state: &mut AppState, tags: &[String]) -> Result<String, String> {
    if tags.is_empty() {
        return Err("bmark: 缺少 tag（:bmark <tag> [tag2...]）".to_owned());
    }
    for t in tags {
        if !valid_mark_keyword(t) {
            return Err(format!("bmark: tag `{t}` 限字母与数字"));
        }
    }
    let tab = state.active_tab();
    if !matches!(tab.location, crate::tab::Location::Physical(_)) {
        return Err("bmark 仅物理视图可用（远端/归档不记录）".to_owned());
    }
    let path = tab.location.base_dir().to_string_lossy().replace('\\', "/");
    for t in tags {
        crate::frecency::set_keyword_topic_now(&path, "dir", t)
            .map_err(|e| format!("bmark 失败: {e}"))?;
    }
    Ok(format!("已标记 {}: {}", tags.join(" "), path))
}

/// M25-2：`:bmarks [tag...]`——列出书签（无参 = 全部；有参 = 指定 tag）。
pub(crate) fn bmarks_list(state: &mut AppState, tags: &[String]) -> Result<String, String> {
    let entries = load_jump_entries(state, JumpKind::Unified)?;
    let mut lines: Vec<String> = Vec::new();
    for e in &entries {
        if e.keyword.is_empty() {
            continue;
        }
        if !tags.is_empty() && !tags.contains(&e.keyword) {
            continue;
        }
        lines.push(format!("{} {}", e.keyword, e.path));
    }
    if lines.is_empty() {
        return Ok("无书签（m<char> / :bmark 设置）".to_owned());
    }
    Ok(lines.join("；"))
}

/// M25-2：`:bmgo <tag> [tag2...]`——按序取首个有书签条目的最高排名项跳转
/// （目录 cd / 文件定位，同 `'<char>` 语义）。
pub(crate) fn bmgo(state: &mut AppState, tags: &[String]) -> Result<String, String> {
    if tags.is_empty() {
        return Err("bmgo: 缺少 tag（:bmgo <tag> [tag2...]）".to_owned());
    }
    let entries = load_jump_entries(state, JumpKind::Unified)?;
    for t in tags {
        if let Some(entry) = quickjump_target(&entries, t) {
            let entry = entry.clone();
            return jump_to(state, &entry);
        }
    }
    Err(format!(
        "无 tag {} 的书签（:bmarks 查看；m<char> / :bmark 设置）",
        tags.join(" ")
    ))
}

/// M25-2：`:delbmarks [tag...]`（无参 = 全部书签）/ `:delbmarks! <path...>`
/// （按路径清除）。清除 = 追加空 keyword 事件（保留累积分数，M23-2 语义）。
pub(crate) fn delbmarks(
    state: &mut AppState,
    force_paths: &[String],
    tags: &[String],
) -> Result<String, String> {
    let entries = load_jump_entries(state, JumpKind::Unified)?;
    if !force_paths.is_empty() {
        let mut cleared = 0usize;
        for p in force_paths {
            for e in entries
                .iter()
                .filter(|e| &e.path == p && !e.keyword.is_empty())
            {
                crate::frecency::set_keyword_topic_now(&e.path, &e.topic, "")
                    .map_err(|err| format!("delbmarks 失败: {err}"))?;
                cleared += 1;
            }
        }
        if cleared == 0 {
            return Err(format!("路径无书签: {}", force_paths.join(" ")));
        }
        return Ok(format!("已清除 {cleared} 条书签"));
    }
    let targets: Vec<crate::frecency::ScoredEntry> = entries
        .iter()
        .filter(|e| !e.keyword.is_empty() && (tags.is_empty() || tags.contains(&e.keyword)))
        .cloned()
        .collect();
    if targets.is_empty() {
        return Err("无书签可清除".to_owned());
    }
    let mut cleared = 0usize;
    for e in &targets {
        crate::frecency::set_keyword_topic_now(&e.path, &e.topic, "")
            .map_err(|err| format!("delbmarks 失败: {err}"))?;
        cleared += 1;
    }
    Ok(format!("已清除 {cleared} 条书签"))
}

// ── M8-12：跳转选择器（o 合一 / :j 仅目录 / :J 仅文件） ─────────────────────

/// 选择器数据（纯只读）：按视角从 frecency 存储加载并排名。
/// Unified = 合一表 + 全局统一时钟（全部条目混合排名）；
/// Dir/File = 仅对应 topic + 独立类型时钟。交互与 `pick` 一致（分级 + aging）。
pub(crate) fn load_jump_entries(
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
            let now_cmd = store
                .type_tick("cmd")
                .map_err(|e| format!("时钟读取失败: {e}"))?;
            let topic = if kind == JumpKind::Dir { "dir" } else { "file" };
            crate::frecency::replay(&snapshot, &events, now_dir, now_file, now_cmd)
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
pub(crate) fn merge_session_visits(
    entries: &mut Vec<crate::frecency::ScoredEntry>,
    state: &AppState,
) {
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
pub(crate) fn filter_jump(
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
pub(crate) fn jump_to(
    state: &mut AppState,
    entry: &crate::frecency::ScoredEntry,
) -> Result<String, String> {
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
pub(crate) fn exec_command_impl(state: &mut AppState, line: &str) -> Result<ExecOutcome, String> {
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
                | "compare" | "sync" | "theme" | "bmark" | "bmarks" | "bmgo" | "delbmarks"
                | "queue",
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
                | "touch" | "copy" | "move" | "mk" | "theme" | "queue",
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
            // M29-2：compare 重执行豁免——dp/do 后可重跑对比（结果替换）。
            Some("find" | "grep" | "sort" | "compare" | "q" | "quit" | "help" | "theme") => {}
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
        Some("bmark") => {
            // M25-2：为当前目录设置书签 tag（vifm 对齐；与 marks 共用数据）。
            let tags: Vec<String> = parts.map(|s| s.to_owned()).collect();
            let m = bmark_set(state, &tags)?;
            Ok(ExecOutcome::Done { msg: Some(m) })
        }
        Some("bmarks") => {
            // M25-2：列出书签（无参 = 全部；有参 = 指定 tag）。
            let tags: Vec<String> = parts.map(|s| s.to_owned()).collect();
            let m = bmarks_list(state, &tags)?;
            Ok(ExecOutcome::Done { msg: Some(m) })
        }
        Some("bmgo") => {
            // M25-2：按 tag 跳转（首个命中 tag 的最高排名条目）。
            let tags: Vec<String> = parts.map(|s| s.to_owned()).collect();
            let m = bmgo(state, &tags)?;
            Ok(ExecOutcome::Done { msg: Some(m) })
        }
        Some("delbmarks") => {
            // M25-2：`:delbmarks [tag...]` / `:delbmarks! <path...>`。
            let args: Vec<String> = parts.map(|s| s.to_owned()).collect();
            let (force_paths, tags) = match args.split_first() {
                Some((bang, rest)) if bang == "!" => (rest.to_vec(), Vec::new()),
                _ => (Vec::new(), args),
            };
            let m = delbmarks(state, &force_paths, &tags)?;
            Ok(ExecOutcome::Done { msg: Some(m) })
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
        Some("frecency") => {
            // M38-4：frecency 管理——stats/export/purge/prune。
            match parts.next() {
                Some("stats") => {
                    let store = match crate::frecency::open_store_readonly() {
                        Ok(Some(s)) => s,
                        Ok(None) => {
                            return Ok(ExecOutcome::Done {
                                msg: Some("frecency: 无数据（库缺失）".to_owned()),
                            });
                        }
                        Err(e) => return Err(format!("frecency: {e}")),
                    };
                    let snapshot = store.snapshot_all().map_err(|e| format!("frecency: {e}"))?;
                    let dir_n = snapshot.iter().filter(|e| e.topic == "dir").count();
                    let file_n = snapshot.iter().filter(|e| e.topic == "file").count();
                    let cmd_n = snapshot.iter().filter(|e| e.topic == "cmd").count();
                    let kw_n = snapshot.iter().filter(|e| !e.keyword.is_empty()).count();
                    Ok(ExecOutcome::Done {
                        msg: Some(format!(
                            "frecency: dir {dir_n} · file {file_n} · cmd {cmd_n} · 书签 {kw_n}"
                        )),
                    })
                }
                Some("export") => {
                    let path = parts
                        .next()
                        .ok_or_else(|| "frecency: export 缺少文件路径".to_owned())?;
                    let store = match crate::frecency::open_store_readonly() {
                        Ok(Some(s)) => s,
                        Ok(None) => return Err("frecency: 无数据".to_owned()),
                        Err(e) => return Err(format!("frecency: {e}")),
                    };
                    let snapshot = store.snapshot_all().map_err(|e| format!("frecency: {e}"))?;
                    let json = serde_json::to_string_pretty(&snapshot)
                        .map_err(|e| format!("frecency: 序列化失败: {e}"))?;
                    std::fs::write(path, json).map_err(|e| format!("frecency: 导出失败: {e}"))?;
                    Ok(ExecOutcome::Done {
                        msg: Some(format!("frecency: 已导出 {} 条到 {path}", snapshot.len())),
                    })
                }
                Some("purge") => {
                    // 全清（确认走现有 Confirm 模式过重——直接执行 + 不可恢复提示）。
                    let db_path = crate::config::frecency_db_path();
                    std::fs::remove_file(&db_path)
                        .map_err(|e| format!("frecency: 清除失败: {e}"))?;
                    Ok(ExecOutcome::Done {
                        msg: Some("frecency: 已清除全部数据（不可恢复）".to_owned()),
                    })
                }
                Some(other) => Err(format!(
                    "frecency: 未知子命令 {other}（stats/export/purge）"
                )),
                None => Err("frecency: 需要子命令（stats/export/purge）".to_owned()),
            }
        }
        Some("queue") => {
            // M36-7：传输队列覆盖层（j/k 选择、x 取消、Esc 关闭）。
            Ok(ExecOutcome::EnterQueue { cursor: 0 })
        }
        Some("compare") => {
            // M24-8：左右 pane 目录对比（缺省按文件名+大小）→ 结果虚拟目录。
            // M29-1：`:compare bycontents` 内容判定（SHA-256，护栏内）；
            // `:compare byname` 显式等价缺省；未知模式可读错误。
            let mode = parts.next().unwrap_or("byname");
            if mode != "byname" && mode != "bycontents" {
                return Err(format!(
                    "compare: 未知模式 {mode}（缺省 byname；内容对比用 bycontents）"
                ));
            }
            let msg = run_compare(state, mode == "bycontents")?;
            Ok(ExecOutcome::Done { msg: Some(msg) })
        }
        Some("du") => {
            // M37-4：磁盘占用——当前目录递归聚合（并行省略——顺序遍历
            // 万条上限内可接受；SIMPLIFIED）；结果消息 top-5 + 总计。
            let base = physical_base(state)?;
            let mut entries: Vec<(String, u64)> = Vec::new();
            let mut total: u64 = 0;
            let mut stack = vec![base.clone()];
            let mut count = 0usize;
            while let Some(dir) = stack.pop() {
                if count >= 10_000 {
                    break;
                }
                let Ok(rd) = std::fs::read_dir(&dir) else {
                    continue;
                };
                for e in rd.flatten() {
                    count += 1;
                    let meta = match e.metadata() {
                        Ok(m) => m,
                        Err(_) => continue,
                    };
                    if meta.is_dir() {
                        stack.push(e.path());
                    } else {
                        total += meta.len();
                        if !e.path().eq(&base) {
                            entries.push((
                                e.path()
                                    .strip_prefix(&base)
                                    .map(|p| p.to_string_lossy().into_owned())
                                    .unwrap_or_default(),
                                meta.len(),
                            ));
                        }
                    }
                }
            }
            entries.sort_by_key(|e| std::cmp::Reverse(e.1));
            let top: Vec<String> = entries
                .iter()
                .take(5)
                .map(|(n, s)| format!("  {n}: {}", crate::tab::format_size(*s)))
                .collect();
            Ok(ExecOutcome::Done {
                msg: Some(format!(
                    "du: 总计 {}（{} 项扫描）\n{}",
                    crate::tab::format_size(total),
                    count,
                    top.join("\n")
                )),
            })
        }
        Some("dup") => {
            // M37-5：查重——size 分组 → 全文 SHA-256 精确比对（万条上限）。
            let base = physical_base(state)?;
            let mut files: Vec<(PathBuf, u64)> = Vec::new();
            let mut stack = vec![base.clone()];
            let mut count = 0usize;
            while let Some(dir) = stack.pop() {
                if count >= 10_000 {
                    break;
                }
                let Ok(rd) = std::fs::read_dir(&dir) else {
                    continue;
                };
                for e in rd.flatten() {
                    count += 1;
                    if let Ok(meta) = e.metadata() {
                        if meta.is_dir() {
                            stack.push(e.path());
                        } else if meta.len() > 0 {
                            files.push((e.path(), meta.len()));
                        }
                    }
                }
            }
            use sha2::{Digest, Sha256};
            let mut hash_map: std::collections::HashMap<Vec<u8>, Vec<String>> =
                std::collections::HashMap::new();
            for (path, _size) in &files {
                let Ok(data) = std::fs::read(path) else {
                    continue;
                };
                let mut h = Sha256::new();
                h.update(&data);
                let key = h.finalize().to_vec();
                hash_map
                    .entry(key)
                    .or_default()
                    .push(path.display().to_string().replace('\\', "/"));
            }
            let dups: Vec<&Vec<String>> = hash_map.values().filter(|v| v.len() > 1).collect();
            let n_files: usize = dups.iter().map(|v| v.len()).sum();
            let listing: Vec<String> = dups
                .iter()
                .take(5)
                .map(|v| format!("  {} 份", v.len()))
                .collect();
            Ok(ExecOutcome::Done {
                msg: Some(format!(
                    "dup: {} 组重复（{} 个文件）\n{}",
                    dups.len(),
                    n_files,
                    listing.join("\n")
                )),
            })
        }
        Some("checksum") => {
            // M37-6：校验和——选中项生成 sidecar（md5sum -c 兼容）。
            let names = selected_names(state);
            if names.is_empty() {
                return Err("checksum: 无选中条目".to_owned());
            }
            let base = physical_base(state)?;
            let mut lines = Vec::new();
            for name in &names {
                let p = base.join(name);
                match std::fs::read(&p) {
                    Ok(data) => {
                        use md5::Md5;
                        use sha2::Digest as _;
                        let mut h = Md5::new();
                        h.update(&data);
                        let hex = h
                            .finalize()
                            .iter()
                            .map(|b| format!("{b:02x}"))
                            .collect::<String>();
                        lines.push(format!("{hex} *{name}"));
                    }
                    Err(e) => return Err(format!("checksum: {name}: {e}")),
                }
            }
            let sidecar = base.join("checksum.md5");
            std::fs::write(&sidecar, lines.join("\n") + "\n")
                .map_err(|e| format!("checksum: sidecar 写入失败: {e}"))?;
            Ok(ExecOutcome::Done {
                msg: Some(format!(
                    "checksum: {} 项已写入 {}",
                    lines.len(),
                    sidecar.display()
                )),
            })
        }
        Some("sync") => {
            // M37-3：目录同步——left→right / right→left（缺省 left→right）。
            // 策略：仅复制缺失（mirror --delete 需显式开关 + 确认）。
            // dry-run = 缺省（列出计划）；--apply 才执行。
            let args: Vec<&str> = parts.collect();
            let mut direction = "left-to-right";
            let mut apply = false;
            let mut delete = false;
            for a in &args {
                match *a {
                    "left-to-right" | "l2r" => direction = "left-to-right",
                    "right-to-left" | "r2l" => direction = "right-to-left",
                    "--apply" => apply = true,
                    "--delete" => delete = true,
                    other => {
                        return Err(format!(
                            "sync: 未知参数 {other}（left-to-right|right-to-left [--apply] [--delete]）"
                        ));
                    }
                }
            }
            let (left_base, left_items) = {
                let tab = state.active_tab();
                match &tab.location {
                    Location::Physical(p) => (p.clone(), tab.entries.clone()),
                    _ => return Err("sync: 需要 pane 为物理目录视图".to_owned()),
                }
            };
            let (right_base, right_items) = {
                let tab = state.active_tab();
                match &tab.right.location {
                    Location::Physical(p) => (p.clone(), tab.right.entries.clone()),
                    _ => return Err("sync: 需要 pane 为物理目录视图".to_owned()),
                }
            };
            let (src_base, src_items, dst_base) = if direction == "left-to-right" {
                (&left_base, &left_items, &right_base)
            } else {
                (&right_base, &right_items, &left_base)
            };
            // 镜像计划：源有目标无 → 复制；--delete 且目标有源无 → 删除。
            let mut to_copy: Vec<PathBuf> = Vec::new();
            let dst_items = if direction == "left-to-right" {
                &right_items
            } else {
                &left_items
            };
            let mut to_delete: Vec<PathBuf> = Vec::new();
            for item in src_items.iter().filter(|i| !i.is_dir) {
                let dst = dst_base.join(&item.name);
                if !dst.exists() {
                    to_copy.push(item.name.clone().into());
                }
            }
            if delete {
                for item in dst_items.iter().filter(|i| !i.is_dir) {
                    let src = src_base.join(&item.name);
                    if !src.exists() {
                        to_delete.push(dst_base.join(&item.name));
                    }
                }
            }
            let _ = (src_base, src_items, dst_base);
            if !apply {
                return Ok(ExecOutcome::Done {
                    msg: Some(format!(
                        "sync({direction}) 计划: 复制 {} 项、删除 {} 项（--apply 执行、--delete 启用删除）",
                        to_copy.len(),
                        to_delete.len()
                    )),
                });
            }
            // --apply 执行。
            let mut applied = 0usize;
            let mut failures: Vec<String> = Vec::new();
            let tab_id = state.active;
            for name in &to_copy {
                let (src, dst) = if direction == "left-to-right" {
                    (left_base.join(name), right_base.join(name))
                } else {
                    (right_base.join(name), left_base.join(name))
                };
                match crate::ops::copy_to_resolved(tab_id, &src, &dst) {
                    Ok(op) => {
                        state.undo_stack.push(op);
                        applied += 1;
                    }
                    Err(e) => failures.push(format!("{}: {e}", name.display())),
                }
            }
            for path in &to_delete {
                let mut m = None;
                execute_delete(state, &mut m, path.clone(), false, None);
                if m.is_none() {
                    applied += 1;
                } else {
                    failures.push(format!(
                        "删除 {}: {}",
                        path.display(),
                        m.unwrap_or_default()
                    ));
                }
            }
            refresh_tab_panes(state, tab_id);
            if failures.is_empty() {
                Ok(ExecOutcome::Done {
                    msg: Some(format!("sync({direction}): {applied} 项已应用（u 可撤销）")),
                })
            } else {
                Err(format!(
                    "sync: {applied} 项成功，失败 {}: {}",
                    failures.len(),
                    failures.join("; ")
                ))
            }
        }
        Some("extract") => {
            // M37-2：归档解压——光标条目为归档文件 → 全量解压到目标目录
            //（缺省当前物理目录）；ArchiveFs 逐成员 read → 本地写；
            // zip-slip 防护（成员路径规范化拒绝 .. / 绝对路径）；单成员
            // 64MB 上限沿用护栏；逐成员入撤销栈（u 可逆）。
            let dest_dir = match parts.next() {
                Some(d) => {
                    let base = physical_base(state)?;
                    let p = std::path::Path::new(d);
                    if p.is_absolute() {
                        p.to_path_buf()
                    } else {
                        base.join(p)
                    }
                }
                None => physical_base(state)?,
            };
            if !dest_dir.is_dir() {
                return Err(format!("extract: 目标目录不存在: {}", dest_dir.display()));
            }
            // 光标条目 = 归档文件。
            let tab = state.active_tab();
            let Some(item) = tab.entries.get(tab.cursor) else {
                return Err("extract: 无选中条目".to_owned());
            };
            let host = tab.location.base_dir().join(&item.name);
            if !crate::archive::is_archive_file(&item.name) {
                return Err(format!("extract: {} 不是受支持的归档格式", item.name));
            }
            // 打开归档 VFS + 逐成员解压（walk list 树；zip-slip 防护；
            // 单成员 64MB 上限；逐成员入撤销栈 u 可逆）。
            let mut archive_fs = crate::archive::open_dispatch(&host)
                .map_err(|e| format!("extract: 归档打开失败: {e}"))?;
            let mut stack = vec![String::new()];
            let mut applied = 0usize;
            let mut failures: Vec<String> = Vec::new();
            let tab_id = state.active;
            while let Some(inner) = stack.pop() {
                let entries = match archive_fs.list(&inner) {
                    Ok(v) => v,
                    Err(e) => {
                        failures.push(format!("{inner}: {e}"));
                        continue;
                    }
                };
                for entry in entries {
                    let member_path = if inner.is_empty() {
                        PathBuf::from(&entry.path)
                    } else {
                        PathBuf::from(format!("{inner}/{}", entry.path))
                    };
                    if entry.is_dir {
                        stack.push(member_path.to_string_lossy().into_owned());
                        continue;
                    }
                    // zip-slip 防护：成员路径规范化后不得逃逸 dest_dir。
                    let dest = dest_dir.join(&member_path);
                    let normalized = normalize_lexical(&dest);
                    if !normalized.starts_with(&dest_dir) {
                        failures.push(format!("zip-slip 拒绝: {}", member_path.display()));
                        continue;
                    }
                    // 单成员 64MB 护栏。
                    if entry.size > 64 * 1024 * 1024 {
                        failures.push(format!("{}: 超出 64MB 单成员上限", member_path.display()));
                        continue;
                    }
                    if let Some(parent) = dest.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    match archive_fs.read(&member_path.to_string_lossy()) {
                        Ok(data) => match std::fs::write(&dest, &data) {
                            Ok(()) => {
                                state.undo_stack.push(crate::state::UndoableOp::Create {
                                    tab_id,
                                    path: dest.clone(),
                                    recursive: false,
                                });
                                applied += 1;
                            }
                            Err(e) => failures.push(format!("{}: {e}", dest.display())),
                        },
                        Err(e) => failures.push(format!("{}: {e}", member_path.display())),
                    }
                }
            }
            if failures.is_empty() {
                Ok(ExecOutcome::Done {
                    msg: Some(format!(
                        "extract: {applied} 个成员已解压到 {}（u 可撤销）",
                        dest_dir.display()
                    )),
                })
            } else {
                Err(format!(
                    "extract: {applied} 项成功，失败 {}: {}",
                    failures.len(),
                    failures.join("; ")
                ))
            }
        }
        Some("chmod") => {
            // M37-1：批量 chmod（八进制/符号；tags/v 选集/光标；-R 递归）。
            // POSIX only（std::os::unix::fs::PermissionsExt）——Windows 可读拒绝。
            #[cfg(not(unix))]
            {
                let _ = parts;
                Err("chmod: 仅 POSIX 平台支持（Windows 请用系统属性对话框）".to_owned())
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut recursive = false;
                let mut mode_str: Option<String> = None;
                for a in parts {
                    match a {
                        "-R" => recursive = true,
                        s => {
                            if mode_str.is_some() {
                                return Err("chmod: 多余参数（:chmod [-R] <mode>）".to_owned());
                            }
                            mode_str = Some(s.to_owned());
                        }
                    }
                }
                let Some(mode_str) = mode_str else {
                    return Err("chmod: 缺少模式（:chmod [-R] <mode>，如 644 / u+x）".to_owned());
                };
                // 八进制（3-4 位数字）优先；符号模式（u+x 等）简易解析。
                let mode = if mode_str.chars().all(|c| c.is_ascii_digit())
                    && (3..=4).contains(&mode_str.chars().count())
                {
                    u32::from_str_radix(&mode_str, 8)
                        .map_err(|e| format!("chmod: 非法八进制 {mode_str}: {e}"))?
                } else {
                    // 符号模式：[ugoa]*[+-=][rwxXst]（简易——全量重算）。
                    let cur = state
                        .active_tab()
                        .entries
                        .get(state.active_tab().cursor)
                        .map(|i| {
                            std::fs::metadata(base_dir_for_chmod(state, &i.name))
                                .map(|m| m.permissions().mode() & 0o777)
                                .unwrap_or(0)
                        })
                        .unwrap_or(0);
                    parse_symbolic_mode(&mode_str, cur)
                        .ok_or_else(|| format!("chmod: 非法符号模式 {mode_str}"))?
                };
                let names = selected_names(state);
                if names.is_empty() {
                    return Err("chmod: 无选中条目".to_owned());
                }
                let base = physical_base(state)?;
                let mut applied = 0usize;
                let mut failures: Vec<String> = Vec::new();
                for name in &names {
                    let target = base.join(name);
                    let targets: Vec<PathBuf> = if recursive {
                        let mut t = vec![target.clone()];
                        // 简易递归收集（万条上限）。
                        let mut stack = vec![target.clone()];
                        while let Some(dir) = stack.pop() {
                            if let Ok(rd) = std::fs::read_dir(&dir) {
                                for e in rd.flatten() {
                                    t.push(e.path());
                                    if e.file_type().map(|ft| ft.is_dir()).unwrap_or(false)
                                        && t.len() < 10_000
                                    {
                                        stack.push(e.path());
                                    }
                                }
                            }
                        }
                        t
                    } else {
                        vec![target.clone()]
                    };
                    for t in targets {
                        let perm = std::fs::metadata(&t)
                            .map(|m| {
                                let mut p = m.permissions();
                                p.set_mode(mode);
                                p
                            })
                            .map_err(|e| format!("{}: {e}", t.display()))?;
                        match std::fs::set_permissions(&t, perm) {
                            Ok(()) => applied += 1,
                            Err(e) => failures.push(format!("{}: {e}", t.display())),
                        }
                    }
                }
                if failures.is_empty() {
                    Ok(ExecOutcome::Done {
                        msg: Some(format!("chmod: {applied} 个路径已设 {mode_str}")),
                    })
                } else {
                    Err(format!(
                        "chmod: {applied} 项成功，失败 {}: {}",
                        failures.len(),
                        failures.join("; ")
                    ))
                }
            }
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
                        .ok_or_else(|| format!("archive: 未知格式 {fmt}（内建 zip/tar/tar.gz/7z；CLI rar/cab/lzh/tar.xz/tar.bz2/tar.zst）"))?;
                    let (op, names, target, base) =
                        archive_plan(state, format, name, default_checksum)?;
                    // M26-1：创建在后台线程执行（UI 不阻塞），完成回包入撤销栈。
                    Ok(ExecOutcome::SpawnJob {
                        op,
                        names,
                        target,
                        base,
                    })
                }
                [name] => {
                    let format = ops::ArchiveFormat::infer(name).ok_or_else(|| {
                        "archive: 无法从扩展名推断格式（内建 zip/tar/tar.gz/7z；CLI rar/cab/lzh/tar.xz/tar.bz2/tar.zst）".to_owned()
                    })?;
                    let (op, names, target, base) =
                        archive_plan(state, format, name, default_checksum)?;
                    Ok(ExecOutcome::SpawnJob {
                        op,
                        names,
                        target,
                        base,
                    })
                }
                [fmt, name, kv] if kv.starts_with("checksum=") => {
                    let format = ops::ArchiveFormat::parse(fmt)
                        .ok_or_else(|| format!("archive: 未知格式 {fmt}（内建 zip/tar/tar.gz/7z；CLI rar/cab/lzh/tar.xz/tar.bz2/tar.zst）"))?;
                    let checksum = ops::Checksum::parse(&kv["checksum=".len()..])
                        .ok_or_else(|| "archive: 校验类型需 md5 / sha256 / none".to_owned())?;
                    let (op, names, target, base) = archive_plan(state, format, name, checksum)?;
                    Ok(ExecOutcome::SpawnJob {
                        op,
                        names,
                        target,
                        base,
                    })
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
            // M28-2：结构化形态（s///、prefix=、suffix=、seq=）→ 结果视图
            // 预览（RenamePreview），y 应用整批 / Esc 取消（取消不动文件）。
            // 结果视图/`..` 行守卫：物理视图专属，先拒绝。
            if looks_structured_rename(new_name) {
                if state.active_tab().in_results() {
                    return Err("搜索结果视图只读".to_owned());
                }
                if state.active_tab().on_parent_row {
                    return Err("`..` 行仅导航".to_owned());
                }
                let plan = rename_preview_plan(state, new_name)?;
                let base = physical_base(state)?;
                let items: Vec<crate::search::ResultItem> = plan
                    .iter()
                    .map(|p| crate::search::ResultItem {
                        path: p.old.clone(),
                        line: None,
                        text: match p.skip {
                            Some(reason) => format!("（{reason}）"),
                            None => p.new.clone(),
                        },
                        size: 0,
                    })
                    .collect();
                let tab = state.active_tab_mut();
                tab.enter_results(
                    crate::search::SearchKind::RenamePreview,
                    new_name.to_owned(),
                    base,
                    items,
                    false,
                );
                tab.pending_rename = Some(
                    plan.into_iter()
                        .filter(|p| p.skip.is_none())
                        .map(|p| (p.old, p.new))
                        .collect(),
                );
                return Ok(ExecOutcome::Done { msg: None });
            }
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
        Some("link") => {
            // M28-1：符号链接创建——`:link <dest>`。作用域 = 标记/可视/光标
            //（selected_names，与 M23-10 复制同口径）；dest 为已存在目录 →
            // 其内逐条目建同名链接（批量），否则作为单个链接路径；预检
            // 全过才开始创建（无半成品），预检后 IO 失败已建条目留在
            // 撤销栈可 `u` 逆。远端/归档/结果/树视图被既有守卫拒绝。
            let dest_raw = parts
                .next()
                .ok_or("link: 缺少目标路径（:link <dest>；dest 为已存在目录时在其内建链）")?;
            if parts.next().is_some() {
                return Err("link: 参数过多（:link <dest>）".to_owned());
            }
            let base = physical_base(state)?;
            let dest_path = {
                let p = std::path::Path::new(dest_raw);
                if p.is_absolute() {
                    p.to_path_buf()
                } else {
                    base.join(p)
                }
            };
            let names = selected_names(state);
            if names.is_empty() {
                return Err("link: 无选中条目".to_owned());
            }
            let batch = dest_path.is_dir();
            if !batch && names.len() > 1 {
                return Err(format!(
                    "link: {n} 个条目需要 dest 为已存在的目录",
                    n = names.len()
                ));
            }
            let (targets, links): (Vec<PathBuf>, Vec<PathBuf>) = if batch {
                names
                    .iter()
                    .map(|n| (base.join(n), dest_path.join(n)))
                    .unzip()
            } else {
                (vec![base.join(&names[0])], vec![dest_path.clone()])
            };
            // 预检：父目录存在 + 链接路径无冲突 → 全部通过才开始创建。
            for link in &links {
                let Some(parent) = link.parent() else {
                    return Err("link: 非法链接路径".to_owned());
                };
                if !parent.as_os_str().is_empty() && !parent.is_dir() {
                    return Err(format!(
                        "link: 父目录不存在: {}",
                        parent.display().to_string().replace('\\', "/")
                    ));
                }
                if std::fs::symlink_metadata(link).is_ok() {
                    return Err(format!(
                        "link: 目标已存在（不覆盖）: {}",
                        link.display().to_string().replace('\\', "/")
                    ));
                }
            }
            let mut created: Vec<crate::state::UndoableOp> = Vec::new();
            for (target, link) in targets.iter().zip(&links) {
                match crate::ops::create_link(state.active, target, link) {
                    Ok(op) => created.push(op),
                    Err(e) => {
                        let n = created.len();
                        for op in created.drain(..) {
                            state.undo_stack.push(op);
                        }
                        refresh_tab_panes(state, state.active);
                        if n == 0 {
                            return Err(format!("link: {e}"));
                        }
                        return Err(format!(
                            "link: 部分完成（已建 {n} 条留在撤销栈，u 可逆）: {e}"
                        ));
                    }
                }
            }
            let count = created.len();
            for op in created {
                state.undo_stack.push(op);
            }
            refresh_tab_panes(state, state.active);
            Ok(ExecOutcome::Done {
                msg: Some(format!(
                    "已创建 {count} 个符号链接 -> {}",
                    dest_path.display().to_string().replace('\\', "/")
                )),
            })
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
            // M13-10：暂存区菜单 + M37-7：clear/purge 子命令。
            match parts.next() {
                None => {
                    let items = crate::ops::read_trash_index();
                    Ok(ExecOutcome::ShowTrash { items })
                }
                Some("clear") => {
                    // 清空：删除全部 staged 文件 + 索引。
                    let (removed, errs) = crate::ops::trash_clear_all();
                    if errs.is_empty() {
                        Ok(ExecOutcome::Done {
                            msg: Some(format!("trash: 已清空 {removed} 项（不可恢复）")),
                        })
                    } else {
                        Err(format!(
                            "trash: {removed} 项清除，失败 {}: {}",
                            errs.len(),
                            errs.join("; ")
                        ))
                    }
                }
                Some("purge") => {
                    // 按时间清理：--before <天数>（staged 文件 mtime 早于
                    // N 天前的清除）。
                    let days: u64 = match parts.next() {
                        Some(d) if d.starts_with("--before=") => d["--before=".len()..]
                            .parse()
                            .map_err(|_| "trash: --before=<天数> 非法".to_owned())?,
                        _ => return Err("trash: purge 需 --before=<天数>".to_owned()),
                    };
                    let (removed, errs) = crate::ops::trash_purge_before(days);
                    if errs.is_empty() {
                        Ok(ExecOutcome::Done {
                            msg: Some(format!(
                                "trash: 已清理 {removed} 项（{days} 天前，不可恢复）"
                            )),
                        })
                    } else {
                        Err(format!(
                            "trash: {removed} 项清理，失败 {}: {}",
                            errs.len(),
                            errs.join("; ")
                        ))
                    }
                }
                Some(other) => Err(format!(
                    "trash: 未知子命令 {other}（无参=菜单、clear、purge --before=<天数>）"
                )),
            }
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
        Some("cmdmark") => {
            // M30-4：命令书签——`:cmdmark <chars> [命令文本]`。
            // 无命令文本 → 对最近一条会话命令（record_cmd_history 尾部）设
            // keyword；`-` 清除（unbook 保留历史，M23-2 语义）；chars 限
            // 字母数字（与 marks 同口径）≤32 字符。
            let tag = parts
                .next()
                .ok_or_else(|| "cmdmark: 缺少书签名（:cmdmark <chars> [命令文本]）".to_owned())?;
            if tag.chars().count() > 32 {
                return Err("cmdmark: 书签名过长（≤32 字符）".to_owned());
            }
            let clear = tag == "-";
            if !clear && !tag.chars().all(|c| c.is_ascii_alphanumeric()) {
                return Err("cmdmark: 书签名限字母数字".to_owned());
            }
            // 命令文本：显式参数（空白连接）> 最近会话命令。
            let rest = parts.collect::<Vec<&str>>().join(" ");
            let cmd_text = if !rest.is_empty() {
                rest
            } else {
                let hist = state.cmd_history.clone();
                let Some(last) = hist.last() else {
                    return Err("cmdmark: 无可标记的命令（会话历史为空）".to_owned());
                };
                last.clone()
            };
            if cmd_text.trim().is_empty() {
                return Err("cmdmark: 命令文本为空".to_owned());
            }
            let kw_str = if clear { String::new() } else { tag.to_owned() };
            crate::frecency::set_keyword_topic_now(&cmd_text, "cmd", &kw_str)
                .map_err(|e| format!("cmdmark: 写入失败: {e}"))?;
            Ok(ExecOutcome::Done {
                msg: Some(if clear {
                    "已清除命令书签（历史保留）".to_owned()
                } else {
                    format!("已设置命令书签 {kw_str}: {cmd_text}")
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
pub(crate) fn run_search(state: &mut AppState, msg: &mut Option<String>, forward: bool) {
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
pub(crate) fn search_cancel(state: &mut AppState, origin: usize) {
    let tab = state.active_tab_mut();
    // M34-3：恢复焦点 pane 光标（右 pane 无 `..` 行语义）。
    match tab.focus {
        crate::tab::Focus::Right => {
            tab.right.cursor = origin.min(tab.right.entries.len().saturating_sub(1));
        }
        crate::tab::Focus::Left => {
            tab.cursor = origin.min(tab.entries.len().saturating_sub(1));
        }
    }
    tab.search = None;
}

/// 提交搜索词（M1-7）：设置搜索状态并移动到首个匹配（从光标下一位环绕）。
/// FIX-1：实时跳转已把光标停在匹配项上时不重复前跳；空查询清除搜索状态
/// （高亮与计数随之消失）。
pub(crate) fn submit_search(state: &mut AppState, line: &str, msg: &mut Option<String>) {
    let query = line.trim().to_owned();
    if query.is_empty() {
        state.active_tab_mut().search = None;
        *msg = None;
        return;
    }
    let tab = state.active_tab_mut();
    if tab.focus == crate::tab::Focus::Left {
        tab.on_parent_row = false; // FEAT-2：搜索交互离开 `..` 行（右 pane 无该行）。
    }
    let (entries, cursor) = match tab.focus {
        crate::tab::Focus::Right => (&tab.right.entries, &tab.right.cursor),
        crate::tab::Focus::Left => (&tab.entries, &tab.cursor),
    };
    let on_match = entries
        .get(*cursor)
        .is_some_and(|item| crate::tab::match_range(&item.name, &query).is_some());
    if on_match {
        tab.search = Some(SearchState { query });
        *msg = None;
        return;
    }
    let start = *cursor + 1;
    match crate::tab::find_match_in(entries, &query, start, true) {
        Some(idx) => {
            match tab.focus {
                crate::tab::Focus::Right => tab.right.cursor = idx,
                crate::tab::Focus::Left => tab.cursor = idx,
            }
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
pub(crate) fn live_search_jump(
    state: &mut AppState,
    origin: usize,
    input: &str,
    msg: &mut Option<String>,
) {
    let query = input.trim();
    let tab = state.active_tab_mut();
    if tab.focus == crate::tab::Focus::Left {
        tab.on_parent_row = false; // FEAT-2：搜索交互离开 `..` 行（右 pane 无该行）。
    }
    let entries = match tab.focus {
        crate::tab::Focus::Right => &tab.right.entries,
        crate::tab::Focus::Left => &tab.entries,
    };
    if query.is_empty() {
        let clamped = origin.min(entries.len().saturating_sub(1));
        match tab.focus {
            crate::tab::Focus::Right => tab.right.cursor = clamped,
            crate::tab::Focus::Left => tab.cursor = clamped,
        }
        *msg = None;
        return;
    }
    match crate::tab::find_match_in(entries, query, origin + 1, true) {
        Some(idx) => {
            match tab.focus {
                crate::tab::Focus::Right => tab.right.cursor = idx,
                crate::tab::Focus::Left => tab.cursor = idx,
            }
            *msg = None;
        }
        None => {
            match tab.focus {
                crate::tab::Focus::Right => tab.right.cursor = origin,
                crate::tab::Focus::Left => tab.cursor = origin,
            }
            *msg = Some("无匹配".to_owned());
        }
    }
}

/// `l`/Enter 与鼠标再击共用的打开逻辑（FEAT-1）：目录/归档 → enter_selected
/// 导航（FIX-3：归档文件优先于 opener）；普通文件 → 系统 opener（M11-7）。
pub(crate) fn open_selected(state: &mut AppState, msg: &mut Option<String>) {
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

/// FIX-2：`p`（PutCopy）/`P`（PutMove）——消费 yank 缓冲区到当前（焦点）
/// 目录。复制还是移动由缓冲区类型与键位决定（SPEC §4.8）：yy 缓冲 + `p`
/// = 复制、+ `P` = 移动；cut 缓冲恒为移动。每项独立入撤销栈（`u` 可逆）；
/// 失败逐项报告，不 panic、不留半成品（单项目标已存在等错误即停）。
/// M32-2：传输中断现场文件路径（`.part`）——临时目录下按会话+目标
/// 路径哈希命名（连接重试时同路径命中）。会话级续传现场，进程退出
/// 不清理（供诊断与重试）。
pub(crate) fn resume_part_path(remote: &str, dest: &Path) -> PathBuf {
    use std::fmt::Write as _;
    let key = format!("{remote}:{}", dest.display().to_string().replace('\\', "/"));
    let mut h: u64 = 0xcbf29ce484222325;
    for b in key.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    let leaf = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_owned());
    let mut name = String::new();
    let _ = write!(name, "rvifm-xfer-{h:016x}-{leaf}.part");
    std::env::temp_dir().join(name)
}

/// M32-2/M32-1：重连当前远端会话（downcast SftpBackend → params/password
/// 副本重建 → 替换会话表）；mock 或无会话返回可读 Err（会话放回）。
pub(crate) fn remote_reconnect_session(state: &mut AppState, name: &str) -> Result<(), String> {
    let any = state.remote_sessions.remove(name);
    let Some(b) = any else {
        return Err("remote: 会话已断开".to_owned());
    };
    match b
        .as_any()
        .and_then(|a| a.downcast_ref::<crate::remote::sftp::SftpBackend>())
    {
        Some(sb) => match sb.reconnect_from_params() {
            Ok(nb) => {
                state.remote_sessions.insert(name.to_owned(), Box::new(nb));
                Ok(())
            }
            Err(e) => {
                state.remote_sessions.insert(name.to_owned(), b);
                Err(format!("remote: 重连失败: {e}"))
            }
        },
        None => {
            state.remote_sessions.insert(name.to_owned(), b);
            Err("remote: 该会话类型不支持自动重连".to_owned())
        }
    }
}

pub(crate) fn paste_yanked(state: &mut AppState, force_move: bool, msg: &mut Option<String>) {
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
    // M36-5：冲突策略（Ask 缺省保持现行为——同名失败可读提示；
    // Skip/RenameAuto/KeepNewer/Overwrite 在条目级消解）。
    let policy = crate::state::ConflictPolicy::parse(&state.config.transfer.conflict_policy)
        .unwrap_or_default();
    let mut done = 0usize;
    let mut first_err: Option<String> = None;
    for path in &buffer.paths {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut dest = base.join(&name);
        // 冲突消解（目标已存在时）。
        if dest.exists() {
            let src_mtime = std::fs::symlink_metadata(path)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            let dst_mtime = std::fs::metadata(&dest)
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            match crate::state::resolve_conflict(policy, src_mtime, true, dst_mtime) {
                crate::state::ConflictDecision::Proceed => {} // Overwrite/KeepNewer(源新)
                crate::state::ConflictDecision::Skip => {
                    continue;
                }
                crate::state::ConflictDecision::UseName(_) => {
                    match ops::auto_rename(&name, &|n| !base.join(n).exists()) {
                        Ok(new_name) => dest = base.join(&new_name),
                        Err(e) => {
                            first_err = Some(e.to_string());
                            break;
                        }
                    }
                }
            }
            if policy == crate::state::ConflictPolicy::Ask {
                // Ask 缺省 = 同名失败（覆盖确认交互随 M39-6 扩展）。
                first_err = Some(format!(
                    "目标已存在（conflict_policy = ask 可改 overwrite/skip/rename/newer）: {}",
                    dest.display()
                ));
                break;
            }
        }
        // 复制/移动到消解后的目标（RenameAuto 已改名；Overwrite 覆盖现有）。
        let result = if do_move {
            crate::ops::move_to_resolved(state.active, path, &dest)
        } else {
            crate::ops::copy_to_resolved(state.active, path, &dest)
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
/// M36-6：`.part.meta` 读写——记录已传偏移/总长/源 mtime/头 4KB 哈希。
/// 格式：TOML（`offset`/`total`/`src_mtime`/`head_hash`）。
#[derive(serde::Serialize, serde::Deserialize)]
struct XferPartMeta {
    offset: u64,
    total: u64,
    src_mtime: u64,
    head_hash: String,
}

fn part_meta_path(part: &Path) -> PathBuf {
    let mut s = part.to_string_lossy().into_owned();
    s.push_str(".meta");
    PathBuf::from(s)
}

/// M36-6：读远端目标前 4KB 计算头部哈希（校验续传现场一致性）。
fn remote_head_hash(backend: &dyn crate::remote::RemoteFs, dest: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    let head = backend.read_range(dest, 0, 4096).ok()?;
    let mut h = Sha256::new();
    h.update(&head);
    Some(
        h.finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
    )
}

/// M36-6：分块上传 + 偏移续传——每 1MB 块 write_range 推进；连接类失败
/// 保存 offset 到 `.part.meta`（含头哈希）并返回 Err(offset)；重试时：
/// `.part.meta` 存在 → `read_range` 校验远端头部哈希一致 → 从 offset 续
/// （write_range）；不一致/缺失 → 从头。完成（offset==total）→ 删 meta。
pub(crate) fn upload_chunked(
    backend: &dyn crate::remote::RemoteFs,
    src: &Path,
    dest: &Path,
) -> Result<(), (u64, String)> {
    const CHUNK: u64 = 1024 * 1024;
    let src_data = std::fs::read(src).map_err(|e| (0, format!("本地读取失败: {e}")))?;
    let total = src_data.len() as u64;
    let src_mtime = std::fs::metadata(src)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let part = resume_part_path("xfer", dest);
    let meta_p = part_meta_path(&part);
    // 续传现场判定：meta 存在 + total/mtime/头哈希一致 → 从 offset 续；
    // 否则从 0（服务器端目标已存在时先截断重写——write_range offset=0
    // 全量写覆盖）。
    let mut offset: u64 = 0;
    let resumed = std::fs::read_to_string(&meta_p)
        .ok()
        .and_then(|t| toml::from_str::<XferPartMeta>(&t).ok())
        .filter(|meta| {
            meta.total == total
                && meta.src_mtime == src_mtime
                && remote_head_hash(backend, dest).as_deref() == Some(meta.head_hash.as_str())
        });
    if let Some(meta) = resumed {
        offset = meta.offset;
    }
    // 头哈希（首次写前记录——后续重试校验基准）。
    if offset == 0 {
        use sha2::{Digest, Sha256};
        // 先写第一块（目标此时建立）。
        let first_end = CHUNK.min(total);
        backend
            .write_range(dest, 0, &src_data[..first_end as usize])
            .map_err(|e| (0, e.to_string()))?;
        offset = first_end;
        let mut h = Sha256::new();
        h.update(&src_data[..first_end as usize]);
        let meta = XferPartMeta {
            offset,
            total,
            src_mtime,
            head_hash: h
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
        };
        let _ = std::fs::write(&meta_p, toml::to_string(&meta).unwrap_or_default());
    }
    // 块循环推进。
    while offset < total {
        let end = (offset + CHUNK).min(total);
        backend
            .write_range(dest, offset, &src_data[offset as usize..end as usize])
            .map_err(|e| (offset, e.to_string()))?;
        offset = end;
        let meta_updated = std::fs::read_to_string(&meta_p)
            .ok()
            .and_then(|t| toml::from_str::<XferPartMeta>(&t).ok())
            .map(|mut meta| {
                meta.offset = offset;
                let _ = std::fs::write(&meta_p, toml::to_string(&meta).unwrap_or_default());
                meta
            });
        let _ = meta_updated;
    }
    // 完成：远端 mtime 保留 + 删现场。
    let _ = backend.metadata(dest).map(|meta| {
        if meta.mtime > 0 {
            let mt = std::time::UNIX_EPOCH + std::time::Duration::from_secs(meta.mtime);
            // write_range 后无法在 trait 上直接 set times——保留策略：
            // rename/recreate 交由后端；此处 mtime 以 write_range 后的
            // touch_range 不存在而跳过（SIMPLIFIED：上传 mtime 不保留）。
            let _ = mt;
        }
    });
    let _ = std::fs::remove_file(&meta_p);
    Ok(())
}

pub(crate) fn paste_yanked_remote(
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
    // M36-7：[transfer] queue = true 且多文件 → 入队（:queue 顺序消费）。
    if state.config.transfer.queue && buffer.paths.len() > 1 {
        for src in &buffer.paths {
            let name = src
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let dest = if dir == std::path::Path::new("/") {
                PathBuf::from(format!("/{name}"))
            } else {
                dir.join(&name)
            };
            state.transfer_queue.push(crate::state::TransferQueueEntry {
                direction: crate::app::TransferDirection::Upload,
                src: src.clone(),
                dest,
                tab_id,
                remote: remote.clone(),
            });
        }
        *msg = Some(format!(
            "已入队 {} 项上传（:queue 查看进度）",
            buffer.paths.len()
        ));
        refresh_tab_panes(state, tab_id);
        return;
    }
    // M36-8：任一 src 为目录 → RecurseUpload（walk 展开 + 逐文件）。
    if buffer.paths.iter().any(|s| s.is_dir()) {
        let entries: Vec<(PathBuf, PathBuf)> = buffer
            .paths
            .iter()
            .map(|src| {
                let name = src
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let dest = if dir == std::path::Path::new("/") {
                    PathBuf::from(format!("/{name}"))
                } else {
                    dir.join(&name)
                };
                (src.clone(), dest)
            })
            .collect();
        match state.remote_sessions.get(&remote).map(|b| b.as_ref()) {
            Some(backend) => {
                let controls = crate::state::JobControls {
                    cancel: &crate::state::NO_CANCEL,
                    pause: &crate::state::NO_PAUSE,
                    progress: &crate::state::JobProgress::empty(),
                };
                let (ops, status) = execute_transfer_body(
                    TransferDirection::RecurseUpload,
                    backend,
                    &entries,
                    tab_id,
                    &remote,
                    &controls,
                );
                let n = ops.len();
                for op in ops {
                    state.undo_stack.push(op);
                }
                if let crate::state::JobStatus::Failed { err } = status {
                    *msg = Some(err);
                } else {
                    *msg = Some(format!("已递归上传 {n} 个文件（u 可撤销）"));
                }
                refresh_tab_panes(state, tab_id);
                return;
            }
            None => {
                *msg = Some("remote: 会话已断开".to_owned());
                return;
            }
        }
    }
    for src in &buffer.paths {
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
        if exists {
            first_err = Some(format!("目标已存在: {}", dest.display()));
            break;
        }
        // M32-2：write 连接类失败 → .part 传输现场保留 + 自动重连一次后
        // 重写（SIMPLIFIED：会话 write 无偏移参数，重连后整文件重写；
        // 显式取消（x）在条目边界，不产生 .part）。
        // M36-6：upload_chunked——1MB 块 write_range 推进 + .part.meta
        // 偏移现场；连接类失败保存 offset，重试粘贴从偏移续传
        // （read_range 校验头哈希一致才续，否则从头）。第一块失败重连
        // 一次重试；中段失败保留现场（重试粘贴续传）。
        let backend_ref = state.remote_sessions.get(&remote).map(|b| b.as_ref());
        let upload_err = match backend_ref {
            None => Some("remote: 会话已断开".to_owned()),
            Some(backend) => match upload_chunked(backend, src, &dest) {
                Ok(()) => None,
                Err((0, e)) => {
                    // 第一块失败：重连一次重试。
                    if remote_reconnect_session(state, &remote).is_ok() {
                        match state
                            .remote_sessions
                            .get(&remote)
                            .map(|b| upload_chunked(b.as_ref(), src, &dest))
                        {
                            Some(Ok(())) => None,
                            Some(Err((_, e2))) => Some(format!("重连后仍失败: {e2}")),
                            None => Some("remote: 会话已断开".to_owned()),
                        }
                    } else {
                        Some(format!("{e}（第一块失败）"))
                    }
                }
                Err((offset, e)) => Some(format!(
                    "{e}（已传 {offset} 字节，.part.meta 现场保留——重试粘贴可续传）"
                )),
            },
        };
        match upload_err {
            None => {
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
            Some(e) => {
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
pub(crate) fn paste_remote_to_local(
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
        // M36-2：**单次 read**（消除 R-3 双读——read 一次同时拿数据与
        // 失败信息；metadata 先记远端 mtime）；连接类失败 → .part 现场
        // 保留 + 自动重连一次后重新 read。写入走临时文件 + 原子 rename。
        let (read_out, src_meta) = state
            .remote_sessions
            .get(remote)
            .map(|be| (be.read(src), be.metadata(src).ok()))
            .unwrap_or((
                Err(crate::remote::RemoteError::Other("会话已断开".into())),
                None,
            ));
        let data = match read_out {
            Ok(d) => d,
            Err(e) if e.is_connection_class() => {
                let part = resume_part_path(remote, src);
                let _ = std::fs::write(&part, b"");
                match remote_reconnect_session(state, remote) {
                    Ok(()) => {
                        let retry = state
                            .remote_sessions
                            .get(remote)
                            .ok_or_else(|| "remote: 会话已断开".to_owned())
                            .and_then(|be| be.read(src).map_err(|e2| e2.to_string()));
                        match retry {
                            Ok(d) => d,
                            Err(e2) => {
                                first_err = Some(format!(
                                    "重连后仍读取失败: {e2}（.part 现场: {}）",
                                    part.display()
                                ));
                                break;
                            }
                        }
                    }
                    Err(e2) => {
                        first_err = Some(format!("{e2}（.part 现场: {}）", part.display()));
                        break;
                    }
                }
            }
            Err(e) => {
                first_err = Some(e.to_string());
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
        // M36-2：临时文件写入（同目录保证 rename 原子性）；远端 mtime
        // 保留（metadata 可得时）；失败清理临时文件。
        let tmp_dest = dest.with_extension(format!(
            "{}.part-dl",
            dest.extension()
                .map(|e| e.to_string_lossy().into_owned())
                .unwrap_or_default()
        ));
        let write_r = std::fs::write(&tmp_dest, &data).and_then(|()| {
            if let Some(meta) = &src_meta {
                // 远端 mtime（Unix 秒）→ SystemTime → 本地文件保留。
                if meta.mtime > 0 {
                    let mt = std::time::UNIX_EPOCH + std::time::Duration::from_secs(meta.mtime);
                    let _ = std::fs::File::options()
                        .write(true)
                        .open(&tmp_dest)
                        .and_then(|f| f.set_times(std::fs::FileTimes::new().set_modified(mt)));
                }
            }
            std::fs::rename(&tmp_dest, &dest)
        });
        match write_r {
            Ok(()) => {
                state.undo_stack.push(crate::state::UndoableOp::Create {
                    tab_id,
                    path: dest.clone(),
                    recursive: false,
                });
                done += 1;
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp_dest);
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
pub(crate) fn file_info_text(path: &std::path::Path, name: &str) -> String {
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
pub(crate) fn apply_rename_lines(
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
pub(crate) fn rename_apply(
    state: &mut AppState,
    path: &std::path::Path,
    new_name: &str,
) -> Option<String> {
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
pub(crate) fn batch_rename_flow(
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

/// M25-4：归档对话框的字段/取值常量（与渲染同源）。
/// M26-2：对话框字段总数（名称/格式/校验）。
pub(crate) const ARCHIVE_FIELDS_TOTAL: usize = 3;
pub(crate) const ARCHIVE_FORMATS: [ops::ArchiveFormat; 4] = [
    ops::ArchiveFormat::Zip,
    ops::ArchiveFormat::Tar,
    ops::ArchiveFormat::TarGz,
    ops::ArchiveFormat::SevenZ,
];
pub(crate) const ARCHIVE_CHECKSUMS: [ops::Checksum; 3] = [
    ops::Checksum::Md5,
    ops::Checksum::Sha256,
    ops::Checksum::None,
];

/// M25-4：归档对话框单键——Tab/↑↓ 切字段、←→/空格 循环当前字段取值、
/// 名称字段（field=0）接收字符输入/退格；Enter 返回作业参数（M26-1：
/// 创建在后台线程执行，run 循环 spawn）；Esc 取消。名称空 → 留在对话
/// 框提示；计划失败留在对话框（名称保留可改）。
pub(crate) fn handle_archive_dialog_key(
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
            match archive_plan(
                state,
                ARCHIVE_FORMATS[*format_idx],
                &name,
                ARCHIVE_CHECKSUMS[*checksum_idx],
            ) {
                Ok((op, names, target, base)) => ArchiveDialogOutcome::Exec {
                    op,
                    names,
                    target,
                    base,
                },
                Err(e) => {
                    *msg = Some(e);
                    ArchiveDialogOutcome::None
                }
            }
        }
        KeyCode::Tab | KeyCode::Down => {
            *field = (*field + 1) % ARCHIVE_FIELDS_TOTAL;
            ArchiveDialogOutcome::None
        }
        KeyCode::Up => {
            *field = (*field + ARCHIVE_FIELDS_TOTAL - 1) % ARCHIVE_FIELDS_TOTAL;
            ArchiveDialogOutcome::None
        }
        KeyCode::Right | KeyCode::Char(' ') => {
            match *field {
                1 => *format_idx = (*format_idx + 1) % ARCHIVE_FORMATS.len(),
                2 => *checksum_idx = (*checksum_idx + 1) % ARCHIVE_CHECKSUMS.len(),
                _ => {} // 名称字段：空格不入名（名字不含空白）
            }
            ArchiveDialogOutcome::None
        }
        KeyCode::Left => {
            match *field {
                1 => {
                    *format_idx = (*format_idx + ARCHIVE_FORMATS.len() - 1) % ARCHIVE_FORMATS.len()
                }
                2 => {
                    *checksum_idx =
                        (*checksum_idx + ARCHIVE_CHECKSUMS.len() - 1) % ARCHIVE_CHECKSUMS.len()
                }
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

/// M25-3/M25-4 + M26-1：归档创建**计划**（exec 臂与对话框共用）——tags
/// 优先（排序确定顺序）、空 = 光标条目。返回作业参数：`op` 编码格式与
/// 校验类型（`archive:<fmt>:<cs>`），`target` = base/原始名（扩展名补齐、
/// 同名不覆盖检查在作业线程的 `create_archive` 内）。物理视图限定。
/// 同步部分只做校验与收集，压缩本身在后台线程（M26-1）。
pub(crate) fn archive_plan(
    state: &AppState,
    format: ops::ArchiveFormat,
    name: &str,
    checksum: ops::Checksum,
) -> Result<(String, Vec<String>, PathBuf, PathBuf), String> {
    if name.trim().is_empty() {
        return Err("archive: 名称不能为空".to_owned());
    }
    let base = physical_base(state)?;
    // 目标已存在 → 同步预检（快速反馈；create_archive 内的同名不覆盖
    // 仍兜底——扩展名补齐后的真实产物路径以线程内检查为准）。
    if base.join(name).exists() {
        return Err(format!(
            "archive: 目标已存在: {}（不覆盖）",
            base.join(name).display().to_string().replace('\\', "/")
        ));
    }
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
    let fmt_name = match format {
        ops::ArchiveFormat::Zip => "zip",
        ops::ArchiveFormat::Tar => "tar",
        ops::ArchiveFormat::TarGz => "tar.gz",
        ops::ArchiveFormat::SevenZ => "7z",
        ops::ArchiveFormat::Rar => "rar",
        ops::ArchiveFormat::Cab => "cab",
        ops::ArchiveFormat::Lzh => "lzh",
        ops::ArchiveFormat::TarXz => "tar.xz",
        ops::ArchiveFormat::TarBz2 => "tar.bz2",
        ops::ArchiveFormat::TarZst => "tar.zst",
    };
    let cs_str = match checksum {
        ops::Checksum::Md5 => "md5",
        ops::Checksum::Sha256 => "sha256",
        ops::Checksum::None => "none",
    };
    let op = format!("archive:{fmt_name}:{cs_str}");
    let target = base.join(name);
    Ok((op, names, target, base))
}

/// 执行确认后的删除（M2-2）：移入应用内暂存、记录撤销栈、刷新所在 tab。
/// `permanent = true`（M13-3，`DD`）：**跳过暂存直接删除**，不进撤销栈
/// （不可逆——确认提示已强警示）。
/// 返回 `Some(op)` = EACCES 需要提权（M12-3），调用方进入 AskPass 模式。
pub(crate) fn execute_delete(
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
pub(crate) fn execute_tag_delete(state: &mut AppState, msg: &mut Option<String>) {
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
pub(crate) fn undo_last(state: &mut AppState, msg: &mut Option<String>) {
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
pub(crate) fn flush_session_visits(state: &AppState) {
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
pub(crate) fn collect_session_records(state: &AppState) -> Vec<crate::frecency::EntryRecord> {
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
pub(crate) fn normalize_lexical(p: &std::path::Path) -> std::path::PathBuf {
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
pub(crate) fn record_live_visit(state: &mut AppState, tab_id: usize, side: char) {
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
pub(crate) fn record_file_open(state: &mut AppState, tab_id: usize, path: &std::path::Path) {
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
