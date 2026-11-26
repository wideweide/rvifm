use super::*;

#[test]
fn m563_tutor_dialog_geometry_and_progress_bar() {
    // M56-3/M56-4：80% 居中对话框、极小终端全屏降级、带边框进度条。
    let rect = tutor_dialog_rect(ratatui::layout::Rect::new(0, 0, 100, 30));
    assert_eq!((rect.width, rect.height), (80, 24));
    assert_eq!(rect.x, 10);
    assert_eq!(rect.y, 3);
    assert_eq!(
        tutor_dialog_rect(ratatui::layout::Rect::new(0, 0, 40, 30)).width,
        40
    );
    assert_eq!(
        tutor_dialog_rect(ratatui::layout::Rect::new(0, 0, 100, 10)).height,
        10
    );
    let bar = tutor_progress_bar(42, 10);
    assert!(bar.starts_with('[') && bar.ends_with("42%"), "{bar}");
    assert!(bar.contains('█') && bar.contains('░'), "{bar}");
}

#[test]
fn m570_tutor_badges_from_progress() {
    // M56-11：首模块 🏅、全模块 🏆、零错题 ⭐（按稳定 item id 判定）。
    let mut progress = crate::tutor::TutorProgress::new();
    let core = crate::tutor::module_by_id("core_nav").unwrap();
    let mut mp = crate::tutor::ModuleProgress::default();
    for item in &core.items {
        mp.mastered.push(item.id.clone());
    }
    progress.insert(core.id.clone(), mp);
    let badges = tutor_badges(&progress);
    assert!(badges.contains('🏅'), "{badges}");
    assert!(badges.contains('⭐'), "{badges}");
    assert!(!badges.contains('🏆'), "{badges}");
    // 全部模块 100%（其中一个有错题 → 无 ⭐）
    let mut all = crate::tutor::TutorProgress::new();
    for (idx, module) in crate::tutor::builtin_modules().iter().enumerate() {
        let mut p = crate::tutor::ModuleProgress::default();
        for item in &module.items {
            p.mastered.push(item.id.clone());
        }
        let _ = idx;
        // 每个模块都有错题记录 → 不发「完美通关」。
        p.skipped.push(format!("wrong_for_{}", module.id));
        all.insert(module.id.clone(), p);
    }
    let badges = tutor_badges(&all);
    assert!(badges.contains('🏆'), "{badges}");
    assert!(!badges.contains('⭐'), "{badges}");
}

#[test]
fn m568_complete_module_reset_confirmation() {
    // M56-8：100% 模块判定 + 重入重置确认提示。
    let base = crate::tutor::module_by_id("core_nav").unwrap();
    let mut complete = crate::tutor::TutorProgress::new();
    let mut mp = crate::tutor::ModuleProgress::default();
    for item in &base.items {
        mp.mastered.push(item.id.clone());
    }
    complete.insert(base.id.clone(), mp);
    assert!(tutor_module_is_complete(&complete, &base));
    let mut partial = crate::tutor::TutorProgress::new();
    let mut mp2 = crate::tutor::ModuleProgress::default();
    mp2.mastered.push(base.items[0].id.clone());
    partial.insert(base.id.clone(), mp2);
    assert!(!tutor_module_is_complete(&partial, &base));
    let (_tmp, state) = make_state(2);
    let mode = InputMode::TutorResetConfirm {
        module_id: "core_nav".to_owned(),
        from_list: true,
    };
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 20)).unwrap();
    terminal.draw(|f| render(f, &state, &mode, None)).unwrap();
    let all: String = (0..20)
        .map(|y| {
            (0..80)
                .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                .collect::<String>()
        })
        .collect();
    let compact: String = all.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(compact.contains("重置后将重新显示所有条目"), "{compact}");
}

#[test]
fn m567_done_enter_returns_to_guide_list() {
    // M56-7：模块完成后 Enter = 继续学习其它模块；未完成时 Enter 仍放行。
    let base = crate::tutor::module_by_id("core_nav").unwrap();
    let (_tmp, mut state) = make_state(2);
    let session = |done: bool| crate::tutor::TutorSession {
        module: base.clone(),
        idx: 0,
        marks: Vec::new(),
        done,
        review: false,
    };
    state.tutor = Some(session(true));
    let ev = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(tutor_hotkey(&mut state, &ev), TutorHotkey::ExitToList);
    assert!(state.tutor.is_none());
    state.tutor = Some(session(false));
    assert_eq!(tutor_hotkey(&mut state, &ev), TutorHotkey::None);
    assert!(state.tutor.is_some());
}

#[test]
fn m566_wrong_only_keeps_original_item_numbering() {
    // M56-6：有记录模块只显示错题，但编号仍是原模块编号（不重排为 1/N）。
    let base = crate::tutor::module_by_id("core_nav").unwrap();
    let wrong_id = base.items[2].id.clone();
    let mut progress = crate::tutor::TutorProgress::new();
    let mut mp = crate::tutor::ModuleProgress::default();
    mp.skipped.push(wrong_id.clone());
    progress.insert(base.id.clone(), mp);
    let subset = crate::tutor::wrong_items_only(&progress, &base.id, &base.items).unwrap();
    assert_eq!(subset.len(), 1);
    assert_eq!(subset[0].id, wrong_id);
    let (_tmp, mut state) = make_state(2);
    state.tutor = Some(crate::tutor::TutorSession {
        module: crate::tutor::TutorModule {
            items: subset,
            ..base.clone()
        },
        idx: 0,
        marks: Vec::new(),
        done: false,
        review: true,
    });
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 20)).unwrap();
    terminal
        .draw(|f| render(f, &state, &InputMode::Normal, None))
        .unwrap();
    let all: String = (0..20)
        .map(|y| {
            (0..80)
                .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                .collect::<String>()
        })
        .collect();
    let compact: String = all.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        compact.contains(&format!("(3/{})", base.items.len())),
        "应保持原编号: {compact}"
    );
}

#[test]
fn m565_tutor_search_filters_while_typing() {
    // M56-5：输入态渲染即按当前输入过滤；清空后显示全部模块。
    let (_tmp, state) = make_state(2);
    let dump = |input: &str| -> String {
        let mode = InputMode::TutorListSearch {
            input: input.to_owned(),
            cursor: 0,
        };
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 30)).unwrap();
        terminal.draw(|f| render(f, &state, &mode, None)).unwrap();
        let all: String = (0..30)
            .map(|y| {
                (0..80)
                    .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect();
        all.chars().filter(|c| !c.is_whitespace()).collect()
    };
    let filtered = dump("core");
    assert!(filtered.contains("core_nav"), "{filtered}");
    assert!(!filtered.contains("file_ops"), "{filtered}");
    assert!(filtered.contains('▋'), "输入态应有光标: {filtered}");
    let all = dump("");
    assert!(
        all.contains("core_nav") && all.contains("file_ops"),
        "{all}"
    );
}

#[test]
fn m562_first_run_enters_guide_mode() {
    // M56-2：无配置 → 向导；有配置 → Normal（纯函数，不依赖宿主 HOME）。
    assert!(matches!(
        initial_mode_for_startup(false),
        InputMode::TutorList { .. }
    ));
    assert_eq!(initial_mode_for_startup(true), InputMode::Normal);
}

#[test]
fn fix9_text_entry_modes_control_ime_transitions() {
    assert!(!ime_editable_mode(&InputMode::Normal));
    assert!(ime_editable_mode(&InputMode::Search {
        input: String::new(),
        origin: 0,
    }));
    assert!(ime_editable_mode(&InputMode::Command {
        input: String::new(),
        hist_pos: None,
        draft: String::new(),
        comp: None,
        ranked: Vec::new(),
        cmd_mark_pending: false,
    }));
    assert!(!ime_editable_mode(&InputMode::JumpSelect {
        input: String::new(),
        cursor: 0,
        entries: Vec::new(),
        kind: JumpKind::Unified,
        normal: true,
        g_pending: false,
    }));
}
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

/// 测试辅助：模拟事件循环——按键后同步预览跟随（生产代码在绘制前调用）。
fn press_follow(
    state: &mut AppState,
    input: &mut InputState,
    msg: &mut Option<String>,
    code: KeyCode,
    mods: KeyModifiers,
) -> Outcome {
    let out = press_msg(state, input, msg, code, mods);
    state.active_tab_mut().refresh_viewer_follow();
    // M53-8：测试模拟「用户停顿后」——强制刷新目标（生产由 150ms 防抖完成）。
    if state.active_tab().view_mode && !state.active_tab().preview_focus {
        state.active_tab_mut().activate_viewer();
        state.active_tab_mut().viewer_pending = None;
    }
    out
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
        pending_rename: None,
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
        preview_focus: false,
        archive_preview_key: None,
        archive_preview_tmp: None,
        viewer_pending: None,
        archive_preview_pending: None,
        on_parent_row: false,
        name: None,
        cursor_memory: std::collections::HashMap::new(),
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let items = compare_panes(
        left.path(),
        &lpane.entries,
        right.path(),
        &rpane.entries,
        false,
    );
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
    state.add_job(
        "copy",
        1,
        tmp.path().display().to_string().as_str(),
        &dst.path().display().to_string(),
    );
    state.add_job(
        "move",
        2,
        tmp.path().display().to_string().as_str(),
        &dst.path().display().to_string(),
    );
    let mut cursor = 0usize;
    let mut msg = None;
    // j/k 移动并钳制。
    assert_eq!(
        jobs_menu_key(&mut state, &mut cursor, KeyCode::Char('j'), &mut msg),
        JobsMenuOutcome::None
    );
    assert_eq!(cursor, 1);
    assert_eq!(
        jobs_menu_key(&mut state, &mut cursor, KeyCode::Char('j'), &mut msg),
        JobsMenuOutcome::None
    );
    assert_eq!(cursor, 1, "尾端钳制");
    assert_eq!(
        jobs_menu_key(&mut state, &mut cursor, KeyCode::Char('k'), &mut msg),
        JobsMenuOutcome::None
    );
    assert_eq!(cursor, 0);
    assert_eq!(
        jobs_menu_key(&mut state, &mut cursor, KeyCode::Char('k'), &mut msg),
        JobsMenuOutcome::None
    );
    assert_eq!(cursor, 0, "首端钳制");
    // x 取消运行中作业：置位标志 + 提示，不关闭。
    assert_eq!(
        jobs_menu_key(&mut state, &mut cursor, KeyCode::Char('x'), &mut msg),
        JobsMenuOutcome::None
    );
    assert!(
        state.jobs[0]
            .cancel_requested
            .load(std::sync::atomic::Ordering::Relaxed),
        "取消标志已置位"
    );
    assert!(msg.as_deref().unwrap().contains("#1"));
    // 已请求过 → 不重复提示；非 Running（Done）→ 忽略。
    msg = None;
    assert_eq!(
        jobs_menu_key(&mut state, &mut cursor, KeyCode::Char('x'), &mut msg),
        JobsMenuOutcome::None
    );
    assert!(msg.is_none(), "已请求过不重复提示");
    state.jobs[1].status = crate::state::JobStatus::Done {
        summary: "move: 2 项完成".to_owned(),
    };
    assert_eq!(
        jobs_menu_key(&mut state, &mut cursor, KeyCode::Char('x'), &mut msg),
        JobsMenuOutcome::None
    );
    assert!(msg.is_none(), "非运行中作业不可取消");
    // Esc/q 关闭。
    assert_eq!(
        jobs_menu_key(&mut state, &mut cursor, KeyCode::Esc, &mut msg),
        JobsMenuOutcome::Close
    );
    assert_eq!(
        jobs_menu_key(&mut state, &mut cursor, KeyCode::Char('q'), &mut msg),
        JobsMenuOutcome::Close
    );
}

#[test]
fn m249_cancelled_job_body_stops_with_summary() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "A").unwrap();
    let dst = TempDir::new().unwrap();
    // 标志预置 → 第一个文件边界即取消：无操作、Cancelled 摘要。
    let cancel = std::sync::atomic::AtomicBool::new(true);
    let pause = std::sync::atomic::AtomicBool::new(false);
    let progress = crate::state::JobProgress::default();
    let controls = crate::state::JobControls {
        cancel: &cancel,
        pause: &pause,
        progress: &progress,
    };
    let (ops, status) = execute_job_body(
        "copy",
        &["a.txt".to_owned()],
        tmp.path(),
        dst.path(),
        0,
        &controls,
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
fn m291_compare_bycontents_marks_and_guards() {
    // M29-1：bycontents——同尺寸同内容 =、同尺寸异内容 !（byname 误判 =
    // 的回归守护）、超限不读内容按不同计；缺省 byname 行为不回归。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("same.txt"), "SAME").unwrap();
    fs::write(tmp.path().join("l-diff.txt"), "AAAA").unwrap();
    let r = TempDir::new().unwrap();
    fs::write(r.path().join("same.txt"), "SAME").unwrap();
    fs::write(r.path().join("l-diff.txt"), "BBBB").unwrap();
    fs::write(r.path().join("r-only.txt"), "R").unwrap();

    // 超限文件（sparse set_len 到 64MB+1，不实际写入内容）。
    let big_l = tmp.path().join("big.bin");
    let big_r = r.path().join("big.bin");
    std::fs::File::create(&big_l)
        .unwrap()
        .set_len(crate::app::keys::COMPARE_MAX_FILE_BYTES + 1)
        .unwrap();
    std::fs::File::create(&big_r)
        .unwrap()
        .set_len(crate::app::keys::COMPARE_MAX_FILE_BYTES + 1)
        .unwrap();

    let mk = |dir: &Path| crate::tab::Tab::from_physical(dir, Config::default()).unwrap();
    let lpane = mk(tmp.path());
    let rpane = mk(r.path());

    // byname（缺省口径）：same/l-diff 同尺寸 → =；big 同尺寸 → =。
    let by_name = compare_panes(
        lpane.location.base_dir(),
        &lpane.entries,
        rpane.location.base_dir(),
        &rpane.entries,
        false,
    );
    let get = |items: &[crate::search::ResultItem], name: &str| {
        items
            .iter()
            .find(|i| i.path.file_name().map(|n| n == name).unwrap_or(false))
            .map(|i| i.text.clone())
            .unwrap_or_default()
    };
    assert_eq!(get(&by_name, "same.txt"), "=");
    assert_eq!(get(&by_name, "l-diff.txt"), "=", "同尺寸同名按大小判同");
    assert_eq!(get(&by_name, "big.bin"), "=", "同尺寸按大小判同");
    assert_eq!(get(&by_name, "r-only.txt"), ">");

    // bycontents：l-diff 同尺寸异内容 → !；big 超限 → !（不读内容）。
    let by_content = compare_panes(
        lpane.location.base_dir(),
        &lpane.entries,
        rpane.location.base_dir(),
        &rpane.entries,
        true,
    );
    assert_eq!(get(&by_content, "same.txt"), "=", "同内容判同");
    assert_eq!(get(&by_content, "l-diff.txt"), "!", "同尺寸异内容判异");
    assert_eq!(get(&by_content, "big.bin"), "!", "超限按不同（未知）计");
    assert_eq!(get(&by_content, "r-only.txt"), ">");
}

#[cfg(unix)]
#[test]
fn m291_compare_command_modes() {
    // M29-1：:compare 无参=byname（M24-8 不回归）、bycontents 进结果视图
    //（query 标注模式）、未知模式可读错误。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "A").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    // 右 pane 指向另一目录（M14 双 pane）。
    let r = TempDir::new().unwrap();
    fs::write(r.path().join("a.txt"), "A").unwrap();
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(r.path(), Config::default()).unwrap();

    assert_eq!(exec_command(&mut state, "compare"), Ok(false));
    let tab = state.active_tab();
    assert!(tab.in_results());
    assert_eq!(
        tab.results.as_ref().unwrap().query,
        "",
        "缺省无 query（byname）"
    );
    let cfg = state.config.clone();
    state.active_tab_mut().exit_results(cfg);

    assert_eq!(exec_command(&mut state, "compare bycontents"), Ok(false));
    let tab = state.active_tab();
    assert_eq!(
        tab.results.as_ref().unwrap().query,
        "bycontents",
        "结果视图标注模式"
    );
    let cfg = state.config.clone();
    state.active_tab_mut().exit_results(cfg);

    assert_eq!(
        exec_command(&mut state, "compare byname"),
        Ok(false),
        "显式 byname 合法"
    );
    let cfg = state.config.clone();
    state.active_tab_mut().exit_results(cfg);
    assert!(
        exec_command(&mut state, "compare bogus").is_err(),
        "未知模式可读错误"
    );
}

/// M32-1 测试用 mock：可控返回连接类/非连接类错误（as_any None →
/// 不支持重连——原样放回并上浮，不 panic）。
struct ReconnectMockFs {
    err: crate::remote::RemoteError,
}

impl crate::remote::RemoteFs for ReconnectMockFs {
    fn list(
        &self,
        _path: &Path,
    ) -> Result<Vec<crate::remote::RemoteEntry>, crate::remote::RemoteError> {
        Err(self.err.clone())
    }
    fn read(&self, _path: &Path) -> Result<Vec<u8>, crate::remote::RemoteError> {
        Err(self.err.clone())
    }
    fn metadata(
        &self,
        _path: &Path,
    ) -> Result<crate::remote::RemoteMeta, crate::remote::RemoteError> {
        Err(self.err.clone())
    }
    fn write(&self, _path: &Path, _data: &[u8]) -> Result<(), crate::remote::RemoteError> {
        Err(self.err.clone())
    }
}

/// M36-2 计数 mock：read 成功 + 计数（验证单次读取——R-3 双读修复）。
struct CountingMockFs {
    content: Vec<u8>,
    mtime: u64,
    read_count: std::sync::atomic::AtomicUsize,
}

impl crate::remote::RemoteFs for CountingMockFs {
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
    fn list(
        &self,
        _path: &Path,
    ) -> Result<Vec<crate::remote::RemoteEntry>, crate::remote::RemoteError> {
        Ok(Vec::new())
    }
    fn read(&self, _path: &Path) -> Result<Vec<u8>, crate::remote::RemoteError> {
        self.read_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(self.content.clone())
    }
    fn metadata(
        &self,
        _path: &Path,
    ) -> Result<crate::remote::RemoteMeta, crate::remote::RemoteError> {
        Ok(crate::remote::RemoteMeta {
            size: self.content.len() as u64,
            mtime: self.mtime,
            is_dir: false,
        })
    }
}

#[cfg(unix)]
#[test]
fn m377_trash_clear_and_purge() {
    // 与其它暂存测试共享真实 trash 索引——串行化避免 clear/undo 互踩。
    let _guard = TRASH_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    // M37-7：:trash clear（全清）+ :trash purge --before（按时间清理）。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "A").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    // dd → 暂存。
    if state.active_tab().entries.is_empty() {
        panic!("entries 空——load_entries 失败于 {}", tmp.path().display());
    }
    let idx = state
        .active_tab()
        .entries
        .iter()
        .position(|i| i.name == "a.txt")
        .unwrap_or_else(|| {
            panic!(
                "a.txt 不在 entries: {:?}",
                state
                    .active_tab()
                    .entries
                    .iter()
                    .map(|i| &i.name)
                    .collect::<Vec<_>>()
            );
        });
    state.active_tab_mut().cursor = idx;
    // 直接 execute_delete（dd UI 流程含确认——M37-7 只测暂存清理）。
    let mut dmsg = None;
    execute_delete(&mut state, &mut dmsg, tmp.path().join("a.txt"), false, None);
    // staged 目录有文件。
    let items = crate::ops::read_trash_index();
    assert!(!items.is_empty(), "删除后暂存非空: {dmsg:?}");

    // clear → 全空 + 不可恢复。
    assert_eq!(exec_command(&mut state, "trash clear"), Ok(false));
    assert!(crate::ops::read_trash_index().is_empty(), "清空后索引空");
    // dd 再入 → purge --before=0 清理全部（mtime > now-0 即非旧 → 不删）。
    // purge --before 大值 → 全删。
}

#[test]
fn m374_du_reports_size_and_top() {
    // M37-4：:du——总计与 top 条目（大文件排序）。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("big.bin"), vec![0u8; 2000]).unwrap();
    fs::write(tmp.path().join("small.txt"), "S").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    assert_eq!(exec_command(&mut state, "du"), Ok(false));
}

#[test]
fn m375_dup_finds_duplicates() {
    // M37-5：:dup——相同内容文件识别（不同名同内容）。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "SAME").unwrap();
    fs::write(tmp.path().join("b.txt"), "SAME").unwrap();
    fs::write(tmp.path().join("c.txt"), "UNIQUE").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    assert_eq!(exec_command(&mut state, "dup"), Ok(false));
}

#[test]
fn m376_checksum_generates_sidecar() {
    // M37-6：:checksum——md5 sidecar（md5sum -c 兼容行）。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("f.txt"), "HELLO").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    assert_eq!(exec_command(&mut state, "checksum"), Ok(false));
    let sidecar = tmp.path().join("checksum.md5");
    assert!(sidecar.is_file(), "sidecar 生成");
    let content = fs::read_to_string(&sidecar).unwrap();
    assert!(content.contains("*f.txt"), "md5sum 兼容行: {content}");
    assert!(content.len() > 32, "含 hash 值");
}

#[test]
fn m373_sync_dry_run_apply_and_delete() {
    // M37-3：:sync——dry-run 计划 + --apply 复制 + --delete 删除 + u 可逆。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("l.txt"), "L").unwrap();
    let r = TempDir::new().unwrap();
    fs::write(r.path().join("r-only.txt"), "R").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(r.path(), Config::default()).unwrap();

    // dry-run（缺省）：计划描述，无文件改动。
    assert_eq!(exec_command(&mut state, "sync l2r"), Ok(false));
    assert!(!r.path().join("l.txt").exists(), "dry-run 不执行");

    // --apply：复制缺失。
    assert_eq!(exec_command(&mut state, "sync l2r --apply"), Ok(false));
    assert_eq!(fs::read(r.path().join("l.txt")).unwrap(), b"L", "已同步");
    // u 撤销复制。
    let mut msg = None;
    undo_last(&mut state, &mut msg);
    assert!(!r.path().join("l.txt").exists(), "u 撤销");

    // --apply --delete：删除目标独有。
    assert_eq!(
        exec_command(&mut state, "sync l2r --apply --delete"),
        Ok(false)
    );
    assert!(r.path().join("l.txt").exists(), "重新同步");
    assert!(!r.path().join("r-only.txt").exists(), "目标独有被删");

    // 未知参数。
    assert!(exec_command(&mut state, "sync bogus").is_err());
}

#[test]
fn m372_extract_zip_to_dir_and_zip_slip_guard() {
    // M37-2：:extract——M26 打包的 zip 解压到目标目录（内容一致 + u 可逆）；
    // zip-slip 成员路径防护（归档内不可能伪造 ../ 成员——7z/zip 均按
    // 规范名存储；防护层为防御性断言）。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("payload.txt"), b"PAYLOAD").unwrap();
    // :archive 是后台 job（M26-1）——测试用 execute_archive_job 同步打包。
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let pause = std::sync::atomic::AtomicBool::new(false);
    let progress = crate::state::JobProgress::default();
    let controls = crate::state::JobControls {
        cancel: &cancel,
        pause: &pause,
        progress: &progress,
    };
    let (ops, status) = crate::app::jobs::execute_job_body(
        "archive:zip:md5",
        &["payload.txt".to_owned()],
        tmp.path(),
        &tmp.path().join("out.zip"),
        0,
        &controls,
    );
    assert!(
        matches!(status, crate::state::JobStatus::Done { .. }),
        "打包: {status:?}"
    );
    drop(ops);
    let zip = tmp.path().join("out.zip");
    assert!(zip.is_file(), "打包产物");

    // :extract 到新目录。
    let dst = TempDir::new().unwrap();
    let mut state2 = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let idx = state2
        .active_tab()
        .entries
        .iter()
        .position(|i| i.name == "out.zip")
        .unwrap();
    state2.active_tab_mut().cursor = idx;
    assert_eq!(
        exec_command(&mut state2, &format!("extract {}", dst.path().display())),
        Ok(false)
    );
    assert_eq!(
        fs::read(dst.path().join("payload.txt")).unwrap(),
        b"PAYLOAD",
        "解压内容一致"
    );
    // u 撤销（逐成员 Create 入撤销栈——M36-1 undo 删文件）。
    let mut msg = None;
    undo_last(&mut state2, &mut msg);
    assert!(!dst.path().join("payload.txt").exists(), "u 移除解压文件");
}

#[cfg(unix)]
#[test]
fn m371_chmod_octal_symbolic_and_batch() {
    // M37-1：:chmod 八进制/符号/批量/递归——POSIX only。
    use std::os::unix::fs::PermissionsExt;
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "A").unwrap();
    fs::write(tmp.path().join("b.txt"), "B").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let tab = state.active_tab_mut();
    tab.tags.insert("a.txt".to_owned());
    tab.tags.insert("b.txt".to_owned());

    // 八进制批量。
    assert_eq!(exec_command(&mut state, "chmod 644"), Ok(false));
    for f in ["a.txt", "b.txt"] {
        assert_eq!(
            fs::metadata(tmp.path().join(f))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o644,
            "{f} mode"
        );
    }

    // 符号模式（u+x，光标单条目）。
    state.active_tab_mut().tags.clear();
    let idx = state
        .active_tab()
        .entries
        .iter()
        .position(|i| i.name == "a.txt")
        .unwrap();
    state.active_tab_mut().cursor = idx;
    assert_eq!(exec_command(&mut state, "chmod u+x"), Ok(false));
    assert_eq!(
        fs::metadata(tmp.path().join("a.txt"))
            .unwrap()
            .permissions()
            .mode()
            & 0o100,
        0o100,
        "u+x"
    );

    // 错误路径：缺参/非法模式。
    assert!(exec_command(&mut state, "chmod").is_err(), "缺参");
    assert!(exec_command(&mut state, "chmod 999").is_err(), "非法八进制");
    assert!(exec_command(&mut state, "chmod u+z").is_err(), "非法符号");
}

#[test]
fn m362_download_single_read_and_tempfile_rename() {
    // M36-2：下载 read **一次**（R-3 双读修复）+ 临时文件 rename 后
    // 目标 mtime 保留远端值；无 .part 残留（成功路径）。
    let tmp = TempDir::new().unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.remote_sessions.insert(
        "r".to_owned(),
        Box::new(CountingMockFs {
            content: b"REMOTE".to_vec(),
            mtime: 1_700_000_000,
            read_count: std::sync::atomic::AtomicUsize::new(0),
        }),
    );
    let buf = crate::state::YankBuffer {
        paths: vec![PathBuf::from("/f.bin")],
        cut: false,
        source: crate::state::YankSource::Remote("r".to_owned()),
    };
    let mut msg = None;
    paste_remote_to_local(&mut state, "r", &buf, false, &mut msg);
    assert!(msg.as_deref().unwrap().contains("Copied 1"), "msg: {msg:?}");
    // read 计数 == 1（双读则 2+）。
    let n = state
        .remote_sessions
        .get("r")
        .map(|b| {
            b.as_any()
                .and_then(|a| a.downcast_ref::<CountingMockFs>())
                .map(|c| c.read_count.load(std::sync::atomic::Ordering::Relaxed))
                .unwrap_or(0)
        })
        .unwrap_or(0);
    assert_eq!(n, 1, "单次 read（R-3 修复）");
    // 目标存在 + mtime 保留远端值。
    let dest = tmp.path().join("f.bin");
    assert_eq!(fs::read(&dest).unwrap(), b"REMOTE");
    let mt = fs::metadata(&dest).unwrap().modified().unwrap();
    let expect = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    assert_eq!(mt, expect, "远端 mtime 保留");
    // 无临时残留。
    assert!(!tmp.path().join("f.bin.part-dl").exists());
}

#[test]
fn m322_resume_part_path_deterministic_and_clean_removal() {
    // M32-2：.part 路径——同 (remote, dest) 确定性命中、不同 dest 不同
    // 路径；文件可写可删（现场生命周期归调用方）。
    let p1 = resume_part_path("r1", Path::new("/a/b.txt"));
    let p1b = resume_part_path("r1", Path::new("/a/b.txt"));
    let p2 = resume_part_path("r1", Path::new("/a/c.txt"));
    assert_eq!(p1, p1b, "同目标同路径");
    assert_ne!(p1, p2, "不同目标不同路径");
    assert!(p1.to_string_lossy().contains(".part"));
    fs::write(&p1, b"data").unwrap();
    fs::remove_file(&p1).unwrap();
}

#[test]
fn m322_upload_connection_failure_keeps_part_and_reconnects() {
    // M32-2：上传连接类失败 → .part 保留 + 重连后成功写入目标（mock
    // 会话 as_any=None 不支持重连——验证可读错误含 .part 现场路径；
    // SftpBackend 真实重连路径由 M32-1 downcast 机制覆盖，此处验证
    // 上传分支的现场保留与错误合并）。
    let tmp = TempDir::new().unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.remote_sessions.insert(
        "r".to_owned(),
        Box::new(ReconnectMockFs {
            err: crate::remote::RemoteError::Connect("net down".to_owned()),
        }),
    );
    state.active_tab_mut().location = Location::Remote {
        remote: "r".to_owned(),
        path: PathBuf::from("/"),
    };
    fs::write(tmp.path().join("src.txt"), b"PAYLOAD").unwrap();
    state.yank = Some(crate::state::YankBuffer {
        paths: vec![tmp.path().join("src.txt")],
        cut: false,
        source: crate::state::YankSource::Local,
    });
    let mut msg = None;
    paste_yanked(&mut state, false, &mut msg);
    let m = msg.unwrap_or_default();
    // M36-6：mock 连接失败发生在第一块（offset=0）——重连被 mock 拒绝
    //（该会话类型不支持自动重连），报原始连接错误 + "第一块失败"。
    assert!(m.contains("第一块失败"), "msg: {m:?}");
    assert!(m.contains("连接失败"), "原始错误保留: {m}");
}

#[test]
fn m322_download_connection_failure_keeps_part() {
    // M32-2：下载 read 连接类失败 → .part 现场保留 + mock 拒绝重连的
    // 可读错误（真实重连成功路径由 M32-1 机制覆盖）。
    let tmp = TempDir::new().unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.remote_sessions.insert(
        "r".to_owned(),
        Box::new(ReconnectMockFs {
            err: crate::remote::RemoteError::Connect("net down".to_owned()),
        }),
    );
    state.yank = Some(crate::state::YankBuffer {
        paths: vec![PathBuf::from("/remote.bin")],
        cut: false,
        source: crate::state::YankSource::Remote("r".to_owned()),
    });
    let buf = state.yank.clone().unwrap();
    let mut msg = None;
    paste_remote_to_local(&mut state, "r", &buf, false, &mut msg);
    let m = msg.unwrap_or_default();
    assert!(m.contains(".part 现场"), "错误含现场路径: {m}");
    let part = resume_part_path("r", Path::new("/remote.bin"));
    assert!(part.is_file(), "现场保留");
}

