//! 动作驱动的键位系统（M10-1/M10-3；SPEC §4.8、NOTES §4.7）。
//!
//! `Action` 枚举承载文件操作语义；`Keymap` 是 **键序列 → Action** 的唯一
//! 映射源；`Resolver` 消费按键并产出 `Feed` 结果（解析层不侵入导航/文件
//! 操作核心逻辑）。默认键位与 vifm 对齐（`yy`→Yank、`p`→PutCopy、
//! `P`→PutMove、`dd`→DeleteSelected、`gg`→ToTop、`gt`/`gT`→切标签）。
//!
//! M10-2 决策（NOTES §8）：keymap-rs 家族采用量过低（500–700 下载），
//! 采纳降级路线自写本解析层；TOML 分层在 M10-5 接入。

// M10-1/M10-3 固化键位形状；Resolver 为独立测试用入口（handle_key 内联序列
// 解析），PutCopy/PutMove 的 yy 缓冲区在 M10-7 接入。
#![allow(dead_code)]

use ratatui::crossterm::event::KeyCode;

/// 用户可触发的语义动作（M10-1）。
/// 用户只能绑定到内置 Action，不能自定义动作实现（SPEC §4.8）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    // 导航
    CursorDown,
    CursorUp,
    EnterSelected,
    ReturnParent,
    ToTop,
    ToBottom,
    HalfPageDown,
    HalfPageUp,
    // 搜索与过滤
    StartSearch,
    SearchNext,
    SearchPrev,
    StartFilter,
    /// M54 ZKEY-2：隐藏文件显示切换（za/zo/zm，与 show_hidden 同源）。
    ToggleHiddenFiles,
    ShowHiddenFiles,
    HideHiddenFiles,
    /// M54 ZKEY-3：过滤器临时快照/恢复/清空永久过滤器。
    ResetFiltersTemp,
    RestoreFiltersTemp,
    ClearPermanentFilter,
    /// M54 ZKEY-4：视口定位（zt/zz/zb，不动光标索引）。
    ViewportTop,
    ViewportCenter,
    ViewportBottom,
    // 选择与标记
    ToggleVisual,
    ToggleTag,
    // 文件操作
    DeleteSelected,
    /// yank（FIX-2：yy——选中条目放入复制缓冲区，p/P 消费）。
    Yank,
    /// 永久删除（M13-3：DD——跳过应用内暂存，不可逆强提示）。
    DeletePermanent,
    /// 批量重命名（M13-6：cw——多选名单交 $EDITOR 编辑；M21-E4：R 同此）。
    RenameBatch,
    /// 内联重命名（M21-E4：I——光标在名首，保留原名）。
    RenameStart,
    /// 内联重命名（M21-E4：cc——清空原名）。
    RenameClear,
    /// 内联重命名（M21-E4：A——光标在名尾，保留原名）。
    RenameAppend,
    /// 复制名称（M23-10：yn——名称；多选=标记/可视全部）。
    CopyName,
    /// 复制全路径（M23-10：yf）。
    CopyPath,
    /// 复制内容（M23-10：yc——文件内容/目录条目清单）。
    CopyContent,
    /// 打开编辑器（M23-10：e——$EDITOR 打开光标文件）。
    OpenEditor,
    /// 执行自定义外部命令（M23-11：u16 索引指向全局命令表——保持 Copy）。
    RunCommand(u16),
    /// 切换 view 模式（M13-9：w——预览窗格完整文件视图）。
    ToggleView,
    PutCopy,
    PutMove,
    /// M29-2：对比结果视图——取回另一侧条目到当前侧（仅 `>` 条目）。
    DiffPut,
    /// M29-2：对比结果视图——删除当前侧多出条目（仅 `<` 条目，可恢复）。
    DiffObtain,
    // 撤销
    Undo,
    /// 目录历史后退（M16 NAV-3：仅用户键位映射，无默认键）。
    HistPrev,
    /// 目录历史前进（M16 NAV-3：仅用户键位映射，无默认键）。
    HistNext,
    // 多标签
    NextTab,
    PrevTab,
    // 模式与命令
    EnterCommand,
    /// M50-4：打开外部命令输入模式（预填 `!`）；不设默认绑定。
    OpenExternalCommandLine,
    /// 打开合一跳转选择器（M8-12：o，全局统一时钟视角）。
    OpenJump,
    /// 打开 git TUI（M25-6：gi——PATH 优先级探测 gitui→ziggity→lazygit）。
    GitUi,
    Quit,
}

