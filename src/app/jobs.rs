use super::*;

/// 刷新 tab 的双 pane（M14-1）：左 pane 走 Tab::refresh，右 pane 走 Pane::refresh。
pub(crate) fn refresh_tab_panes(state: &mut AppState, tab_id: usize) {
    // M92-1：物理目录走后台 ListLoader（generation 丢弃旧结果），UI 线程不
    // 再同步 read_dir + 逐条 metadata；归档/远端仍走原同步刷新。
    let cfg = state.config.clone();
    let (left_dir, right_dir) = state
        .tabs
        .get(tab_id)
        .map(|tab| {
            (
                match &tab.location {
                    Location::Physical(p) => Some(p.clone()),
                    _ => None,
                },
                match &tab.right.location {
                    Location::Physical(p) => Some(p.clone()),
                    _ => None,
                },
            )
        })
        .unwrap_or((None, None));
    if let Some(dir) = left_dir {
        state.request_dir_load(tab_id, false, &dir, cfg.clone());
    }
    if let Some(dir) = right_dir {
        state.request_dir_load(tab_id, true, &dir, cfg.clone());
    }
    if let Some(tab) = state.tabs.get_mut(tab_id) {
        if !matches!(tab.location, Location::Physical(_)) {
            let _ = tab.refresh(cfg.clone());
        }
        if !matches!(tab.right.location, Location::Physical(_)) {
            let _ = tab.right.refresh(cfg);
        }
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

/// M28-3：文件边界暂停等待——`pause` 置位期间每 100ms 轮询，
/// `cancel` 置位立即返回（暂停中取消即刻终止，不无限阻塞）。
fn wait_while_paused(
    pause: &std::sync::atomic::AtomicBool,
    cancel: &std::sync::atomic::AtomicBool,
) {
    while pause.load(std::sync::atomic::Ordering::Relaxed)
        && !cancel.load(std::sync::atomic::Ordering::Relaxed)
    {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

/// M26-1：归档创建作业体——`rest = "<fmt>:<cs>"`。产物（含扩展名补齐 /
/// 同名不覆盖，由 `create_archive` 保证）与校验文件各入一条撤销记录；
/// 失败 Err（半成品已由 create_archive 清理）。
fn execute_archive_job(
    rest: &str,
    names: &[String],
    base: &std::path::Path,
    target: &std::path::Path,
    tab_id: usize,
    controls: &crate::state::JobControls<'_>,
) -> (Vec<crate::state::UndoableOp>, crate::state::JobStatus) {
    use crate::state::{JobStatus, UndoableOp};
    // M28-3：暂停在启动边界等待（归档创建为整段原子操作，无逐项暂停点）。
    wait_while_paused(controls.pause, controls.cancel);
    if controls.cancel.load(std::sync::atomic::Ordering::Relaxed) {
        return (
            Vec::new(),
            JobStatus::Cancelled {
                summary: "archive: 已取消".to_owned(),
            },
        );
    }
    let mut seg = rest.splitn(2, ':');
    let fmt = seg.next().unwrap_or("zip");
    let cs_str = seg.next().unwrap_or("md5");
    let Some(format) = ops::ArchiveFormat::parse(fmt) else {
        return (
            Vec::new(),
            JobStatus::Failed {
                err: format!("archive: 未知格式 {fmt}"),
            },
        );
    };
    let checksum = ops::Checksum::parse(cs_str).unwrap_or(ops::Checksum::Md5);
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    match ops::create_archive(tab_id, base, &name, format, names) {
        Ok(op) => {
            let dest = match &op {
                UndoableOp::Create { path, .. } => path.clone(),
                _ => std::path::PathBuf::new(),
            };
            let mut ops_out = vec![op];
            // M25-4：校验文件（失败不回滚归档——归档已入撤销栈可 u 移除）。
            if let Ok(Some(c)) = ops::write_checksum(&dest, checksum) {
                ops_out.push(UndoableOp::Create {
                    tab_id,
                    path: c,
                    recursive: false,
                });
            }
            let cs_label = match checksum {
                ops::Checksum::Md5 => "，已生成 .md5",
                ops::Checksum::Sha256 => "，已生成 .sha256",
                ops::Checksum::None => "",
            };
            (
                ops_out,
                JobStatus::Done {
                    summary: format!(
                        "archive: 已创建 {}（{} 项{cs_label}，u 可撤销）",
                        dest.display().to_string().replace('\\', "/"),
                        names.len()
                    ),
                },
            )
        }
        Err(e) => (
            Vec::new(),
            JobStatus::Failed {
                err: format!("archive: {e}"),
            },
        ),
    }
}

/// 后台作业执行体（M13-8）：对 `base` 下 `names` 逐项 copy/move 到 target。
/// M26-1：`op = "archive:<fmt>:<cs>"` → 归档创建（tags 打包 + 校验文件，
/// 产物与校验各入一条撤销记录）。返回（撤销记录列表, 终态）；纯同步，
/// 供后台线程与测试共用。
pub(crate) fn execute_job_body(
    op: &str,
    names: &[String],
    base: &std::path::Path,
    target: &std::path::Path,
    tab_id: usize,
    controls: &crate::state::JobControls<'_>,
) -> (Vec<crate::state::UndoableOp>, crate::state::JobStatus) {
    // M26-1：归档创建分支——整段原子（不做逐项取消；取消在启动前检查）。
    if let Some(rest) = op.strip_prefix("archive:") {
        return execute_archive_job(rest, names, base, target, tab_id, controls);
    }
    let mut ops = Vec::new();
    let mut first_err: Option<String> = None;
    let mut done = 0usize;
    let mut cancelled = false;
    for name in names {
        // M28-3：文件边界等待暂停（cancel 优先——暂停中取消立即终止）。
        wait_while_paused(controls.pause, controls.cancel);
        // M24-9：协作式取消——文件边界检查标志，已完成项保留（u 可逆）。
        if controls.cancel.load(std::sync::atomic::Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        let src = base.join(name);
        if !src.exists() {
            continue; // 已消失的条目跳过
        }
        // M28-3：字节计数（传输量按源条目大小）。
        let item_bytes = std::fs::metadata(&src).map(|m| m.len()).unwrap_or(0);
        let result = if op == "copy" {
            crate::ops::copy_to(tab_id, &src, target)
        } else {
            crate::ops::move_to(tab_id, &src, target)
        };
        match result {
            Ok(uop) => {
                ops.push(uop);
                done += 1;
                use std::sync::atomic::Ordering;
                controls.progress.processed.fetch_add(1, Ordering::Relaxed);
                controls
                    .progress
                    .bytes
                    .fetch_add(item_bytes, Ordering::Relaxed);
                controls.progress.record();
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
/// M28-3：`:jobs` 菜单按键结果——None 继续菜单、Close 关闭、
/// OpenDetail 打开详情覆盖层（Enter）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JobsMenuOutcome {
    None,
    Close,
    OpenDetail,
}

pub(crate) fn jobs_menu_key(
    state: &mut AppState,
    cursor: &mut usize,
    code: KeyCode,
    msg: &mut Option<String>,
) -> JobsMenuOutcome {
    match code {
        KeyCode::Esc | KeyCode::Char('q') => JobsMenuOutcome::Close,
        KeyCode::Char('j') | KeyCode::Down => {
            if !state.jobs.is_empty() {
                *cursor = (*cursor + 1).min(state.jobs.len() - 1);
            }
            JobsMenuOutcome::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            *cursor = cursor.saturating_sub(1);
            JobsMenuOutcome::None
        }
        KeyCode::Char('x') => {
            if state.cancel_job(*cursor) {
                let id = state.jobs[*cursor].id;
                *msg = Some(format!("已请求取消作业 #{id}（当前文件完成后停止）"));
            }
            JobsMenuOutcome::None
        }
        KeyCode::Char('p') => {
            // M28-3：暂停/恢复切换（仅 Running 且未请求取消的作业生效）。
            let (id, paused) = match state.jobs.get(*cursor) {
                Some(j) => (j.id, state.job_paused(*cursor)),
                None => (0, false),
            };
            if state.pause_job(*cursor, !paused) {
                *msg = Some(if paused {
                    format!("作业 #{id} 已恢复运行")
                } else {
                    format!("作业 #{id} 已暂停（p 恢复；文件边界生效）")
                });
            } else {
                *msg = Some("该作业不可暂停（已完成/已取消/归档启动前场景）".to_owned());
            }
            JobsMenuOutcome::None
        }
        KeyCode::Enter => {
            if !state.jobs.is_empty() {
                JobsMenuOutcome::OpenDetail
            } else {
                JobsMenuOutcome::None
            }
        }
        _ => JobsMenuOutcome::None,
    }
}

/// M36-8：递归远端目录遍历（list walk；返回 (绝对路径, 相对路径) 列表）。
/// 万条上限（与 :find/:tree 同口径）。
#[allow(dead_code)]
pub(crate) fn walk_remote(
    backend: &dyn crate::remote::RemoteFs,
    root: &Path,
) -> Result<Vec<(std::path::PathBuf, String)>, crate::remote::RemoteError> {
    const MAX: usize = 10_000;
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in backend.list(&dir)? {
            let full = dir.join(&entry.name);
            let rel = full
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| entry.name.clone());
            if entry.is_dir {
                if out.len() >= MAX {
                    break;
                }
                stack.push(full);
            } else {
                if out.len() >= MAX {
                    break;
                }
                out.push((full, rel));
            }
        }
    }
    Ok(out)
}

/// M36-4：Transfer 传输方向（跨 VFS 三方向；本地→本地不走 Transfer）。
/// M36-7 `:queue` 接线后移除 allow（届时 paste/queue 统一路由到执行体）。
/// M40-1：pub（lib 化——state::TransferQueueEntry 公开字段引用此类型，
/// private_interfaces lint）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferDirection {
    /// 本地 → 远端（本地 read → 远端 write）。
    Upload,
    /// 远端 → 本地（远端 read → 本地写临时 + rename + mtime）。
    Download,
    /// M36-8：本地目录 → 远端（walk 本地树 + 逐文件 Upload）。
    RecurseUpload,
    /// M36-8：远端目录 → 本地（walk 远端树 + 逐文件 Download）。
    RecurseDownload,
}

/// M36-4：Transfer 执行体（纯同步，供后台线程与测试共用）。
/// `entries` = (来源路径, 目标路径) 对；冲突（目标已存在）→ 逐项失败
/// 汇总 Failed（M36-5 策略层接管前保持"同名失败"口径）；取消在条目边界；
/// 下载写走本地临时文件 + 原子 rename + 远端 mtime 保留；失败清理现场。
/// M36-7 `:queue` 接线后移除 allow（届时 paste/queue 统一路由到执行体）。
#[allow(dead_code)]
pub(crate) fn execute_transfer_body(
    direction: TransferDirection,
    backend: &dyn crate::remote::RemoteFs,
    entries: &[(std::path::PathBuf, std::path::PathBuf)],
    tab_id: usize,
    remote_name: &str,
    controls: &crate::state::JobControls,
) -> (Vec<crate::state::UndoableOp>, crate::state::JobStatus) {
    use crate::state::{JobStatus, RemoteUndo, UndoableOp};
    use std::sync::atomic::Ordering;
    let mut ops = Vec::new();
    let mut done = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let mut cancelled = false;
    // M36-8：Recurse 方向先展开目录树为逐文件 entries（IO 失败 → Failed
    // 汇总，不 panic）。
    let entries: Vec<(std::path::PathBuf, std::path::PathBuf)> = match direction {
        TransferDirection::RecurseUpload => {
            let mut expanded = Vec::new();
            for (src_dir, dest_dir) in entries {
                let mut stack = vec![src_dir.clone()];
                while let Some(dir) = stack.pop() {
                    let rd = match std::fs::read_dir(&dir) {
                        Ok(rd) => rd,
                        Err(e) => {
                            failures.push(format!("{}: {e}", dir.display()));
                            continue;
                        }
                    };
                    for entry in rd.flatten() {
                        let rel = entry
                            .path()
                            .strip_prefix(src_dir)
                            .map(|p| p.to_path_buf())
                            .unwrap_or_else(|_| entry.path());
                        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                        if is_dir {
                            stack.push(entry.path());
                        } else {
                            expanded.push((entry.path(), dest_dir.join(&rel)));
                        }
                    }
                }
            }
            expanded
        }
        TransferDirection::RecurseDownload => {
            let mut expanded = Vec::new();
            for (src_dir, dest_dir) in entries {
                match walk_remote(backend, src_dir) {
                    Ok(list) => {
                        for (full, rel) in list {
                            expanded.push((full, dest_dir.join(&rel)));
                        }
                    }
                    Err(e) => {
                        failures.push(format!("{}: {e}", src_dir.display()));
                    }
                }
            }
            expanded
        }
        _ => entries.to_vec(),
    };
    // Recurse 展开后逐文件方向 = 对应单文件方向。
    let file_direction = match direction {
        TransferDirection::RecurseUpload => TransferDirection::Upload,
        TransferDirection::RecurseDownload => TransferDirection::Download,
        other => other,
    };
    for (src, dest) in &entries {
        wait_while_paused(controls.pause, controls.cancel);
        if controls.cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        // 冲突预检（M36-5 策略前保持"同名失败"口径）。
        let conflict = match file_direction {
            TransferDirection::Upload => backend.metadata(dest).is_ok(),
            TransferDirection::Download => dest.exists(),
            TransferDirection::RecurseUpload | TransferDirection::RecurseDownload => {
                unreachable!("file_direction 已映射为单文件方向")
            }
        };
        if conflict {
            failures.push(format!("目标已存在: {}", dest.display()));
            continue;
        }
        let part = resume_part_path("xfer", dest);
        let result = match file_direction {
            TransferDirection::RecurseUpload | TransferDirection::RecurseDownload => {
                unreachable!("file_direction 已映射")
            }
            TransferDirection::Upload => {
                let data = std::fs::read(src);
                match data {
                    Ok(d) => backend.write(dest, &d).map(|()| UndoableOp::Remote {
                        tab_id,
                        remote: remote_name.to_owned(),
                        kind: RemoteUndo::Create {
                            path: dest.clone(),
                            is_dir: false,
                        },
                    }),
                    Err(e) => Err(crate::remote::RemoteError::Other(format!(
                        "本地读取失败: {e}"
                    ))),
                }
            }
            TransferDirection::Download => {
                let data = backend.read(src);
                match data {
                    Ok(d) => {
                        // 本地临时文件 + 原子 rename（半成品不可见）+ mtime。
                        let tmp = dest.with_extension("part-dl");
                        let w = std::fs::write(&tmp, &d).and_then(|()| {
                            // 远端 mtime > 0 时保留（嵌套 if 合并——clippy collapsible）。
                            if let Some(meta) = backend.metadata(src).ok().filter(|m| m.mtime > 0) {
                                let mt = std::time::UNIX_EPOCH
                                    + std::time::Duration::from_secs(meta.mtime);
                                let _ =
                                    std::fs::File::options()
                                        .write(true)
                                        .open(&tmp)
                                        .and_then(|f| {
                                            f.set_times(std::fs::FileTimes::new().set_modified(mt))
                                        });
                            }
                            std::fs::rename(&tmp, dest)
                        });
                        match w {
                            Ok(()) => Ok(UndoableOp::Create {
                                tab_id,
                                path: dest.clone(),
                                recursive: false,
                            }),
                            Err(e) => {
                                let _ = std::fs::remove_file(&tmp);
                                Err(crate::remote::RemoteError::Other(format!(
                                    "本地写入失败: {e}"
                                )))
                            }
                        }
                    }
                    Err(e) => Err(e),
                }
            }
        };
        match result {
            Ok(op) => {
                ops.push(op);
                done += 1;
                controls.progress.processed.fetch_add(1, Ordering::Relaxed);
                controls.progress.record();
            }
            Err(e) => {
                let _ = std::fs::remove_file(&part);
                failures.push(format!("{}: {e}", src.display()));
            }
        }
    }
    let status = if cancelled {
        JobStatus::Cancelled {
            summary: format!("transfer: 已取消（{done} 项完成）"),
        }
    } else if failures.len() == entries.len() && !entries.is_empty() {
        JobStatus::Failed {
            err: failures.join("; "),
        }
    } else {
        let mut summary = format!("transfer: {done} 项完成");
        if !failures.is_empty() {
            summary.push_str(&format!("; 失败 {}: {}", failures.len(), failures[0]));
        }
        JobStatus::Done { summary }
    };
    (ops, status)
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
    pause: std::sync::Arc<std::sync::atomic::AtomicBool>,
    progress: std::sync::Arc<crate::state::JobProgress>,
) {
    let tx = job_tx.clone();
    let op = spec.op.to_owned();
    let names = spec.names.to_vec();
    let base = spec.base.to_path_buf();
    let target = spec.target.to_path_buf();
    let tab_id = spec.tab_id;
    std::thread::spawn(move || {
        let controls = crate::state::JobControls {
            cancel: &cancel,
            pause: &pause,
            progress: &progress,
        };
        let (ops, status) = execute_job_body(&op, &names, &base, &target, tab_id, &controls);
        let _ = tx.send(crate::state::JobUpdate {
            job_id,
            tab_id,
            ops,
            status,
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::AtomicBool;
    use tempfile::TempDir;

    /// M36-4 mock：read/write/metadata 存储（上传/下载执行体测试）。
    struct TransferMockFs {
        store: std::sync::Mutex<std::collections::BTreeMap<String, Vec<u8>>>,
    }

    impl TransferMockFs {
        fn new() -> Self {
            Self {
                store: std::sync::Mutex::new(std::collections::BTreeMap::new()),
            }
        }
        fn put(&self, path: &str, data: &[u8]) {
            self.store
                .lock()
                .unwrap()
                .insert(path.to_owned(), data.to_vec());
        }
    }

    impl crate::remote::RemoteFs for TransferMockFs {
        fn list(
            &self,
            _path: &Path,
        ) -> Result<Vec<crate::remote::RemoteEntry>, crate::remote::RemoteError> {
            Ok(Vec::new())
        }
        fn read(&self, path: &Path) -> Result<Vec<u8>, crate::remote::RemoteError> {
            self.store
                .lock()
                .unwrap()
                .get(&path.to_string_lossy().into_owned())
                .cloned()
                .ok_or_else(|| {
                    crate::remote::RemoteError::NotFound(path.to_string_lossy().into_owned())
                })
        }
        fn metadata(
            &self,
            path: &Path,
        ) -> Result<crate::remote::RemoteMeta, crate::remote::RemoteError> {
            let s = self.store.lock().unwrap();
            match s.get(&path.to_string_lossy().into_owned()) {
                Some(d) => Ok(crate::remote::RemoteMeta {
                    size: d.len() as u64,
                    mtime: 0,
                    is_dir: false,
                }),
                None => Err(crate::remote::RemoteError::NotFound(
                    path.to_string_lossy().into_owned(),
                )),
            }
        }
        fn write(&self, path: &Path, data: &[u8]) -> Result<(), crate::remote::RemoteError> {
            self.store
                .lock()
                .unwrap()
                .insert(path.to_string_lossy().into_owned(), data.to_vec());
            Ok(())
        }
    }

    #[test]
    fn m364_transfer_upload_and_download_roundtrip() {
        // M36-4：Upload（本地 → 远端 write）+ Download（远端 → 本地临时
        // + rename）往返一致；进度计数递增。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("up.txt"), b"UP").unwrap();
        let mock = TransferMockFs::new();
        let cancel = AtomicBool::new(false);
        let pause = AtomicBool::new(false);
        let progress = crate::state::JobProgress::default();
        let controls = crate::state::JobControls {
            cancel: &cancel,
            pause: &pause,
            progress: &progress,
        };

        let (ops, status) = execute_transfer_body(
            TransferDirection::Upload,
            &mock,
            &[(tmp.path().join("up.txt"), PathBuf::from("/up.txt"))],
            0,
            "r",
            &controls,
        );
        assert!(matches!(status, crate::state::JobStatus::Done { .. }));
        assert_eq!(
            crate::remote::RemoteFs::read(&mock, Path::new("/up.txt")).unwrap(),
            b"UP"
        );
        assert_eq!(ops.len(), 1);
        assert!(matches!(ops[0], crate::state::UndoableOp::Remote { .. }));
        assert_eq!(
            progress
                .processed
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );

        let (ops2, status2) = execute_transfer_body(
            TransferDirection::Download,
            &mock,
            &[(PathBuf::from("/up.txt"), tmp.path().join("down.txt"))],
            0,
            "r",
            &controls,
        );
        assert!(matches!(status2, crate::state::JobStatus::Done { .. }));
        assert_eq!(fs::read(tmp.path().join("down.txt")).unwrap(), b"UP");
        assert_eq!(ops2.len(), 1);
        assert!(matches!(ops2[0], crate::state::UndoableOp::Create { .. }));
    }

    #[test]
    fn m364_transfer_conflict_and_cancel() {
        // M36-4：目标已存在 → Failed 汇总（M36-5 策略前同名失败）；
        // 取消 → Cancelled。
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("f.txt"), b"F").unwrap();
        let mock = TransferMockFs::new();
        mock.put("/f.txt", b"EXIST");
        let pause = AtomicBool::new(false);
        let progress = crate::state::JobProgress::default();
        let controls = crate::state::JobControls {
            cancel: &AtomicBool::new(false),
            pause: &pause,
            progress: &progress,
        };
        let (ops, status) = execute_transfer_body(
            TransferDirection::Upload,
            &mock,
            &[(tmp.path().join("f.txt"), PathBuf::from("/f.txt"))],
            0,
            "r",
            &controls,
        );
        assert!(ops.is_empty());
        assert!(matches!(status, crate::state::JobStatus::Failed { .. }));

        // 取消 → Cancelled。
        let controls2 = crate::state::JobControls {
            cancel: &AtomicBool::new(true),
            pause: &pause,
            progress: &progress,
        };
        fs::write(tmp.path().join("g.txt"), b"G").unwrap();
        let (ops, status) = execute_transfer_body(
            TransferDirection::Upload,
            &mock,
            &[(tmp.path().join("g.txt"), PathBuf::from("/g.txt"))],
            0,
            "r",
            &controls2,
        );
        assert!(ops.is_empty());
        assert!(matches!(status, crate::state::JobStatus::Cancelled { .. }));
    }

    #[test]
    fn wait_while_paused_returns_immediately_when_cancelled() {
        // M28-3：暂停中取消——不阻塞、立即返回（cancel 优先）。
        let pause = AtomicBool::new(true);
        let cancel = AtomicBool::new(true);
        let t0 = std::time::Instant::now();
        wait_while_paused(&pause, &cancel);
        assert!(t0.elapsed() < std::time::Duration::from_secs(1));
    }

    #[test]
    fn wait_while_paused_exits_when_unpaused() {
        // M28-3：暂停阻塞——另一线程 50ms 后解除，等待正常退出。
        let pause = std::sync::Arc::new(AtomicBool::new(true));
        let cancel = AtomicBool::new(false);
        let p2 = pause.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            p2.store(false, std::sync::atomic::Ordering::Relaxed);
        });
        let t0 = std::time::Instant::now();
        wait_while_paused(&pause, &cancel);
        assert!(
            t0.elapsed() >= std::time::Duration::from_millis(40),
            "暂停期间确实等待"
        );
        assert!(t0.elapsed() < std::time::Duration::from_secs(3));
    }

    #[test]
    fn job_body_counts_progress_and_bytes() {
        // M28-3：进度计数——processed/bytes 随复制条目递增。
        let tmp = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.txt"), "AAAA").unwrap();
        fs::write(tmp.path().join("b.txt"), "BB").unwrap();
        let cancel = AtomicBool::new(false);
        let pause = AtomicBool::new(false);
        let progress = crate::state::JobProgress::default();
        let controls = crate::state::JobControls {
            cancel: &cancel,
            pause: &pause,
            progress: &progress,
        };
        let (ops, status) = execute_job_body(
            "copy",
            &["a.txt".to_owned(), "b.txt".to_owned()],
            tmp.path(),
            dst.path(),
            0,
            &controls,
        );
        assert_eq!(ops.len(), 2);
        assert!(matches!(status, crate::state::JobStatus::Done { .. }));
        assert_eq!(progress.processed(), 2);
        assert_eq!(progress.bytes(), 6, "按源条目大小计（4+2 字节）");
    }
}