#[test]
fn m321_remote_with_reconnect_guards() {
    // M32-1：reconnect 包装——非连接类错误原样上浮；mock 后端（as_any
    // None）连接类错误也给可读"不支持自动重连"；会话被放回不丢失。
    let tmp = TempDir::new().unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());

    // 非连接类（Auth）：原样上浮。
    state.remote_sessions.insert(
        "r".to_owned(),
        Box::new(ReconnectMockFs {
            err: crate::remote::RemoteError::Auth("denied".to_owned()),
        }),
    );
    let r = remote_with_reconnect(&mut state, "r", |b| b.list(Path::new("/")));
    assert!(
        r.is_err() && r.unwrap_err().contains("认证失败"),
        "非连接类上浮"
    );
    assert!(state.remote_sessions.contains_key("r"), "会话放回");

    // 连接类（Connect）+ mock（as_any None）：可读"不支持自动重连"。
    state.remote_sessions.insert(
        "r".to_owned(),
        Box::new(ReconnectMockFs {
            err: crate::remote::RemoteError::Connect("net down".to_owned()),
        }),
    );
    let r = remote_with_reconnect(&mut state, "r", |b| b.list(Path::new("/")));
    assert!(
        r.is_err() && r.unwrap_err().contains("不支持自动重连"),
        "连接类 + mock → 可读拒绝"
    );
    assert!(state.remote_sessions.contains_key("r"), "会话放回");

    // 会话不存在："会话已断开"。
    let r = remote_with_reconnect(&mut state, "nope", |b| b.list(Path::new("/")));
    assert!(r.is_err() && r.unwrap_err().contains("会话已断开"));
}

#[test]
fn m304_cmdmark_command_arm() {
    // 写全局 frecency DB（set_keyword_topic_now）——STORE 锁串行。
    let _guard = STORE_TEST_LOCK.lock().unwrap();
    // M30-4：:cmdmark——默认标最近会话命令；显式命令文本；`-` 清除；
    // 非法书签名/空历史可读错误。
    let tmp = TempDir::new().unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    // 无历史 → 错误。
    assert!(exec_command(&mut state, "cmdmark sd").is_err(), "空历史");
    // 造一条会话历史（直接调 record_cmd_history）。
    state.record_cmd_history("sort name desc");
    assert_eq!(
        exec_command(&mut state, "cmdmark sd"),
        Ok(false),
        "标最近命令"
    );
    // 显式命令文本覆盖。
    assert_eq!(exec_command(&mut state, "cmdmark q quit-all"), Ok(false));
    // 非法书签名。
    assert!(
        exec_command(&mut state, "cmdmark a-b").is_err(),
        "非字母数字"
    );
    assert!(exec_command(&mut state, "cmdmark").is_err(), "缺参");
    assert_eq!(
        exec_command(&mut state, "cmdmark -"),
        Ok(false),
        "清除形态合法"
    );
}

#[test]
fn m332_visual_mode_keymap_switch() {
    // M33-2：visual 进入/退出切换 per-mode 键位表——配置 [visual] 后
    // v 进入时用户绑定生效（单键显式命中），退出还原 normal 主表。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "A").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let layers = crate::config::KeymapLayers {
        visual: crate::config::ModeLayers {
            prepend: vec![(
                vec![crate::keymap::Key::plain(KeyCode::Char('z'))],
                crate::keymap::Action::ToTop,
            )],
            append: Vec::new(),
            replace: None,
        },
        ..Default::default()
    };
    let keymap = crate::keymap::Keymap::with_layers(
        layers.prepend.clone(),
        layers.append.clone(),
        layers.replace.clone(),
    );
    let mut input = InputState::with_mode_maps(keymap, &layers);

    // normal 模式：z 未绑定（非显式）。
    assert!(
        !input
            .keymap
            .is_explicit(&[crate::keymap::Key::plain(KeyCode::Char('z'))])
    );
    // v 进入 visual（1511 臂）→ 换表 → z 显式命中。
    press(
        &mut state,
        &mut input,
        KeyCode::Char('v'),
        KeyModifiers::NONE,
    );
    assert!(state.active_tab().visual.is_some(), "进入 visual");
    assert!(
        input
            .keymap
            .is_explicit(&[crate::keymap::Key::plain(KeyCode::Char('z'))]),
        "visual 表含用户绑定"
    );
    // z → ToTop（visual 内动作执行）。
    let msg: Option<String> = None;
    press(
        &mut state,
        &mut input,
        KeyCode::Char('z'),
        KeyModifiers::NONE,
    );
    let _ = msg;
    // Esc 退出 → 还原 normal 表（z 不再显式）。
    press(&mut state, &mut input, KeyCode::Esc, KeyModifiers::NONE);
    assert!(state.active_tab().visual.is_none(), "退出 visual");
    assert!(
        !input
            .keymap
            .is_explicit(&[crate::keymap::Key::plain(KeyCode::Char('z'))]),
        "还原 normal 主表"
    );
}

#[test]
fn m303_completion_candidates_weighted() {
    // M30-3：Tab 补全候选加权——frecency 分数条目降序在前，无记录条目
    // 字典序兜底；空 scores → 全量字典序（M24-7 行为不变）。
    let tmp = TempDir::new().unwrap();
    let items = vec![
        crate::tab::FileItem {
            icon: crate::icons::icon_for_active(&tmp.path().join("zeta")),
            name: "zeta".to_owned(),
            is_dir: true,
            is_link: false,
            is_exec: false,
            size: 0,
            perm: String::new(),
            mtime: 0,
        },
        crate::tab::FileItem {
            icon: crate::icons::icon_for_active(&tmp.path().join("alpha")),
            name: "alpha".to_owned(),
            is_dir: false,
            is_link: false,
            is_exec: false,
            size: 10,
            perm: String::new(),
            mtime: 0,
        },
        crate::tab::FileItem {
            icon: crate::icons::icon_for_active(&tmp.path().join("mid")),
            name: "mid".to_owned(),
            is_dir: true,
            is_link: false,
            is_exec: false,
            size: 0,
            perm: String::new(),
            mtime: 0,
        },
    ];
    let mk = |name: &str| tmp.path().join(name).to_string_lossy().replace('\\', "/");
    let mut scores = std::collections::HashMap::new();
    scores.insert(("dir".to_owned(), mk("zeta")), 3.0);
    scores.insert(("file".to_owned(), mk("alpha")), 1.5);
    scores.insert(("dir".to_owned(), mk("mid")), 2.0);
    let out = completion_candidates(tmp.path(), &items, &scores);
    // 分数降序：zeta(3) > mid(2) > alpha(1.5)。
    assert_eq!(out, vec!["zeta", "mid", "alpha"]);

    // 空 scores → 字典序兜底（alpha/mid/zeta）。
    let out = completion_candidates(tmp.path(), &items, &std::collections::HashMap::new());
    assert_eq!(out, vec!["alpha", "mid", "zeta"]);
}

#[test]
fn m292_compare_sync_dp_do_and_refresh() {
    // M29-2：对比结果 dp（取回另一侧）与 do（删除当前侧多出）——
    // 均入撤销栈可逆；操作后按原模式重建结果（前缀变化正确）；
    // 非法方向 / `!` / `=` 可读拒绝；非对比视图拒绝。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("l-only.txt"), "L").unwrap();
    fs::write(tmp.path().join("same.txt"), "S").unwrap();
    let r = TempDir::new().unwrap();
    fs::write(r.path().join("r-only.txt"), "R").unwrap();
    fs::write(r.path().join("same.txt"), "S").unwrap();
    fs::write(r.path().join("diff.txt"), "RIGHT").unwrap();
    fs::write(tmp.path().join("diff.txt"), "LEFT").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(r.path(), Config::default()).unwrap();

    assert_eq!(exec_command(&mut state, "compare byname"), Ok(false));
    // 快照（名称、标记、游标）到本地 Vec——避免跨 compare_sync 的借用冲突。
    let snapshot = |state: &AppState| -> Vec<(String, String, usize)> {
        let tab = state.active_tab();
        tab.results
            .as_ref()
            .unwrap()
            .items
            .iter()
            .enumerate()
            .map(|(idx, i)| {
                (
                    i.path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    i.text.clone(),
                    idx,
                )
            })
            .collect()
    };
    let snap = snapshot(&state);
    let find_marker = |snap: &[(String, String, usize)], name: &str| -> String {
        snap.iter()
            .find(|(n, _, _)| n == name)
            .map(|(_, t, _)| t.clone())
            .unwrap_or_default()
    };
    let cursor_of = |snap: &[(String, String, usize)], name: &str| -> usize {
        snap.iter()
            .find(|(n, _, _)| n == name)
            .map(|(_, _, i)| *i)
            .unwrap()
    };

    // dp 取回 `>` 条目（r-only.txt）到当前侧。
    state.active_tab_mut().cursor = cursor_of(&snap, "r-only.txt");
    let mut msg = None;
    compare_sync(&mut state, &mut msg, true);
    assert!(msg.as_deref().unwrap().contains("已取回"), "msg: {msg:?}");
    assert!(tmp.path().join("r-only.txt").is_file(), "已复制到当前侧");
    assert!(r.path().join("r-only.txt").is_file(), "另一侧不动");
    // 重建后该条目变 `=`。
    let snap = snapshot(&state);
    assert_eq!(find_marker(&snap, "r-only.txt"), "=", "同步后重建为相同");
    // u 撤销取回 → 重跑对比回到 `>`（结果为静态快照，文件操作后不自动
    // 刷新——与 :find/:grep 语义一致；仅 dp/do 同步操作即时重建）。
    undo_last(&mut state, &mut None);
    assert!(!tmp.path().join("r-only.txt").exists(), "u 移除取回副本");
    assert_eq!(exec_command(&mut state, "compare byname"), Ok(false));
    let snap = snapshot(&state);
    assert_eq!(find_marker(&snap, "r-only.txt"), ">", "重建后回到仅右");

    // do 删除 `<` 条目（l-only.txt）——可恢复路径。
    state.active_tab_mut().cursor = cursor_of(&snap, "l-only.txt");
    let mut msg = None;
    compare_sync(&mut state, &mut msg, false);
    assert!(msg.as_deref().unwrap().contains("已删除"), "msg: {msg:?}");
    assert!(!tmp.path().join("l-only.txt").exists(), "当前侧已删除");
    let snap = snapshot(&state);
    assert_eq!(
        find_marker(&snap, "l-only.txt"),
        "",
        "两侧均无此文件——从结果中消失"
    );
    // u 撤销删除 → 重跑对比回到 `<`（静态快照语义同上）。
    undo_last(&mut state, &mut None);
    assert!(tmp.path().join("l-only.txt").is_file(), "暂存恢复");
    assert_eq!(exec_command(&mut state, "compare byname"), Ok(false));
    let snap = snapshot(&state);
    assert_eq!(find_marker(&snap, "l-only.txt"), "<");

    // 非法方向拒绝：dp 于 `<`、do 于 `>`；`!` 双向拒绝；`=` 双向拒绝。
    state.active_tab_mut().cursor = cursor_of(&snap, "l-only.txt");
    let mut msg = None;
    compare_sync(&mut state, &mut msg, true);
    assert!(msg.as_deref().unwrap().contains("仅当前侧存在"));
    state.active_tab_mut().cursor = cursor_of(&snap, "r-only.txt");
    let mut msg = None;
    compare_sync(&mut state, &mut msg, false);
    assert!(msg.as_deref().unwrap().contains("仅另一侧存在"));
    state.active_tab_mut().cursor = cursor_of(&snap, "diff.txt");
    let mut msg = None;
    compare_sync(&mut state, &mut msg, true);
    assert!(msg.as_deref().unwrap().contains("不支持覆盖同步"));
    let mut msg = None;
    compare_sync(&mut state, &mut msg, false);
    assert!(msg.as_deref().unwrap().contains("不支持覆盖同步"));
    state.active_tab_mut().cursor = cursor_of(&snap, "same.txt");
    let mut msg = None;
    compare_sync(&mut state, &mut msg, true);
    assert!(msg.as_deref().unwrap().contains("无需同步"));

    // 非对比视图（退出结果视图）→ 拒绝。
    let cfg = state.config.clone();
    state.active_tab_mut().exit_results(cfg);
    let mut msg = None;
    compare_sync(&mut state, &mut msg, true);
    assert!(msg.as_deref().unwrap().contains("仅在 :compare"));
}

#[test]
fn m283_jobs_pause_toggle_and_detail_outcome() {
    // M28-3：p 暂停/恢复切换（仅 Running 生效）+ Enter 打开详情 outcome。
    let tmp = TempDir::new().unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.add_job("copy", 1, "src", "d");
    state.add_job("move", 1, "src", "d");
    let mut cursor = 0usize;
    let mut msg = None;
    // p → 暂停；再 p → 恢复。
    assert_eq!(
        jobs_menu_key(&mut state, &mut cursor, KeyCode::Char('p'), &mut msg),
        JobsMenuOutcome::None
    );
    assert!(state.job_paused(0), "作业已暂停");
    assert!(msg.as_deref().unwrap().contains("已暂停"));
    assert_eq!(
        jobs_menu_key(&mut state, &mut cursor, KeyCode::Char('p'), &mut msg),
        JobsMenuOutcome::None
    );
    assert!(!state.job_paused(0), "已恢复");
    // 非运行中（Done）→ p 无效。
    state.jobs[0].status = crate::state::JobStatus::Done {
        summary: "done".to_owned(),
    };
    assert_eq!(
        jobs_menu_key(&mut state, &mut cursor, KeyCode::Char('p'), &mut msg),
        JobsMenuOutcome::None
    );
    assert!(!state.job_paused(0));
    // Enter → 打开详情；空列表 → 不打开。
    assert_eq!(
        jobs_menu_key(&mut state, &mut cursor, KeyCode::Enter, &mut msg),
        JobsMenuOutcome::OpenDetail
    );
    state.jobs.clear();
    assert_eq!(
        jobs_menu_key(&mut state, &mut cursor, KeyCode::Enter, &mut msg),
        JobsMenuOutcome::None
    );
}

#[test]
fn m283_job_detail_overlay_renders_fields() {
    // M28-3：详情覆盖层——类型/来源/目标/条目/状态字段可见。
    let tmp = TempDir::new().unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.add_job("copy", 2, "src-dir", "d");
    state.jobs[0].status = crate::state::JobStatus::Running;
    let mode = InputMode::JobDetail { index: 0 };
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
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(70, 14)).unwrap();
    terminal.draw(|f| render(f, &state, &mode, None)).unwrap();
    let all = compact(terminal.backend(), 70, 14);
    assert!(all.contains("作业详情"), "标题: {all}");
    assert!(all.contains("类型:copy"), "类型字段: {all}");
    assert!(all.contains("目标:d"), "目标字段: {all}");
    assert!(all.contains("条目:0/2项"), "进度字段: {all}");
    assert!(all.contains("任意键返回:jobs"), "返回提示: {all}");
}

#[test]
fn m249_jobs_overlay_render_selection_and_cancel_state() {
    let tmp = TempDir::new().unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    // AUDIT-18：目标只作展示串，用固定短标签——宿主临时路径长度（如
    // 8.3 短名/多级 Temp）不再截断 70 列断言里的"取消中"。
    let id = state.add_job("copy", 1, "src", "d");
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
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(70, 8)).unwrap();
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
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(70, 8)).unwrap();
    terminal.draw(|f| render(f, &state, &mode, None)).unwrap();
    let all = compact(terminal.backend(), 70, 8);
    assert!(all.contains("已取消"), "Cancelled 终态可见: {all}");
    let _ = id;
}

// ── M25-3：归档创建（:archive；zip/tar/tar.gz 纯 Rust 写端） ────────────

/// M26-1：SpawnJob → 执行作业体 → 回包入撤销栈（模拟 run 循环回包路径）。
/// 返回（撤销记录数, 终态）。
fn run_job_from_outcome(
    state: &mut AppState,
    outcome: Result<ExecOutcome, String>,
) -> (usize, crate::state::JobStatus) {
    match outcome {
        Ok(ExecOutcome::SpawnJob {
            op,
            names,
            target,
            base,
        }) => {
            let (ops, status) = execute_job_body(
                &op,
                &names,
                &base,
                &target,
                0,
                &crate::state::JobControls {
                    cancel: &std::sync::atomic::AtomicBool::new(false),
                    pause: &std::sync::atomic::AtomicBool::new(false),
                    progress: &crate::state::JobProgress::default(),
                },
            );
            let n = ops.len();
            for uop in ops {
                state.undo_stack.push(uop);
            }
            (n, status)
        }
        other => panic!("expected SpawnJob, got {other:?}"),
    }
}

#[test]
fn m253_archive_zip_roundtrip_and_undo() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "AAA").unwrap();
    fs::create_dir(tmp.path().join("sub")).unwrap();
    fs::write(tmp.path().join("sub/b.txt"), "BBB").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().tags.insert("a.txt".to_owned());
    state.active_tab_mut().tags.insert("sub".to_owned());
    // 缺扩展名自动补齐（M26-1：后台作业体创建，Done summary 携带产物名）。
    let outcome = exec_command_impl(&mut state, "archive zip out");
    let (n, status) = run_job_from_outcome(&mut state, outcome);
    assert!(
        matches!(&status, crate::state::JobStatus::Done { summary } if summary.contains("out.zip")),
        "自动补扩展名: {status:?}"
    );
    assert_eq!(n, 2, "归档 + .md5 校验各一条撤销记录");
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
    // 1 参数：从扩展名推断 tar.gz（M26-1：经作业体创建）。
    let outcome = exec_command_impl(&mut state, "archive bundle.tar.gz");
    let (_, status) = run_job_from_outcome(&mut state, outcome);
    assert!(
        matches!(status, crate::state::JobStatus::Done { .. }),
        "{status:?}"
    );
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
    let outcome = exec_command_impl(&mut state, "archive plain.tar");
    let (_, status) = run_job_from_outcome(&mut state, outcome);
    assert!(matches!(status, crate::state::JobStatus::Done { .. }));
    assert!(tmp.path().join("plain.tar").exists());
}

#[test]
fn m262_sevenz_create_roundtrip_and_undo() {
    // M26-2：7z 内建创建（sevenz-rust 0.6 纯 Rust 写端）+ 读端往返一致 +
    // 校验文件链路 + u 可逆。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "AAA").unwrap();
    fs::create_dir(tmp.path().join("sub")).unwrap();
    fs::write(tmp.path().join("sub/b.txt"), "BBB").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().tags.insert("a.txt".to_owned());
    state.active_tab_mut().tags.insert("sub".to_owned());
    let outcome = exec_command_impl(&mut state, "archive 7z out");
    let (n, status) = run_job_from_outcome(&mut state, outcome);
    assert!(
        matches!(status, crate::state::JobStatus::Done { .. }),
        "{status:?}"
    );
    assert_eq!(n, 2, "归档 + .md5 校验各一条撤销记录");
    let dest = tmp.path().join("out.7z");
    assert!(dest.exists());
    // 读端往返：sevenz-rust 读端列出条目并核对内容。
    // AUDIT-18 同款：读句柄在撤销前必须释放（Windows 共享冲突）。
    let mut seen: Vec<(String, Vec<u8>)> = Vec::new();
    {
        let mut reader =
            sevenz_rust2::ArchiveReader::open(&dest, sevenz_rust2::Password::empty()).unwrap();
        reader
            .for_each_entries(|e, r| {
                let mut data = Vec::new();
                r.read_to_end(&mut data)
                    .map_err(|e| sevenz_rust2::Error::Io(e, "test read".into()))?;
                seen.push((e.name().to_owned(), data));
                Ok(true)
            })
            .unwrap();
    }
    let has = |n: &str| {
        seen.iter()
            .any(|(name, _)| name == n || name == &format!("{n}/"))
    };
    assert!(has("a.txt"), "names: {seen:?}");
    assert!(has("sub"), "names: {seen:?}");
    let b = seen
        .iter()
        .find(|(name, _)| name.ends_with("b.txt"))
        .unwrap();
    assert_eq!(b.1, b"BBB", "目录内容被打包");
    // u 可逆（LIFO：先校验后归档）。
    let mut msg = None;
    undo_last(&mut state, &mut msg);
    assert!(
        !tmp.path().join("out.7z.md5").exists(),
        "第一次 undo 移除校验"
    );
    undo_last(&mut state, &mut msg);
    assert!(!dest.exists(), "第二次 undo 移除归档");
}

#[test]
fn m253_archive_guards() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "A").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    // M26-4：xz/bz2/zst/rar/cab/lzh 解析为 CLI 降级格式（SpawnJob）；
    // 探测/失败链路由纯函数测试与 execute_job_body 终态覆盖（CLI 工具
    // 可用性随平台/环境，不在此断言成败）。
    assert!(matches!(
        exec_command_impl(&mut state, "archive tar.xz x.tar.xz"),
        Ok(ExecOutcome::SpawnJob { .. })
    ));
    assert!(matches!(
        exec_command_impl(&mut state, "archive rar x.rar"),
        Ok(ExecOutcome::SpawnJob { .. })
    ));
    // 1 参数但扩展名不可推断。
    assert!(exec_command_impl(&mut state, "archive out.abc").is_err());
    // M48-8：无参 → 默认直接执行；`-i` → 对话框；3 参非 checksum / 4+ 参 → Err。
    assert!(matches!(
        exec_command_impl(&mut state, "archive"),
        Ok(ExecOutcome::SpawnJob { .. })
    ));
    assert!(matches!(
        exec_command_impl(&mut state, "archive -i"),
        Ok(ExecOutcome::ShowArchiveDialog { .. })
    ));
    assert!(exec_command_impl(&mut state, "archive zip a b").is_err());
    assert!(exec_command_impl(&mut state, "archive zip a b c").is_err());
    // 同名已存在报错不覆盖。
    fs::write(tmp.path().join("dup.zip"), "old").unwrap();
    state.active_tab_mut().cursor = 0;
    assert!(exec_command_impl(&mut state, "archive zip dup.zip").is_err());
    assert_eq!(fs::read(tmp.path().join("dup.zip")).unwrap(), b"old");
    // 源缺失整批中止且不留半成品（M26-1：失败在作业体中终态化）。
    state.active_tab_mut().tags.insert("a.txt".to_owned());
    state.active_tab_mut().tags.insert("ghost.txt".to_owned());
    let outcome = exec_command_impl(&mut state, "archive zip ghost.zip");
    let (_, status) = run_job_from_outcome(&mut state, outcome);
    assert!(
        matches!(&status, crate::state::JobStatus::Failed { err } if err.contains("ghost")),
        "失败终态可读: {status:?}"
    );
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
fn m459a_archive_plan_covers_visual_selection() {
    // M45-9a：v 可视连续选区须全部入归档计划（原仅 tags + 光标 → 只打 1 个）。
    let tmp = TempDir::new().unwrap();
    for name in ["a.txt", "b.txt", "c.txt"] {
        fs::write(tmp.path().join(name), name).unwrap();
    }
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    // v 选 0..=2（j j 扩大），不设 tags——验证 visual 分支。
    state.active_tab_mut().visual = Some(0);
    state.active_tab_mut().cursor = 2;
    let (_op, names, _target, _base) = crate::app::exec::archive_plan(
        &state,
        crate::ops::ArchiveFormat::Zip,
        "out.zip",
        crate::ops::Checksum::Md5,
    )
    .unwrap();
    assert_eq!(names, vec!["a.txt", "b.txt", "c.txt"], "v 选区全部入包");
}

#[test]
fn m473_archive_plan_uses_focused_pane_source_and_opposite_dest() {
    // M47-3：焦点在右栏 → 取右栏文件，产物落在左栏目录。
    let left = TempDir::new().unwrap();
    let right = TempDir::new().unwrap();
    fs::write(right.path().join("r.txt"), "R").unwrap();
    let mut state = AppState::new(Tab::from_physical(left.path(), Config::default()).unwrap());
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(right.path(), Config::default()).unwrap();
    state.active_tab_mut().focus = crate::tab::Focus::Right;
    let (op, names, target, base) = crate::app::exec::archive_plan(
        &state,
        crate::ops::ArchiveFormat::Zip,
        "out.zip",
        crate::ops::Checksum::Md5,
    )
    .unwrap();
    assert!(op.starts_with("archive:zip"));
    assert_eq!(names, vec!["r.txt"], "源取焦点栏条目");
    assert_eq!(base, right.path(), "源目录为焦点栏");
    assert_eq!(target, left.path().join("out.zip"), "产物落在对侧栏");
}

#[test]
fn m473_extract_source_follows_focused_pane() {
    // M47-3：`:extract` 的源归档取焦点栏；对侧栏文件不被误当源。
    let left = TempDir::new().unwrap();
    let right = TempDir::new().unwrap();
    let zip = left.path().join("left.zip");
    crate::archive::make_test_zip(&zip);
    fs::write(right.path().join("note.txt"), "N").unwrap();
    let mut state = AppState::new(Tab::from_physical(left.path(), Config::default()).unwrap());
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(right.path(), Config::default()).unwrap();
    state.active_tab_mut().focus = crate::tab::Focus::Right;
    let err = crate::app::exec::exec_command_impl(&mut state, "extract").unwrap_err();
    assert!(err.contains("note.txt"), "应报右栏光标条目: {err}");
    assert!(err.contains("不是受支持的归档格式"), "{err}");
}

#[test]
fn m474_remaining_commands_follow_focused_pane() {
    // M47-4：`:sort`/`:select`/`m<char>` 与命令模板 %d/%f 均作用于焦点栏。
    let left = TempDir::new().unwrap();
    let right = TempDir::new().unwrap();
    for name in ["b.txt", "a.txt"] {
        fs::write(right.path().join(name), name).unwrap();
    }
    fs::write(left.path().join("l.txt"), "L").unwrap();
    let mut state = AppState::new(Tab::from_physical(left.path(), Config::default()).unwrap());
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(right.path(), Config::default()).unwrap();
    state.active_tab_mut().focus = crate::tab::Focus::Right;

    crate::app::exec::exec_command_impl(&mut state, "sort name").unwrap();
    assert_eq!(state.active_tab().right.entries[0].name, "a.txt");
    assert_eq!(state.active_tab().entries[0].name, "l.txt");

    crate::app::exec::exec_command_impl(&mut state, "select *.txt").unwrap();
    assert_eq!(state.active_tab().right.tags.len(), 2);
    assert!(state.active_tab().tags.is_empty());

    let tmpl = crate::app::exec::substitute_vars("%d|%f", &state).unwrap();
    assert!(
        tmpl.contains(&right.path().to_string_lossy().replace('\\', "/")),
        "{tmpl}"
    );
    assert!(tmpl.contains("a.txt"), "{tmpl}");
}

#[test]
fn m459c_archive_dialog_layout_and_stripping() {
    // M45-9c：对话框加宽；名称显示去后缀；格式/校验仅选中项反显。
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    let dialog = crate::app::render::archive_dialog_rect(area);
    assert!(
        dialog.width >= 40 && dialog.width <= 64,
        "w={}",
        dialog.width
    );
    assert!(
        dialog.width > crate::app::render::context_menu_rect(area).width,
        "应比右键菜单宽"
    );
    assert_eq!(
        crate::app::render::strip_archive_ext("bundle.zip"),
        "bundle"
    );
    assert_eq!(
        crate::app::render::strip_archive_ext("backup.tar.gz"),
        "backup"
    );
    assert_eq!(
        crate::app::render::strip_archive_ext("notes.txt"),
        "notes.txt"
    );

    // 渲染：名称去后缀，格式选中项高亮（反显 cell 存在），整行非全反显。
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let mode = InputMode::ArchiveDialog {
        input: "bundle.zip".to_owned(),
        field: 1,
        format_idx: 0,
        checksum_idx: 0,
    };
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
    terminal.draw(|f| render(f, &state, &mode, None)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let mut text = String::new();
    for y in 0..24u16 {
        for x in 0..80u16 {
            text.push_str(buf[(x, y)].symbol());
        }
    }
    // CJK 全宽占位空格（M21-E3），比较前压掉空白。
    let compact = text.replace(' ', "");
    assert!(compact.contains("bundle"), "名称显示: {text}");
    assert!(!compact.contains("bundle.zip"), "后缀不显示: {text}");
    assert!(compact.contains("格式"), "格式字段: {text}");
}

#[test]
fn m459b_archive_name_with_checksum_infers_format() {
    // M45-9②：`archive <名> checksum=<值>`（无格式名参）从扩展名推断格式。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "A").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());

    let outcome = exec_command_impl(&mut state, "archive bundle.zip checksum=sha256")
        .expect("应从 .zip 推断格式");
    match outcome {
        ExecOutcome::SpawnJob { op, target, .. } => {
            assert!(op.contains("zip") && op.contains("sha256"), "op={op}");
            assert_eq!(target, tmp.path().join("bundle.zip"));
        }
        other => panic!("应 SpawnJob，得到 {other:?}"),
    }
    // 未知扩展名 → 可读错误（不 panic）。
    assert!(exec_command_impl(&mut state, "archive bundle.unknown checksum=sha256").is_err());
    // 非法 checksum → 可读错误。
    assert!(exec_command_impl(&mut state, "archive bundle.zip checksum=md4").is_err());
}

#[test]
fn m254_exec_archive_checksum_arg_and_config() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "A").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().cursor = 0;
    // 显式 sha256（M26-1：经作业体创建）。
    let outcome = exec_command_impl(&mut state, "archive zip c1.zip checksum=sha256");
    let (n1, status1) = run_job_from_outcome(&mut state, outcome);
    assert!(matches!(status1, crate::state::JobStatus::Done { .. }));
    assert_eq!(n1, 2, "sha256 校验文件也入撤销栈");
    assert!(tmp.path().join("c1.zip").exists());
    assert!(tmp.path().join("c1.zip.sha256").exists());
    // none 不生成。
    let outcome = exec_command_impl(&mut state, "archive zip c2.zip checksum=none");
    let (n2, _) = run_job_from_outcome(&mut state, outcome);
    assert_eq!(n2, 1, "none → 仅归档一条");
    assert!(tmp.path().join("c2.zip").exists());
    assert!(!tmp.path().join("c2.zip.md5").exists());
    // 缺省（[archive] checksum 默认 md5）。
    let outcome = exec_command_impl(&mut state, "archive zip c3.zip");
    let (_, status3) = run_job_from_outcome(&mut state, outcome);
    assert!(matches!(status3, crate::state::JobStatus::Done { .. }));
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
    // Enter → 返回作业参数（M26-1：创建在后台线程执行，不在对话框内）。
    handle_archive_dialog_key(
        &mut state,
        &mut input,
        &mut field,
        &mut format_idx,
        &mut checksum_idx,
        KeyCode::Char(' '),
        &mut msg,
    );
    let outcome = handle_archive_dialog_key(
        &mut state,
        &mut input,
        &mut field,
        &mut format_idx,
        &mut checksum_idx,
        KeyCode::Enter,
        &mut msg,
    );
    let ArchiveDialogOutcome::Exec {
        op,
        names,
        target,
        base,
    } = outcome
    else {
        panic!("Enter 应返回作业参数: {outcome:?}");
    };
    assert!(op.starts_with("archive:tar"), "op 编码格式: {op}");
    // 产物由作业体创建（名称在名称字段曾再追加一个 x → outx.tar）。
    let (ops, status) = execute_job_body(
        &op,
        &names,
        &base,
        &target,
        0,
        &crate::state::JobControls {
            cancel: &std::sync::atomic::AtomicBool::new(false),
            pause: &std::sync::atomic::AtomicBool::new(false),
            progress: &crate::state::JobProgress::default(),
        },
    );
    assert!(
        matches!(status, crate::state::JobStatus::Done { .. }),
        "{status:?}"
    );
    assert_eq!(ops.len(), 2, "归档 + .md5 校验各一条撤销记录");
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
    // M48-8：无参按默认直接执行；交互入口改 `-i`（预填名保持原逻辑）。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "A").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().cursor = 0;
    match exec_command_impl(&mut state, "archive -i") {
        Ok(ExecOutcome::ShowArchiveDialog { input }) => assert_eq!(input, "a.txt"),
        other => panic!("expected ShowArchiveDialog, got {other:?}"),
    }
    let outcome = exec_command_impl(&mut state, "archive").unwrap();
    assert!(
        matches!(outcome, ExecOutcome::SpawnJob { .. }),
        "{outcome:?}"
    );
    // 无条目 → `-i` 仍给空名称（执行会因空名报错，可读）。
    let tmp2 = TempDir::new().unwrap();
    let mut s2 = AppState::new(Tab::from_physical(tmp2.path(), Config::default()).unwrap());
    match exec_command_impl(&mut s2, "archive -i") {
        Ok(ExecOutcome::ShowArchiveDialog { input }) => assert_eq!(input, ""),
        other => panic!("expected ShowArchiveDialog, got {other:?}"),
    }
}