impl Action {
    /// GAP-1：TOML `run` 字段的动作名（snake_case 为主，附 vifm 惯用名）。
    pub fn from_name(name: &str) -> Option<Action> {
        use Action::*;
        Some(match name {
            "cursor_down" | "down" | "j" => CursorDown,
            "cursor_up" | "up" | "k" => CursorUp,
            "enter_selected" | "enter" | "l" => EnterSelected,
            "return_parent" | "parent" | "h" => ReturnParent,
            "to_top" | "gg" => ToTop,
            "to_bottom" | "G" => ToBottom,
            "half_page_down" | "half_down" => HalfPageDown,
            "half_page_up" | "half_up" => HalfPageUp,
            "start_search" | "search" => StartSearch,
            "search_next" | "next_match" => SearchNext,
            "search_prev" | "prev_match" => SearchPrev,
            "start_filter" | "filter" => StartFilter,
            "toggle_hidden" | "toggle_hidden_files" => ToggleHiddenFiles,
            "show_hidden_files" => ShowHiddenFiles,
            "hide_hidden_files" => HideHiddenFiles,
            "reset_filters_temp" => ResetFiltersTemp,
            "restore_filters_temp" => RestoreFiltersTemp,
            "clear_permanent_filter" => ClearPermanentFilter,
            "viewport_top" => ViewportTop,
            "viewport_center" => ViewportCenter,
            "viewport_bottom" => ViewportBottom,
            "toggle_visual" | "visual" => ToggleVisual,
            "toggle_tag" | "tag" => ToggleTag,
            "delete_selected" | "delete" => DeleteSelected,
            "yank" => Yank,
            "delete_permanent" | "delete_hard" => DeletePermanent,
            "rename_batch" | "batch_rename" | "rename" => RenameBatch,
            "rename_start" | "rename_here" => RenameStart,
            "rename_clear" => RenameClear,
            "rename_append" => RenameAppend,
            "copy_name" => CopyName,
            "copy_path" => CopyPath,
            "copy_content" => CopyContent,
            "open_editor" | "edit" => OpenEditor,
            "toggle_view" | "view" => ToggleView,
            "put_copy" | "paste_copy" => PutCopy,
            "diff_put" => DiffPut,
            "diff_obtain" => DiffObtain,
            "put_move" | "paste_move" => PutMove,
            "undo" => Undo,
            "next_tab" | "tab_next" => NextTab,
            "prev_tab" | "tab_prev" => PrevTab,
            "enter_command" | "command" => EnterCommand,
            "open_external_command_line" => OpenExternalCommandLine,
            "open_jump" | "jump" => OpenJump,
            "quit" => Quit,
            "histprev" | "hist_prev" => HistPrev,
            "gitui" | "git_tui" => GitUi,
            "histnext" | "hist_next" => HistNext,
            _ => return None,
        })
    }
}

/// GAP-1：TOML `on` 字段的按键解析——"P"（字符）、"C-o"（Ctrl）、
/// "enter"/"esc"/"tab"/"backspace"/"space"/方向/Home/End/PageUp/PageDown。
pub fn parse_key(spec: &str) -> Option<Key> {
    let (ctrl, rest) = if let Some(r) = spec.strip_prefix("C-") {
        (true, r)
    } else if let Some(r) = spec.strip_prefix("c-") {
        (true, r)
    } else {
        (false, spec)
    };
    let code = match rest.to_lowercase().as_str() {
        "enter" | "cr" => KeyCode::Enter,
        "esc" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "backspace" | "bs" => KeyCode::Backspace,
        "space" | "spc" => KeyCode::Char(' '),
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        other if other.chars().count() == 1 => KeyCode::Char(other.chars().next()?),
        _ => return None,
    };
    Some(Key { code, ctrl })
}

/// 带修饰键的按键（键位表的原子单位）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    pub code: KeyCode,
    pub ctrl: bool,
}

impl Key {
    pub fn plain(code: KeyCode) -> Self {
        Self { code, ctrl: false }
    }

    pub fn ctrl(code: KeyCode) -> Self {
        Self { code, ctrl: true }
    }
}

/// 键位表：按键序列 → Action。多键序列（`gg`/`dd`/`gt`）按完整序列存储。
/// `explicit` 记录用户 TOML 显式提供的序列（GAP-1：单键命中时覆盖内建）。
#[derive(Debug, Default, Clone)]
pub struct Keymap {
    bindings: Vec<(Vec<Key>, Action)>,
    explicit: std::collections::HashSet<Vec<Key>>,
}

