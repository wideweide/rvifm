//! `AppState`：tabs 列表 + active 索引 + 全局撤销栈（SPEC §4.2 / §4.3）。

use std::path::PathBuf;

use crate::tab::Tab;

/// 可逆操作记录（M2-1）。每条都携带所属 tab 与路径（ACCEPTANCE §4），
/// 撤销栈全局共享，跨 tab 撤销据此定位。
/// 各操作的构造与 `u` 执行逻辑随 M2-2～M2-5 接入，此前仅结构落地。
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum UndoableOp {
    /// 删除（可恢复）：`path` 为原路径，`staged` 为应用内暂存路径（undo 移回）。
    Delete {
        tab_id: usize,
        path: PathBuf,
        staged: PathBuf,
    },
    /// 重命名：`from` → `to`（undo 反向）。
    Rename {
        tab_id: usize,
        from: PathBuf,
        to: PathBuf,
    },
    /// 移动：`from` → `to`（undo 反向）。
    Move {
        tab_id: usize,
        from: PathBuf,
        to: PathBuf,
    },
    /// 新建目录/文件：undo 即移除 `path`。`recursive` 标记目录树整体为新建产物
    /// （copy 目录，M2-4b），undo 可整树移除；mkdir 只允许移除空目录（保护内容）。
    Create {
        tab_id: usize,
        path: PathBuf,
        recursive: bool,
    },
}

#[allow(dead_code)] // 消费方：M2-2 起的撤销执行路径
impl UndoableOp {
    /// 所属 tab（跨 tab 撤销定位用）。
    pub fn tab_id(&self) -> usize {
        match self {
            UndoableOp::Delete { tab_id, .. }
            | UndoableOp::Rename { tab_id, .. }
            | UndoableOp::Move { tab_id, .. }
            | UndoableOp::Create { tab_id, .. } => *tab_id,
        }
    }

    /// 主路径（提示与定位用）。
    pub fn primary_path(&self) -> &PathBuf {
        match self {
            UndoableOp::Delete { path, .. } | UndoableOp::Create { path, .. } => path,
            UndoableOp::Rename { from, .. } | UndoableOp::Move { from, .. } => from,
        }
    }

    /// 人类可读描述（`u` 成功后的状态栏提示，M2-6）。
    pub fn describe(&self) -> String {
        match self {
            UndoableOp::Delete { path, .. } => format!("删除 {}", path.display()),
            UndoableOp::Rename { from, to, .. } => {
                format!("重命名 {} -> {}", from.display(), to.display())
            }
            UndoableOp::Move { from, to, .. } => {
                format!("移动 {} -> {}", from.display(), to.display())
            }
            UndoableOp::Create { path, .. } => format!("新建 {}", path.display()),
        }
    }
}

/// 全局撤销栈（不是 per-tab，SPEC §4.3）。LIFO：`u` 逆序弹出最近操作。
#[derive(Debug, Default)]
#[allow(dead_code)] // ops 由 M2-2 起写入、`u`（M2-2）读取
pub struct UndoStack {
    ops: Vec<UndoableOp>,
}

#[allow(dead_code)] // 消费方：M2-2 起的 `u` 键处理
impl UndoStack {
    /// 记录一次已完成的操作。
    pub fn push(&mut self, op: UndoableOp) {
        self.ops.push(op);
    }

    /// 弹出最近一次操作（`u` 消费；空栈返回 None）。
    pub fn pop(&mut self) -> Option<UndoableOp> {
        self.ops.pop()
    }

    /// 栈内操作数（测试与提示用）。
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

/// 应用状态：所有标签 + 当前标签索引 + 全局撤销栈。
#[derive(Debug)]
pub struct AppState {
    pub tabs: Vec<Tab>,
    pub active: usize,
    /// 写入（操作完成后记录）与读取（`u` 撤销）在 M2-2 起接入。
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
    use crate::tab::{Location, SearchState, Tab};
    use std::path::PathBuf;

    fn tab_at(path: &str) -> Tab {
        Tab {
            location: Location::Physical(PathBuf::from(path)),
            entries: Vec::new(),
            cursor: 0,
            scroll: 0,
            search: None,
            history: Vec::new(),
        }
    }

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
    fn per_tab_state_independent() {
        // M1-6 验收：两个 tab 打开不同目录，切换后各自状态独立保持。
        let mut s = AppState::new(tab_at("C:/a"));
        s.push_tab(tab_at("C:/b"));
        s.active = 0; // push_tab 会切到新 tab，归位到第一个

        s.active_tab_mut().cursor = 3;
        s.active_tab_mut().scroll = 5;
        s.active_tab_mut().search = Some(SearchState {
            query: "abc".to_owned(),
        });

        // 切到 tab b：未被 a 的状态污染。
        s.next_tab();
        assert_eq!(
            s.active_tab().location,
            Location::Physical(PathBuf::from("C:/b"))
        );
        assert_eq!(s.active_tab().cursor, 0);
        assert_eq!(s.active_tab().scroll, 0);
        assert!(s.active_tab().search.is_none());
        s.active_tab_mut().cursor = 7;
        s.active_tab_mut().scroll = 9;

        // 切回 tab a：光标、滚动、搜索状态全部保持。
        s.prev_tab();
        assert_eq!(
            s.active_tab().location,
            Location::Physical(PathBuf::from("C:/a"))
        );
        assert_eq!(s.active_tab().cursor, 3);
        assert_eq!(s.active_tab().scroll, 5);
        assert_eq!(s.active_tab().search.as_ref().unwrap().query, "abc");
    }

    #[test]
    fn undo_stack_lifo_with_tab_and_path() {
        use std::path::PathBuf;

        let mut s = AppState::new(tab_at("C:/a"));
        assert!(s.undo_stack.is_empty());

        // 两次操作：tab 0 删除、tab 1 移动。
        s.undo_stack.push(UndoableOp::Delete {
            tab_id: 0,
            path: PathBuf::from("C:/a/x.txt"),
            staged: PathBuf::from("C:/trash/1-x.txt"),
        });
        s.undo_stack.push(UndoableOp::Move {
            tab_id: 1,
            from: PathBuf::from("C:/b/y.txt"),
            to: PathBuf::from("C:/b/z/y.txt"),
        });
        assert_eq!(s.undo_stack.len(), 2);

        // LIFO：先弹 tab 1 的移动，再弹 tab 0 的删除（`u` 逆序撤销）。
        let op = s.undo_stack.pop().expect("stack should not be empty");
        assert_eq!(op.tab_id(), 1);
        assert_eq!(op.primary_path(), &PathBuf::from("C:/b/y.txt"));
        assert!(matches!(op, UndoableOp::Move { .. }));

        let op = s.undo_stack.pop().expect("stack should not be empty");
        assert_eq!(op.tab_id(), 0);
        assert!(matches!(op, UndoableOp::Delete { .. }));
        assert!(s.undo_stack.is_empty());
        assert_eq!(s.undo_stack.pop(), None);
    }
}