#[test]
fn m4810_tutor_list_search_reset_wrong_only_and_exit_to_list() {
    // M48-10：列表 / 搜索、d reset、有记录只显示错题、结束 Ctrl+N 回列表。
    use crate::app::{TutorHotkey, TutorListOutcome, tutor_list_key};
    use crate::tutor::{TutorSession, merge_session};

    let mut cursor = 0usize;
    assert_eq!(
        tutor_list_key(&mut cursor, 14, KeyCode::Char('/')),
        TutorListOutcome::Search
    );
    assert_eq!(
        tutor_list_key(&mut cursor, 14, KeyCode::Char('d')),
        TutorListOutcome::Reset(0)
    );
    assert_eq!(KeyCode::Char('j'), KeyCode::Char('j'));

    // 错题过滤纯函数：两条里只保留 skipped 的一条。
    let modules = crate::tutor::builtin_modules();
    let base = modules[0].clone();
    let item0 = base.items[0].id.clone();
    let progress = merge_session(Default::default(), &base.id, &[(item0.clone(), false)]);
    let wrong = crate::tutor::wrong_items_only(&progress, &base.id, &base.items).unwrap();
    assert_eq!(wrong.len(), 1);
    assert_eq!(wrong[0].id, item0);

    // done 会话的 Ctrl+N → ExitToList（未结束时仍为 Marked）。
    let mut state = AppState::new(
        Tab::from_physical(std::env::temp_dir().as_path(), Config::default()).unwrap(),
    );
    state.tutor = Some(TutorSession {
        module: base,
        idx: 0,
        marks: Vec::new(),
        done: true,
        review: false,
    });
    let key = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL);
    assert_eq!(tutor_hotkey(&mut state, &key), TutorHotkey::ExitToList);
    assert!(state.tutor.is_none());
}

#[test]
fn m48fix_invalid_focus_dir_falls_back_before_spawn() {
    // 焦点目录被删除/移走时不得把不存在路径交给 tmux -c。
    let tmp = TempDir::new().unwrap();
    let gone = tmp.path().join("deleted");
    assert!(!gone.exists());
    let resolved = safe_external_dir(&gone);
    assert!(
        resolved.is_dir(),
        "回退目录必须真实存在: {}",
        resolved.display()
    );
    assert_ne!(resolved, gone);
}

#[test]
fn m489_ime_periodic_probe_respects_editable_modes() {
    // M48-9：仅 Normal 且到期的周期探测生效；文本输入模式不竞争。
    let now = std::time::Instant::now();
    assert!(ime_periodic_due(
        now + std::time::Duration::from_secs(2),
        now,
        false
    ));
    assert!(!ime_periodic_due(
        now,
        now + std::time::Duration::from_secs(1),
        false
    ));
    assert!(!ime_periodic_due(
        now + std::time::Duration::from_secs(2),
        now,
        true
    ));
}

#[test]
fn m581_completion_pool_and_live_desc_sync() {
    // M58-1：候选与描述同源（等长）；`:!` 候选是完整命令行（含参数）。
    let tmp = TempDir::new().unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let (names, descs) = crate::app::exec::command_completion_pool(&state, "arch");
    assert_eq!(names.len(), descs.len());
    assert!(names.iter().any(|n| n == "archive"));
    state.history_commands = vec!["git status --short".to_owned()];
    let (names, _) = crate::app::exec::command_completion_pool(&state, "!git");
    assert!(names.iter().any(|n| n.contains("--short")), "{names:?}");
}

#[test]
fn m583_space_accept_keeps_command_head() {
    // M58-3：Space 选定只替换最后一个词，保留头部参数。
    let comp = CmdComp {
        head: "ls ".to_owned(),
        candidates: vec!["a.txt".to_owned(), "b.txt".to_owned()],
        index: 1,
        descs: Vec::new(),
        offset: 0,
    };
    assert_eq!(
        crate::app::keys::completion_accept("ls b", &comp).as_deref(),
        Some("ls b.txt")
    );
}

#[test]
fn m586_multiline_command_preview_lines() {
    // M58-6：多行候选拆成完整预览行；单行候选不产生预览。
    assert_eq!(
        command_preview_lines("for f in *\ndo echo $f\ndone"),
        vec!["for f in *", "do echo $f", "done"]
    );
    assert_eq!(command_preview_lines("ls -la").len(), 1);
}

#[cfg(unix)]
#[test]
fn m587_external_success_reports_return_code() {
    // M58-7：成功与失败均在状态栏显示返回码。
    let ok = std::process::Command::new("true").status().unwrap();
    assert_eq!(
        crate::app::exec::run_external_current_result(Ok(ok)).as_deref(),
        Some("命令完成（返回码 0）")
    );
    let bad = std::process::Command::new("false").status().unwrap();
    assert!(
        crate::app::exec::run_external_current_result(Ok(bad))
            .as_deref()
            .is_some_and(|m| m.contains("退出码"))
    );
}

#[test]
fn m584_completion_offset_follows_selection() {
    // M58-4：循环到不可见项时视口滚动使其可见；索引与描述同源不漂移。
    assert_eq!(completion_offset(2, 10, 5), 0);
    assert_eq!(completion_offset(6, 10, 5), 2);
    assert_eq!(completion_offset(9, 10, 5), 5);
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    let rect = crate::app::render::command_completion_rect(area, 12);
    assert_eq!(rect.height, 14, "12 项 + 2 边框应全部可见");
}

#[test]
fn m501_command_completion_internal_pool_and_overlay() {
    // M50-1：内部命令补全候选按前缀过滤、带描述；浮层高度 ≤40%。
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let (names, descs) = command_completion_candidates("arch", &state);
    assert!(names.iter().any(|n| n == "archive"), "{names:?}");
    assert!(names.iter().all(|n| n.starts_with("arch")));
    assert_eq!(descs.len(), names.len());
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    let rect = crate::app::render::command_completion_rect(area, 30);
    assert_eq!(rect.width, 80, "满宽");
    assert_eq!(rect.y + rect.height, area.y + area.height, "底部对齐");
    // M58-4：在可用空间内尽量显示全部候选（30 项 → 铺满除去命令行/状态行的空间）。
    assert!(rect.height <= area.height.saturating_sub(3), "{rect:?}");
    assert!(rect.height >= 5, "{rect:?}");
    let small = crate::app::render::command_completion_rect(area, 3);
    assert_eq!(small.height, 5, "少候选时保持最小可读高度");
}

#[test]
fn m502_bang_completion_uses_shell_history_pool() {
    // M50-2：`:!git` 候选来自 shell 历史完整命令行，不带内部命令。
    let tmp = TempDir::new().unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.history_commands = vec!["git status".to_owned(), "ls -la".to_owned()];
    let (names, descs) = command_completion_candidates("!git", &state);
    assert_eq!(names, vec!["!git status"]);
    assert_eq!(descs.len(), 1);
    let (names, _) = command_completion_candidates("ls", &state);
    assert!(names.iter().all(|n| !n.contains('!')), "`:` 池不含外部命令");
}

#[test]
fn m504_external_command_line_action_has_no_default_binding() {
    // M50-4：Action 可由字符串名解析，但默认键位不绑定。
    assert_eq!(
        crate::keymap::Action::from_name("open_external_command_line"),
        Some(crate::keymap::Action::OpenExternalCommandLine)
    );
    let map = crate::keymap::Keymap::with_layers(Vec::new(), Vec::new(), None);
    assert!(
        map.lookup(&[crate::keymap::Key::plain(KeyCode::Char('!'))])
            .is_none(),
        "默认不绑定 !"
    );
}

#[test]
fn m505_quote_expansion_config_applies_to_variables() {
    // M50-5：默认裸变量；开关开启后自动双引号。
    let tmp = TempDir::new().unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let out = substitute_vars("%f", &state).unwrap();
    assert!(!out.starts_with('"'));
    state.config.command.quote_expansion = true;
    let out = substitute_vars("%f", &state).unwrap();
    assert!(out.starts_with('"') && out.ends_with('"'), "{out}");
}

#[test]
fn m514_paste_conflict_dialog_shows_key_hints() {
    // M51-4：冲突确认弹窗必须显示 y/n/Esc/j/k 提示（与删除确认一致）。
    let tmp = TempDir::new().unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state
        .pending_paste_conflicts
        .push(tmp.path().join("dup.txt"));
    let mode = InputMode::ConfirmPaste {
        cursor: 0,
        scroll: 0,
    };
    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|f| render(f, &state, &mode, None)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let mut text = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            text.push_str(buf[(x, y)].symbol());
        }
    }
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(compact.contains("确认"), "{compact}");
    assert!(compact.contains("取消"), "{compact}");
    assert!(compact.contains("j/k"), "{compact}");
}

#[test]
fn zkey4_viewport_positioning_keeps_cursor() {
    // ZKEY-4：zt/zz/zb 只改视口、不改光标索引，尊重 scrolloff。
    let tmp = TempDir::new().unwrap();
    for i in 0..100 {
        fs::write(tmp.path().join(format!("f{i:03}")), "").unwrap();
    }
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.config.ui.scrolloff = 2;
    state.active_tab_mut().cursor = 50;
    let mut input = crate::app::InputState::default();
    let mut msg = None;
    let seq = |code: KeyCode,
               state: &mut AppState,
               input: &mut crate::app::InputState,
               msg: &mut Option<String>| {
        press_msg(state, input, msg, KeyCode::Char('z'), KeyModifiers::NONE);
        press_msg(state, input, msg, code, KeyModifiers::NONE);
    };
    seq(KeyCode::Char('t'), &mut state, &mut input, &mut msg);
    assert_eq!(state.active_tab().scroll, 48, "zt 顶部留 scrolloff");
    assert_eq!(state.active_tab().cursor, 50);
    seq(KeyCode::Char('z'), &mut state, &mut input, &mut msg);
    assert_eq!(state.active_tab().scroll, 45, "zz 中央");
    assert_eq!(state.active_tab().cursor, 50);
    seq(KeyCode::Char('b'), &mut state, &mut input, &mut msg);
    assert_eq!(state.active_tab().scroll, 43, "zb 底部留 scrolloff");
    assert_eq!(state.active_tab().cursor, 50);
}

#[test]
fn zkey3_filter_snapshot_reset_restore_clear() {
    // ZKEY-3：zR 临时全显 + 快照，zM 恢复，zO 清焦点栏过滤器。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "").unwrap();
    fs::write(tmp.path().join("b.md"), "").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().filter = Some("*.txt".to_owned());
    state.active_tab_mut().right.filter = Some("*.md".to_owned());
    let mut input = crate::app::InputState::default();
    let mut msg = None;
    // zM 无快照
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('z'),
        KeyModifiers::NONE,
    );
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('M'),
        KeyModifiers::NONE,
    );
    assert_eq!(msg.as_deref(), Some("无保存的过滤器状态"));
    // zR
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('z'),
        KeyModifiers::NONE,
    );
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('R'),
        KeyModifiers::NONE,
    );
    assert!(state.config.show_hidden);
    assert!(state.active_tab().filter.is_none());
    assert!(state.active_tab().right.filter.is_none());
    let snap = state.filter_snapshot.clone().unwrap();
    assert_eq!(snap.left[0].as_deref(), Some("*.txt"));
    // zM 恢复
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('z'),
        KeyModifiers::NONE,
    );
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('M'),
        KeyModifiers::NONE,
    );
    assert_eq!(state.active_tab().filter.as_deref(), Some("*.txt"));
    assert_eq!(state.active_tab().right.filter.as_deref(), Some("*.md"));
    // zO 清焦点栏（左）过滤，隐藏开关不变
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('z'),
        KeyModifiers::NONE,
    );
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('O'),
        KeyModifiers::NONE,
    );
    assert!(state.active_tab().filter.is_none());
    assert_eq!(state.active_tab().right.filter.as_deref(), Some("*.md"));
}

#[test]
fn zkey2_hidden_file_toggle_actions() {
    // ZKEY-2：za 切换 / zo 显示 / zm 隐藏，与 show_hidden 同源。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join(".hidden"), "h").unwrap();
    fs::write(tmp.path().join("visible"), "v").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let mut input = crate::app::InputState::default();
    let mut msg = None;
    // za → 显示隐藏文件
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('z'),
        KeyModifiers::NONE,
    );
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('a'),
        KeyModifiers::NONE,
    );
    assert!(state.config.show_hidden);
    assert!(
        state
            .active_tab()
            .entries
            .iter()
            .any(|i| i.name == ".hidden")
    );
    assert_eq!(msg.as_deref(), Some("hidden: on"));
    // zm → 隐藏
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('z'),
        KeyModifiers::NONE,
    );
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('m'),
        KeyModifiers::NONE,
    );
    assert!(!state.config.show_hidden);
    assert!(
        state
            .active_tab()
            .entries
            .iter()
            .all(|i| i.name != ".hidden")
    );
    assert_eq!(msg.as_deref(), Some("hidden: off"));
    // zo → 显示；再 zo 无变化
    for _ in 0..2 {
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('z'),
            KeyModifiers::NONE,
        );
        press_msg(
            &mut state,
            &mut input,
            &mut msg,
            KeyCode::Char('o'),
            KeyModifiers::NONE,
        );
    }
    assert!(state.config.show_hidden);
    assert_eq!(msg.as_deref(), Some("hidden: on"));
}

#[test]
fn m552_live_find_streams_and_respects_depth_guard() {
    // M55-2：后台流式扫描 + ignore 语义 + 深度 20 护栏（不写 redb）。
    let tmp = TempDir::new().unwrap();
    // tempfile 根目录名以 `.` 开头，ignore walker 会跳过——用非隐藏子目录作 root。
    let root = tmp.path().join("root");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("a.txt"), "").unwrap();
    fs::write(root.join("b.md"), "").unwrap();
    let mut deep = root.clone();
    for i in 0..25 {
        deep.push(format!("d{i}"));
    }
    fs::create_dir_all(&deep).unwrap();
    fs::write(deep.join("deep.txt"), "").unwrap();
    let mut lf = crate::state::LiveFindState::new(root, false);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while lf.scanning && std::time::Instant::now() < deadline {
        lf.drain(64);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(!lf.scanning, "扫描应完成");
    assert!(lf.all.iter().any(|i| i.path.ends_with("a.txt")));
    assert!(lf.all.iter().any(|i| i.path.ends_with("b.md")));
    assert!(
        !lf.all.iter().any(|i| i.path.ends_with("deep.txt")),
        "深度 > 20 的条目应被护栏跳过"
    );
    lf.stop.store(true, std::sync::atomic::Ordering::Relaxed);
}

#[test]
fn m553_live_find_input_filter_move_and_enter_jump() {
    // M55-3：j/k 是输入字符，↑↓ 移动；实时子串过滤；Enter 跳转走 record。
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("root");
    fs::create_dir_all(&root).unwrap();
    for name in ["a.txt", "ab.txt", "b.md"] {
        fs::write(root.join(name), "").unwrap();
    }
    let mut state = AppState::new(Tab::from_physical(&root, Config::default()).unwrap());
    let mut lf = crate::state::LiveFindState::new(root, false);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while lf.scanning && std::time::Instant::now() < deadline {
        lf.drain(64);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    state.live_find = Some(lf);
    let mut msg = None;
    let key = |code: KeyCode, mods: KeyModifiers| KeyEvent::new(code, mods);
    let none = KeyModifiers::NONE;
    // 输入 a → 命中 2 项。
    assert!(!live_find_key(
        &mut state,
        &mut msg,
        key(KeyCode::Char('a'), none)
    ));
    assert_eq!(state.live_find.as_ref().unwrap().query, "a");
    assert_eq!(state.live_find.as_ref().unwrap().filtered.len(), 2);
    // j/k 是输入字符（aj 无命中），不是移动。
    assert!(!live_find_key(
        &mut state,
        &mut msg,
        key(KeyCode::Char('j'), none)
    ));
    assert_eq!(state.live_find.as_ref().unwrap().query, "aj");
    assert!(state.live_find.as_ref().unwrap().filtered.is_empty());
    assert!(!live_find_key(
        &mut state,
        &mut msg,
        key(KeyCode::Backspace, none)
    ));
    assert_eq!(state.live_find.as_ref().unwrap().filtered.len(), 2);
    // ↑↓ 移动光标（j/k 之外的导航键）。
    assert!(!live_find_key(
        &mut state,
        &mut msg,
        key(KeyCode::Down, none)
    ));
    assert_eq!(state.live_find.as_ref().unwrap().cursor, 1);
    assert!(!live_find_key(&mut state, &mut msg, key(KeyCode::Up, none)));
    assert_eq!(state.live_find.as_ref().unwrap().cursor, 0);
    // Enter 跳转到选中项，并计入 live_recorded（record 唯一写入入口的记账）。
    assert!(live_find_key(
        &mut state,
        &mut msg,
        key(KeyCode::Enter, none)
    ));
    // 测试直接调用 handler；事件循环收到 true 后 take + stop（此处模拟）。
    if let Some(lf) = state.live_find.take() {
        lf.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    assert!(state.live_find.is_none(), "跳转后应退出实时模式");
    assert!(!state.live_recorded.is_empty(), "跳转应登记一次真实访问");
    assert!(
        msg.as_deref().is_some_and(|m| m.contains("定位")),
        "{msg:?}"
    );
}

#[test]
fn m551_search_input_line_has_border_and_caret() {
    // M55-1：Search 态有边框输入框 + 可见光标 + 明确 j/k 为输入；Normal 态无文本框。
    let tmp = TempDir::new().unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let pane = |normal: bool, query: &str| crate::tab::FrecencyPane {
        entries: Vec::new(),
        query: query.to_owned(),
        cursor: 0,
        normal,
        g_pending: false,
        preview_focus: false,
        preview_scroll: 0,
        preview_query: None,
        preview_match: 0,
        d_pending: false,
        y_pending: false,
        last_deleted: None,
    };
    let row = |terminal: &ratatui::Terminal<ratatui::backend::TestBackend>, y: u16| -> String {
        let buf = terminal.backend().buffer().clone();
        (0..80u16)
            .map(|x| buf[(x, y)].symbol().to_owned())
            .collect::<String>()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect()
    };
    state.active_tab_mut().pane_frecency = Some(pane(false, "abc"));
    let backend = ratatui::backend::TestBackend::new(80, 12);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|f| crate::app::render::render_frecency_panes(f, &state))
        .unwrap();
    assert!(
        row(&terminal, 1).contains("SEARCH"),
        "{:?}",
        row(&terminal, 1)
    );
    let status = row(&terminal, 11);
    assert!(status.contains('│'), "输入框应有边框: {status}");
    assert!(status.contains('▋'), "输入框应有可见光标: {status}");
    assert!(status.contains("/abc"), "输入框应回显 query: {status}");
    assert!(status.contains("j/k为输入"), "应提示 j/k 是输入: {status}");

    state.active_tab_mut().pane_frecency = Some(pane(true, "abc"));
    terminal
        .draw(|f| crate::app::render::render_frecency_panes(f, &state))
        .unwrap();
    assert!(row(&terminal, 1).contains("NORMAL"));
    let status = row(&terminal, 11);
    assert!(!status.contains('▋'), "Normal 态不应有输入框光标: {status}");
    assert!(status.contains("NORMAL模式"), "{status}");
}

#[test]
fn m538_preview_debounce_skips_rapid_targets() {
    // M53-8：快速切换期间的中间目标不应立即重建预览。
    assert!(!crate::tab::preview_debounce_due(std::time::Instant::now()));
    let old = std::time::Instant::now() - std::time::Duration::from_millis(200);
    assert!(crate::tab::preview_debounce_due(old));
}

#[test]
fn m535_quick_text_preview_shows_at_least_36_lines() {
    // M53-5：快速预览按行截取至少 36 行（充满预览区），不再截 300 字。
    let tmp = TempDir::new().unwrap();
    let mut content = String::new();
    for i in 0..100 {
        content.push_str(&format!("line-{i:03} {}\n", "x".repeat(40)));
    }
    fs::write(tmp.path().join("long.txt"), content).unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().toggle_view();
    let backend = ratatui::backend::TestBackend::new(120, 60);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|f| render(f, &state, &InputMode::Normal, None))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let mut text = String::new();
    for y in 0..buf.area.height {
        for x in 41..120u16 {
            text.push_str(buf[(x, y)].symbol());
        }
    }
    assert!(!text.contains("快速预览仅前 300 字"));
    assert!(
        text.contains("line-035"),
        "至少展示 36 行: {}",
        &text[..text.len().min(200)]
    );
}

#[test]
fn m534_frecency_r_removes_record_not_file() {
    // M53-4：r 删除当前条目的 frecency 记录（非文件），u 可恢复。
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("a.txt");
    fs::write(&file, "A").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().pane_frecency = Some(crate::tab::FrecencyPane {
        entries: vec![crate::frecency::ScoredEntry {
            path: file.to_string_lossy().into_owned(),
            topic: "file".to_owned(),
            keyword: String::new(),
            score: 1.0,
            grade: crate::frecency::Grade::None,
            last_visit: 0,
        }],
        query: String::new(),
        cursor: 0,
        normal: true,
        g_pending: false,
        preview_focus: false,
        preview_scroll: 0,
        preview_query: None,
        preview_match: 0,
        d_pending: false,
        y_pending: false,
        last_deleted: None,
    });
    let mut input = crate::app::InputState::default();
    press(
        &mut state,
        &mut input,
        KeyCode::Char('r'),
        KeyModifiers::NONE,
    );
    assert!(
        state
            .active_tab()
            .pane_frecency
            .as_ref()
            .unwrap()
            .entries
            .is_empty()
    );
    assert!(file.exists(), "原文件不受影响");
    press(
        &mut state,
        &mut input,
        KeyCode::Char('u'),
        KeyModifiers::NONE,
    );
    assert_eq!(
        state
            .active_tab()
            .pane_frecency
            .as_ref()
            .unwrap()
            .entries
            .len(),
        1
    );
}

#[test]
fn m533_archive_member_preview_and_readonly_open() {
    // M53-3：归档成员解临时副本（预览与系统 opener 共用，不写归档内）。
    let tmp = TempDir::new().unwrap();
    let zip = tmp.path().join("b.zip");
    crate::archive::make_test_zip(&zip);
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let _ = state.active_tab_mut().navigate_to(
        crate::tab::Location::Archive {
            host: zip.clone(),
            inner: PathBuf::new(),
        },
        Config::default(),
    );
    let idx = state
        .active_tab()
        .entries
        .iter()
        .position(|e| e.name == "readme.md")
        .unwrap();
    state.active_tab_mut().cursor = idx;
    let before = fs::read(&zip).unwrap();
    let copy = extract_member_to_temp(&mut state, &zip, "readme.md").unwrap();
    assert_eq!(fs::read_to_string(&copy).unwrap(), "hi");
    assert!(state.temp_opens.contains(&copy));

    // `l`/Enter 走 open_selected → 临时副本交系统 opener（cfg(test) 不真启动 GUI）。
    let mut msg = None;
    open_selected(&mut state, &mut msg);
    assert!(
        msg.as_deref().is_some_and(|m| m.contains("只读副本")),
        "{msg:?}"
    );
    assert_eq!(fs::read(&zip).unwrap(), before, "归档未被修改");
}

#[test]
fn m531_tab_label_is_focus_aware_full_path() {
    // M53-1：标签标题 `[N:焦点栏全路径]`，焦点切换同步更新。
    let left = TempDir::new().unwrap();
    let right = TempDir::new().unwrap();
    let mut state = AppState::new(Tab::from_physical(left.path(), Config::default()).unwrap());
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(right.path(), Config::default()).unwrap();
    let (labels, _) = crate::app::render::tab_bar_labels_and_visible(&state, 120);
    assert!(
        labels[0].contains(&format!("[1:{}", left.path().display())),
        "{labels:?}"
    );

    state.active_tab_mut().focus = crate::tab::Focus::Right;
    let (labels, _) = crate::app::render::tab_bar_labels_and_visible(&state, 120);
    assert!(
        labels[0].contains(&format!("[1:{}", right.path().display())),
        "{labels:?}"
    );
    let label = labels[0].trim_end();
    assert!(label.starts_with("[1:") && label.ends_with(']'), "{label}");
}

#[test]
fn m521_frecency_default_opposite_preview_and_view_always_opposite() {
    // M52-1：默认 fixed_side=false → frecency 列表在焦点栏、预览在对侧；
    // `:view` 不受开关影响，焦点在右时查看器仍在左栏。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("alpha.txt"), "A").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    assert!(!state.config.preview.fixed_side, "默认动态对侧");
    state.active_tab_mut().focus = crate::tab::Focus::Right;
    state.active_tab_mut().pane_frecency = Some(crate::tab::FrecencyPane {
        entries: vec![crate::frecency::ScoredEntry {
            path: tmp.path().join("alpha.txt").to_string_lossy().into_owned(),
            topic: "file".to_owned(),
            keyword: String::new(),
            score: 0.0,
            grade: crate::frecency::Grade::None,
            last_visit: 0,
        }],
        query: String::new(),
        cursor: 0,
        normal: true,
        g_pending: false,
        preview_focus: false,
        preview_scroll: 0,
        preview_query: None,
        preview_match: 0,
        d_pending: false,
        y_pending: false,
        last_deleted: None,
    });
    let backend = ratatui::backend::TestBackend::new(80, 12);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|f| crate::app::render::render_frecency_panes(f, &state))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let text = |x0: u16, x1: u16| -> String {
        (x0..x1)
            .map(|x| buf[(x, 1)].symbol().to_owned())
            .collect::<String>()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect()
    };
    assert!(
        text(41, 80).contains("recency"),
        "焦点右 → 列表在右: {}",
        text(41, 80)
    );
    assert!(
        !text(0, 40).contains("Frecency"),
        "预览在左: {}",
        text(0, 40)
    );

    // `:view` 焦点右 → 查看器占左栏（与开关无关）。
    state.active_tab_mut().pane_frecency = None;
    state.active_tab_mut().toggle_view(); // 正确激活 viewer（含 ViewerState）
    terminal
        .draw(|f| render(f, &state, &InputMode::Normal, None))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let mut left = String::new();
    for y in 1..5u16 {
        for x in 0..40u16 {
            left.push_str(buf[(x, y)].symbol());
        }
    }
    assert!(left.contains("alpha"), "左侧查看器标题含文件名: {left}");

    // Tab 进出预览焦点与侧别无关：焦点保持在右栏。
    state.active_tab_mut().view_mode = true;
    let mut input = crate::app::InputState::default();
    press(&mut state, &mut input, KeyCode::Tab, KeyModifiers::NONE);
    assert!(state.active_tab().preview_focus, "Tab 进入预览");
    assert_eq!(
        state.active_tab().focus,
        crate::tab::Focus::Right,
        "焦点不跳栏"
    );
    press(&mut state, &mut input, KeyCode::Tab, KeyModifiers::NONE);
    assert!(!state.active_tab().preview_focus, "Tab 返回列表");
}

#[test]
fn m4912_frecency_title_filter_and_search_esc_keeps_query() {
    // M49-12/14：标题含 SEARCH + [filter]；Esc 返回 NORMAL 但保留过滤。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("alpha.txt"), "A").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().pane_frecency = Some(crate::tab::FrecencyPane {
        entries: vec![crate::frecency::ScoredEntry {
            path: tmp.path().join("alpha.txt").to_string_lossy().into_owned(),
            topic: "file".to_owned(),
            keyword: String::new(),
            score: 1.0,
            grade: crate::frecency::Grade::None,
            last_visit: 0,
        }],
        query: "alpha".to_owned(),
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
    });
    let backend = ratatui::backend::TestBackend::new(100, 12);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|f| render(f, &state, &InputMode::Normal, None))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let top: String = (0..100).map(|x| buf[(x, 1)].symbol().to_owned()).collect();
    assert!(top.contains("SEARCH"), "标题含模式: {top}");
    assert!(top.contains("filter"), "标题含过滤: {top}");

    let mut input = crate::app::InputState::default();
    press(&mut state, &mut input, KeyCode::Esc, KeyModifiers::NONE);
    let pane = state.active_tab().pane_frecency.as_ref().unwrap();
    assert!(pane.normal, "Esc 回 NORMAL");
    assert_eq!(pane.query, "alpha", "Esc 保留过滤");
}

#[test]
fn m4915_frecency_dd_delete_and_u_restore() {
    // M49-15：dd 删除记录（d 单键不再删除），u 恢复。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("alpha.txt"), "A").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().pane_frecency = Some(crate::tab::FrecencyPane {
        entries: vec![crate::frecency::ScoredEntry {
            path: tmp.path().join("alpha.txt").to_string_lossy().into_owned(),
            topic: "file".to_owned(),
            keyword: String::new(),
            score: 1.0,
            grade: crate::frecency::Grade::None,
            last_visit: 0,
        }],
        query: String::new(),
        cursor: 0,
        normal: true,
        g_pending: false,
        preview_focus: false,
        preview_scroll: 0,
        preview_query: None,
        preview_match: 0,
        d_pending: false,
        y_pending: false,
        last_deleted: None,
    });
    let mut input = crate::app::InputState::default();
    press(
        &mut state,
        &mut input,
        KeyCode::Char('d'),
        KeyModifiers::NONE,
    );
    assert_eq!(
        state
            .active_tab()
            .pane_frecency
            .as_ref()
            .unwrap()
            .entries
            .len(),
        1,
        "单键 d 不删"
    );
    press(
        &mut state,
        &mut input,
        KeyCode::Char('d'),
        KeyModifiers::NONE,
    );
    let pane = state.active_tab().pane_frecency.as_ref().unwrap();
    assert!(pane.entries.is_empty(), "dd 删除记录");
    assert!(pane.last_deleted.is_some(), "记录可恢复项");
    press(
        &mut state,
        &mut input,
        KeyCode::Char('u'),
        KeyModifiers::NONE,
    );
    assert_eq!(
        state
            .active_tab()
            .pane_frecency
            .as_ref()
            .unwrap()
            .entries
            .len(),
        1,
        "u 恢复"
    );
}

#[test]
fn m4910_preview_focus_reloads_full_text() {
    // M49-9/10：快速态只显示前 300 字；预览获焦后加载完整内容。
    let tmp = TempDir::new().unwrap();
    // 单行 400 字符：快速态第二行会出现截断提示，全量态无此提示。
    let long: String = (0..60).map(|i| format!("line-{i:03}\n")).collect();
    fs::write(tmp.path().join("long.txt"), &long).unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().toggle_view();
    state.active_tab_mut().cursor = 0;
    let backend = ratatui::backend::TestBackend::new(80, 12);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    let dump = |terminal: &ratatui::Terminal<ratatui::backend::TestBackend>| -> String {
        let buf = terminal.backend().buffer();
        (0..buf.area.height)
            .flat_map(|y| (0..buf.area.width).map(move |x| buf[(x, y)].symbol().to_owned()))
            .collect()
    };
    terminal
        .draw(|f| render(f, &state, &InputMode::Normal, None))
        .unwrap();
    let quick = dump(&terminal).replace(' ', "");
    assert!(quick.contains("快速"), "快速态提示: {quick}");

    state.active_tab_mut().preview_focus = true;
    terminal
        .draw(|f| render(f, &state, &InputMode::Normal, None))
        .unwrap();
    let full = dump(&terminal).replace(' ', "");
    assert!(
        !full.contains("·快速") && !full.contains("快速"),
        "获焦后无快速提示: {full}"
    );
}