impl Keymap {
    /// 分层合并（M10-5 GAP-1）：prepend 优先于默认、append 低于默认、
    /// replace 完全替换默认；用户提供的序列全部记为显式绑定。
    pub fn with_layers(
        prepend: Vec<(Vec<Key>, Action)>,
        append: Vec<(Vec<Key>, Action)>,
        replace: Option<Vec<(Vec<Key>, Action)>>,
    ) -> Self {
        let mut explicit = std::collections::HashSet::new();
        let mut bindings: Vec<(Vec<Key>, Action)> = Vec::new();
        for (keys, action) in &prepend {
            explicit.insert(keys.clone());
            bindings.push((keys.clone(), *action));
        }
        match replace {
            Some(replaced) => {
                for (keys, action) in &replaced {
                    explicit.insert(keys.clone());
                    bindings.push((keys.clone(), *action));
                }
            }
            None => bindings.extend(Self::default_map().bindings),
        }
        for (keys, action) in &append {
            explicit.insert(keys.clone());
            bindings.push((keys.clone(), *action));
        }
        Self { bindings, explicit }
    }

    /// 该序列是否为用户显式绑定（GAP-1：单键命中时优先于内建处理）。
    pub fn is_explicit(&self, keys: &[Key]) -> bool {
        self.explicit.contains(keys)
    }
}

impl Keymap {
    /// vifm 对齐的默认键位表（M10-1/M10-3）。
    pub fn default_map() -> Self {
        use Action::*;
        use KeyCode::*;
        let k = Key::plain;
        let kc = Key::ctrl;
        let pairs: Vec<(Vec<Key>, Action)> = vec![
            (vec![k(Char('j'))], CursorDown),
            (vec![k(Char('k'))], CursorUp),
            // M41-2：方向键与 k/j/h/l 同语义（内建分支别名优先，注册与
            // j/k 同模式；用户显式绑定经 is_explicit 前置拦截覆盖）。
            (vec![k(Up)], CursorUp),
            (vec![k(Down)], CursorDown),
            (vec![k(Left)], ReturnParent),
            (vec![k(Right)], EnterSelected),
            (vec![k(Char('l'))], EnterSelected),
            (vec![k(Enter)], EnterSelected),
            (vec![k(Char('h'))], ReturnParent),
            (vec![k(Char('g')), k(Char('g'))], ToTop),
            (vec![k(Char('g')), k(Char('i'))], GitUi),
            (vec![k(Char('G'))], ToBottom),
            (vec![kc(Char('d'))], HalfPageDown),
            (vec![kc(Char('u'))], HalfPageUp),
            (vec![k(Char('d')), k(Char('d'))], DeleteSelected),
            // M29-2：对比同步——vimdiff 语义借名（do=取回/obtain 由 SPEC M29-2
            // 定义为 dp=复制另一侧到当前、do=删除当前侧多出；仅对比视图生效）。
            (vec![k(Char('d')), k(Char('p'))], DiffPut),
            (vec![k(Char('d')), k(Char('o'))], DiffObtain),
            (vec![k(Char('y')), k(Char('y'))], Yank),
            (vec![k(Char('D')), k(Char('D'))], DeletePermanent),
            (vec![k(Char('c')), k(Char('w'))], RenameBatch),
            (vec![k(Char('c')), k(Char('c'))], RenameClear),
            (vec![k(Char('I'))], RenameStart),
            (vec![k(Char('A'))], RenameAppend),
            (vec![k(Char('R'))], RenameBatch),
            (vec![k(Char('y')), k(Char('n'))], CopyName),
            (vec![k(Char('y')), k(Char('f'))], CopyPath),
            (vec![k(Char('y')), k(Char('c'))], CopyContent),
            (vec![k(Char('e'))], OpenEditor),
            (vec![k(Char('w'))], ToggleView),
            (vec![k(Char('v'))], ToggleVisual),
            (vec![k(Char('t'))], ToggleTag),
            (vec![k(Char('p'))], PutCopy),
            (vec![k(Char('P'))], PutMove),
            (vec![k(Char('u'))], Undo),
            (vec![k(Char('q'))], Quit),
            (vec![k(Char('/'))], StartSearch),
            (vec![k(Char('n'))], SearchNext),
            (vec![k(Char('N'))], SearchPrev),
            (vec![k(Char('='))], StartFilter),
            (vec![k(Char('g')), k(Char('t'))], NextTab),
            (vec![k(Char('g')), k(Char('T'))], PrevTab),
            (vec![k(Char(':'))], EnterCommand),
            (vec![k(Char('o'))], OpenJump),
            // M54 ZKEY-2..4：z 系列两键序列（兄弟目录跳转不内置）。
            (vec![k(Char('z')), k(Char('a'))], ToggleHiddenFiles),
            (vec![k(Char('z')), k(Char('o'))], ShowHiddenFiles),
            (vec![k(Char('z')), k(Char('m'))], HideHiddenFiles),
            (vec![k(Char('z')), k(Char('R'))], ResetFiltersTemp),
            (vec![k(Char('z')), k(Char('M'))], RestoreFiltersTemp),
            (vec![k(Char('z')), k(Char('O'))], ClearPermanentFilter),
            (vec![k(Char('z')), k(Char('t'))], ViewportTop),
            (vec![k(Char('z')), k(Char('z'))], ViewportCenter),
            (vec![k(Char('z')), k(Char('b'))], ViewportBottom),
        ];
        Self {
            bindings: pairs,
            explicit: std::collections::HashSet::new(),
        }
    }

