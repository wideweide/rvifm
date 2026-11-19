use super::*;

/// 刷新 tab 的双 pane（M14-1）：左 pane 走 Tab::refresh，右 pane 走 Pane::refresh。
pub(crate) fn refresh_tab_panes(state: &mut AppState, tab_id: usize) {
    let cfg = state.config.clone();
    if let Some(tab) = state.tabs.get_mut(tab_id) {
        let _ = tab.refresh(cfg.clone());
        let _ = tab.right.refresh(cfg);
    }
}

/// M21-F5：监听目标——活动 tab 的物理目录；归档/远端视图不监听（None）。
pub(crate) fn watch_target(tab: &Tab) -> Option<std::path::PathBuf> {
    match &tab.location {
        Location::Physical(p) => Some(p.clone()),
        _ => None,
    }
}

/// M21-F5：事件相关性——创建/删除/重命名/修改触发刷新；访问类忽略。
/// （notify 将重命名归入 Modify(ModifyKind::Name)，含在 Modify 内。）
pub(crate) fn fs_event_relevant(kind: &notify::EventKind) -> bool {
    matches!(
        kind,
        notify::EventKind::Create(_) | notify::EventKind::Modify(_) | notify::EventKind::Remove(_)
    )
}

/// M21-F5：同步监听目标——活动 tab 物理目录变化时重挂 watcher
/// （NonRecursive）；归档/远端卸载监听；创建/挂载失败静默降级（无监听，
/// TUI 照常工作）。
pub(crate) fn sync_watcher(state: &mut AppState) {
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
/// 否则钳制在列表内。AUDIT-8：刷新的权限/IO 失败不再静默——返回可读
/// 提示（调用方进状态栏）；失败时列表未更新，光标保持原位。
pub(crate) fn refresh_on_fs_change(state: &mut AppState) -> Option<String> {
    let active = state.active;
    let cur_name = state.tabs[active]
        .entries
        .get(state.tabs[active].cursor)
        .map(|i| i.name.clone());
    let cfg = state.config.clone();
    let mut err: Option<String> = None;
    if let Some(tab) = state.tabs.get_mut(active) {
        if let Err(e) = tab.refresh(cfg.clone()) {
            err = Some(format!("自动刷新失败: {e}"));
        } else if let Err(e) = tab.right.refresh(cfg) {
            err = Some(format!("自动刷新失败: {e}"));
        }
    }
    if err.is_some() {
        return err;
    }
    let tab = &state.tabs[active];
    let new_cursor = cur_name
        .as_deref()
        .and_then(|n| tab.entries.iter().position(|i| i.name == n))
        .unwrap_or_else(|| tab.cursor.min(tab.entries.len().saturating_sub(1)));
    state.tabs[active].cursor = new_cursor;
    None
}

/// 后台作业执行体（M13-8）：对 `base` 下 `names` 逐项 copy/move 到 target。
/// 返回（撤销记录列表, 终态）；纯同步，供后台线程与测试共用。
pub(crate) fn execute_job_body(
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
pub(crate) fn jobs_menu_key(
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
pub(crate) struct JobSpec<'a> {
    pub(crate) op: &'a str,
    pub(crate) names: &'a [String],
    pub(crate) base: &'a std::path::Path,
    pub(crate) target: &'a std::path::Path,
    pub(crate) tab_id: usize,
}

/// spawn 后台线程执行作业并把更新发送回主线程（M13-8 run 侧薄壳）。
/// M24-9：`cancel` 为作业的取消标志（AppState 侧与线程共享）。
pub(crate) fn spawn_job_thread(
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