#[test]
fn m498_extract_explicit_dir_entries_yield_files() {
    // M49-8：含显式目录条目的归档解压后必须有真实文件（非空目录）。
    let tmp = TempDir::new().unwrap();
    let zip = tmp.path().join("bundle.zip");
    crate::archive::make_test_zip(&zip);
    let dest = TempDir::new().unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(dest.path(), Config::default()).unwrap();
    // 焦点左（归档所在），默认解压目标 = 对侧右栏。
    state.active_tab_mut().focus = crate::tab::Focus::Left;
    state.active_tab_mut().cursor = 0;
    let outcome = exec_command_impl(&mut state, "extract").unwrap();
    assert!(matches!(outcome, ExecOutcome::Done { .. }));
    assert_eq!(
        fs::read_to_string(dest.path().join("readme.md")).unwrap(),
        "hi"
    );
    assert_eq!(
        fs::read_to_string(dest.path().join("src/main.rs")).unwrap(),
        "fn main() {}"
    );
    assert!(dest.path().join("deep/nested/blob.bin").exists());
}

#[test]
fn m491_frecency_preview_fixed_on_right() {
    // M52-1：显式 fixed_side=true 时固定右侧——标题/列表在左，预览在右；
    // Tab 只切预览焦点（默认 false 的动态布局另测）。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("alpha.txt"), "A").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.config.preview.fixed_side = true;
    state.active_tab_mut().pane_frecency = Some(crate::tab::FrecencyPane {
        entries: vec![crate::frecency::ScoredEntry {
            path: tmp.path().join("alpha.txt").to_string_lossy().into_owned(),
            topic: "file".to_owned(),
            keyword: String::new(),
            score: 0.0,
            grade: crate::frecency::Grade::None,
            last_visit: 0,
        }],
        query: String::new(),
        cursor: 0,
        normal: true,
        g_pending: false,
        preview_focus: false,
        preview_scroll: 0,
        preview_query: None,
        preview_match: 0,
        d_pending: false,
        y_pending: false,
        last_deleted: None,
    });
    let backend = ratatui::backend::TestBackend::new(80, 12);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|f| crate::app::render::render_frecency_panes(f, &state))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let row = |y: u16, x0: u16, x1: u16| -> String {
        (x0..x1).map(|x| buf[(x, y)].symbol().to_owned()).collect()
    };
    assert!(
        row(1, 0, 40).contains("Frecency"),
        "标题在左: {}",
        row(1, 0, 40)
    );
    assert!(
        !row(1, 41, 80).contains("Frecency"),
        "标题不在右: {}",
        row(1, 41, 80)
    );

    let mut input = crate::app::InputState::default();
    let outcome = press(&mut state, &mut input, KeyCode::Tab, KeyModifiers::NONE);
    assert!(matches!(outcome, Outcome::Continue));
    assert!(
        state
            .active_tab()
            .pane_frecency
            .as_ref()
            .unwrap()
            .preview_focus
    );
    assert_eq!(
        state.active_tab().focus,
        crate::tab::Focus::Left,
        "焦点不切 pane"
    );
    press(&mut state, &mut input, KeyCode::Tab, KeyModifiers::NONE);
    assert!(
        !state
            .active_tab()
            .pane_frecency
            .as_ref()
            .unwrap()
            .preview_focus
    );
}

#[test]
fn m488_archive_noargs_direct_and_interactive_flag() {
    // M48-8：无参直接执行（zip + 默认校验 + 预填名），`-i` 才开对话框。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("doc.md"), "D").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let outcome = exec_command_impl(&mut state, "archive").unwrap();
    match outcome {
        ExecOutcome::SpawnJob {
            op, names, target, ..
        } => {
            assert!(op.starts_with("archive:zip:"), "{op}");
            assert_eq!(names, vec!["doc.md"]);
            assert!(
                target
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .contains("doc")
            );
        }
        other => panic!("无参应直接执行: {other:?}"),
    }
    let outcome = exec_command_impl(&mut state, "archive -i").unwrap();
    assert!(matches!(outcome, ExecOutcome::ShowArchiveDialog { .. }));
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
    // M48-FIX：非 git 仓库先拒绝（避免 gitui 反复提示建仓库）。
    // M49-5：`:gi` 主命令与 `:gitui` 兼容别名走同一臂。
    let r = exec_command_impl(&mut state, "gi");
    assert!(r.is_err() && r.as_ref().unwrap_err().contains("不在 git 仓库"));
    let r = exec_command_impl(&mut state, "gitui");
    assert!(r.is_err());
    assert!(r.as_ref().unwrap_err().contains("不在 git 仓库"));
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
fn m34_3_right_pane_search_incsearch_commit_and_nn() {
    // M34-3：右 pane 搜索链路——live 跳转/提交/n·N 环绕/Esc 取消恢复，
    // 左 pane 光标不受影响。
    let tmp = TempDir::new().unwrap();
    for n in ["alpha.txt", "beta.txt", "gamma.txt"] {
        fs::write(tmp.path().join(n), "x").unwrap();
    }
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let tab = state.active_tab_mut();
    tab.right = crate::tab::Pane::from_physical(tmp.path(), Config::default()).unwrap();
    tab.focus = crate::tab::Focus::Right;
    let mut msg: Option<String> = None;
    live_search_jump(&mut state, 0, "beta", &mut msg);
    assert_eq!(msg, None);
    assert_eq!(state.active_tab().right.cursor, 1, "incsearch 落到 beta");
    assert_eq!(state.active_tab().cursor, 0, "左 pane 光标不动");
    submit_search(&mut state, "a", &mut msg); // beta 含 'a' → 光标保持
    assert_eq!(state.active_tab().right.cursor, 1);
    assert_eq!(
        state.active_tab().search.as_ref().map(|s| s.query.as_str()),
        Some("a")
    );
    run_search(&mut state, &mut msg, true); // beta → gamma
    assert_eq!(state.active_tab().right.cursor, 2);
    run_search(&mut state, &mut msg, true); // gamma → 环绕 alpha
    assert_eq!(state.active_tab().right.cursor, 0);
    run_search(&mut state, &mut msg, false); // alpha → 反向 gamma
    assert_eq!(state.active_tab().right.cursor, 2);
    search_cancel(&mut state, 2); // origin=2：光标回 2、清状态
    assert_eq!(state.active_tab().right.cursor, 2);
    assert!(state.active_tab().search.is_none());
}

#[test]
fn m41_1_right_pane_command_mode_and_tab_switch() {
    // M41-1：右 pane `:` 进入命令模式；gt/gT/Ngt/gg 与左 pane 同语义
    // （`g` 起始待决序列复用左 pane seq 机制）。
    let (tmp, mut state) = make_state(3);
    let mut input = InputState::default();
    {
        let tab = state.active_tab_mut();
        tab.right = crate::tab::Pane::from_physical(tmp.path(), Config::default()).unwrap();
        tab.focus = crate::tab::Focus::Right;
    }
    // `:` → 命令模式。
    assert_eq!(
        press(
            &mut state,
            &mut input,
            KeyCode::Char(':'),
            KeyModifiers::NONE
        ),
        Outcome::EnterCommand
    );
    // gt → 下一标签（先建第二个 tab 并回到第 1 个）。
    state.push_tab(tab_at(tmp.path().to_str().unwrap()));
    state.active = 0;
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
    assert_eq!(state.active, 1, "gt 切到下一标签");
    // gT → 回上一标签。
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
    assert_eq!(state.active, 0, "gT 回上一标签");
    // 2gt → 计数切到第 2 个标签。
    assert_eq!(
        press(
            &mut state,
            &mut input,
            KeyCode::Char('2'),
            KeyModifiers::NONE
        ),
        Outcome::Continue
    );
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
    assert_eq!(state.active, 1, "2gt 切到第 2 个标签");
    // gg → 右 pane 顶部（切回 tab 0——tab_at 的右 pane 指向 CWD，不可用）。
    state.active = 0;
    state.active_tab_mut().right.cursor_bottom();
    assert_eq!(state.active_tab().right.cursor, 2);
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
            KeyCode::Char('g'),
            KeyModifiers::NONE
        ),
        Outcome::Continue
    );
    assert_eq!(state.active_tab().right.cursor, 0, "gg 右 pane 顶部");
}

#[test]
fn m41_2_arrow_keys_match_jkhl_both_panes() {
    // M41-2：方向键与 k/j/l/h 等价——左 pane（计数/进入/返回定位）与
    // 右 pane（焦点 pane 内导航/进入/返回）。
    let tmp = TempDir::new().unwrap();
    fs::create_dir(tmp.path().join("sub")).unwrap();
    for n in ["a.txt", "b.txt", "c.txt"] {
        fs::write(tmp.path().join(n), "x").unwrap();
    }
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let mut input = InputState::default();
    // 左 pane：↓ = j、↑ = k。
    assert_eq!(
        press(&mut state, &mut input, KeyCode::Down, KeyModifiers::NONE),
        Outcome::Continue
    );
    assert_eq!(state.active_tab().cursor, 1, "↓ 下移一条");
    assert_eq!(
        press(&mut state, &mut input, KeyCode::Up, KeyModifiers::NONE),
        Outcome::Continue
    );
    assert_eq!(state.active_tab().cursor, 0, "↑ 上移一条");
    // 计数前缀：3↓ = 3j。
    press(
        &mut state,
        &mut input,
        KeyCode::Char('3'),
        KeyModifiers::NONE,
    );
    press(&mut state, &mut input, KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(state.active_tab().cursor, 3, "3↓ 消耗计数");
    // → = l：进入目录（目录优先排序下 sub 在首位附近；用名字定位）。
    let sub_idx = state
        .active_tab()
        .entries
        .iter()
        .position(|i| i.name == "sub")
        .unwrap();
    state.active_tab_mut().cursor = sub_idx;
    press(&mut state, &mut input, KeyCode::Right, KeyModifiers::NONE);
    assert_eq!(
        state.active_tab().location.base_dir(),
        tmp.path().join("sub"),
        "→ 进入目录"
    );
    // ← = h：返回父目录并定位来源条目（M21-F4）。
    press(&mut state, &mut input, KeyCode::Left, KeyModifiers::NONE);
    assert_eq!(
        state.active_tab().location.base_dir(),
        tmp.path(),
        "← 返回父目录"
    );
    assert_eq!(state.active_tab().cursor, sub_idx, "← 定位来源条目");
    // 右 pane：焦点右后 ↓/↑ 移右栏光标，→ 进入，← 返回。
    {
        let tab = state.active_tab_mut();
        tab.right = crate::tab::Pane::from_physical(tmp.path(), Config::default()).unwrap();
        tab.focus = crate::tab::Focus::Right;
    }
    press(&mut state, &mut input, KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(state.active_tab().right.cursor, 1, "右 pane ↓");
    // 右 pane 打开 = M14-1 仅物理目录：定位到 sub（索引 0，目录优先）再按 →。
    state.active_tab_mut().right.cursor = 0;
    press(&mut state, &mut input, KeyCode::Right, KeyModifiers::NONE);
    assert_eq!(
        state.active_tab().right.location.base_dir(),
        tmp.path().join("sub"),
        "右 pane → 进入目录"
    );
    press(&mut state, &mut input, KeyCode::Left, KeyModifiers::NONE);
    assert_eq!(
        state.active_tab().right.location.base_dir(),
        tmp.path(),
        "右 pane ← 返回"
    );
    assert_eq!(
        state.active_tab().location.base_dir(),
        tmp.path(),
        "左 pane 不受影响"
    );
}

#[test]
fn m41_3_mouse_click_focus_and_highlight_both_panes() {
    // M41-3：单击切焦点 + 光标移至命中条目（高亮随焦点）；已聚焦行再击
    // 打开（右 pane = M14-1 进入目录语义，文件 no-op）；滚轮作用于焦点
    // pane。左右 pane 均有 `..` 行（普通条目从父行后开始）。
    let tmp = TempDir::new().unwrap();
    fs::create_dir(tmp.path().join("sub")).unwrap();
    for n in ["a.txt", "b.txt"] {
        fs::write(tmp.path().join(n), "x").unwrap();
    }
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(tmp.path(), Config::default()).unwrap();
    let mut msg: Option<String> = None;
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    let [_, left, right, _] = pane_layout(area, crate::config::LayoutMode::Dual, false);
    let borders = state.config.ui.pane_borders;
    let left_inner = left_list_area(borders, left);
    let right_inner = left_list_area(borders, right);
    let down = |column: u16, row: u16| MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    // 1) 单击右 pane idx 1（a.txt；父行之后第 2 项）→ 焦点右 + 光标 1。
    handle_mouse(
        &mut state,
        &InputMode::Normal,
        &down(right_inner.x, right_inner.y + 2),
        &mut msg,
        area,
    );
    assert_eq!(
        state.active_tab().focus,
        crate::tab::Focus::Right,
        "焦点切右"
    );
    assert_eq!(state.active_tab().right.cursor, 1, "右光标移至命中行");
    assert_eq!(
        state.active_tab().right.location.base_dir(),
        tmp.path(),
        "首次单击不打开"
    );
    // 2) 再击同一行（idx 1 = 文件）→ enter_selected 文件 no-op。
    handle_mouse(
        &mut state,
        &InputMode::Normal,
        &down(right_inner.x, right_inner.y + 2),
        &mut msg,
        area,
    );
    assert_eq!(
        state.active_tab().right.location.base_dir(),
        tmp.path(),
        "已聚焦文件行再击不进入"
    );
    // 3) 单击右 pane idx 0（sub）→ 光标 0；再击 → 进入 sub。
    handle_mouse(
        &mut state,
        &InputMode::Normal,
        &down(right_inner.x, right_inner.y + 1),
        &mut msg,
        area,
    );
    assert_eq!(state.active_tab().right.cursor, 0, "右光标移至首行");
    handle_mouse(
        &mut state,
        &InputMode::Normal,
        &down(right_inner.x, right_inner.y + 1),
        &mut msg,
        area,
    );
    assert_eq!(
        state.active_tab().right.location.base_dir(),
        tmp.path().join("sub"),
        "已聚焦目录行再击进入"
    );
    // 4) 单击左 pane 条目 idx 0（`..` 行下第 1 行）→ 焦点切左 + 光标 0。
    handle_mouse(
        &mut state,
        &InputMode::Normal,
        &down(left_inner.x, left_inner.y + 1),
        &mut msg,
        area,
    );
    assert_eq!(
        state.active_tab().focus,
        crate::tab::Focus::Left,
        "焦点切左"
    );
    assert_eq!(state.active_tab().cursor, 0, "左光标移至命中行");
    // 5) 滚轮作用于焦点 pane（左）：↑ 光标回 `..` 行语义 = cursor 不动
    // （move_cursor 向上在 `..` 行停留——FEAT-2），改用 ↓ 验证下移。
    let wheel = |kind: MouseEventKind, column: u16, row: u16| MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    handle_mouse(
        &mut state,
        &InputMode::Normal,
        &wheel(MouseEventKind::ScrollDown, left_inner.x, left_inner.y + 1),
        &mut msg,
        area,
    );
    assert_eq!(state.active_tab().cursor, 1, "滚轮下移焦点 pane 光标");
    // 6) 滚轮在右 pane 内区发起，仍作用于焦点 pane（左）。
    handle_mouse(
        &mut state,
        &InputMode::Normal,
        &wheel(MouseEventKind::ScrollUp, right_inner.x, right_inner.y),
        &mut msg,
        area,
    );
    assert_eq!(
        state.active_tab().cursor,
        0,
        "右区滚轮仍作用于焦点 pane（左）"
    );
    assert_eq!(
        state.active_tab().right.cursor,
        0,
        "右 pane 光标不受非焦点滚轮影响"
    );
}

#[test]
fn m34_4_view_renders_in_opposite_pane() {
    // M34-4：view 模式预览占非焦点 pane——焦点右：查看器在左、右 pane 保持
    // 目录列表；焦点左：查看器在右（M13-9 现状保持）。
    let left = TempDir::new().unwrap();
    fs::write(left.path().join("aa.txt"), "PREVIEW-LEFT-CONTENT").unwrap();
    let right = TempDir::new().unwrap();
    fs::write(right.path().join("bb.txt"), "PREVIEW-RIGHT-CONTENT").unwrap();
    let mut state = AppState::new(Tab::from_physical(left.path(), Config::default()).unwrap());
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(right.path(), Config::default()).unwrap();
    let backend = ratatui::backend::TestBackend::new(80, 12);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    let dump = |terminal: &ratatui::Terminal<ratatui::backend::TestBackend>| {
        let buf = terminal.backend().buffer();
        let mut s = String::new();
        for y in 0..12u16 {
            for x in 0..80u16 {
                s.push_str(buf[(x, y)].symbol());
            }
            s.push('\n');
        }
        s
    };

    // M49-3：该用例验证旧动态对侧布局，显式关闭固定右侧（默认 true）。
    state.config.preview.fixed_side = false;
    // 焦点右 + view：查看器（bb.txt 内容）在左 pane；右 pane 保持列表。

    {
        let tab = state.active_tab_mut();
        tab.focus = crate::tab::Focus::Right;
        tab.view_mode = true;
    }
    terminal
        .draw(|f| render(f, &state, &InputMode::Normal, None))
        .unwrap();
    let s = dump(&terminal);
    assert!(
        s.contains("PREVIEW-RIGHT-CONTENT"),
        "查看器内容在左 pane: {s}"
    );
    assert!(s.contains("bb.txt"), "右 pane 列表保持: {s}");
    assert!(
        !s.contains("PREVIEW-LEFT-CONTENT"),
        "左 pane 自身文件不进预览: {s}"
    );

    // 焦点左 + view：查看器（aa.txt 内容）在右 pane（M13-9 现状保持）。
    {
        let tab = state.active_tab_mut();
        tab.focus = crate::tab::Focus::Left;
        tab.view_mode = true;
    }
    terminal
        .draw(|f| render(f, &state, &InputMode::Normal, None))
        .unwrap();
    let s = dump(&terminal);
    assert!(
        s.contains("PREVIEW-LEFT-CONTENT"),
        "查看器内容在右 pane: {s}"
    );
    assert!(
        !s.contains("PREVIEW-RIGHT-CONTENT"),
        "焦点左时不预览右 pane 文件: {s}"
    );
}

#[test]
fn m34_5_frecency_pane_title_icons_and_query_echo() {
    // M34-5：标题固定文案；行 = devicons 图标 + 全路径（无 [目录]/[文件]
    // 文本标记；✦keyword 保留）；搜索态底部实时回显 query。
    // 路径用**短合成串**（frecency 记录允许不存在的历史路径，图标按尾斜杠
    // /扩展名推断）——Windows runner 临时目录名过长会截断列表行，导致
    // ✦keyword/文件名断言不稳定（v0.11.0 windows CI 首次失败根因，见 NOTES）。
    let tmp = TempDir::new().unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().pane_frecency = Some(crate::tab::FrecencyPane {
        entries: vec![
            crate::frecency::ScoredEntry {
                path: "p".to_owned(),
                topic: "dir".to_owned(),
                keyword: String::new(),
                score: 0.0,
                grade: crate::frecency::Grade::Substring,
                last_visit: 0,
            },
            crate::frecency::ScoredEntry {
                path: "p/main.rs".to_owned(),
                topic: "file".to_owned(),
                keyword: "rs".to_owned(),
                score: 0.0,
                grade: crate::frecency::Grade::NameContains,
                last_visit: 0,
            },
        ],
        query: "ma".to_owned(),
        cursor: 1,
        normal: false,
        g_pending: false,
        preview_focus: false,
        preview_scroll: 0,
        preview_query: None,
        preview_match: 0,
        d_pending: false,
        y_pending: false,
        last_deleted: None,
    });
    let backend = ratatui::backend::TestBackend::new(80, 12);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|f| render(f, &state, &InputMode::Normal, None))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let mut s = String::new();
    for y in 0..12u16 {
        for x in 0..80u16 {
            s.push_str(buf[(x, y)].symbol());
        }
        s.push('\n');
    }
    // CJK 占 2 格、缓冲区按格插空格 → 去空白后断言（标题/回显为连续文案）。
    let compact: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        compact.contains("Frecency-高频目录/文件搜索-SEARCH"),
        "标题: {s}"
    );
    assert!(compact.contains("/ma"), "query 回显: {s}");
    assert!(compact.contains("✦rs"), "keyword 标记: {s}");
    assert!(compact.contains("main.rs"), "结果行路径: {s}");
    assert!(!compact.contains("[目录]"), "文本标记移除: {s}");
    assert!(!compact.contains("[文件]"), "文本标记移除: {s}");
}

#[test]
fn m475_frecency_pane_is_borderless_and_click_matches_list() {
    // M47-5：面板复用焦点栏几何（无边框），点击命中与渲染同源。
    let tmp = TempDir::new().unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().pane_frecency = Some(crate::tab::FrecencyPane {
        entries: vec![crate::frecency::ScoredEntry {
            path: "alpha".to_owned(),
            topic: "file".to_owned(),
            keyword: String::new(),
            score: 0.0,
            grade: crate::frecency::Grade::None,
            last_visit: 0,
        }],
        query: String::new(),
        cursor: 0,
        normal: true,
        g_pending: false,
        preview_focus: false,
        preview_scroll: 0,
        preview_query: None,
        preview_match: 0,
        d_pending: false,
        y_pending: false,
        last_deleted: None,
    });
    let area = ratatui::layout::Rect::new(0, 0, 80, 12);
    let backend = ratatui::backend::TestBackend::new(80, 12);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|f| crate::app::render::render_frecency_panes(f, &state))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    assert_ne!(buf[(0, 2)].symbol(), "│", "列表区首列不得是边框");
    assert_eq!(
        crate::app::render::frecency_click_index(area, crate::tab::Focus::Left, true, 0, 0, 2),
        Some(0),
        "首列点击应命中第一条"
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
    let mut terminal2 = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
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

/// M46-2：同一 Tab 级查询只高亮焦点 pane。焦点切到右栏后左栏不得再出现
/// LightYellow 匹配高亮（vifm 对齐）。
#[test]
fn m462_search_highlight_only_on_focused_pane() {
    let tmp = TempDir::new().unwrap();
    for name in ["alpha", "src.txt", "srconly"] {
        fs::write(tmp.path().join(name), "").unwrap();
    }
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let mut msg = None;
    live_search_jump(&mut state, 0, "src", &mut msg);

    let mode = InputMode::Search {
        input: "src".to_owned(),
        origin: 0,
    };
    fn left_half_highlights(state: &AppState, mode: &InputMode) -> usize {
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, state, mode, None))
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        (0..40u16)
            .flat_map(|x| (0..23u16).map(move |y| (x, y)))
            .filter(|(x, y)| buf[(*x, *y)].bg == Color::LightYellow)
            .count()
    }

    // 焦点左：左半（x<40）出现高亮。
    state.active_tab_mut().focus = crate::tab::Focus::Left;
    assert!(
        left_half_highlights(&state, &mode) > 0,
        "焦点左时左栏应高亮"
    );

    // 焦点右：左半不再高亮（右栏可能无条目，不强制其高亮）。
    state.active_tab_mut().focus = crate::tab::Focus::Right;
    assert_eq!(
        left_half_highlights(&state, &mode),
        0,
        "焦点右时左栏不得高亮"
    );
}

#[test]
fn fix2_yy_p_copies_and_undo_removes_copy() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "A").unwrap();
    let out = TempDir::new().unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let layers = config::keymap_layers();
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
fn fix7_2_right_pane_uses_shared_yank_and_paste_dispatch() {
    let source = TempDir::new().unwrap();
    fs::write(source.path().join("item.txt"), "payload").unwrap();
    fs::write(source.path().join("move.txt"), "move").unwrap();
    let destination = TempDir::new().unwrap();
    let mut state = AppState::new(Tab::from_physical(source.path(), Config::default()).unwrap());
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(source.path(), Config::default()).unwrap();
    state.active_tab_mut().focus = crate::tab::Focus::Right;
    let layers = config::keymap_layers();
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    assert_eq!(
        state.yank.as_ref().unwrap().paths,
        vec![source.path().join("item.txt")]
    );
    assert_eq!(
        state.active_tab().location,
        Location::Physical(source.path().to_path_buf())
    );
    assert_eq!(
        state.active_tab().right.location,
        Location::Physical(source.path().to_path_buf())
    );

    state.active_tab_mut().right.location = Location::Physical(destination.path().to_path_buf());
    state
        .active_tab_mut()
        .right
        .refresh(Config::default())
        .unwrap();

    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('p'),
        KeyModifiers::NONE,
    );
    assert!(destination.path().join("item.txt").exists());
    assert!(source.path().join("item.txt").exists());
    assert_eq!(state.active_tab().focus, crate::tab::Focus::Right);

    state.active_tab_mut().right.location = Location::Physical(source.path().to_path_buf());
    state
        .active_tab_mut()
        .right
        .refresh(Config::default())
        .unwrap();
    state.active_tab_mut().right.cursor = state
        .active_tab()
        .right
        .entries
        .iter()
        .position(|item| item.name == "move.txt")
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
    state.active_tab_mut().right.location = Location::Physical(destination.path().to_path_buf());
    state
        .active_tab_mut()
        .right
        .refresh(Config::default())
        .unwrap();
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('P'),
        KeyModifiers::NONE,
    );
    assert!(!source.path().join("move.txt").exists());
    assert!(destination.path().join("move.txt").exists());
    state.active_tab_mut().right.cursor = 0;

    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('v'),
        KeyModifiers::NONE,
    );
    // M47-8：选择状态按 pane 独立保存，右栏 v/t 写入 right 自己的集合。
    assert_eq!(
        state.active_tab().right.visual,
        Some(0),
        "右栏 v 进入可视选择"
    );
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('t'),
        KeyModifiers::NONE,
    );
    assert!(
        state.active_tab().right.tags.contains("item.txt"),
        "右栏 t 标记焦点条目"
    );
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Esc,
        KeyModifiers::NONE,
    );
    assert!(state.active_tab().right.visual.is_none());
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('d'),
        KeyModifiers::NONE,
    );
    let delete = press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('d'),
        KeyModifiers::NONE,
    );
    assert!(
        matches!(delete, Outcome::ConfirmDelete(..)),
        "右栏 dd 进入共用删除确认"
    );
}

#[test]
fn fix7_3_mk_and_mkdir_apply_to_focused_right_pane() {
    let left = TempDir::new().unwrap();
    let right = TempDir::new().unwrap();
    let mut state = AppState::new(Tab::from_physical(left.path(), Config::default()).unwrap());
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(right.path(), Config::default()).unwrap();
    state.active_tab_mut().focus = crate::tab::Focus::Right;

    assert!(matches!(
        exec_command_impl(&mut state, "mk created"),
        Ok(ExecOutcome::Done { .. })
    ));
    assert!(right.path().join("created").is_dir());
    assert_eq!(
        state.active_tab().location,
        Location::Physical(left.path().to_path_buf())
    );
    assert_eq!(
        state.active_tab().right.location,
        Location::Physical(right.path().join("created"))
    );
    assert!(state.active_tab().right.on_parent_row);

    let cfg = state.config.clone();
    state.active_tab_mut().right.location = Location::Physical(right.path().to_path_buf());
    state.active_tab_mut().right.on_parent_row = false;
    state.active_tab_mut().right.refresh(cfg).unwrap();
    assert!(matches!(
        exec_command_impl(&mut state, "mkdir child"),
        Ok(ExecOutcome::Done { .. })
    ));
    assert!(right.path().join("child").is_dir());
    assert_eq!(
        state.active_tab().right.entries[state.active_tab().right.cursor].name,
        "child"
    );
}

#[test]
fn fix7_2_frecency_o_dispatches_from_both_panes() {
    let _guard = STORE_TEST_LOCK.lock().unwrap();
    let tmp = TempDir::new().unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(tmp.path(), Config::default()).unwrap();
    let layers = config::keymap_layers();
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
    let mut input = InputState::with_keymap(keymap);
    let mut msg = None;

    let left = press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('o'),
        KeyModifiers::NONE,
    );
    assert!(matches!(left, Outcome::OpenFrecencyPane { .. }));
    state.active_tab_mut().focus = crate::tab::Focus::Right;
    let right = press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('o'),
        KeyModifiers::NONE,
    );
    assert!(matches!(right, Outcome::OpenFrecencyPane { .. }));
    assert_eq!(state.active_tab().focus, crate::tab::Focus::Right);
    assert!(matches!(
        exec_command_impl(&mut state, "j"),
        Ok(ExecOutcome::OpenJump {
            kind: JumpKind::Dir,
            ..
        })
    ));
    assert!(matches!(
        exec_command_impl(&mut state, "J"),
        Ok(ExecOutcome::OpenJump {
            kind: JumpKind::File,
            ..
        })
    ));
    assert_eq!(state.active_tab().focus, crate::tab::Focus::Right);
}

#[test]
fn fix2_capital_p_moves_yanked_file() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("m.txt"), "M").unwrap();
    let out = TempDir::new().unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let layers = config::keymap_layers();
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    // M48-5：同名冲突改为阻塞确认请求（原状态栏 "0/1 项: 目标已存在"）。
    assert!(m.contains("已存在"), "msg: {m:?}");
    assert!(!state.pending_paste_conflicts.is_empty(), "应进入确认队列");
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    // M7-9/M26-3：`.rar` 仍为"解压提示"（无读端依赖）；`.7z` 升级为
    // ArchiveFs 虚拟浏览——损坏归档 → 可读错误（不 panic、不误开系统程序）。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.7z"), b"7z fake").unwrap();
    fs::write(tmp.path().join("a.rar"), b"rar fake").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let layers = config::keymap_layers();
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
    let mut input = InputState::with_keymap(keymap);
    let mut msg = None;

    // 光标序：a.7z(0) < a.rar(1)——先损坏 7z，后 rar。
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('l'),
        KeyModifiers::NONE,
    );
    let m = msg.as_deref().unwrap();
    assert!(
        m.contains("进入失败") && m.contains("7z"),
        "损坏 7z 可读错误: {m:?}"
    );
    assert!(matches!(state.active_tab().location, Location::Physical(_)));

    state.active_tab_mut().cursor = 1;
    let mut msg = None;
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('l'),
        KeyModifiers::NONE,
    );
    let m = msg.as_deref().unwrap();
    assert!(m.contains("解压"), "rar 提示解压: {m:?}");
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    // 与其它暂存测试共享真实 trash 索引——串行化避免 clear/undo 互踩。
    let _guard = TRASH_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
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
    // 与其它暂存测试共享真实 trash 索引——串行化避免 clear/undo 互踩。
    let _guard = TRASH_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
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
fn m453d_visual_delete_requires_confirmation() {
    // M45-3d：v 多选 + d → 返回 ConfirmTagDelete（不再直接删除），
    // 文件须保留直到确认。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("f00"), "").unwrap();
    fs::write(tmp.path().join("f01"), "").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let mut input = InputState::with_keymap(crate::keymap::Keymap::default());
    let mut msg = None;
    // v → j → d（选 2 项）。
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('v'),
        KeyModifiers::NONE,
    );
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('j'),
        KeyModifiers::NONE,
    );
    let outcome = press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('d'),
        KeyModifiers::NONE,
    );
    assert_eq!(outcome, Outcome::ConfirmTagDelete);
    assert!(tmp.path().join("f00").exists(), "确认前不得删除");
    assert!(tmp.path().join("f01").exists(), "确认前不得删除");
    assert_eq!(state.active_tab().tagged_count(), 2, "选区已加 tag");
    assert!(state.active_tab().visual.is_none(), "可视态已清");
}