    /// 精确匹配完整序列。
    pub fn lookup(&self, seq: &[Key]) -> Option<Action> {
        self.bindings
            .iter()
            .find(|(keys, _)| keys.as_slice() == seq)
            .map(|(_, a)| *a)
    }

    /// 是否存在以 `prefix` 为前缀的绑定（多键序列等待判定）。
    pub fn has_prefix(&self, prefix: &[Key]) -> bool {
        self.bindings
            .iter()
            .any(|(keys, _)| keys.len() > prefix.len() && keys[..prefix.len()] == *prefix)
    }
}

/// 解析器状态：待决序列缓冲（M10-3，替代原 G/D 待决枚举）。
#[derive(Debug, Default, Clone)]
pub struct Resolver {
    seq: Vec<Key>,
}

/// `Resolver::feed` 的结果。
#[derive(Debug, PartialEq, Eq)]
pub enum Feed {
    /// 命中完整绑定。
    Complete(Action),
    /// 前缀命中，等待后续按键。
    Pending,
    /// 无匹配（整体丢弃；调用方处理特殊键如数字计数/Esc/Enter 提交类）。
    NotMapped,
}

impl Resolver {
    /// 将按键送入解析层。
    ///
    /// 规则（与 M1-3 行为一致）：
    /// 1. 待决非空 → 新序列 = 待决 + 本键；查表命中则产出 Action，
    ///    前缀命中则继续等待，否则整条序列丢弃（含 q/Ctrl-C）。
    /// 2. 待决为空 → 前缀命中则进入待决；否则查表单键动作。
    pub fn feed(&mut self, keymap: &Keymap, key: Key) -> Feed {
        self.seq.push(key);
        if keymap.has_prefix(&self.seq) {
            return Feed::Pending;
        }
        match keymap.lookup(&self.seq) {
            Some(action) => {
                self.seq.clear();
                Feed::Complete(action)
            }
            None => {
                // 整条序列无映射：丢弃（是否曾为前缀不影响结果）。
                self.seq.clear();
                Feed::NotMapped
            }
        }
    }

    /// 当前待决序列（测试用）。
    pub fn pending_seq(&self) -> &[Key] {
        &self.seq
    }

