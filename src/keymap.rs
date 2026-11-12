//! 动作驱动的键位系统（M10-1；SPEC §4.8、NOTES §4.7）。
//!
//! `Action` 枚举承载文件操作语义；键位解析层只做 **键序列 → Action** 映射，
//! 不侵入导航/文件操作核心逻辑。默认键位与 vifm 对齐（`yy`→Yank、
//! `P`→PutMove、`p`→PutCopy、`dd`→Delete）。TOML 分层定制在 M10-5 接入。
//!
//! M10-1 仅定义 Action/Keymap 与默认表；消费方：M10-3（解析层接入
//! handle_key）、M10-5（TOML 分层）。

// M10-1 仅固化键位形状；消费方：M10-3（解析层接线）、M10-5（TOML 分层）。
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
    // 选择与标记
    ToggleVisual,
    ToggleTag,
    // 文件操作
    DeleteSelected,
    PutMove,
    PutCopy,
    // 多标签
    NextTab,
    PrevTab,
    // 模式与命令
    EnterCommand,
    Undo,
    Quit,
}

/// 键位表：按键序列（规范化的 crossterm KeyCode 序列）→ Action。
/// 多键序列（`gg`/`dd`/`gt`）以首键为前缀存储。
#[derive(Debug, Default, Clone)]
pub struct Keymap {
    bindings: Vec<(Vec<KeyCode>, Action)>,
}

impl Keymap {
    /// vifm 对齐的默认键位表（M10-1）。
    pub fn default_map() -> Self {
        use KeyCode::*;
        let pairs: Vec<(Vec<KeyCode>, Action)> = vec![
            (vec![Char('j')], Action::CursorDown),
            (vec![Char('k')], Action::CursorUp),
            (vec![Char('l')], Action::EnterSelected),
            (vec![Enter], Action::EnterSelected),
            (vec![Char('h')], Action::ReturnParent),
            (vec![Char('g'), Char('g')], Action::ToTop),
            (vec![Char('G')], Action::ToBottom),
            (vec![Char('d'), Char('d')], Action::DeleteSelected),
            (vec![Char('v')], Action::ToggleVisual),
            (vec![Char('t')], Action::ToggleTag),
            (vec![Char('p')], Action::PutCopy),
            (vec![Char('P')], Action::PutMove),
            (vec![Char('u')], Action::Undo),
            (vec![Char('/')], Action::StartSearch),
            (vec![Char('n')], Action::SearchNext),
            (vec![Char('N')], Action::SearchPrev),
            (vec![Char('=')], Action::StartFilter),
            (vec![Char('g'), Char('t')], Action::NextTab),
            (vec![Char('g'), Char('T')], Action::PrevTab),
            (vec![Char(':')], Action::EnterCommand),
        ];
        Self { bindings: pairs }
    }

    /// 精确匹配完整序列。
    pub fn lookup(&self, seq: &[KeyCode]) -> Option<Action> {
        self.bindings
            .iter()
            .find(|(keys, _)| keys.as_slice() == seq)
            .map(|(_, a)| *a)
    }

    /// 是否存在以 `prefix` 为前缀的绑定（多键序列等待判定）。
    pub fn has_prefix(&self, prefix: &[KeyCode]) -> bool {
        self.bindings
            .iter()
            .any(|(keys, _)| keys.len() > prefix.len() && keys[..prefix.len()] == *prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn km() -> Keymap {
        Keymap::default_map()
    }

    #[test]
    fn default_binds_vifm_semantics() {
        // vifm 对齐红线：yy 后 P 移动、p 复制、dd 删除。
        assert_eq!(km().lookup(&[KeyCode::Char('p')]), Some(Action::PutCopy));
        assert_eq!(km().lookup(&[KeyCode::Char('P')]), Some(Action::PutMove));
        assert_eq!(
            km().lookup(&[KeyCode::Char('d'), KeyCode::Char('d')]),
            Some(Action::DeleteSelected)
        );
        assert_eq!(
            km().lookup(&[KeyCode::Char('g'), KeyCode::Char('g')]),
            Some(Action::ToTop)
        );
        assert_eq!(
            km().lookup(&[KeyCode::Char('g'), KeyCode::Char('t')]),
            Some(Action::NextTab)
        );
        assert_eq!(
            km().lookup(&[KeyCode::Char('g'), KeyCode::Char('T')]),
            Some(Action::PrevTab)
        );
    }

    #[test]
    fn prefix_detection_enables_sequences() {
        let k = km();
        // `g` 是 `gg`/`gt`/`gT` 的前缀：等待第二键。
        assert!(k.has_prefix(&[KeyCode::Char('g')]));
        // `j` 是完整绑定，无更长序列：不等待。
        assert!(!k.has_prefix(&[KeyCode::Char('j')]));
        // `d` 是 `dd` 的前缀：等待。
        assert!(k.has_prefix(&[KeyCode::Char('d')]));
        // 未知键：既非绑定也非前缀。
        assert!(!k.has_prefix(&[KeyCode::Char('z')]));
    }

    #[test]
    fn unknown_sequence_returns_none() {
        assert_eq!(km().lookup(&[KeyCode::Char('z')]), None);
        // 部分序列本身不是完整绑定。
        assert_eq!(km().lookup(&[KeyCode::Char('g')]), None);
    }
}