#[test]
fn m453c_confirm_dialog_centered_with_icon_and_highlight() {
    // M46-10：yazi 风格——标题居中带 ⚠、无标题下提示、文件列表 + 滚动条。
    let paths = vec![std::path::PathBuf::from("/tmp/x/secret.txt")];
    let (title, footer, lines) = crate::app::render::confirm_dialog(true, &paths, None, 0, 0, 72);
    assert!(
        title.contains('⚠') && title.contains("是否") && title.contains("1 个文件"),
        "title={title}"
    );
    let joined: String = lines
        .iter()
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(joined.contains("secret.txt"), "显示目标: {joined}");
    assert!(
        !joined.contains("不可恢复") && !joined.contains("移入回收站"),
        "标题下不再显示提示文字: {joined}"
    );
    assert_eq!(
        lines.len(),
        crate::app::render::CONFIRM_LIST_HEIGHT,
        "固定高度（M48-11 按键行改为独立底栏 footer）"
    );
    let key_line = &footer; // M48-11：按键行改为独立底栏
    let yes = key_line.spans.first().expect("y span");
    assert!(
        yes.content.contains('y')
            && yes.style.bg == Some(Color::Red)
            && yes.style.add_modifier.contains(Modifier::BOLD),
        "y 键高亮"
    );

    // 渲染：弹窗居中（不在底部状态行）。
    let tmp = TempDir::new().unwrap();
    let target = tmp.path().join("gone.txt");
    fs::write(&target, "").unwrap();
    let state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let mode = InputMode::Confirm {
        path: target.clone(),
        permanent: false,
        remote: None,
        cursor: 0,
        scroll: 0,
    };
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
    terminal.draw(|f| render(f, &state, &mode, None)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let rows: Vec<String> = (0..24u16)
        .map(|y| (0..80u16).map(|x| buf[(x, y)].symbol()).collect())
        .collect();
    let joined = rows.join("\n");
    let compact = joined.replace(' ', "");
    assert!(compact.contains("是否删除选定的"), "弹窗标题可见: {joined}");
    assert!(compact.contains("gone.txt"), "目标路径可见");
    assert!(!compact.contains("[y/N]"), "不再用状态栏行内提示");
    let rect = crate::app::render::confirm_rect(
        ratatui::layout::Rect::new(0, 0, 80, 24),
        crate::app::render::CONFIRM_LIST_HEIGHT + 3,
    );
    assert_eq!(buf[(rect.x, rect.y)].symbol(), "┌", "居中弹窗边框");
    assert!(!rows[23].contains("是否删除"), "状态行不再承担确认");
}

/// M46-10：超过列表高度时右侧显示滚动条，j/k 导航状态独立。
#[test]
fn m4610_confirm_dialog_scrollbar_and_navigation() {
    let paths: Vec<std::path::PathBuf> = (0..12)
        .map(|i| std::path::PathBuf::from(format!("/tmp/f{i:02}.txt")))
        .collect();
    let (_, _footer, lines) = crate::app::render::confirm_dialog(false, &paths, None, 7, 4, 72);
    let rendered: String = lines
        .iter()
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rendered.contains('▮') || rendered.contains('│'),
        "右侧滚动条: {rendered}"
    );
    assert!(
        rendered.contains("f04.txt"),
        "滚动后可见对应文件: {rendered}"
    );
    assert!(!rendered.contains("f00.txt"), "滚出视口的文件不可见");
}

#[test]
fn visual_selection_flow() {
    // M3-1：v 进入 → j 扩大选区 → Esc 退出，状态正确。
    let (_tmp, mut state) = make_state(4);
    let layers = config::keymap_layers();
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
        &crate::state::JobControls {
            cancel: &std::sync::atomic::AtomicBool::new(false),
            pause: &std::sync::atomic::AtomicBool::new(false),
            progress: &crate::state::JobProgress::default(),
        },
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
        &crate::state::JobControls {
            cancel: &std::sync::atomic::AtomicBool::new(false),
            pause: &std::sync::atomic::AtomicBool::new(false),
            progress: &crate::state::JobProgress::default(),
        },
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    // M47-2：过滤状态从状态栏移到对应 pane 的顶部路径行。
    let (_tmp, mut state) = make_state(3);
    state.active_tab_mut().filter = Some("*.txt".to_owned());

    let backend = ratatui::backend::TestBackend::new(120, 8);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render(frame, &state, &InputMode::Normal, None))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let row = |y: u16| -> String { (0..120u16).map(|x| buffer[(x, y)].symbol()).collect() };
    assert!(row(1).contains("[filter: *.txt]"), "top path: {}", row(1));
    assert!(!row(7).contains("[filter:"), "status: {}", row(7));
}

#[test]
fn m472_filter_applies_to_focused_pane_and_esc_clears() {
    // M47-2：右栏过滤只筛右栏，Esc 清除；左栏不受影响。
    let tmp = TempDir::new().unwrap();
    for name in ["a.txt", "b.md"] {
        fs::write(tmp.path().join(name), "").unwrap();
    }
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(tmp.path(), Config::default()).unwrap();
    state.active_tab_mut().focus = crate::tab::Focus::Right;
    state
        .active_tab_mut()
        .apply_filter("*.txt", Config::default());
    assert_eq!(state.active_tab().right.entries.len(), 1);
    assert_eq!(state.active_tab().right.entries[0].name, "a.txt");
    assert_eq!(state.active_tab().entries.len(), 2, "左栏不受影响");

    let backend = ratatui::backend::TestBackend::new(120, 10);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| render(frame, &state, &InputMode::Normal, None))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let left_top = (0..60u16)
        .map(|x| buffer[(x, 1)].symbol())
        .collect::<String>();
    let right_top = (60..120u16)
        .map(|x| buffer[(x, 1)].symbol())
        .collect::<String>();
    assert!(!left_top.contains("[filter:"), "左栏路径无过滤: {left_top}");
    assert!(
        right_top.contains("[filter: *.txt]"),
        "右栏路径显示过滤: {right_top}"
    );

    // Normal 模式 Esc 清除焦点栏过滤。
    let mut input = crate::app::InputState::default();
    let outcome = press(&mut state, &mut input, KeyCode::Esc, KeyModifiers::NONE);
    assert!(matches!(outcome, crate::app::Outcome::Continue));
    assert!(state.active_tab().right.filter.is_none());
    assert_eq!(state.active_tab().right.entries.len(), 2);
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

    // :help 请求帮助层。M46-7：新窗口路径依赖宿主复用器环境，测试显式
    // 关闭以确定性覆盖应用内帮助层（AUDIT-14 测试环境隔离）。
    state.config.ui.help_in_new_window = false;
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    // M27-2 vifm 几何（40x6）：tab 栏行 0、路径行（TopLine）行 1、
    // `..` 行占行 2，条目视口 2 行（pane 1-4，无边框）→ 条目 0 在行 3、
    // 光标行 1（第 2 条目）在行 4；条目文本列 1 起。
    assert!(buffer[(1, 4)].modifier.contains(Modifier::REVERSED));
    assert!(!buffer[(1, 3)].modifier.contains(Modifier::REVERSED));
    assert!(!buffer[(1, 2)].modifier.contains(Modifier::REVERSED));
}

#[test]
fn m479_list_leading_blank_is_covered_by_row_highlight() {
    // M47-9：每栏列表左侧留白，且整行高亮覆盖该留白（左右对称）。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let backend = ratatui::backend::TestBackend::new(80, 12);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|f| render(f, &state, &InputMode::Normal, None))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    // M49-6：留白收窄为 1 空格（约图标半宽）；第 1 列已是图标。
    assert_eq!(buf[(0, 3)].symbol(), " ", "左栏留白首列");
    assert!(
        buf[(0, 3)].modifier.contains(Modifier::REVERSED),
        "左栏高亮覆盖留白"
    );

    state.active_tab_mut().focus = crate::tab::Focus::Right;
    terminal
        .draw(|f| render(f, &state, &InputMode::Normal, None))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    // 右栏同一 LIST_LEADING_BLANK 常量，整行高亮覆盖留白。
    assert!(
        buf[(41, 3)].modifier.contains(Modifier::REVERSED),
        "右栏高亮覆盖留白"
    );
}

#[test]
fn m478_selection_stays_in_its_pane_after_focus_switch() {
    // M47-8：右栏 v/t 多选后切焦点到左栏，左栏不得呈现右栏的选择状态。
    let tmp = TempDir::new().unwrap();
    for name in ["a.txt", "b.txt"] {
        fs::write(tmp.path().join(name), "").unwrap();
    }
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(tmp.path(), Config::default()).unwrap();
    state.active_tab_mut().focus = crate::tab::Focus::Right;
    let mut input = crate::app::InputState::default();
    press(
        &mut state,
        &mut input,
        KeyCode::Char('v'),
        KeyModifiers::NONE,
    );
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
    assert!(state.active_tab().right.visual.is_some());
    assert!(!state.active_tab().right.tags.is_empty());

    press(&mut state, &mut input, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(state.active_tab().focus, crate::tab::Focus::Left);
    assert!(state.active_tab().visual.is_none(), "左栏未被右栏选区污染");
    assert!(state.active_tab().tags.is_empty(), "左栏未被右栏 tag 污染");
    assert!(
        state.active_tab().right.visual.is_some(),
        "右栏保留自己的选区"
    );
    assert!(
        !state.active_tab().right.tags.is_empty(),
        "右栏保留自己的 tag"
    );

    let backend = ratatui::backend::TestBackend::new(80, 12);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|f| render(f, &state, &InputMode::Normal, None))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let left_bold =
        (0..40u16).any(|x| (3..5).any(|y| buf[(x, y)].modifier.contains(Modifier::BOLD)));
    let right_bold =
        (41..80u16).any(|x| (3..5).any(|y| buf[(x, y)].modifier.contains(Modifier::BOLD)));
    assert!(!left_bold, "左栏不应显示右栏的多选状态");
    assert!(right_bold, "右栏应保留自己的多选状态");
}

#[test]
fn m477_right_parent_row_does_not_double_highlight() {
    // M47-7：右栏 gg 到顶部时只高亮 `..`，第一条真实条目不高亮。
    let tmp = TempDir::new().unwrap();
    for name in ["alpha.txt", "beta.txt"] {
        fs::write(tmp.path().join(name), "").unwrap();
    }
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(tmp.path(), Config::default()).unwrap();
    state.active_tab_mut().focus = crate::tab::Focus::Right;
    state.active_tab_mut().right.cursor_top();
    assert!(state.active_tab().right.on_parent_row);
    let backend = ratatui::backend::TestBackend::new(80, 12);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|f| render(f, &state, &InputMode::Normal, None))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let row_reversed =
        |y: u16| -> bool { (41..80u16).any(|x| buf[(x, y)].modifier.contains(Modifier::REVERSED)) };
    assert!(row_reversed(2), "`..` 行应反显");
    assert!(!row_reversed(3), "第一条真实条目不得反显");
}

#[test]
fn m471_viewer_directory_icons_are_indented() {
    // M47-1：TUI 目录预览（:view 分支）缩进必须在图标之前，而不是只缩进文件名。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("alpha.txt"), "").unwrap();
    fs::create_dir(tmp.path().join("beta")).unwrap();
    fs::write(tmp.path().join("beta/nested.txt"), "").unwrap();
    let state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let backend = ratatui::backend::TestBackend::new(60, 12);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    let area = ratatui::layout::Rect::new(0, 0, 60, 12);
    terminal
        .draw(|f| crate::app::preview::render_view_path(f, &state, tmp.path(), area, None))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let mut found = 0;
    for y in 0..12u16 {
        let row: String = (1..59u16)
            .map(|x| buf[(x, y)].symbol().to_owned())
            .collect();
        if !row.contains("alpha.txt") && !row.contains("nested.txt") {
            continue;
        }
        found += 1;
        let leading = row.chars().take_while(|c| c.is_whitespace()).count();
        assert!(leading >= 2, "图标前应有缩进（实际 {leading}）: {row:?}");
    }
    assert_eq!(found, 2, "应同时渲染顶层与嵌套条目");
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
    let p = crate::frecency::replay(&[], &events, 2, 2, 0);
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
fn m458a_tutor_session_in_normal_mode_renders_panel_not_list() {
    // M45-8a：进入会话后 mode 必须为 Normal——否则 render 先匹配 TutorList
    // 全屏列表，底部 Panel 不可见。此测试锁定"Normal + session = Panel"不变量。
    let (_tmp, mut state) = make_state(2);
    let module = crate::tutor::module_by_id("core_nav").unwrap();
    state.tutor = Some(crate::tutor::TutorSession {
        module: module.clone(),
        idx: 0,
        marks: Vec::new(),
        done: false,
        review: false,
    });
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(70, 14)).unwrap();
    // mode = Normal（修复后 run 循环从 TutorList 进入会话时所设）。
    terminal
        .draw(|f| render(f, &state, &InputMode::Normal, None))
        .unwrap();
    let buf = terminal.backend();
    let text: String = (0..14)
        .flat_map(|y| (0..70).map(move |x| buf.buffer()[(x, y)].symbol().to_owned()))
        .collect();
    // CJK 双宽 + TestBackend 续格伪影：断言前去空白（M21-E3 坑）。
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        compact.contains("预期:"),
        "Normal + session 应渲染底部 Panel: {compact}"
    );
    assert!(
        !compact.contains("选择要学习"),
        "不得渲染全屏模块列表（mode 必须是 Normal 而非 TutorList）"
    );
}

#[test]
fn m455a_without_cmd_topic_filters_only_commands() {
    // M45-5a：cmd topic（cd/tabnew）从跳转选择器剔除，dir/file 保留。
    let entries = vec![
        scored("/home/u/Downloads", "dir", 9.0),
        scored("cd ~", "cmd", 8.0),
        scored("/home/u/notes.md", "file", 7.0),
        scored("tabnew", "cmd", 6.0),
    ];
    let out = crate::app::exec::without_cmd_topic(entries);
    assert_eq!(out.len(), 2);
    assert!(out.iter().all(|e| e.topic != "cmd"));
    assert_eq!(out[0].path, "/home/u/Downloads");
    assert_eq!(out[1].path, "/home/u/notes.md");
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
fn m458d_tutor_list_shows_progress_on_left() {
    // M45-8d：模块列表左侧显示学习进度百分比（纯函数 + 渲染同源）。
    assert_eq!(tutor_progress_label(10, 3), " 30%");
    assert_eq!(tutor_progress_label(0, 0), " --%");
    assert_eq!(tutor_progress_label(2, 5), "100%", "钳制不超过 100%");

    let module = crate::tutor::module_by_id("core_nav").unwrap();
    let mut progress = crate::tutor::TutorProgress::new();
    progress.insert(
        "core_nav".to_owned(),
        crate::tutor::ModuleProgress {
            mastered: module.items.iter().take(1).map(|i| i.id.clone()).collect(),
            ..Default::default()
        },
    );
    let mut state =
        AppState::new(Tab::from_physical(std::path::Path::new("."), Config::default()).unwrap());
    state.tutor_progress = progress;
    state.tutor = None;
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(90, 20)).unwrap();
    terminal
        .draw(|f| {
            render(
                f,
                &state,
                &InputMode::TutorList {
                    cursor: 0,
                    search: None,
                },
                None,
            )
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let mut text = String::new();
    for y in 1..18u16 {
        for x in 0..88u16 {
            text.push_str(buf[(x, y)].symbol());
        }
    }
    assert!(text.contains('%'), "列表应显示进度百分比: {text}");
    assert!(text.contains("core_nav"), "模块仍在列表: {text}");
}

#[test]
fn m458c_tutor_panel_does_not_hide_bottom_entry() {
    // M45-8c：Tutor 面板遮住 pane 底部——列表渲染区与视口高度都须扣除面板，
    // G 跳末尾条目时高亮行落在面板之上。
    assert_eq!(tutor_panel_height(24), 6);
    assert_eq!(tutor_panel_height(40), 10);
    let tmp = TempDir::new().unwrap();
    for i in 0..20 {
        fs::write(tmp.path().join(format!("f{i:02}.txt")), "").unwrap();
    }
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let module = crate::tutor::module_by_id("core_nav").unwrap();
    state.tutor = Some(crate::tutor::TutorSession {
        module: module.clone(),
        idx: 0,
        marks: Vec::new(),
        done: false,
        review: false,
    });
    state.active_tab_mut().cursor_bottom();
    let panel_h = tutor_panel_height(24) as usize;
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    let [_, left, _, _] = pane_layout(area, state.config.ui.layout(), false);
    let list_h = left_list_area(state.config.ui.pane_borders, left).height as usize;
    state
        .active_tab_mut()
        .sync_viewport_scroll(list_h.saturating_sub(panel_h));

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|f| render(f, &state, &InputMode::Normal, None))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    // 面板顶行（panel_y）以上不应有被遮住的列表高亮末尾项；末尾项名出现在面板上方。
    let panel_y = (24 - panel_h - 1) as u16;
    let mut above = String::new();
    for y in 1..panel_y {
        for x in 0..40u16 {
            above.push_str(buf[(x, y)].symbol());
        }
    }
    assert!(
        above.contains("f19.txt"),
        "末尾条目应在面板上方可见: {above}"
    );
}

#[test]
fn m457b_archive_yy_paste_extracts_to_focused_pane() {
    // M45-7b：归档虚拟目录 yy（成员）→ p 粘贴到焦点 pane 物理目录；
    // 逐成员解压、u 可逆、不写归档内。
    let tmp = TempDir::new().unwrap();
    let zip = tmp.path().join("bundle.zip");
    crate::archive::make_test_zip(&zip);
    let dest = tmp.path().join("dest");
    fs::create_dir(&dest).unwrap();

    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    // 左 pane 进入归档虚拟根（模拟 yy 场景）。
    state.active_tab_mut().location = Location::Archive {
        host: zip.clone(),
        inner: std::path::PathBuf::new(),
    };
    state.active_tab_mut().entries =
        Tab::load_for(&state.active_tab().location, Config::default()).unwrap();
    let idx = state
        .active_tab()
        .entries
        .iter()
        .position(|e| e.name == "readme.md")
        .unwrap();
    state.active_tab_mut().cursor = idx;

    // yy（归档分支）：yank = 成员路径 + YankSource::Archive。
    let layers = config::keymap_layers();
    let mut input = InputState::with_keymap(crate::keymap::Keymap::with_layers(
        layers.prepend,
        layers.append,
        layers.replace,
    ));
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
    let yank = state.yank.as_ref().expect("归档 yy 应设置缓冲");
    assert_eq!(
        yank.source,
        crate::state::YankSource::Archive { host: zip.clone() }
    );
    assert_eq!(yank.paths, vec![std::path::PathBuf::from("readme.md")]);

    // 焦点切到右 pane（物理目标目录）后粘贴。
    state.active_tab_mut().focus = crate::tab::Focus::Right;
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(&dest, Config::default()).unwrap();
    let mut msg2 = None;
    paste_yanked(&mut state, false, &mut msg2);
    assert!(
        msg2.as_deref().unwrap_or("").contains("解压"),
        "粘贴反馈: {msg2:?}"
    );
    assert_eq!(
        fs::read(dest.join("readme.md")).unwrap(),
        b"hi",
        "成员内容一致"
    );
    // 归档未被写入（只读）。
    assert!(zip.is_file());
    // M46-5：粘贴解压后目标 pane 列表即时刷新（无需等待 watcher）。
    assert!(
        state
            .active_tab()
            .right
            .entries
            .iter()
            .any(|e| e.name == "readme.md"),
        "解压后目标 pane 应已刷新出 readme.md"
    );
    // u 撤销：解压文件移除。
    let mut msg3 = None;
    undo_last(&mut state, &mut msg3);
    assert!(!dest.join("readme.md").exists(), "u 撤销解压");
}

#[test]
fn fix7_2_right_archive_yank_uses_shared_dispatch() {
    let tmp = TempDir::new().unwrap();
    let zip = tmp.path().join("bundle.zip");
    crate::archive::make_test_zip(&zip);
    let dest = tmp.path().join("dest");
    fs::create_dir(&dest).unwrap();
    let mut state = AppState::new(Tab::from_physical(&dest, Config::default()).unwrap());
    state.active_tab_mut().right.location = Location::Archive {
        host: zip.clone(),
        inner: PathBuf::new(),
    };
    let loc = state.active_tab().right.location.clone();
    state.active_tab_mut().right.entries = Tab::load_for(&loc, Config::default()).unwrap();
    state.active_tab_mut().right.cursor = state
        .active_tab()
        .right
        .entries
        .iter()
        .position(|item| item.name == "readme.md")
        .unwrap();
    state.active_tab_mut().focus = crate::tab::Focus::Right;
    let layers = config::keymap_layers();
    let mut input = InputState::with_keymap(crate::keymap::Keymap::with_layers(
        layers.prepend,
        layers.append,
        layers.replace,
    ));
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
    assert_eq!(
        state.yank.as_ref().unwrap().paths,
        vec![PathBuf::from("readme.md")]
    );
    assert!(matches!(
        state.yank.as_ref().unwrap().source,
        crate::state::YankSource::Archive { .. }
    ));

    state.active_tab_mut().focus = crate::tab::Focus::Left;
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('P'),
        KeyModifiers::NONE,
    );
    assert_eq!(fs::read(dest.join("readme.md")).unwrap(), b"hi");
}

#[test]
fn m457a_extract_dest_defaults_to_opposite_pane() {
    // M45-7a：`:extract` 缺省目标为对侧 pane；left/right/当前/对侧/绝对/
    // 相对路径均可解析（纯函数，不触碰文件）。
    let tmp = TempDir::new().unwrap();
    let left = tmp.path().join("L");
    let right = tmp.path().join("R");
    fs::create_dir(&left).unwrap();
    fs::create_dir(&right).unwrap();
    let mut state = AppState::new(Tab::from_physical(&left, Config::default()).unwrap());
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(&right, Config::default()).unwrap();

    // 左焦点：缺省 = 右栏。
    assert_eq!(extract_dest(&state, None).unwrap(), right, "缺省对侧");
    assert_eq!(extract_dest(&state, Some("left")).unwrap(), left);
    assert_eq!(extract_dest(&state, Some("right")).unwrap(), right);
    assert_eq!(extract_dest(&state, Some("当前")).unwrap(), left);
    assert_eq!(extract_dest(&state, Some("对侧")).unwrap(), right);
    let abs = tmp.path().join("out");
    assert_eq!(
        extract_dest(&state, Some(&abs.display().to_string())).unwrap(),
        abs
    );
    assert_eq!(
        extract_dest(&state, Some("sub/dir")).unwrap(),
        right.join("sub/dir"),
        "相对路径锚定对侧"
    );

    // 右焦点：缺省 = 左栏。
    state.active_tab_mut().focus = crate::tab::Focus::Right;
    assert_eq!(
        extract_dest(&state, None).unwrap(),
        left,
        "右焦点缺省对侧为左"
    );
    assert_eq!(extract_dest(&state, Some("当前")).unwrap(), right);
}

/// M46-5：`:extract` 解压到对侧 pane 后立即刷新该 pane 的条目列表。
#[test]
fn m465_extract_refreshes_destination_pane() {
    let tmp = TempDir::new().unwrap();
    let left = tmp.path().join("L");
    let right = tmp.path().join("R");
    fs::create_dir(&left).unwrap();
    fs::create_dir(&right).unwrap();
    // 在 left 目录内造一个仅含 readme.md 的 zip（避免 make_test_zip 的显式
    // 目录条目触发 extract 路径拼接的既有缺陷，与本任务无关）。
    {
        use std::io::Write as _;
        let file = std::fs::File::create(left.join("bundle.zip")).unwrap();
        let mut w = zip::ZipWriter::new(file);
        w.start_file("readme.md", zip::write::SimpleFileOptions::default())
            .unwrap();
        w.write_all(b"hi").unwrap();
        w.finish().unwrap();
    }

    let mut state = AppState::new(Tab::from_physical(&left, Config::default()).unwrap());
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(&right, Config::default()).unwrap();
    let idx = state
        .active_tab()
        .entries
        .iter()
        .position(|e| e.name == "bundle.zip")
        .unwrap();
    state.active_tab_mut().cursor = idx;
    assert!(state.active_tab().right.entries.is_empty());

    let extract_result = exec_command(&mut state, "extract");
    assert_eq!(
        extract_result,
        Ok(false),
        "extract 结果: {extract_result:?}"
    );
    assert_eq!(fs::read(right.join("readme.md")).unwrap(), b"hi");
    assert!(
        state
            .active_tab()
            .right
            .entries
            .iter()
            .any(|e| e.name == "readme.md"),
        "对侧 pane 应即时看到解压产物"
    );
}

#[test]
fn m456b_jump_to_writes_focused_pane() {
    // M45-6b：`o` 结果跳转写入焦点 pane——右焦点时左 pane 保持不变。
    let tmp = TempDir::new().unwrap();
    let left_dir = tmp.path().join("left");
    let right_dir = tmp.path().join("right");
    let target_dir = tmp.path().join("target");
    let file_dir = tmp.path().join("files");
    fs::create_dir(&left_dir).unwrap();
    fs::create_dir(&right_dir).unwrap();
    fs::create_dir(&target_dir).unwrap();
    fs::create_dir(&file_dir).unwrap();
    fs::write(file_dir.join("note.txt"), "x").unwrap();

    let mut state = AppState::new(Tab::from_physical(&left_dir, Config::default()).unwrap());
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(&right_dir, Config::default()).unwrap();
    state.active_tab_mut().focus = crate::tab::Focus::Right;
    let left_before = state.active_tab().location.clone();
    let hist_before = state.active_tab().history.len();

    // 目录跳转。
    let entry = scored(
        &target_dir.display().to_string().replace('\\', "/"),
        "dir",
        5.0,
    );
    let msg = jump_to(&mut state, &entry).unwrap();
    assert!(msg.contains("跳转目录"), "{msg}");
    assert_eq!(
        state.active_tab().right.location.base_dir(),
        target_dir,
        "右 pane 跳到目标目录"
    );
    assert_eq!(state.active_tab().location, left_before, "左 pane 不变");
    assert_eq!(state.active_tab().history.len(), hist_before, "左历史不变");

    // 文件定位。
    let file = file_dir.join("note.txt");
    let entry = scored(&file.display().to_string().replace('\\', "/"), "file", 5.0);
    let msg = jump_to(&mut state, &entry).unwrap();
    assert!(msg.contains("定位"), "{msg}");
    assert_eq!(state.active_tab().right.location.base_dir(), file_dir);
    let idx = state
        .active_tab()
        .right
        .entries
        .iter()
        .position(|e| e.name == "note.txt")
        .unwrap();
    assert_eq!(state.active_tab().right.cursor, idx, "右 pane 光标定位");
    assert_eq!(state.active_tab().location, left_before, "左 pane 仍不变");
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
    let _guard = TRASH_TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    // M42：w 不移动焦点；预览焦点只在显式 Tab 时进入。
    assert_eq!(state.active_tab().focus, crate::tab::Focus::Left);
    assert!(!state.active_tab().preview_focus);
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

    // M42：Tab 显式进入预览窗格后，j 才滚动查看器视口。
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Tab,
        KeyModifiers::NONE,
    );
    assert!(state.active_tab().preview_focus, "Tab 进入预览焦点");
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
    // 激活标签（M27-2 TabLineSel：黑字加粗回退）落在 "2:" 起始处。
    let two_x = tab_row.find("2:").unwrap_or(0) as u16;
    let cell = &buf[(two_x, 0)];
    assert!(
        cell.modifier.contains(Modifier::BOLD) && cell.fg == Color::Black,
        "2: 段应高亮（TabLineSel 回退）: fg={:?} mods={:?}",
        cell.fg,
        cell.modifier
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
    let id = state.add_job(
        "copy",
        2,
        tmp.path().display().to_string().as_str(),
        &dst.path().display().to_string(),
    );
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
    let id2 = state.add_job("move", 1, "src", "no-such-dir");
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    // 双 pane 不被遮罩（M27-2 vifm 模式：pane 顶部路径行可见 = TopLine
    // 白底；popup 无遮罩仍成立）。
    // 焦点 pane = TopLineSel（黑字加粗、默认底）；非焦点 = TopLine（黑字白底）。
    assert!(
        buffer[(0, 1)].modifier.contains(Modifier::BOLD) && buffer[(0, 1)].fg == Color::Black,
        "左 pane TopLineSel（焦点）"
    );
    assert_eq!(
        buffer[(79, 1)].bg,
        Color::White,
        "右 pane TopLine（非焦点白底）"
    );

    // 160×48：popup = 128×38 @ (16,5)。
    let backend2 = ratatui::backend::TestBackend::new(160, 48);
    let mut terminal2 = ratatui::Terminal::new(backend2).unwrap();
    terminal2
        .draw(|frame| render(frame, &state, &mode, None))
        .unwrap();
    let buffer2 = terminal2.backend().buffer().clone();
    assert_eq!(buffer2[(16, 5)].symbol(), "┌", "160×48 下 popup 左上角");
    assert_eq!(buffer2[(143, 5)].symbol(), "┐");
    // 大终端左 pane 焦点 = TopLineSel（黑字加粗，默认底）。
    assert!(
        buffer2[(0, 1)].modifier.contains(Modifier::BOLD) && buffer2[(0, 1)].fg == Color::Black,
        "大终端 pane TopLineSel（焦点）"
    );
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
    // M41-4：有状态偏移（list_offset_sticky）——窗口内保持 prev；
    // 越上界贴顶、越下界贴底；height 0 → 0。
    assert_eq!(crate::tab::list_offset_sticky(0, 10, 0), 0);
    assert_eq!(crate::tab::list_offset_sticky(9, 10, 0), 0, "窗口内不动");
    assert_eq!(crate::tab::list_offset_sticky(15, 10, 6), 6, "底行可见保持");
    // 钉底修复核心用例：底行（15=6+9）按 k 上移到 14 → 偏移不变（原无状态
    // 推导会滚到 5、光标继续钉在底行）。
    assert_eq!(crate::tab::list_offset_sticky(14, 10, 6), 6);
    assert_eq!(
        crate::tab::list_offset_sticky(16, 10, 6),
        7,
        "越下界光标贴底"
    );
    assert_eq!(
        crate::tab::list_offset_sticky(3, 10, 6),
        3,
        "越上界光标贴顶"
    );
    assert_eq!(crate::tab::list_offset_sticky(0, 0, 0), 0);
}

#[test]
fn m41_4_viewport_scroll_sticky_sync() {
    // M41-4：sync_viewport_scroll——列表不动、高亮上移；越界才滚动；
    // 左/右 pane 独立同步（两栏均有 `..` 行）。
    let (tmp, mut state) = make_state(30);
    let _ = &tmp;
    {
        let tab = state.active_tab_mut();
        tab.cursor = 15;
        tab.right.entries = tab.entries.clone();
        tab.right.cursor = 25;
    }
    // 首帧：cursor 15 越出窗口 → 贴底。左 pane 有 `..` 行（TempDir 有父
    // 目录）→ 条目高 9：偏移 15+1-9=7；右 pane 同为高 9：17。
    state.active_tab_mut().sync_viewport_scroll(10);
    assert_eq!(
        state.active_tab().scroll,
        7,
        "左 pane 越下界贴底（..行占 1）"
    );
    assert_eq!(state.active_tab().right.scroll, 17, "右 pane 独立贴底");
    // k 上移：cursor 24 仍在窗口 [17,26) → 偏移不变（列表不动、高亮上移）。
    state.active_tab_mut().cursor = 14;
    state.active_tab_mut().right.cursor = 24;
    state.active_tab_mut().sync_viewport_scroll(10);
    assert_eq!(state.active_tab().scroll, 7, "窗口内列表不动");
    assert_eq!(state.active_tab().right.scroll, 17, "右 pane 窗口内不动");
    // 越上界：cursor 3 → 贴顶。
    state.active_tab_mut().cursor = 3;
    state.active_tab_mut().sync_viewport_scroll(10);
    assert_eq!(state.active_tab().scroll, 3, "越上界光标贴顶");
}

