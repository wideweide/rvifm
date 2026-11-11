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

    /// 新建标签并切换为 active（M1-5 `:tabnew`）。
    pub fn push_tab(&mut self, tab: Tab) {
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
    }

    /// 切换到下一个 tab（循环；M1-5 `gt`）。
    pub fn next_tab(&mut self) {
        self.active = (self.active + 1) % self.tabs.len();
    }

    /// 切换到上一个 tab（循环；M1-5 `gT`）。
    pub fn prev_tab(&mut self) {
        self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
    }

    /// 跳到第 n 个 tab（1 起始，越界钳制；M1-5 `N gt`）。
    pub fn goto_tab(&mut self, n: usize) {
        if self.tabs.is_empty() {
            return;
        }
        self.active = n.saturating_sub(1).min(self.tabs.len() - 1);
    }

    /// 关闭当前 tab；若关闭的是最后一个 tab 返回 `true`（调用方退出应用）。
    pub fn close_tab(&mut self) -> bool {
        self.tabs.remove(self.active);
        if self.tabs.is_empty() {
            return true;
        }
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        }
        false
    }

    /// 移动当前 tab 到 1 起始位置 `n`；`None` 表示移到末尾（M1-5 `:tabmove`）。
    pub fn move_tab(&mut self, n: Option<usize>) {
        let tab = self.tabs.remove(self.active);
        let idx = n
            .map(|n| n.saturating_sub(1).min(self.tabs.len()))
            .unwrap_or(self.tabs.len());
        self.tabs.insert(idx, tab);
        self.active = idx;
    }

    /// 以当前 tab 的落点目录新建标签（M1-5 `:tabnew`）。
    /// 目录读取失败时返回 `false`，不产生副作用。
    pub fn new_tab_here(&mut self) -> bool {
        let dir = self.active_tab().location.base_dir().to_path_buf();
        match Tab::from_physical(&dir) {
            Ok(tab) => {
                self.push_tab(tab);
                true
            }
            Err(_) => false,
        }
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