    /// 清空待决序列。
    pub fn clear(&mut self) {
        self.seq.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use KeyCode::*;

    fn km() -> Keymap {
        Keymap::default_map()
    }

    fn plain(code: KeyCode) -> Key {
        Key::plain(code)
    }

    #[test]
    fn default_binds_vifm_semantics() {
        // vifm 对齐红线：p 复制、P 移动、dd 删除、gg 顶部、gt/gT 切标签。
        let k = km();
        assert_eq!(k.lookup(&[plain(Char('p'))]), Some(Action::PutCopy));
        assert_eq!(k.lookup(&[plain(Char('P'))]), Some(Action::PutMove));
        assert_eq!(
            k.lookup(&[plain(Char('y')), plain(Char('y'))]),
            Some(Action::Yank)
        );
        assert_eq!(
            k.lookup(&[plain(Char('d')), plain(Char('d'))]),
            Some(Action::DeleteSelected)
        );
        assert_eq!(
            k.lookup(&[plain(Char('g')), plain(Char('g'))]),
            Some(Action::ToTop)
        );
        assert_eq!(
            k.lookup(&[plain(Char('g')), plain(Char('t'))]),
            Some(Action::NextTab)
        );
        assert_eq!(
            k.lookup(&[plain(Char('g')), plain(Char('T'))]),
            Some(Action::PrevTab)
        );
        assert_eq!(
            k.lookup(&[Key::ctrl(Char('d'))]),
            Some(Action::HalfPageDown)
        );
        assert_eq!(k.lookup(&[Key::ctrl(Char('u'))]), Some(Action::HalfPageUp));
    }

    #[test]
    fn m41_2_default_map_binds_arrow_keys() {
        // M41-2：方向键默认绑定（内建分支别名优先；用户显式绑定可覆盖）。
        let k = km();
        assert_eq!(k.lookup(&[plain(KeyCode::Up)]), Some(Action::CursorUp));
        assert_eq!(k.lookup(&[plain(KeyCode::Down)]), Some(Action::CursorDown));
        assert_eq!(
            k.lookup(&[plain(KeyCode::Left)]),
            Some(Action::ReturnParent)
        );
        assert_eq!(
            k.lookup(&[plain(KeyCode::Right)]),
            Some(Action::EnterSelected)
        );
    }

    #[test]
    fn resolver_prefix_then_complete() {
        let mut r = Resolver::default();
        let k = km();
        // g：前缀命中 → Pending。
        assert_eq!(r.feed(&k, plain(Char('g'))), Feed::Pending);
        assert_eq!(r.pending_seq(), [plain(Char('g'))]);
        // gg：命中 ToTop。
        assert_eq!(r.feed(&k, plain(Char('g'))), Feed::Complete(Action::ToTop));
        assert!(r.pending_seq().is_empty());
    }

    #[test]
    fn resolver_discards_unknown_sequence() {
        let mut r = Resolver::default();
        let k = km();
        // gx：g 是前缀但 x 无后续 → 整条丢弃。
        assert_eq!(r.feed(&k, plain(Char('g'))), Feed::Pending);
        assert_eq!(r.feed(&k, plain(Char('x'))), Feed::NotMapped);
        assert!(r.pending_seq().is_empty());
        // 序列中的 q 被当作第二键丢弃，不触发退出。
        assert_eq!(r.feed(&k, plain(Char('g'))), Feed::Pending);
        assert_eq!(r.feed(&k, plain(Char('q'))), Feed::NotMapped);
        // 之后单独的 q 正常映射 Quit。
        assert_eq!(r.feed(&k, plain(Char('q'))), Feed::Complete(Action::Quit));
    }

    #[test]
    fn single_key_immediate_dispatch() {
        let mut r = Resolver::default();
        let k = km();
        // 单键完整绑定立即产出（j 不是任何序列前缀）。
        assert_eq!(
            r.feed(&k, plain(Char('j'))),
            Feed::Complete(Action::CursorDown)
        );
        // Ctrl-d 立即产出。
        assert_eq!(
            r.feed(&k, Key::ctrl(Char('d'))),
            Feed::Complete(Action::HalfPageDown)
        );
    }

    #[test]
    fn layers_merge_prepend_append_replace() {
        // AUDIT-12：GAP-1 分层合并语义直接测试——prepend 覆盖默认、
        // append 补充、replace 完全替换默认表。
        // prepend：同键覆盖默认（P: put_copy 覆盖默认 put_move）。
        let k = Keymap::with_layers(
            vec![(vec![plain(Char('P'))], Action::PutCopy)],
            vec![],
            None,
        );
        assert_eq!(
            k.lookup(&[plain(Char('P'))]),
            Some(Action::PutCopy),
            "prepend 覆盖默认"
        );
        // append：默认未占用的键补充绑定；不影响默认。
        let k = Keymap::with_layers(vec![], vec![(vec![plain(Char('Z'))], Action::Quit)], None);
        assert_eq!(k.lookup(&[plain(Char('Z'))]), Some(Action::Quit));
        assert_eq!(k.lookup(&[plain(Char('P'))]), Some(Action::PutMove));
        // 多键序列经 append 分层 → resolver 完整解析。
        let k = Keymap::with_layers(
            vec![],
            vec![(vec![plain(Char('g')), plain(Char('w'))], Action::NextTab)],
            None,
        );
        let mut r = Resolver::default();
        assert_eq!(r.feed(&k, plain(Char('g'))), Feed::Pending);
        assert_eq!(
            r.feed(&k, plain(Char('w'))),
            Feed::Complete(Action::NextTab)
        );
        // replace：默认表整体失效，仅 replace 表生效。
        let k = Keymap::with_layers(
            vec![],
            vec![],
            Some(vec![(vec![plain(Char('x'))], Action::Quit)]),
        );
        assert_eq!(k.lookup(&[plain(Char('x'))]), Some(Action::Quit));
        assert_eq!(
            k.lookup(&[plain(Char('j'))]),
            None,
            "replace 后默认 j 不可用"
        );
    }
}