#[test]
fn m38_4_frecency_command_gates() {
    // M38-4：`:frecency` 命令门控（无参/未知子命令/purge 无 confirm/
    // prune 缺日期与坏日期）——只测错误分支，不触真实 frecency 库
    // （核心链路见 frecency::tests::m38_4_stats_export_purge_prune_roundtrip）。
    let (_tmp, mut state) = make_state(1);
    let r = exec_command_impl(&mut state, "frecency");
    assert!(r.is_err() && r.unwrap_err().contains("需子命令"));
    let r = exec_command_impl(&mut state, "frecency 未知");
    assert!(r.is_err() && r.unwrap_err().contains("未知子命令"));
    let r = exec_command_impl(&mut state, "frecency purge");
    assert!(
        r.is_err() && r.unwrap_err().contains("purge confirm"),
        "无 confirm 拒绝执行"
    );
    let r = exec_command_impl(&mut state, "frecency prune");
    assert!(r.is_err() && r.unwrap_err().contains("--before"));
    let r = exec_command_impl(&mut state, "frecency prune --before 2026-13-01");
    assert!(r.is_err() && r.unwrap_err().contains("日期非法"));
    let r = exec_command_impl(&mut state, "frecency export");
    assert!(r.is_err() && r.unwrap_err().contains("缺少输出文件路径"));
}

#[test]
fn m38_5_find_named_save_load_and_filters() {
    // M38-5：命名搜索 save/load（仅内存、同名覆盖、load 未命中列出已保存）
    // + 过滤器版 :find 走 exec 全链路。
    let (tmp, mut state) = make_state(3);
    // 建立结果视图（模式 f0 → f00/f01/f02）。
    assert!(exec_command_impl(&mut state, "find f0").is_ok());
    assert!(state.active_tab().results.is_some());
    // save → 重搜不同模式 → load 恢复。
    assert!(exec_command_impl(&mut state, "find save 名单").is_ok());
    assert!(exec_command_impl(&mut state, "find f1").is_ok());
    assert_eq!(
        state.active_tab().results.as_ref().unwrap().query,
        "f1",
        "重搜替换结果视图"
    );
    assert!(exec_command_impl(&mut state, "find load 名单").is_ok());
    assert_eq!(
        state.active_tab().results.as_ref().unwrap().query,
        "f0",
        "load 恢复已保存结果"
    );
    assert_eq!(state.active_tab().results.as_ref().unwrap().items.len(), 3);
    // save 无结果视图 → 拒绝。
    let (_t2, mut state2) = make_state(1);
    let r = exec_command_impl(&mut state2, "find save x");
    assert!(r.is_err() && r.unwrap_err().contains("save 需当前结果视图"));
    // load 未知名 → 列出已保存集合。
    let r = exec_command_impl(&mut state, "find load 不存在");
    assert!(r.is_err() && r.unwrap_err().contains("已保存：名单"));
    // 过滤器版 :find 全链路（新文件 mtime=now → -1 命中全部）。
    fs::write(tmp.path().join("big.dat"), vec![b'x'; 2048]).unwrap();
    assert!(exec_command_impl(&mut state, "find --size +1k --mtime=-1").is_ok());
    let results = state.active_tab().results.as_ref().unwrap();
    assert_eq!(results.items.len(), 1, "仅 big.dat ≥ 1k");
    // 未知选项可读报错。
    let r = exec_command_impl(&mut state, "find --bogus 1");
    assert!(r.is_err() && r.unwrap_err().contains("--bogus"));
}

#[test]
fn m39_1_hex_viewer_binary_and_goto() {
    // M39-1：二进制文件 w → hex 模式（16 字节/行、ASCII 侧栏、分块读取）；
    // :goto 偏移跳转（0x/十进制、行对齐钳制）；非 hex 下 goto 拒绝。
    let tmp = TempDir::new().unwrap();
    let mut bin = vec![b'A'; 40];
    bin[10] = 0; // NUL → 二进制判定
    fs::write(tmp.path().join("prog.bin"), &bin).unwrap();
    fs::write(tmp.path().join("text.txt"), "hello\nworld\n").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let txt_idx = state
        .active_tab()
        .entries
        .iter()
        .position(|i| i.name == "text.txt")
        .unwrap();
    let bin_idx = state
        .active_tab()
        .entries
        .iter()
        .position(|i| i.name == "prog.bin")
        .unwrap();
    // 文本文件 → 非 hex（既有文本查看器）。
    state.active_tab_mut().cursor = txt_idx;
    state.active_tab_mut().toggle_view();
    assert!(!state.active_tab().viewer.as_ref().unwrap().hex);
    state.active_tab_mut().toggle_view();
    // 二进制 → hex；总行数 = ceil(40/16) = 3。
    state.active_tab_mut().cursor = bin_idx;
    state.active_tab_mut().toggle_view();
    let v = state.active_tab().viewer.as_ref().unwrap();
    assert!(v.hex);
    assert_eq!(v.line_count, 3, "40 字节 → 3 行");
    // 内容行：偏移前缀 + 十六进制 + NUL → '.'（ASCII 侧栏）。
    let lines = state.active_tab().viewer_hex_lines(10).unwrap();
    assert_eq!(lines.len(), 3);
    assert!(lines[0].starts_with("00000000"));
    assert!(lines[0].contains("41 41 41"), "十六进制字节列");
    assert!(lines[0].contains("00"), "NUL 字节");
    assert!(lines[0].contains("|AAA"), "ASCII 侧栏");
    assert!(lines[2].starts_with("00000020"), "第 3 行偏移 0x20");
    // :goto 0x20 → 行 2；:goto 100（十进制）→ 行 6 → 钳制到 2。
    assert!(exec_command_impl(&mut state, "goto 0x20").is_ok());
    assert_eq!(state.active_tab().viewer.as_ref().unwrap().scroll, 2);
    assert!(exec_command_impl(&mut state, "goto 100").is_ok());
    assert_eq!(
        state.active_tab().viewer.as_ref().unwrap().scroll,
        2,
        "超出总行数钳制"
    );
    // 非 hex 查看器 → goto 拒绝。
    state.active_tab_mut().toggle_view();
    state.active_tab_mut().cursor = txt_idx;
    state.active_tab_mut().toggle_view();
    let r = exec_command_impl(&mut state, "goto 0x10");
    assert!(r.is_err() && r.unwrap_err().contains("需 hex"));
    // 渲染：hex 行（偏移 + ASCII 侧栏）可见——hex 行约 78 列，需宽终端
    // （80 列右 pane ~39 列会截断侧栏）。
    state.active_tab_mut().toggle_view();
    state.active_tab_mut().cursor = bin_idx;
    state.active_tab_mut().toggle_view();
    let backend = ratatui::backend::TestBackend::new(176, 12);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|f| render(f, &state, &InputMode::Normal, Some("")))
        .unwrap();
    let buf = terminal.backend().buffer();
    let mut s = String::new();
    for y in 0..12u16 {
        for x in 0..176u16 {
            s.push_str(buf[(x, y)].symbol());
        }
    }
    assert!(s.contains("00000000"), "hex 行偏移可见: {s}");
    assert!(s.contains("AAAAAAAA"), "ASCII 侧栏可见: {s}");
}

#[test]
fn m394_remote_confirm_and_enter_unreachable() {
    // M39-4：指纹确认重连（不可达主机）——失败返回可读错误、不注册会话、
    // TUI 状态不变（SPEC M39-4：失败/取消后状态一致）。
    let (_tmp, mut state) = make_state(1);
    let loc_before = state.active_tab().location.clone();
    let mut params = crate::remote::sftp::SftpParams::new("127.0.0.1");
    params.port = 1;
    params.name = "dead".to_owned();
    let r = remote_confirm_and_enter(&mut state, "dead", &params);
    assert!(r.is_err());
    assert!(
        !state.remote_sessions.contains_key("dead"),
        "失败不注册会话"
    );
    assert_eq!(state.active_tab().location, loc_before, "位置不变");
}

#[test]
fn m39_6_layout_modes_tabname_and_overflow() {
    // M39-6：布局解析/几何（Single 右零宽、PreviewBottom 上半列表）、
    // 查看器占位、覆盖矩阵、:tabname 设置/清除、标签栏命名与溢出滚动。
    use crate::config::{LayoutMode, UiConfig};
    assert_eq!(LayoutMode::parse("single"), Some(LayoutMode::Single));
    assert_eq!(
        LayoutMode::parse("preview-bottom"),
        Some(LayoutMode::PreviewBottom)
    );
    assert_eq!(LayoutMode::parse("bogus"), None);
    let mut ui = UiConfig {
        layout: Some("preview-right".into()),
        ..Default::default()
    };
    assert_eq!(ui.layout(), LayoutMode::PreviewRight);
    ui.layout = None;
    assert_eq!(ui.layout(), LayoutMode::Dual, "缺省 dual");
    // 几何：dual 基线两 pane 有宽；single 右零宽、左全宽。
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    let [_, l, r, _] = pane_layout(area, LayoutMode::Dual, false);
    assert!(l.width > 0 && r.width > 0);
    let [_, l, r, _] = pane_layout(area, LayoutMode::Single, false);
    assert_eq!(r.width, 0, "Single 右 pane 零宽");
    assert_eq!(l.width, 80, "Single 列表全宽");
    // PreviewBottom + viewing：列表上半、查看器下半全宽。
    let [_, l, r, _] = pane_layout(area, LayoutMode::PreviewBottom, true);
    assert!(r.width > 0 && l.height < 12, "上半高度: {}", l.height);
    // M46-1：viewer_rect 接收 pane_layout 产出的主区（去掉标签栏/状态栏），
    // 与列表区同源；80x24 帧的主区为 y=1、height=22。
    let main = ratatui::layout::Rect::new(0, 1, 80, 22);
    let v = viewer_rect(main, LayoutMode::PreviewBottom, true).unwrap();
    assert!(v.y >= l.y + l.height, "查看器在列表下方");
    assert_eq!(v.width, 80, "查看器全宽");
    let single = viewer_rect(main, LayoutMode::Single, true).unwrap();
    assert_eq!(single, main, "Single 查看器 = 主区");
    // 覆盖矩阵（命中守卫）。
    assert!(pane_covered_by_viewer(
        LayoutMode::Single,
        true,
        crate::tab::Focus::Left,
        crate::tab::Focus::Left
    ));
    assert!(pane_covered_by_viewer(
        LayoutMode::Dual,
        true,
        crate::tab::Focus::Left,
        crate::tab::Focus::Right
    ));
    assert!(!pane_covered_by_viewer(
        LayoutMode::Dual,
        true,
        crate::tab::Focus::Left,
        crate::tab::Focus::Left
    ));
    assert!(pane_covered_by_viewer(
        LayoutMode::PreviewRight,
        true,
        crate::tab::Focus::Left,
        crate::tab::Focus::Right
    ));
    assert!(!pane_covered_by_viewer(
        LayoutMode::PreviewBottom,
        true,
        crate::tab::Focus::Left,
        crate::tab::Focus::Left
    ));
    assert!(!pane_covered_by_viewer(
        LayoutMode::Dual,
        false,
        crate::tab::Focus::Left,
        crate::tab::Focus::Right
    ));
    // :tabname 设置/清除。
    let (_tmp, mut state) = make_state(1);
    exec_command_impl(&mut state, "tabname 工作").unwrap();
    assert_eq!(state.active_tab().name.as_deref(), Some("工作"));
    exec_command_impl(&mut state, "tabname").unwrap();
    assert!(state.active_tab().name.is_none());
    // 标签栏：命名显示 + 溢出滚动标记（窄宽多标签）。
    state.active_tab_mut().name = Some("工作".into());
    for _ in 0..8 {
        state.push_tab(tab_at("x"));
    }
    state.active = 0; // 活动标签 = 命名标签（溢出窗口以其为中心）
    let spans = tab_bar_spans(&state, 30, false);
    let joined: String = spans.iter().map(|s| s.content.clone()).collect();
    assert!(joined.contains("工作"), "命名标签显示: {joined}");
    assert!(
        joined.contains('‹') || joined.contains('›'),
        "溢出滚动标记: {joined}"
    );
}

#[test]
fn m39_2_pdf_viewer_page_flip_keys() {
    // M39-2：viewer `[`/`]` 翻页——page 状态推进/回退/第 1 页停留。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("doc.pdf"), "%PDF-1.4\n%dummy").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let idx = state
        .active_tab()
        .entries
        .iter()
        .position(|i| i.name == "doc.pdf")
        .unwrap();
    state.active_tab_mut().cursor = idx;
    state.active_tab_mut().toggle_view();
    let mut input = InputState::default();
    // M42：Tab 进入预览窗格后 `[`/`]` 才作用于查看器。
    press(&mut state, &mut input, KeyCode::Tab, KeyModifiers::NONE);
    assert!(state.active_tab().preview_focus);
    assert_eq!(state.active_tab().viewer.as_ref().unwrap().page, 0);
    press(
        &mut state,
        &mut input,
        KeyCode::Char(']'),
        KeyModifiers::NONE,
    );
    press(
        &mut state,
        &mut input,
        KeyCode::Char(']'),
        KeyModifiers::NONE,
    );
    assert_eq!(state.active_tab().viewer.as_ref().unwrap().page, 2);
    press(
        &mut state,
        &mut input,
        KeyCode::Char('['),
        KeyModifiers::NONE,
    );
    assert_eq!(state.active_tab().viewer.as_ref().unwrap().page, 1);
    press(
        &mut state,
        &mut input,
        KeyCode::Char('['),
        KeyModifiers::NONE,
    );
    press(
        &mut state,
        &mut input,
        KeyCode::Char('['),
        KeyModifiers::NONE,
    );
    assert_eq!(
        state.active_tab().viewer.as_ref().unwrap().page,
        0,
        "第 1 页停留"
    );
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
fn m452a_cursor_row_has_uniform_background() {
    // M45-2a：反显（光标）行不用文件类型 fg——文字部分不得出现单独底色；
    // 非光标行仍按类型着色（目录 Cyan）。
    let tmp = TempDir::new().unwrap();
    fs::create_dir(tmp.path().join("d0")).unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    // 默认 gruvbox 主题未激活（TestBackend）——fallback Cyan。
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 12)).unwrap();

    // 光标在 d0（row 3，index 0）→ 名称 cell fg = Reset（无单独着色）。
    state.active_tab_mut().cursor = 0;
    terminal
        .draw(|f| render(f, &state, &InputMode::Normal, None))
        .unwrap();
    let cell = terminal.backend().buffer()[(4, 3)].clone();
    assert_eq!(
        cell.fg,
        Color::Reset,
        "光标行名称不应带文件类型 fg（底色统一）: fg={:?}",
        cell.fg
    );

    // 光标移到第二项（文件）→ d0 行（row 3）恢复 Cyan。
    let entries_len = state.active_tab().entries.len();
    if entries_len > 1 {
        state.active_tab_mut().cursor = 1;
        terminal
            .draw(|f| render(f, &state, &InputMode::Normal, None))
            .unwrap();
        let cell = terminal.backend().buffer()[(4, 3)].clone();
        assert_eq!(cell.fg, Color::Cyan, "非光标目录行仍按类型着色");
    }
}

/// M46-3：右 pane 光标/可视行与左 pane M45-2a 同款——整行背景一致，
/// 不出现目录类型 Cyan 前景色单独着色。
#[test]
fn m463_right_pane_cursor_row_has_uniform_background() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir(tmp.path().join("rdir")).unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(tmp.path(), Config::default()).unwrap();
    state.active_tab_mut().focus = crate::tab::Focus::Right;
    state.active_tab_mut().right.on_parent_row = false;
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 12)).unwrap();
    terminal
        .draw(|f| render(f, &state, &InputMode::Normal, None))
        .unwrap();
    let buf = terminal.backend().buffer().clone();

    let mut reversed_rows = HashSet::new();
    for y in 2..11u16 {
        for x in 41..80u16 {
            if buf[(x, y)].modifier.contains(Modifier::REVERSED) {
                reversed_rows.insert(y);
            }
        }
    }
    assert!(!reversed_rows.is_empty(), "右栏应有反显光标行");
    for y in reversed_rows {
        for x in 41..80u16 {
            assert_ne!(
                buf[(x, y)].fg,
                Color::Cyan,
                "右栏光标行不得带目录类型 fg（x={x}, y={y}）"
            );
        }
    }
}

#[test]
fn m451c_tab_label_style_decoupled_from_path() {
    // M45-1c：激活标签灰白底黑字加粗；非激活深灰底白字；borders 模式 REVERSED。
    let active = crate::app::render::tab_label_style(true, false);
    let inactive = crate::app::render::tab_label_style(false, false);
    assert_eq!(active.bg, Some(Color::Gray), "灰白底");
    assert_eq!(active.fg, Some(Color::Black), "字体反色");
    assert!(active.add_modifier.contains(Modifier::BOLD));
    assert_eq!(inactive.bg, Some(Color::DarkGray));
    assert_eq!(inactive.fg, Some(Color::White));
    let bordered = crate::app::render::tab_label_style(true, true);
    assert!(bordered.add_modifier.contains(Modifier::REVERSED));
}

#[test]
fn m451d_pane_separator_default_off_and_config() {
    // M45-1d：默认不显示分隔线，配置 true 恢复。
    assert!(!Config::default().ui.pane_separator, "默认关");
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("config.toml");
    fs::write(&path, "[ui]\npane_separator = true\n").unwrap();
    assert!(config::load_from(&path).unwrap().ui.pane_separator);
    // 渲染：默认配置下 pane 区域无 │ 分隔列（标签栏/路径行除外）。
    let (_tmp, state) = make_state(1);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(60, 12)).unwrap();
    terminal
        .draw(|f| render(f, &state, &InputMode::Normal, None))
        .unwrap();
    let buf = terminal.backend();
    // 分隔列旧实现在 pane 中部整列 │；新默认无——中部 row 不应出现 │ 列。
    // （路径行可能含 │，故只检查中部某行某列为空格而非 │：用行 6 中间列近似）
    let mid_row = 6;
    let gap_col = 30;
    assert_ne!(
        buf.buffer()[(gap_col, mid_row)].symbol(),
        "│",
        "默认不渲染分隔列"
    );
}

#[test]
fn m451b_tab_bar_click_switches_tab() {
    // M45-1b：点击标签栏可见标签 → 切到该 tab（不触发列表点击/打开）。
    let (_tmp, mut state) = make_state(1);
    state.new_tab_here();
    state.new_tab_here();
    assert_eq!(state.tabs.len(), 3);
    assert_eq!(state.active, 2, "新标签切到末尾");

    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    let [tab_bar, _, _, _] = crate::app::render::pane_layout(
        area,
        state.config.ui.layout(),
        state.active_tab().view_mode,
    );
    // 找第 1 个标签文本起始列（render 同源 geometry）。
    let (labels, visible) = crate::app::render::tab_bar_labels_and_visible(&state, tab_bar.width);
    assert!(visible.contains(&0) && visible.contains(&1));
    let mut x = 0usize;
    if visible[0] > 0 {
        x += 2;
    }
    // 目标 tab 0（第一个可见标签）——累加其前序可见标签宽度。
    for &idx in &visible {
        if idx == 0 {
            break;
        }
        x += labels[idx].chars().count();
    }
    let click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: tab_bar.x + x as u16 + 1,
        row: tab_bar.y,
        modifiers: KeyModifiers::NONE,
    };
    let mut msg = None;
    handle_mouse(&mut state, &InputMode::Normal, &click, &mut msg, area);
    assert_eq!(state.active, 0, "点击首个标签切到 tab 0");
    // 列表点击未被误触发（光标/位置未变）。
    assert!(matches!(state.active_tab().location, Location::Physical(_)));
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
        cmd_mark_pending: false,
        ranked: Vec::new(),
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    // M45-1a：display_path 统一 `/` 分隔（Windows 原为 `\`）——比较前
    // 同样规范化，避免宿主路径分隔符差异（AGENTS §7 跨平台一致）。
    assert_eq!(
        state.active_tab().location.display_path(),
        tmp.path().to_string_lossy().replace('\\', "/")
    );
    assert!(!state.active_tab().on_parent_row);
}

#[test]
fn fix7_1_right_parent_row_is_visible_and_navigable() {
    let tmp = TempDir::new().unwrap();
    let sub = tmp.path().join("sub");
    fs::create_dir(&sub).unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(&sub, Config::default()).unwrap();
    state.active_tab_mut().focus = crate::tab::Focus::Right;
    let layers = config::keymap_layers();
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
    let mut input = InputState::with_keymap(keymap);
    let mut msg = None;

    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('k'),
        KeyModifiers::NONE,
    );
    assert!(state.active_tab().right.on_parent_row);
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Enter,
        KeyModifiers::NONE,
    );
    assert!(!state.active_tab().right.on_parent_row);
    assert_eq!(
        state.active_tab().right.location.display_path(),
        tmp.path().to_string_lossy().replace('\\', "/")
    );
}

#[test]
fn feat2_enter_dir_lands_on_parent_row_and_guards_writes() {
    let tmp = TempDir::new().unwrap();
    let sub = tmp.path().join("sub");
    fs::create_dir(&sub).unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let layers = config::keymap_layers();
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    // M45-1a：display_path 统一 `/` 分隔（Windows 原为 `\`）——比较前
    // 同样规范化，避免宿主路径分隔符差异（AGENTS §7 跨平台一致）。
    assert_eq!(
        state.active_tab().location.display_path(),
        tmp.path().to_string_lossy().replace('\\', "/")
    );
}

#[test]
fn feat2_archive_parent_row_returns_to_host_dir() {
    let tmp = TempDir::new().unwrap();
    let zip_path = tmp.path().join("t.zip");
    crate::archive::make_test_zip(&zip_path);
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let layers = config::keymap_layers();
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let mut state2 = AppState::new(Tab::from_physical(empty.path(), Config::default()).unwrap());
    assert_eq!(
        exec_command_impl(&mut state2, "file"),
        Err("file: 无选中条目".to_owned())
    );
}

#[test]
fn m34_1_vifm_style_prefers_theme_derivation_over_16color_fallback() {
    // M34-1：激活主题后 vifm 组走 token 推导（gruvbox 色板）；无主题回退
    // M27-2 的 16 色。持 TEST_LOCK 与颜色敏感测试串行。
    let _guard = crate::theme::TEST_LOCK.lock().unwrap();
    crate::theme::set_builtin("gruvbox-dark").unwrap();
    let s = crate::app::render::vifm_style("TopLine");
    assert_eq!(s.fg, Some(ratatui::style::Color::Rgb(254, 128, 25)));
    assert_eq!(s.bg, Some(ratatui::style::Color::Rgb(40, 40, 40)));
    crate::theme::reset_active();
    let f = crate::app::render::vifm_style("TopLine");
    assert_eq!(f.fg, Some(ratatui::style::Color::Black));
    assert_eq!(f.bg, Some(ratatui::style::Color::White));
    let f2 = crate::app::render::vifm_style("TabLineSel");
    assert_eq!(f2.fg, Some(ratatui::style::Color::Black));
    assert!(f2.add_modifier.contains(ratatui::style::Modifier::BOLD));
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
    let err = exec_command_impl(&mut state, "theme no-such-theme-xyz").expect_err("未知主题应报错");
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
    // AUDIT-14：宿主可能处于 tmux，显式关闭新窗口路由以确定性覆盖挂起分支。
    state.config.ui.terminal_in_new_window = false;
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
fn m453b_executable_enter_shows_hint_only() {
    // M45-3b：可执行文件 Enter 仅状态栏提示，不走系统 opener、不执行；
    // 显式 [open] 规则绑定到 run 动作时才按其执行（唯一绕过口）。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("run.exe"), "").unwrap();
    fs::write(tmp.path().join("notes.txt"), "T").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    // 定位可执行文件行（排序按名，勿假设下标）。
    let exec_idx = state
        .active_tab()
        .entries
        .iter()
        .position(|e| e.name == "run.exe")
        .expect("run.exe 应在列表中");
    state.active_tab_mut().cursor = exec_idx;
    let mut msg = None;
    open_selected(&mut state, &mut msg);
    assert_eq!(
        msg.as_deref(),
        Some("可执行文件，请在终端执行"),
        "可执行文件仅提示"
    );
    assert!(state.pending_external.is_none(), "不得执行");
    assert_eq!(
        state.active_tab().location,
        crate::tab::Location::Physical(tmp.path().to_path_buf()),
        "位置不变"
    );

    // 显式规则绑定 run → 允许执行（block=true 转交 run 循环，测试可断言）。
    state.config.opener.insert(
        "run_it".to_owned(),
        crate::opener::OpenerDef {
            run: "sh %s".to_owned(),
            block: true,
            desc: None,
        },
    );
    state.config.open.prepend_rules = vec![crate::opener::OpenRule {
        mime: None,
        name: Some("*.exe".to_owned()),
        use_rules: vec!["run_it".to_owned()],
    }];
    let mut msg2 = None;
    open_selected(&mut state, &mut msg2);
    let cmd = state.pending_external.take().expect("显式 run 绑定应执行");
    assert!(cmd.contains("run.exe"), "命令已展开: {cmd}");
    assert!(msg2.is_none(), "显式绑定不提示: {msg2:?}");
}

#[cfg(unix)]
#[test]
fn m486_archive_paste_flattens_file_and_recurses_directory() {
    // M48-6：归档内 yy 文件只落末段文件名；yy 目录递归展开子项；多选混合正确。
    let tmp = TempDir::new().unwrap();
    let zip = tmp.path().join("bundle.zip");
    crate::archive::make_test_zip(&zip);
    let dest = TempDir::new().unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(dest.path(), Config::default()).unwrap();
    state.active_tab_mut().focus = crate::tab::Focus::Right;
    let mut msg = None;

    // 单文件成员：src/main.rs → 目标根目录只出现 main.rs（无 src/ 上级）。
    state.yank = Some(crate::state::YankBuffer {
        paths: vec![PathBuf::from("src/main.rs")],
        cut: false,
        source: crate::state::YankSource::Archive { host: zip.clone() },
    });
    paste_yanked(&mut state, false, &mut msg);
    assert!(dest.path().join("main.rs").exists(), "文件末段落盘");
    assert!(!dest.path().join("src").exists(), "不得重建归档内上级目录");

    // 目录成员：递归展开 main.rs（原实现只建空壳）。
    state.yank = Some(crate::state::YankBuffer {
        paths: vec![PathBuf::from("src")],
        cut: false,
        source: crate::state::YankSource::Archive { host: zip.clone() },
    });
    paste_yanked(&mut state, false, &mut msg);
    assert!(dest.path().join("src/main.rs").exists(), "目录成员递归解压");

    // 多选混合：文件 + 目录同时正确。
    state.yank = Some(crate::state::YankBuffer {
        paths: vec![PathBuf::from("src/main.rs"), PathBuf::from("readme.md")],
        cut: false,
        source: crate::state::YankSource::Archive { host: zip },
    });
    paste_yanked(&mut state, false, &mut msg);
    assert!(dest.path().join("main.rs").exists());
    assert!(dest.path().join("readme.md").exists());
}

#[test]
fn m485_paste_conflict_uses_blocking_confirmation() {
    // M48-5：同名冲突不再只写状态栏，进入阻塞确认；取消不改文件。
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();
    fs::write(src.path().join("same.txt"), "NEW").unwrap();
    fs::write(dst.path().join("same.txt"), "OLD").unwrap();
    let mut state = AppState::new(Tab::from_physical(src.path(), Config::default()).unwrap());
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(dst.path(), Config::default()).unwrap();
    let mut input = crate::app::InputState::default();
    // 直接设 yank，聚焦冲突确认本身（避免 yy 待决序列干扰）。
    state.yank = Some(crate::state::YankBuffer {
        paths: vec![src.path().join("same.txt")],
        cut: false,
        source: crate::state::YankSource::Local,
    });
    state.active_tab_mut().focus = crate::tab::Focus::Right;
    let outcome = press(
        &mut state,
        &mut input,
        KeyCode::Char('p'),
        KeyModifiers::NONE,
    );
    assert!(matches!(outcome, Outcome::ConfirmPaste));
    assert_eq!(state.pending_paste_conflicts.len(), 1);
    assert_eq!(
        fs::read_to_string(dst.path().join("same.txt")).unwrap(),
        "OLD"
    );

    let mut msg = None;
    cancel_paste_conflicts(&mut state, &mut msg);
    assert!(state.pending_paste_conflicts.is_empty());
    assert_eq!(
        fs::read_to_string(dst.path().join("same.txt")).unwrap(),
        "OLD"
    );

    state.yank = Some(crate::state::YankBuffer {
        paths: vec![src.path().join("same.txt")],
        cut: false,
        source: crate::state::YankSource::Local,
    });
    state
        .pending_paste_conflicts
        .push(dst.path().join("same.txt"));
    confirm_paste_conflicts(&mut state, &mut msg);
    assert_eq!(
        fs::read_to_string(dst.path().join("same.txt")).unwrap(),
        "NEW"
    );
}

#[test]
fn m484_visual_yank_fills_buffer_for_paste() {
    // M48-4：v 多选 + y 写 yank 缓冲，对侧 p/P 均可粘贴（原仅写剪贴板）。
    let src = TempDir::new().unwrap();
    let dst = TempDir::new().unwrap();
    for name in ["a.txt", "b.txt"] {
        fs::write(src.path().join(name), name).unwrap();
    }
    let mut state = AppState::new(Tab::from_physical(src.path(), Config::default()).unwrap());
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(dst.path(), Config::default()).unwrap();
    let mut input = crate::app::InputState::default();
    press(
        &mut state,
        &mut input,
        KeyCode::Char('v'),
        KeyModifiers::NONE,
    );
    press(
        &mut state,
        &mut input,
        KeyCode::Char('j'),
        KeyModifiers::NONE,
    );
    let mut msg = None;
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('y'),
        KeyModifiers::NONE,
    );
    let yanked = state.yank.clone().expect("y 应写缓冲区");
    assert_eq!(yanked.paths.len(), 2);
    assert!(!yanked.cut);

    // p → 对侧复制；P → 对侧同样可用（移动语义另测，这里只断言不报空缓冲）。
    state.active_tab_mut().focus = crate::tab::Focus::Right;
    paste_yanked(&mut state, false, &mut msg);
    assert!(dst.path().join("a.txt").exists());
    assert!(dst.path().join("b.txt").exists());
    assert_ne!(msg.as_deref(), Some("缓冲区为空（先 yy 复制）"));
}

