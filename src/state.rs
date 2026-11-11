//! `AppState`：tabs 列表 + active 索引 + 全局撤销栈（SPEC §4.2）。

use crate::tab::Tab;

/// 全局撤销栈（不是 per-tab）。写入与逆序撤销在 M2-1/M2-6 接入。
#[derive(Debug, Default)]
#[allow(dead_code)]
pub struct UndoStack {
    ops: Vec<UndoableOp>,
}

/// 撤销操作记录。M1 阶段仅预留 `tab_id`（ACCEPTANCE §3）；
/// M2-1 扩展为携带路径与操作类型的完整 `UndoableOp`。
#[derive(Debug)]
#[allow(dead_code)]
pub struct UndoableOp {
    pub tab_id: usize,
}

/// 应用状态：所有标签 + 当前标签索引 + 全局撤销栈。
#[derive(Debug)]
pub struct AppState {
    pub tabs: Vec<Tab>,
    pub active: usize,
    /// 读写接线在 M2（撤销闭环）接入。
    #[allow(dead_code)]
    pub undo_stack: UndoStack,
}

impl AppState {
    /// 以一个初始标签启动。
    pub fn new(initial: Tab) -> Self {
        Self {
            tabs: vec![initial],
            active: 0,
            undo_stack: UndoStack::default(),
        }
    }

    pub fn active_tab(&self) -> &Tab {
        &self.tabs[self.active]
    }

    /// 当前 tab 的可变访问（j/k、目录切换等动作使用）。
    pub fn active_tab_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    /// 新建标签并切换为 active（键位接线在 M1-5）。
    #[allow(dead_code)]
    pub fn push_tab(&mut self, tab: Tab) {
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tab::Location;
    use std::path::PathBuf;

    fn tab(path: &str) -> Tab {
        Tab {
            location: Location::Physical(PathBuf::from(path)),
            entries: Vec::new(),
            cursor: 0,
            scroll: 0,
            search: None,
            history: Vec::new(),
        }
    }

    #[test]
    fn starts_with_single_active_tab() {
        let s = AppState::new(tab("C:/a"));
        assert_eq!(s.tabs.len(), 1);
        assert_eq!(s.active, 0);
        assert_eq!(
            s.active_tab().location,
            Location::Physical(PathBuf::from("C:/a"))
        );
    }

    #[test]
    fn tabs_hold_independent_state() {
        let mut s = AppState::new(tab("C:/a"));
        s.active_tab_mut().cursor = 3;
        s.active_tab_mut().scroll = 5;

        s.push_tab(tab("C:/b"));
        assert_eq!(s.active, 1);
        assert_eq!(
            s.active_tab().location,
            Location::Physical(PathBuf::from("C:/b"))
        );
        // 切换后 tab 0 的光标与滚动独立保持。
        assert_eq!(s.tabs[0].cursor, 3);
        assert_eq!(s.tabs[0].scroll, 5);
    }

    #[test]
    fn undo_stack_reserved_with_tab_id() {
        let mut s = AppState::new(tab("C:/a"));
        assert_eq!(s.undo_stack.ops.len(), 0);
        s.undo_stack.ops.push(UndoableOp { tab_id: 0 });
        assert_eq!(s.undo_stack.ops[0].tab_id, 0);
    }
}