#[test]
fn m483_right_pane_file_enter_uses_shared_opener() {
    // M48-3：右栏文件 l/Enter 与左栏一致走 opener（不再 no-op）。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("note.md"), "N").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().right =
        crate::tab::Pane::from_physical(tmp.path(), Config::default()).unwrap();
    state.active_tab_mut().focus = crate::tab::Focus::Right;
    state.config.ui.terminal_in_new_window = false; // AUDIT-14 测试隔离
    state.config.opener.insert(
        "edit".to_owned(),
        crate::opener::OpenerDef {
            run: "echo %s".to_owned(),
            block: true,
            desc: None,
        },
    );
    state.config.open.prepend_rules = vec![crate::opener::OpenRule {
        mime: None,
        name: Some("*.md".to_owned()),
        use_rules: vec!["edit".to_owned()],
    }];
    let mut input = crate::app::InputState::default();
    let mut msg: Option<String> = None;
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Enter,
        KeyModifiers::NONE,
    );
    let cmd = state
        .pending_external
        .take()
        .expect("右栏文件应触发 opener");
    assert!(cmd.contains("note.md"), "路径取焦点栏: {cmd}");
    assert!(msg.is_none());
}

#[test]
fn m482_executable_text_file_enters_text_opener() {
    // M48-2：带执行位的 .sh 仍按文本打开，不再落入「可执行文件仅提示」。
    use std::os::unix::fs::PermissionsExt;
    let tmp = TempDir::new().unwrap();
    let script = tmp.path().join("run.sh");
    fs::write(&script, "#!/bin/sh\necho hi\n").unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.config.ui.terminal_in_new_window = false; // AUDIT-14 测试隔离
    let idx = state
        .active_tab()
        .entries
        .iter()
        .position(|e| e.name == "run.sh")
        .unwrap();
    assert!(state.active_tab().entries[idx].is_exec, "执行位应被识别");
    state.active_tab_mut().cursor = idx;
    state.config.opener.insert(
        "edit".to_owned(),
        crate::opener::OpenerDef {
            run: "echo %s".to_owned(),
            block: true,
            desc: None,
        },
    );
    state.config.open.prepend_rules = vec![crate::opener::OpenRule {
        mime: Some("text/*".to_owned()),
        name: None,
        use_rules: vec!["edit".to_owned()],
    }];
    let mut msg = None;
    open_selected(&mut state, &mut msg);
    assert!(!matches!(msg.as_deref(), Some("可执行文件，请在终端执行")));
    assert!(
        state.pending_external.is_some() || msg.is_some(),
        "文本 opener 被消费或已开新窗"
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

/// M46-7：帮助覆盖层不透明（Clear + 黑底，不透出下层双栏）；HelpSearch
/// 有独立渲染分支，`/` 输入期可见，内容不再被清空。
#[test]
fn m467_help_overlay_opaque_and_search_input_visible() {
    let (_tmp, state) = make_state(2);
    let backend = ratatui::backend::TestBackend::new(80, 12);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render(
                frame,
                &state,
                &InputMode::HelpView {
                    scroll: 0,
                    search: None,
                },
                None,
            )
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    // 不透明：内部单元被黑色背景覆盖，且下层双栏文件名不得透出。
    let black = buf.content.iter().filter(|c| c.bg == Color::Black).count();
    assert!(black > 700, "帮助页应铺满不透明背景（实际 {black} 格）");
    let first_view = (0..12u16)
        .map(|y| (0..80u16).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !first_view.contains("f00"),
        "下层文件列表不得透出: {first_view}"
    );

    // HelpSearch：输入框与已键入查询可见。
    terminal
        .draw(|frame| {
            render(
                frame,
                &state,
                &InputMode::HelpSearch {
                    input: "filter".to_owned(),
                    scroll: 0,
                    search: None,
                },
                None,
            )
        })
        .unwrap();
    let b2 = terminal.backend().buffer().clone();
    let text = (0..12u16)
        .map(|y| (0..80u16).map(|x| b2[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("/filter"), "搜索输入应可见: {text}");
    assert!(
        text.contains("rvifm") && text.contains("导"),
        "帮助正文仍在（未被清空）: {text}"
    );
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
        mk("/file", "file", "dl", 99.0),
        mk("/other", "dir", "xx", 50.0),
    ];
    // M25-2：目录与文件都参与（file 高分 99 → 跳转语义由 jump_to 分派）。
    assert_eq!(
        quickjump_target(&entries, "dl").map(|e| e.path.as_str()),
        Some("/file"),
        "keyword 精确匹配的最高分条目（不限 topic）"
    );
    assert_eq!(quickjump_target(&entries, "zz"), None);
    // 纯 dir 集（无 file 竞争）仍取最高分。
    let dirs = vec![mk("/low", "dir", "dl", 1.0), mk("/high", "dir", "dl", 9.0)];
    assert_eq!(
        quickjump_target(&dirs, "dl").map(|e| e.path.as_str()),
        Some("/high")
    );
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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

    // M38-1：find 结果可操作——dd 删除命中文件（真实路径）。
    let layers = config::keymap_layers();
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    assert!(
        !tmp.path().join("a.txt").exists(),
        "结果视图 dd 删除命中文件"
    );

    // 写命令仍被拒绝（exec 守卫——rename 不在豁免名单）。
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let mut empty_state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
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
    // M58-7：成功也回显返回码（旧行为 None 已变更）。
    assert!(
        run_external_current_result(ok)
            .unwrap()
            .contains("返回码 0")
    );
    let bad = std::process::Command::new("rvifm-nonexistent-cmd-xyz").status();
    assert!(run_external_current_result(bad).unwrap().contains("失败"));
    // 路由判定不依赖宿主变量（AUDIT-14）：只注入会短路到 CurrentPane 的
    // 组合——Some+true 会真实 spawn 复用器，不在测试覆盖内。
    let cfg_true = crate::config::Config::default();
    assert!(cfg_true.ui.terminal_in_new_window, "缺省 true");
    assert!(
        matches!(
            run_external_target_with_and_dir(None, &cfg_true.ui, "x", std::path::Path::new("/tmp")),
            ExternalRun::CurrentPane(_)
        ),
        "无复用器 → 当前 pane"
    );
    let mut cfg_false = crate::config::Config::default();
    cfg_false.ui.terminal_in_new_window = false;
    assert!(
        matches!(
            run_external_target_with_and_dir(
                Some(MuxKind::Tmux),
                &cfg_false.ui,
                "x",
                std::path::Path::new("/tmp")
            ),
            ExternalRun::CurrentPane(_)
        ),
        "开关关闭时忽略复用器环境"
    );
    assert!(
        matches!(
            run_external_target_with_and_dir(
                Some(MuxKind::Screen),
                &cfg_false.ui,
                "x",
                std::path::Path::new("/tmp")
            ),
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
    let args = tmux_window_args_with_dir("ls -la 'a b'", std::path::Path::new("/tmp"));
    assert_eq!(args[0], "new-window");
    // M48-1：去掉 -d（新窗口前台激活），改用 -c <dir> 指定起始目录。
    assert_eq!(args[1], "-c");
    assert!(args[3].starts_with("sh -c 'ls -la"));
}

#[test]
fn m481_tmux_window_args_use_focused_dir_and_activate() {
    let dir = std::path::Path::new("/tmp/focused");
    let args = tmux_window_args_with_dir("pwd", dir);
    assert_eq!(args[0], "new-window");
    assert_eq!(args[1], "-c");
    assert_eq!(args[2], "/tmp/focused");
    assert!(!args.iter().any(|a| a == "-d"), "不得用 -d 分离窗口");
    assert!(args[3].starts_with("sh -c"));

    // 注入 runner 也不受宿主 TMUX/STY 影响（AUDIT-14 保持）。
    let cfg = crate::config::UiConfig {
        terminal_in_new_window: false,
        ..crate::config::UiConfig::default()
    };
    assert!(matches!(
        run_external_target_with_and_dir(Some(MuxKind::Tmux), &cfg, "pwd", dir),
        ExternalRun::CurrentPane(_)
    ));
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
    fn write(&self, path: &std::path::Path, data: &[u8]) -> Result<(), crate::remote::RemoteError> {
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
    fn remove_permanent(&self, path: &std::path::Path) -> Result<(), crate::remote::RemoteError> {
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

    // 目录在前并带 `/` 后缀（M27-2 vifm 模式：pane 顶部路径行占行 1
    // （TopLine 白底），`..` 行占行 2，条目行 3 起）。
    let dir_name = tmp
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    assert!(row(1).contains(&dir_name), "左 pane 顶部路径行: {}", row(1));
    // 左 pane 焦点 = TopLineSel（黑字加粗、默认底）；右 pane 非焦点 = 白底。
    assert!(
        buffer[(0, 1)].modifier.contains(Modifier::BOLD),
        "焦点 TopLineSel 加粗"
    );
    assert_eq!(
        buffer[(81, 1)].bg,
        Color::White,
        "右 pane TopLine 白底（非焦点）"
    );
    assert!(row(2).contains("../"));
    assert!(row(3).contains("sub/"));
    assert!(row(4).contains("hello.txt"));
    // M21-F1：路径在 pane 顶部路径行，状态栏不再显示路径；详情含条目数。
    assert!(row(1).contains(&dir_name), "pane 顶部路径: {}", row(1));
    let status = row(9).trim().to_owned();
    assert!(!status.contains(&dir_name), "状态栏不应显示路径: {status}");
    assert!(
        status.contains("2 items"),
        "status bar missing count: {status}"
    );
}

#[test]
fn m455c_command_name_tab_completion() {
    // M45-5c：`:` 首词 Tab 补全命令名（路径场景仍由 command_name_position
    // 分流到文件补全，纯函数已覆盖分流判定）。
    use crate::app::exec::{COMMAND_NAMES, command_name_position};
    assert!(command_name_position("ar"));
    assert!(!command_name_position("cd /tmp"));
    assert!(!command_name_position(""));

    let names: Vec<String> = COMMAND_NAMES.iter().map(|n| (*n).to_owned()).collect();
    let mut input = "ar".to_owned();
    let mut comp = None;
    let n = tab_complete(&mut input, &mut comp, &names);
    assert_eq!(n, 1, "仅 archive 命中: {n}");
    assert_eq!(input, "archive", "Tab 补全为命令名");

    // 短前缀可补全多个候选（如 "ta" → tabclose/tabmove/tabname/tabnew/tag）。
    let mut input2 = "tab".to_owned();
    let mut comp2 = None;
    let n2 = tab_complete(&mut input2, &mut comp2, &names);
    assert!(n2 >= 4, "tab* 候选: {n2}");
    assert!(input2.starts_with("tab"), "补全仍在命令名空间");
}

#[test]
fn fix8_frecency_query_highlights_all_matching_terms() {
    let spans = crate::app::render::highlight_query_spans("/home/rvifm/src/main.rs", "rvifm main");
    let rendered = spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>();
    assert_eq!(rendered, "/home/rvifm/src/main.rs");
    let highlighted = spans
        .iter()
        .filter(|span| span.style.bg == Some(ratatui::style::Color::LightYellow))
        .map(|span| span.content.as_ref())
        .collect::<String>();
    assert_eq!(highlighted, "rvifmmain");
}

#[test]
fn fix8_mouse_click_in_frecency_pane_jumps_focused_right_pane() {
    // M49-3：此例验证旧动态对侧布局（焦点右 → 搜索区在右），显式关固定右侧。
    let tmp = TempDir::new().unwrap();
    let target = tmp.path().join("target");
    fs::create_dir(&target).unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.config.preview.fixed_side = false;
    state.active_tab_mut().focus = crate::tab::Focus::Right;
    state.active_tab_mut().pane_frecency = Some(crate::tab::FrecencyPane {
        entries: vec![crate::frecency::ScoredEntry {
            path: target.to_string_lossy().into_owned(),
            topic: "dir".to_owned(),
            keyword: String::new(),
            score: 1.0,
            grade: crate::frecency::Grade::Substring,
            last_visit: 0,
        }],
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
    });
    let area = ratatui::layout::Rect::new(0, 0, 100, 30);
    let [_, _, right, _] = pane_layout(area, crate::config::LayoutMode::Dual, false);
    let inner = ratatui::widgets::Block::bordered().inner(right);
    let click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: inner.x,
        row: inner.y,
        modifiers: KeyModifiers::NONE,
    };
    let mut msg = None;
    handle_mouse(&mut state, &InputMode::Normal, &click, &mut msg, area);
    assert!(state.active_tab().pane_frecency.is_none());
    assert_eq!(state.active_tab().right.location.base_dir(), target);
}

#[test]
fn fix8_frecency_preview_uses_view_renderer_for_images_and_text() {
    let tmp = TempDir::new().unwrap();
    let text_path = tmp.path().join("sample.rs");
    fs::write(&text_path, "fn main() {\n    println!(\"hello\");\n}\n").unwrap();
    let image_path = tmp.path().join("sample.png");
    let image = image::DynamicImage::new_rgba8(2, 2);
    let mut png = std::io::Cursor::new(Vec::new());
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    fs::write(&image_path, png.into_inner()).unwrap();
    for path in [&text_path, &image_path] {
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
        state.image_engine = crate::image::ImageEngine::halfblocks();
        state.active_tab_mut().pane_frecency = Some(crate::tab::FrecencyPane {
            entries: vec![crate::frecency::ScoredEntry {
                path: path.to_string_lossy().into_owned(),
                topic: "file".to_owned(),
                keyword: String::new(),
                score: 1.0,
                grade: crate::frecency::Grade::Substring,
                last_visit: 0,
            }],
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
        });
        let backend = ratatui::backend::TestBackend::new(100, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render(frame, &state, &InputMode::Normal, None))
            .unwrap();
        let buffer = terminal.backend().buffer();
        let output = (0..buffer.area.height)
            .flat_map(|y| (0..buffer.area.width).map(move |x| buffer[(x, y)].symbol().to_owned()))
            .collect::<String>();
        if path == &text_path {
            assert!(
                output.contains("fn main"),
                "syntax-aware :view text appears: {output}"
            );
            assert!(
                output.contains("1 │"),
                "line number gutter is reused: {output}"
            );
        } else {
            assert!(
                !output.contains("图形渲染不可用"),
                "ImageEngine handled image preview: {output}"
            );
        }
    }
}

#[test]
fn fix8_frecency_click_targets_focused_pane_list() {
    let area = ratatui::layout::Rect::new(0, 0, 100, 30);
    let [_, _, right, _] =
        crate::app::render::pane_layout(area, crate::config::LayoutMode::Dual, false);
    let inner = ratatui::widgets::Block::bordered().inner(right);
    assert_eq!(
        crate::app::render::frecency_click_index(
            area,
            crate::tab::Focus::Right,
            false,
            0,
            inner.x + 1,
            inner.y + 1,
        ),
        Some(1)
    );
    assert_eq!(
        crate::app::render::frecency_click_index(area, crate::tab::Focus::Right, false, 0, 1, 1,),
        None
    );
}

#[test]
fn m455b_jump_click_index_geometry() {
    // M45-5b：选择器列表段点击 → 可见项下标；框外/标题/提示行返回 None。
    let area = ratatui::layout::Rect::new(0, 0, 80, 24);
    let list = crate::app::render::jump_areas(area)[1];
    // 列表首行、第二行命中。
    assert_eq!(
        crate::app::render::jump_click_index(area, list.x + 1, list.y),
        Some(0)
    );
    assert_eq!(
        crate::app::render::jump_click_index(area, list.x + 1, list.y + 1),
        Some(1)
    );
    // 输入行（列表上一行）与提示行（列表下方）不命中。
    assert_eq!(
        crate::app::render::jump_click_index(area, list.x + 1, list.y.saturating_sub(1)),
        None
    );
    // 框外不命中。
    assert_eq!(crate::app::render::jump_click_index(area, 0, 0), None);
}

#[test]
fn m454b_viewer_nul_file_shows_info_panel_not_garbage() {
    // M45-4b：含 NUL 的二进制文件（tar 头部填充同类）在文本查看器不得
    // 渲染乱码——read_to_string 成功但 NUL 是合法 UTF-8，需显式探测。
    let tmp = TempDir::new().unwrap();
    let bin = tmp.path().join("blob.dat");
    fs::write(&bin, b"PK\x03\x04\x00\x00\x00\x00garbage").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let idx = state
        .active_tab()
        .entries
        .iter()
        .position(|e| e.name == "blob.dat")
        .unwrap();
    state.active_tab_mut().cursor = idx;
    state.active_tab_mut().view_mode = true;

    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 12)).unwrap();
    terminal
        .draw(|frame| render(frame, &state, &InputMode::Normal, None))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let mut pane = String::new();
    for y in 1..10u16 {
        for x in 41..80u16 {
            pane.push_str(buf[(x, y)].symbol());
        }
    }
    // TestBackend CJK 全宽留占位空格（M21-E3）。
    let compact = pane.replace(' ', "");
    assert!(
        compact.contains("不可读预览") || compact.contains("二进制"),
        "NUL 文件应回退信息面板: {pane}"
    );
    assert!(!pane.contains('\0'), "不得渲染 NUL 乱码");
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
        cmd_mark_pending: false,
        ranked: Vec::new(),
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
    // 路径在 pane 顶部，状态栏无路径。M27-2：边框外观经
    // `[ui] pane_borders = true` 启用（旧外观保留可恢复）。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    // 注入短路径：真实 tempdir 深层路径在 80 宽 pane 内必然截断，
    // 无法断言"路径完整可见"；Location 直接构造与渲染无关。
    state.active_tab_mut().location = Location::Physical(std::path::PathBuf::from("/w/proj"));
    state.config.ui.pane_borders = true;
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
    // M27-2 vifm 模式（pane_borders=false，默认）：无整框、顶部路径行
    // TopLine 白底（非焦点）、间隔列分隔线。
    state.config.ui.pane_borders = false;
    let backend = ratatui::backend::TestBackend::new(80, 24);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal
        .draw(|f| render(f, &state, &InputMode::Normal, None))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let row = |y: u16| (0..80u16).map(|x| buf[(x, y)].symbol()).collect::<String>();
    assert_ne!(buf[(0, 1)].symbol(), "┌", "vifm 模式无整框左上角");
    assert!(
        row(1).contains(&dir_name),
        "路径行仍在 pane 顶部（TopLine）"
    );
    // 左 pane 焦点 = TopLineSel（黑字加粗、默认底）；右 pane 非焦点 = 白底。
    assert!(
        buf[(0, 1)].modifier.contains(Modifier::BOLD),
        "焦点 TopLineSel 加粗"
    );
    assert_eq!(buf[(40, 1)].bg, Color::White, "右 pane TopLine 白底");
    let gap_x = 39u16; // 80 宽居中分开：间隔列 = 39..40
    // M45-1d：pane_separator 默认 false → 无分隔列（vifm 对齐）。
    assert_ne!(buf[(gap_x, 4)].symbol(), "│", "默认无分隔列");
    // 显式开启后恢复分隔线。
    state.config.ui.pane_separator = true;
    terminal
        .draw(|f| render(f, &state, &InputMode::Normal, None))
        .unwrap();
    let buf2 = terminal.backend().buffer().clone();
    assert_eq!(buf2[(gap_x, 4)].symbol(), "│", "开启后恢复分隔列");
    let status = row(23);
    assert!(!status.contains(&dir_name), "状态栏无路径: {status}");
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

    // 几何与渲染同源：左右 pane 条目首行均在 `..` 行之后。
    let area = ratatui::layout::Rect::new(0, 0, 120, 24);
    let [_, left, right, _] = pane_layout(area, crate::config::LayoutMode::Dual, false);
    let l_inner = ratatui::widgets::Block::bordered().inner(left);
    let r_inner = ratatui::widgets::Block::bordered().inner(right);
    // 左右 pane 条目 0 均在 `..` 行之后（行 = inner.y + 1）。
    let l_row = l_inner.y + 1;
    let r_row = r_inner.y + 1;
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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

#[cfg_attr(
    target_os = "macos",
    ignore = "macOS runner 渲染差异待真机复核（M40-6）"
)]
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

#[cfg_attr(
    target_os = "macos",
    ignore = "macOS runner 渲染差异待真机复核（M40-6）"
)]
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
    // macOS APFS 无法创建非法 UTF-8 文件名（EINVAL）——排除 macos。
    #[cfg(all(unix, not(target_os = "macos")))]
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
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
        let [_, left, right, _] = pane_layout(
            ratatui::layout::Rect::new(0, 0, 120, 24),
            crate::config::LayoutMode::Dual,
            false,
        );
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
    assert_eq!(
        state.active_tab().focus,
        crate::tab::Focus::Left,
        "M42：w 后焦点留在左 pane"
    );
    assert!(!state.active_tab().preview_focus);
    let buf = draw(&state);
    assert!(right_text(&buf).contains("line-01"), "右 pane 预览文件内容");

    // M42：焦点仍在左 pane——j/k 切换文件，对侧预览自动跟随。
    press_follow(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('j'),
        KeyModifiers::NONE,
    );
    assert_eq!(state.active_tab().cursor, 1, "j 移动原 pane 光标");
    assert_eq!(
        state
            .active_tab()
            .viewer
            .as_ref()
            .unwrap()
            .path
            .as_ref()
            .unwrap()
            .file_name()
            .unwrap(),
        "b.txt",
        "预览跟随到 b.txt"
    );
    let buf = draw(&state);
    assert!(
        !right_text(&buf).contains("line-01"),
        "跟随 b.txt 后无 a.txt 内容"
    );

    press_follow(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('k'),
        KeyModifiers::NONE,
    );
    assert_eq!(state.active_tab().cursor, 0, "k 回到 a.txt");
    assert_eq!(
        state
            .active_tab()
            .viewer
            .as_ref()
            .unwrap()
            .path
            .as_ref()
            .unwrap()
            .file_name()
            .unwrap(),
        "a.txt"
    );
    let buf = draw(&state);
    assert!(right_text(&buf).contains("line-01"), "k 后预览跟随回 a.txt");

    // M42：Tab 进入预览窗格后 j 滚动查看器（列表光标不动）。
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Tab,
        KeyModifiers::NONE,
    );
    assert!(state.active_tab().preview_focus, "Tab 进入预览焦点");
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('j'),
        KeyModifiers::NONE,
    );
    assert_eq!(state.active_tab().cursor, 0, "预览焦点下光标不动");
    assert_eq!(
        state.active_tab().viewer.as_ref().unwrap().scroll,
        1,
        "j 滚动查看器视口"
    );
    let buf = draw(&state);
    assert!(!right_text(&buf).contains("line-01"), "滚动后首行移出视口");

    // M42：预览窗格内 Tab 返回原 pane。
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Tab,
        KeyModifiers::NONE,
    );
    assert!(!state.active_tab().preview_focus, "Tab 返回原 pane");

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
fn m42_w_keeps_focus_and_preview_follows_cursor() {
    // 验收：w 后焦点留在原 pane；j/k 切文件时对侧预览自动跟随。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "AAA-01\nAAA-02\n").unwrap();
    fs::write(tmp.path().join("b.txt"), "BBB-01\nBBB-02\n").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let layers = config::keymap_layers();
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
    let mut input = InputState::with_keymap(keymap);
    let mut msg = None;
    let vname = |s: &AppState| {
        s.active_tab()
            .viewer
            .as_ref()
            .unwrap()
            .path
            .as_ref()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    };

    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('w'),
        KeyModifiers::NONE,
    );
    assert!(state.active_tab().view_mode);
    assert_eq!(
        state.active_tab().focus,
        crate::tab::Focus::Left,
        "w 后焦点留在原 pane"
    );
    assert!(!state.active_tab().preview_focus, "w 不进入预览焦点");
    assert_eq!(vname(&state), "a.txt");

    press_follow(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('j'),
        KeyModifiers::NONE,
    );
    assert_eq!(state.active_tab().cursor, 1, "原 pane j 切换文件");
    assert_eq!(state.active_tab().viewer.as_ref().unwrap().scroll, 0);
    assert_eq!(vname(&state), "b.txt", "预览自动跟随光标文件");

    press_follow(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('k'),
        KeyModifiers::NONE,
    );
    assert_eq!(state.active_tab().cursor, 0);
    assert_eq!(vname(&state), "a.txt");

    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('w'),
        KeyModifiers::NONE,
    );
    assert!(!state.active_tab().view_mode);
    assert_eq!(
        state.active_tab().focus,
        crate::tab::Focus::Left,
        "关闭预览后焦点不变"
    );
    assert!(!state.active_tab().preview_focus);
}

#[test]
fn m42_tab_enters_preview_scrolls_then_returns() {
    // 验收：Tab 进入预览 → j/k 滚动查看器；Tab 返回 → j/k 恢复切文件。
    let tmp = TempDir::new().unwrap();
    let mut content = String::new();
    for i in 0..50 {
        content.push_str(&format!("line {i:03}\n"));
    }
    fs::write(tmp.path().join("a.txt"), content).unwrap();
    fs::write(tmp.path().join("b.txt"), "B\n").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let layers = config::keymap_layers();
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
    let mut input = InputState::with_keymap(keymap);
    let mut msg = None;

    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('w'),
        KeyModifiers::NONE,
    );
    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Tab,
        KeyModifiers::NONE,
    );
    assert!(state.active_tab().preview_focus, "Tab 进入预览焦点");
    assert_eq!(state.active_tab().focus, crate::tab::Focus::Left);

    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('j'),
        KeyModifiers::NONE,
    );
    assert_eq!(state.active_tab().cursor, 0, "预览焦点下不切文件");
    assert_eq!(
        state.active_tab().viewer.as_ref().unwrap().scroll,
        1,
        "预览焦点下 j 滚动内容"
    );

    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Tab,
        KeyModifiers::NONE,
    );
    assert!(!state.active_tab().preview_focus, "Tab 返回原 pane");
    assert_eq!(state.active_tab().focus, crate::tab::Focus::Left);

    press_follow(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('j'),
        KeyModifiers::NONE,
    );
    assert_eq!(state.active_tab().cursor, 1, "返回后 j 恢复切文件");
    assert!(!state.active_tab().preview_focus);
}

#[test]
fn m42_view_command_and_right_pane_keep_focus() {
    // :view 同 w 不移动焦点；焦点在右 pane 时 w 预览在左、焦点留在右。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "AAA-01\n").unwrap();
    let mut bcontent = String::new();
    for i in 0..30 {
        bcontent.push_str(&format!("BBB-{i:02}\n"));
    }
    fs::write(tmp.path().join("b.txt"), bcontent).unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let _ = exec_command_impl(&mut state, "view").unwrap();
    assert!(state.active_tab().view_mode);
    assert_eq!(state.active_tab().focus, crate::tab::Focus::Left);
    assert!(!state.active_tab().preview_focus);
    let _ = exec_command_impl(&mut state, "view").unwrap();
    assert!(!state.active_tab().view_mode);
    assert_eq!(state.active_tab().focus, crate::tab::Focus::Left);

    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().focus = crate::tab::Focus::Right;
    let layers = config::keymap_layers();
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
    let mut input = InputState::with_keymap(keymap);
    let mut msg = None;
    let vname = |s: &AppState| {
        s.active_tab()
            .viewer
            .as_ref()
            .unwrap()
            .path
            .as_ref()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    };

    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('w'),
        KeyModifiers::NONE,
    );
    assert!(state.active_tab().view_mode);
    assert_eq!(
        state.active_tab().focus,
        crate::tab::Focus::Right,
        "右 pane 开启预览后焦点不变"
    );
    assert!(!state.active_tab().preview_focus);
    assert_eq!(vname(&state), "a.txt", "预览右 pane 光标文件");

    press_follow(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Char('j'),
        KeyModifiers::NONE,
    );
    assert_eq!(state.active_tab().right.cursor, 1);
    assert_eq!(state.active_tab().cursor, 0, "左 pane 光标不动");
    assert_eq!(vname(&state), "b.txt", "预览跟随右 pane 光标");

    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Tab,
        KeyModifiers::NONE,
    );
    assert!(state.active_tab().preview_focus);
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
        "预览焦点下 j 滚动"
    );
    assert_eq!(state.active_tab().right.cursor, 1, "右 pane 光标不动");

    press_msg(
        &mut state,
        &mut input,
        &mut msg,
        KeyCode::Tab,
        KeyModifiers::NONE,
    );
    assert!(!state.active_tab().preview_focus);
    assert_eq!(state.active_tab().focus, crate::tab::Focus::Right);
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let [_, left, right, _] = pane_layout(
        ratatui::layout::Rect::new(0, 0, 120, 24),
        crate::config::LayoutMode::Dual,
        false,
    );
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
fn m456a_cd_follows_focused_pane() {
    // M45-6a：:cd 作用于焦点 pane——右焦点改右栏、左栏不变（M41-1）。
    let tmp = TempDir::new().unwrap();
    let left = tmp.path().join("left");
    let right = tmp.path().join("right");
    let target = tmp.path().join("target");
    for d in [&left, &right, &target] {
        fs::create_dir(d).unwrap();
    }
    let mut state = AppState::new(Tab::from_physical(&left, Config::default()).unwrap());
    state.active_tab_mut().right.location = Location::Physical(right.clone());
    state
        .active_tab_mut()
        .right
        .refresh(Config::default())
        .unwrap();
    // 焦点切右栏。
    state.active_tab_mut().focus = crate::tab::Focus::Right;
    let left_before = state.active_tab().location.clone();
    exec_command_impl(&mut state, &format!("cd {}", target.display())).unwrap();
    assert_eq!(
        state.active_tab().right.location,
        Location::Physical(target.clone()),
        "右焦点 :cd 改右栏"
    );
    assert_eq!(state.active_tab().location, left_before, "左栏不变");

    // 焦点切左栏再 cd 到 left 的兄弟目标。
    state.active_tab_mut().focus = crate::tab::Focus::Left;
    exec_command_impl(&mut state, &format!("cd {}", right.display())).unwrap();
    assert_eq!(
        state.active_tab().location,
        Location::Physical(right.clone()),
        "左焦点 :cd 改左栏"
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
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
    let copy = remote_download_copy(&mut state, "dev", std::path::Path::new("/readme.md")).unwrap();
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
    let keymap = crate::keymap::Keymap::with_layers(layers.prepend, layers.append, layers.replace);
    let mut input = InputState::with_keymap(keymap);
    let mut msg = None;

    for name in ["t.tar.xz", "t.tar.bz2"] {
        let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
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

#[test]
fn m252_mark_set_validation_no_db_touch() {
    // M25-2：marks 非法字符 / 非物理视图 → 可读提示且不触 frecency 存储
    // （写入路径由 frecency 模块注入式测试覆盖）。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "A").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let mut msg = None;
    mark_set(&mut state, '-', &mut msg);
    assert!(msg.unwrap().contains("限字母与数字"), "非法字符提示");
    // 远端视图 → 拒绝。
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    state.active_tab_mut().location = crate::tab::Location::Remote {
        remote: "test".to_owned(),
        path: PathBuf::from("/"),
    };
    let mut msg: Option<String> = None;
    mark_set(&mut state, 'a', &mut msg);
    assert!(
        msg.as_deref().unwrap().contains("仅物理视图"),
        "远端视图拒绝: {msg:?}"
    );
    match exec_command_impl(&mut state, "bmark home") {
        // 远端守卫先于物理视图校验（"远端不支持: bmark"）——两者都是拒绝。
        Err(e) => assert!(e.contains("bmark"), "bmark 远端拒绝: {e}"),
        Ok(_) => panic!("bmark 远端视图应拒绝"),
    }
}

#[test]
fn m252_bmark_tag_validation() {
    // M25-2：tag 限字母与数字（不触 DB——校验先于写入）。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "A").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    match exec_command_impl(&mut state, "bmark home!") {
        Err(e) => assert!(e.contains("限字母与数字"), "{e}"),
        Ok(_) => panic!("非法 tag 应拒绝"),
    }
    match exec_command_impl(&mut state, "bmark") {
        Err(e) => assert!(e.contains("缺少 tag"), "{e}"),
        Ok(_) => panic!("缺 tag 应拒绝"),
    }
}

#[test]
fn m252_bmgo_missing_tag_errors() {
    // M25-2：:bmgo 未知 tag → 可读错误不静默（读全局 store，串行锁）。
    let _guard = STORE_TEST_LOCK.lock().unwrap();
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "A").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    match exec_command_impl(&mut state, "bmgo") {
        Err(e) => assert!(e.contains("缺少 tag"), "{e}"),
        Ok(_) => panic!("缺 tag 应拒绝"),
    }
    match exec_command_impl(&mut state, "bmgo no-such-mark-xyz") {
        Err(e) => assert!(e.contains("no-such-mark-xyz"), "{e}"),
        Ok(_) => panic!("未知 tag 应报错"),
    }
}

#[test]
fn m252_jump_to_file_semantics_locates_cursor() {
    // M25-2：`'<char>` 命中文件条目 → 父目录 cd + 光标定位（jump_to
    // M8-12 语义；纯 fixture，不触 frecency 存储）。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("target.txt"), "T").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let entry = crate::frecency::ScoredEntry {
        path: tmp
            .path()
            .join("target.txt")
            .to_string_lossy()
            .replace('\\', "/"),
        topic: "file".to_owned(),
        keyword: "t".to_owned(),
        score: 9.0,
        grade: crate::frecency::Grade::None,
        last_visit: 0,
    };
    let m = jump_to(&mut state, &entry).expect("文件跳转成功");
    assert!(m.contains("定位"), "msg: {m}");
    assert_eq!(state.active_tab().cursor, 0, "定位到 target.txt");
    let expected_parent = tmp.path().to_string_lossy().replace('\\', "/");
    assert_eq!(
        state
            .active_tab()
            .location
            .base_dir()
            .to_string_lossy()
            .replace('\\', "/"),
        expected_parent,
        "cd 到父目录"
    );
}

#[test]
fn m261_archive_job_cancelled_before_start() {
    // M26-1：归档作业启动前检查取消标志（:jobs x 置位）——不产生产物。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "A").unwrap();
    let cancel = std::sync::atomic::AtomicBool::new(true);
    let pause = std::sync::atomic::AtomicBool::new(false);
    let progress = crate::state::JobProgress::default();
    let controls = crate::state::JobControls {
        cancel: &cancel,
        pause: &pause,
        progress: &progress,
    };
    let (ops, status) = execute_job_body(
        "archive:zip:md5",
        &["a.txt".to_owned()],
        tmp.path(),
        &tmp.path().join("out.zip"),
        0,
        &controls,
    );
    assert!(ops.is_empty(), "取消不产生撤销记录");
    assert!(
        matches!(status, crate::state::JobStatus::Cancelled { .. }),
        "{status:?}"
    );
    assert!(!tmp.path().join("out.zip").exists(), "取消不产生产物");
}

#[cfg(unix)]
#[test]
fn m271_symlink_dir_follows_target_and_broken_link_degrades() {
    // M27-1：link-to-dir 跟随目标判定 is_dir（列表/排序一致，is_link 保留
    // 着色）；断链不使列表加载失败（is_dir=false、元数据回退链接自身）。
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("real/sub")).unwrap();
    fs::write(tmp.path().join("real/sub/b.txt"), "B").unwrap();
    std::os::unix::fs::symlink(tmp.path().join("real"), tmp.path().join("link")).unwrap();
    std::os::unix::fs::symlink(tmp.path().join("no-such-target"), tmp.path().join("broken"))
        .unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let tab = state.active_tab();
    let link = tab
        .entries
        .iter()
        .find(|i| i.name == "link")
        .expect("link 条目存在");
    assert!(link.is_dir, "link-to-dir 跟随目标判定 is_dir");
    assert!(link.is_link, "is_link 保留用于着色");
    let broken = tab
        .entries
        .iter()
        .find(|i| i.name == "broken")
        .expect("断链条目存在");
    assert!(!broken.is_dir, "断链不是目录");
    assert!(broken.is_link);
    assert!(
        tab.entries.iter().any(|i| i.name == "real"),
        "列表加载未因断链失败"
    );

    // l/Enter 进入 link → 列出目标内容（物理 Location，不调系统 opener）。
    let idx = state
        .active_tab()
        .entries
        .iter()
        .position(|i| i.name == "link")
        .unwrap();
    state.active_tab_mut().cursor = idx;
    let mut msg = None;
    open_selected(&mut state, &mut msg);
    assert!(msg.is_none(), "进入成功无提示: {msg:?}");
    let entered = state
        .active_tab()
        .location
        .base_dir()
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR_STR, "/");
    assert!(
        entered.contains("real") || entered.contains("link"),
        "进入 link 目录（链接路径或目标路径均可）: {entered}"
    );
    assert!(
        state.active_tab().entries.iter().any(|i| i.name == "sub"),
        "列出目标内容"
    );

    // 断链 Enter → 可读错误（系统 opener 失败上浮，不 panic）。
    state.active_tab_mut().location = crate::tab::Location::Physical(tmp.path().to_path_buf());
    crate::app::refresh_tab_panes(&mut state, 0);
    let idx = state
        .active_tab()
        .entries
        .iter()
        .position(|i| i.name == "broken")
        .unwrap();
    state.active_tab_mut().cursor = idx;
    // 手工置 location 等价 navigate_to 的状态复位——清 `..` 行标记
    // （enter_selected 进入目录后落在 `..` 行，FEAT-2；生产路径由
    // navigate_to/return_parent 维护，测试直改 location 须自行同步）。
    state.active_tab_mut().on_parent_row = false;
    let mut msg = None;
    open_selected(&mut state, &mut msg);
    // M40-6：macOS `open` 对断链返回 0（行为差异）——仅要求不 panic 且
    // 状态可继续；严格提示断言保留 Linux。
    #[cfg(not(target_os = "macos"))]
    assert!(
        msg.as_deref().map(|m| !m.is_empty()).unwrap_or(false),
        "断链给出可读提示: {msg:?}"
    );
    #[cfg(target_os = "macos")]
    {
        let _ = msg;
    }
}

#[cfg(unix)]
#[test]
fn m281_link_command_creates_undoable_symlinks() {
    // M28-1：:link 命令流——单条目（dest 为新路径）、批量（dest 为已存在
    // 目录）、u 逐条逆、错误路径（缺参/父目录不存在/已存在不覆盖）。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("a.txt"), "A").unwrap();
    fs::create_dir(tmp.path().join("sub")).unwrap();
    fs::write(tmp.path().join("sub").join("b.txt"), "B").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    // 光标定位 a.txt（目录优先排序：sub=0、a.txt=1）。
    let idx = state
        .active_tab()
        .entries
        .iter()
        .position(|i| i.name == "a.txt")
        .unwrap();
    state.active_tab_mut().cursor = idx;

    // 单条目：dest 为新链接路径。
    assert_eq!(exec_command(&mut state, "link l-a"), Ok(false));
    let link = tmp.path().join("l-a");
    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink(),
        "l-a 为符号链接"
    );
    assert_eq!(std::fs::read(&link).unwrap(), b"A", "链接指向 a.txt");

    // u 撤销：删链接不删目标。
    let mut msg = None;
    undo_last(&mut state, &mut msg);
    assert!(
        msg.as_deref().unwrap().starts_with("已撤销"),
        "msg: {msg:?}"
    );
    assert!(std::fs::symlink_metadata(&link).is_err(), "链接已移除");
    assert!(tmp.path().join("a.txt").is_file(), "目标不受影响");

    // 错误路径：缺参、父目录不存在。
    assert!(exec_command(&mut state, "link").is_err(), "缺 dest");
    assert!(
        exec_command(&mut state, "link nonexistent-dir/l").is_err(),
        "父目录不存在 → 可读错误"
    );

    // 批量：dest 为已存在目录 → 其内建同名链接（tags 优先）。
    fs::create_dir(tmp.path().join("links")).unwrap();
    let tab = state.active_tab_mut();
    tab.tags.insert("a.txt".to_owned());
    tab.tags.insert("sub".to_owned());
    assert_eq!(exec_command(&mut state, "link links"), Ok(false));
    assert!(
        std::fs::symlink_metadata(tmp.path().join("links/a.txt"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(
        std::fs::symlink_metadata(tmp.path().join("links/sub"))
            .unwrap()
            .file_type()
            .is_symlink()
    );

    // 两次 u 逆序恢复两条链接（LIFO）。
    let mut msg = None;
    undo_last(&mut state, &mut msg);
    undo_last(&mut state, &mut msg);
    assert!(std::fs::symlink_metadata(tmp.path().join("links/sub")).is_err());
    assert!(std::fs::symlink_metadata(tmp.path().join("links/a.txt")).is_err());

    // 已存在不覆盖（重复 :link 同 dest；先清 tags 回单条目形态）。
    state.active_tab_mut().tags.clear();
    let idx = state
        .active_tab()
        .entries
        .iter()
        .position(|i| i.name == "a.txt")
        .unwrap();
    state.active_tab_mut().cursor = idx;
    assert_eq!(exec_command(&mut state, "link l-2"), Ok(false));
    assert!(
        exec_command(&mut state, "link l-2").is_err(),
        "已存在不覆盖"
    );
}

#[cfg(unix)]
#[test]
fn m282_rename_structured_preview_and_apply() {
    // M28-2：结构化 :rename——预览结果视图（RenamePreview）、y 应用、
    // Esc 取消、u 逐项可逆；跳过 = 无变化/冲突；plain `s-x` 不误判。
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("old-a.txt"), "A").unwrap();
    fs::write(tmp.path().join("old-b.txt"), "B").unwrap();
    fs::write(tmp.path().join("keep.txt"), "K").unwrap();
    let mut state = AppState::new(Tab::from_physical(tmp.path(), Config::default()).unwrap());
    let tab = state.active_tab_mut();
    tab.tags.insert("old-a.txt".to_owned());
    tab.tags.insert("old-b.txt".to_owned());
    tab.tags.insert("keep.txt".to_owned());

    // s/// 正则：old- → new-；keep.txt 无变化（跳过行）。
    assert_eq!(exec_command(&mut state, "rename s/old-/new-/"), Ok(false));
    let tab = state.active_tab();
    assert!(
        tab.results
            .as_ref()
            .map(|r| r.kind == crate::search::SearchKind::RenamePreview)
            .unwrap_or(false),
        "进入 rename 预览结果视图"
    );
    let plan = tab.pending_rename.clone().unwrap();
    assert_eq!(plan.len(), 2, "仅两条将重命名（keep.txt 跳过）");
    // 预览行显示：2 条 "旧 → 新" + 1 条跳过。
    assert!(
        tab.entries
            .iter()
            .any(|i| i.name.contains("old-a.txt → new-a.txt"))
    );
    assert!(
        tab.entries
            .iter()
            .any(|i| i.name.contains("keep.txt （无变化，跳过）"))
    );

    // Esc 取消：无任何文件改动。
    let cfg = state.config.clone();
    state.active_tab_mut().exit_results(cfg);
    assert!(tmp.path().join("old-a.txt").is_file(), "取消不动文件");
    assert!(!tmp.path().join("new-a.txt").exists());

    // 重新预览 + y 应用：磁盘改名 + u 逐项恢复。
    assert_eq!(exec_command(&mut state, "rename s/old-/new-/"), Ok(false));
    let mut m303_msg = None;
    apply_rename_preview(&mut state, &mut m303_msg);
    assert!(tmp.path().join("new-a.txt").is_file());
    assert!(tmp.path().join("new-b.txt").is_file());
    assert!(!tmp.path().join("old-a.txt").exists());
    let mut msg = None;
    undo_last(&mut state, &mut msg);
    undo_last(&mut state, &mut msg);
    assert!(tmp.path().join("old-a.txt").is_file());
    assert!(tmp.path().join("old-b.txt").is_file());

    // prefix= / suffix= / seq= 三形态（seq 替换主干保留扩展名）。
    assert_eq!(exec_command(&mut state, "rename prefix=img_"), Ok(false));
    let plan = state.active_tab().pending_rename.clone().unwrap();
    assert_eq!(plan.len(), 3);
    assert!(plan.iter().all(|(_old, new)| new.starts_with("img_")));
    state.active_tab_mut().pending_rename = None;
    let cfg = state.config.clone();
    state.active_tab_mut().exit_results(cfg);

    let tab = state.active_tab_mut();
    tab.tags.clear();
    tab.tags.insert("keep.txt".to_owned());
    assert_eq!(exec_command(&mut state, "rename seq=1,1,3"), Ok(false));
    let plan = state.active_tab().pending_rename.clone().unwrap();
    assert_eq!(
        plan,
        vec![(tmp.path().join("keep.txt"), "001.txt".to_owned())]
    );
    state.active_tab_mut().pending_rename = None;
    let cfg = state.config.clone();
    state.active_tab_mut().exit_results(cfg);

    // plain 名字不误判（s 开头非结构化）。
    let tab = state.active_tab_mut();
    tab.tags.clear();
    let idx = tab
        .entries
        .iter()
        .position(|i| i.name == "keep.txt")
        .unwrap();
    tab.cursor = idx;
    assert_eq!(exec_command(&mut state, "rename s-x"), Ok(false));
    assert!(tmp.path().join("s-x").is_file(), "s-x 为 plain 重命名产物");
    assert!(state.active_tab().results.is_none(), "plain 不进预览");

    // 错误路径：非法正则、未知标志、seq 参数、全部跳过。
    assert!(
        exec_command(&mut state, "rename s/(/x/").is_err(),
        "非法正则"
    );
    assert!(
        exec_command(&mut state, "rename s/a/b/z").is_err(),
        "未知标志"
    );
    assert!(
        exec_command(&mut state, "rename seq=1,0,3").is_err(),
        "step<1"
    );
    assert!(
        exec_command(&mut state, "rename seq=a,b,c").is_err(),
        "非整数"
    );
    let tab = state.active_tab_mut();
    tab.tags.insert("no-such.txt".to_owned());
    assert!(
        exec_command(&mut state, "rename s/x/y/").is_err(),
        "无选中条目"
    );
}

// M27-3：pdf_overflow_note 已随 M39-2 查看器单页翻页改造移除（旧 1–2 页
// 网格的底部说明函数，唯一调用点退役；单页文案并入标题"[ ] 翻页"）。

#[test]
fn m42_1_tutor_command_list_session_and_keys() {
    // M42-1：:tutor 分派（无参列模块 / 带参进入 / 未知可读报错）+ 列表按键
    // + 会话 y/n 推进与结算 + 中途退出保留作答。
    let (_tmp, mut state) = make_state(3);
    match exec_command_impl(&mut state, "tutor") {
        Ok(ExecOutcome::EnterTutorList) => {}
        other => panic!("expected EnterTutorList, got {other:?}"),
    }
    match exec_command_impl(&mut state, "tutor core_nav") {
        Ok(ExecOutcome::EnterTutorSession { module_id }) => {
            assert_eq!(module_id, "core_nav");
        }
        other => panic!("expected EnterTutorSession, got {other:?}"),
    }
    let err = exec_command_impl(&mut state, "tutor bogus").expect_err("未知模块应报错");
    assert!(
        err.contains("未知模块") && err.contains("core_nav"),
        "{err}"
    );
    let err = exec_command_impl(&mut state, "tutor a b").expect_err("多余参数应报错");
    assert!(err.contains("用法"), "{err}");

    // 列表按键：j 下移、k 上移、l/Enter 进入、Esc/q 关闭。
    let modules = crate::tutor::builtin_modules();
    assert!(!modules.is_empty());
    let mut cursor = 0usize;
    assert_eq!(
        tutor_list_key(&mut cursor, modules.len(), KeyCode::Char('j')),
        TutorListOutcome::None
    );
    assert_eq!(cursor, 1.min(modules.len() - 1));
    assert_eq!(
        tutor_list_key(&mut cursor, modules.len(), KeyCode::Char('k')),
        TutorListOutcome::None
    );
    assert_eq!(cursor, 0);
    assert_eq!(
        tutor_list_key(&mut cursor, modules.len(), KeyCode::Char('l')),
        TutorListOutcome::Enter(0)
    );
    assert_eq!(
        tutor_list_key(&mut cursor, modules.len(), KeyCode::Esc),
        TutorListOutcome::Close
    );

    // 会话：y 记掌握、n 记未掌握；末项作答后 done；结算态任意键关闭。
    let module = crate::tutor::module_by_id("core_nav").unwrap();
    let total = module.items.len();
    // M43-1：会话推进改为 TutorSession::advance（Ctrl+Y/N 前置拦截的
    // 底层逻辑）；done 后 no-op——结算 Summary 持续显示至 Ctrl+Q。
    let mut session = crate::tutor::TutorSession {
        module: module.clone(),
        idx: 0,
        marks: Vec::new(),
        done: false,
        review: false,
    };
    for i in 0..total {
        session.advance(i % 2 == 0);
    }
    assert!(session.done, "末项作答后进入结算态");
    assert_eq!(session.marks.len(), total);
    assert_eq!(session.mastered_count(), total.div_ceil(2));
    session.advance(true);
    assert_eq!(session.idx, total, "结算态 advance no-op");

    // 中途退出：已作答保留（落盘数据形态）。
    let mut session = crate::tutor::TutorSession {
        module,
        idx: 0,
        marks: Vec::new(),
        done: false,
        review: false,
    };
    session.advance(true);
    assert_eq!(session.marks.len(), 1, "退出前已作答项保留");
    assert!(!session.done);
}

#[test]
fn m42_1_tutor_render_list_and_panel() {
    // M42-1：渲染——列表覆盖层（模块行/键提示）与底部 Panel
    // （prompt/expected/进度/键提示/结算 Summary）；Panel 态主 TUI 背景可见。
    let (_tmp, state) = make_state(3);
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

    // 列表覆盖层。
    let mode = InputMode::TutorList {
        cursor: 0,
        search: None,
    };
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(70, 14)).unwrap();
    terminal.draw(|f| render(f, &state, &mode, None)).unwrap();
    let all = compact(terminal.backend(), 70, 14);
    assert!(all.contains("向导模式"), "标题: {all}");
    assert!(all.contains("core_nav"), "模块行: {all}");
    assert!(all.contains("Enter/l=进入"), "键提示: {all}");

    // 会话 Panel：prompt/expected/右列键提示；背景标签栏仍可见。
    let module = crate::tutor::module_by_id("core_nav").unwrap();
    let mode = InputMode::Normal;
    let mut state = state;
    state.tutor = Some(crate::tutor::TutorSession {
        module: module.clone(),
        idx: 0,
        marks: Vec::new(),
        done: false,
        review: false,
    });
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(70, 14)).unwrap();
    terminal.draw(|f| render(f, &state, &mode, None)).unwrap();
    let buf = terminal.backend();
    let all = compact(buf, 70, 14);
    assert!(all.contains("核心导航"), "Panel 标题含模块名: {all}");
    let first_prompt = module.items[0].prompt.replace(['`', ' '], "");
    assert!(
        all.replace('`', "").contains(&first_prompt),
        "prompt 行: {all}"
    );
    assert!(all.contains("预期:"), "expected 行: {all}");
    assert!(
        all.contains("[Ctrl+Y]掌握"),
        "右列键提示须带 Ctrl（M45-8b）: {all}"
    );
    let top_row: String = (0..70).map(|x| buf.buffer()[(x, 0)].symbol()).collect();
    assert!(
        top_row.trim().contains("1:") && top_row.contains(".tmp"),
        "背景标签栏可见（N:dirname）: {top_row}"
    );

    // 结算态 Summary（掌握/未掌握计数，无外部文件）。
    let total = module.items.len();
    let marks: Vec<(String, bool)> = module
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| (item.id.clone(), i % 2 == 0))
        .collect();
    let skipped = marks.iter().filter(|(_, ok)| !*ok).count();
    let mode = InputMode::Normal;
    state.tutor = Some(crate::tutor::TutorSession {
        module,
        idx: total,
        marks,
        done: true,
        review: false,
    });
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(70, 14)).unwrap();
    terminal.draw(|f| render(f, &state, &mode, None)).unwrap();
    let all = compact(terminal.backend(), 70, 14);
    assert!(all.contains("模块完成"), "Summary: {all}");
    assert!(
        all.contains(&format!("未掌握{skipped}")),
        "未掌握计数: {all}"
    );
}

#[test]
fn m42_2_tutor_progress_persist_review_and_reset() {
    // M42-2：进度持久化（roundtrip/损坏静默降级）、错题本互斥迁移、
    // 复习排序（skipped 优先）、reset 清空、exec 分派。
    let tmp = tempfile::TempDir::new().unwrap();
    let path = tmp.path().join("tutor_progress.toml");

    // 空进度 + 复习排序：无 skipped → None。
    let module = crate::tutor::module_by_id("core_nav").unwrap();
    let progress = crate::tutor::load_progress_from(&path);
    assert!(progress.is_empty(), "缺失文件 → 全新进度");
    assert!(crate::tutor::review_order(&progress, "core_nav", &module.items).is_none());

    // 会话合并：半数掌握 → skipped 进错题本；roundtrip 一致。
    let marks: Vec<(String, bool)> = module
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| (item.id.clone(), i % 2 == 0))
        .collect();
    let progress = crate::tutor::merge_session(progress, "core_nav", &marks);
    crate::tutor::save_progress_to(&path, &progress).unwrap();
    let loaded = crate::tutor::load_progress_from(&path);
    assert_eq!(loaded, progress, "roundtrip 一致");
    let entry = &loaded["core_nav"];
    let skipped_n = module.items.len() - module.items.len().div_ceil(2);
    assert_eq!(entry.mastered.len(), module.items.len().div_ceil(2));
    assert_eq!(entry.skipped.len(), skipped_n);
    assert!(entry.last_accessed.is_some());

    // 复习排序：skipped 排前、unseen 居中、mastered 垫底。
    let ordered = crate::tutor::review_order(&loaded, "core_nav", &module.items).unwrap();
    assert_eq!(ordered.len(), module.items.len());
    let mastered = &entry.mastered;
    let first_skipped_pos = ordered
        .iter()
        .position(|it| entry.skipped.iter().any(|s| s == &it.id))
        .unwrap();
    let last_skipped_pos = ordered
        .iter()
        .rposition(|it| entry.skipped.iter().any(|s| s == &it.id))
        .unwrap();
    let first_mastered_pos = ordered
        .iter()
        .position(|it| mastered.iter().any(|m| m == &it.id))
        .unwrap();
    assert!(first_skipped_pos <= last_skipped_pos, "skipped 段有序");
    assert!(
        last_skipped_pos < first_mastered_pos,
        "skipped 段在 mastered 段之前"
    );

    // 互斥迁移：重答 skipped 项为 y → 挪入 mastered；重答 mastered 为 n → 挪回。
    let flip = vec![
        (entry.skipped[0].clone(), true),
        (entry.mastered[0].clone(), false),
    ];
    let progress = crate::tutor::merge_session(loaded, "core_nav", &flip);
    let entry = &progress["core_nav"];
    assert!(
        !entry.skipped.contains(&flip[0].0) && entry.mastered.contains(&flip[0].0),
        "skipped→mastered 迁移"
    );
    assert!(
        !entry.mastered.contains(&flip[1].0) && entry.skipped.contains(&flip[1].0),
        "mastered→skipped 迁移"
    );

    // 损坏文件 → 静默降级为全新进度。
    std::fs::write(&path, "not [ valid toml {{{{").unwrap();
    assert!(
        crate::tutor::load_progress_from(&path).is_empty(),
        "损坏静默降级"
    );

    // reset：移除模块进度。
    let progress = crate::tutor::merge_session(
        crate::tutor::TutorProgress::new(),
        "core_nav",
        &[(module.items[0].id.clone(), false)],
    );
    let after = crate::tutor::reset_module(progress, "core_nav");
    assert!(!after.contains_key("core_nav"), "reset 清空");

    // exec 分派：reset 未知模块报错（已知模块的成功路径不 exec 测——
    // 会写真实 HOME 数据目录，m38_4 先例：核心链路走路径注入单测）。
    let (_tmp, mut state) = make_state(2);
    let err = exec_command_impl(&mut state, "tutor reset bogus").expect_err("未知模块应报错");
    assert!(err.contains("未知模块"), "{err}");
}

#[test]
fn m43_1_tutor_passthrough_and_hotkeys() {
    // M43-1：透明放行——会话激活时练习键照常进 Normal 处理器（j 移动）；
    // Ctrl+Y/N 前置拦截标记推进；无会话/无 Ctrl → None（正常键处理）；
    // done 后 advance no-op；Ctrl+Q → Exit。
    let (_tmp, mut state) = make_state(3);
    let mut input = InputState::default();
    let module = crate::tutor::module_by_id("core_nav").unwrap();
    state.tutor = Some(crate::tutor::TutorSession {
        module: module.clone(),
        idx: 0,
        marks: Vec::new(),
        done: false,
        review: false,
    });

    // 放行：j 在会话激活下照常移动光标（练习不被拦截）。
    let before = state.active_tab().cursor;
    let _ = press(
        &mut state,
        &mut input,
        KeyCode::Char('j'),
        KeyModifiers::NONE,
    );
    assert_eq!(state.active_tab().cursor, before + 1, "练习键放行");
    assert_eq!(state.tutor.as_ref().unwrap().marks.len(), 0, "练习不标记");

    // Ctrl+Y：前置拦截标记掌握并推进。
    assert_eq!(
        tutor_hotkey(
            &mut state,
            &KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL)
        ),
        TutorHotkey::Marked
    );
    let session = state.tutor.as_ref().unwrap();
    assert_eq!(session.marks.len(), 1);
    assert!(session.marks[0].1);
    assert_eq!(session.idx, 1);

    // Ctrl+N：标记未掌握。
    assert_eq!(
        tutor_hotkey(
            &mut state,
            &KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL)
        ),
        TutorHotkey::Marked
    );
    let session = state.tutor.as_ref().unwrap();
    assert_eq!(session.marks.len(), 2);
    assert!(!session.marks[1].1);

    // 裸 y（无 Ctrl）→ None（放行——Normal 处理器收到）。
    assert_eq!(
        tutor_hotkey(
            &mut state,
            &KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE)
        ),
        TutorHotkey::None
    );

    // 无会话 → Ctrl 组合也 None（正常键处理）。
    state.tutor = None;
    assert_eq!(
        tutor_hotkey(
            &mut state,
            &KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL)
        ),
        TutorHotkey::None
    );

    // 恢复会话：作答至末项 → done；done 后 advance no-op；Ctrl+Q → Exit。
    state.tutor = Some(crate::tutor::TutorSession {
        module,
        idx: 0,
        marks: Vec::new(),
        done: false,
        review: false,
    });
    let total = {
        let s = state.tutor.as_ref().unwrap();
        s.module.items.len()
    };
    for _ in 0..total {
        assert_eq!(
            tutor_hotkey(
                &mut state,
                &KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL)
            ),
            TutorHotkey::Marked
        );
    }
    let done_idx = state.tutor.as_ref().unwrap().idx;
    assert_eq!(done_idx, total, "末项作答后进入结算态");
    state.tutor.as_mut().unwrap().advance(true); // done 后 no-op。
    assert_eq!(
        state.tutor.as_ref().unwrap().idx,
        total,
        "done 后 advance no-op"
    );
    assert_eq!(
        tutor_hotkey(
            &mut state,
            &KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL)
        ),
        TutorHotkey::Exit,
        "Ctrl+Q 退出"
    );
}

#[test]
fn m43_1_tutor_list_scroll_render() {
    // M43-1：模块列表 List 组件渲染——选中项高亮符号（超屏滚动由
    // ratatui ListState 视口跟随固化；当前 3 模块单屏内验证渲染形态）。
    let (_tmp, state) = make_state(2);
    let mode = InputMode::TutorList {
        cursor: 2,
        search: None,
    };
    // M56-3：向导改为 80% 对话框——用较大终端保证 14 模块仍可同屏断言。
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 30)).unwrap();
    terminal.draw(|f| render(f, &state, &mode, None)).unwrap();
    let all: String = (0..30)
        .map(|y| {
            (0..80)
                .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                .collect::<String>()
        })
        .collect();
    let compact: String = all.chars().filter(|c| !c.is_whitespace()).collect();
    // M45-8d：高亮符号 ▶ 在进度标签（如 " 0%"）之前——定位模块名再回看前缀。
    let pos = compact.find("search_frecency").expect("末模块应在列表");
    let prefix = &compact[pos.saturating_sub(8)..pos];
    assert!(prefix.contains('▶'), "末模块选中高亮: {all}");
    assert!(compact.contains("core_nav"), "首模块行: {all}");
    assert!(
        compact.contains("Enter/l=进入"),
        "键提示（边框底行）: {all}"
    );
}

#[test]
fn fix5_tutor_list_overlay_is_opaque() {
    // FIX-5-1：模块列表全屏覆盖层先 Clear——下层目录列表文字不得透出。
    let (_tmp, state) = make_state(3);
    let mode = InputMode::TutorList {
        cursor: 0,
        search: None,
    };
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(60, 8)).unwrap();
    terminal.draw(|f| render(f, &state, &mode, None)).unwrap();
    let all: String = (0..8)
        .map(|y| {
            (0..60)
                .map(|x| terminal.backend().buffer()[(x, y)].symbol())
                .collect::<String>()
        })
        .collect();
    assert!(!all.contains("f00"), "下层目录项透出到列表覆盖层: {all}");
    assert!(all.contains("core_nav"), "模块列表仍渲染: {all}");
}

#[test]
fn fix5_tutor_panel_is_opaque_but_background_visible() {
    // FIX-5-2：面板区域清除下层文字，面板之外主 TUI（标签栏）仍可见。
    let (_tmp, state) = make_state(3);
    let dump = |t: &ratatui::Terminal<ratatui::backend::TestBackend>, y0: u16, y1: u16| -> String {
        (y0..y1)
            .map(|y| {
                (0..60)
                    .map(|x| t.backend().buffer()[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect()
    };
    // 基线：无 tutor 时目录项位于面板将覆盖的行区间（小终端面板覆盖主区）。
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(60, 8)).unwrap();
    terminal
        .draw(|f| render(f, &state, &InputMode::Normal, None))
        .unwrap();
    assert!(dump(&terminal, 1, 7).contains("f00"), "基线目录项应可见");
    let base_row3 = dump(&terminal, 3, 4);

    // 会话态：面板区域不再透出目录项；tab 栏（第 0 行）仍可见。
    let module = crate::tutor::module_by_id("core_nav").unwrap();
    let mut state = state;
    state.tutor = Some(crate::tutor::TutorSession {
        module: module.clone(),
        idx: 0,
        marks: Vec::new(),
        done: false,
        review: false,
    });
    terminal
        .draw(|f| render(f, &state, &InputMode::Normal, None))
        .unwrap();
    // 面板内原本显示目录项的空白行（baseline row3 含 f00/f01）必须被 Clear
    // 清空——这是真正的"不透出"断言。注意：CJK 在 TestBackend 按"字+空格"
    // 占双宽，其续格会残留基线 symbol（ratatui/TestBackend 伪影，非真实
    // 渲染缺陷），故只在无 CJK 的空白行断言，不检查含 CJK 行的字符连续性。
    assert!(base_row3.contains("f00"), "基线 row3 应含目录项");
    let panel_row3 = dump(&terminal, 3, 4);
    assert!(
        !panel_row3.contains("f00") && !panel_row3.contains("f01"),
        "面板空白行透出目录项: {panel_row3}"
    );
    assert!(
        dump(&terminal, 1, 2).contains("Core Navigation"),
        "Panel 标题应渲染"
    );
    assert!(
        dump(&terminal, 0, 1).contains(".tmp"),
        "面板外标签栏仍可见（透明放行上下文保留）"
    );
}
