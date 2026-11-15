//! `Location` 与 `Tab`：浏览位置模型与每标签独立状态（SPEC §4.1 / §4.2）。
//!
//! 归档浏览行为在 M7 接入；`Location::Archive` 的数据形状现在固定，
//! 避免后期改动所有 tab 相关代码。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::config::Config;

/// 浏览位置：物理目录或归档内虚拟路径（只读）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    Physical(PathBuf),
    /// `host` 为归档文件的物理路径，`inner` 为归档内路径；构造与导航在 M7 接入
    /// （形状由 SPEC §4.1 现在固定）。
    #[allow(dead_code)]
    Archive {
        host: PathBuf,
        inner: PathBuf,
    },
}

impl Location {
    /// 状态栏/面包屑显示。归档位置显示为 `host:/inner/path`（SPEC §4.1）。
    pub fn display_path(&self) -> String {
        match self {
            Location::Physical(p) => p.to_string_lossy().into_owned(),
            Location::Archive { host, inner } => format!(
                "{}:/{}",
                host.display(),
                inner.to_string_lossy().replace('\\', "/")
            ),
        }
    }

    /// `:tabnew` 的落点目录：Physical 为该目录；Archive 为宿主目录
    /// （归档内新建标签复用宿主，M7 再细化）。
    pub fn base_dir(&self) -> &Path {
        match self {
            Location::Physical(p) => p,
            Location::Archive { host, .. } => host,
        }
    }
}

/// 单个目录项。M1-4 起携带大小与权限摘要。
#[derive(Debug, Clone)]
pub struct FileItem {
    /// 类型图标（M9-1 devicons）。
    pub icon: crate::icons::IconInfo,
    pub name: String,
    pub is_dir: bool,
    /// 文件字节数（目录同样记录底层值，状态栏对目录省略显示）。
    pub size: u64,
    /// 平台相关权限摘要：unix 为 3 位八进制（如 `644`），Windows 为 `ro`/`rw`。
    pub perm: String,
}

/// 搜索状态。`query` 由 `/` 设置、`n/N` 消费（M1-7）。
#[derive(Debug, Clone)]
pub struct SearchState {
    pub query: String,
}

/// 单个 pane 的最小导航状态（M14-1）：右 pane 使用。
#[derive(Debug, Clone)]
pub struct Pane {
    pub location: Location,
    pub entries: Vec<FileItem>,
    pub cursor: usize,
    pub scroll: usize,
}

impl Pane {
    /// 从物理目录构建 pane（加载排序条目，M5-2 options 生效）。
    pub fn from_physical(dir: &Path, config: Config) -> std::io::Result<Self> {
        Ok(Self {
            location: Location::Physical(dir.to_path_buf()),
            entries: load_entries(dir, &config)?,
            cursor: 0,
            scroll: 0,
        })
    }

    /// 光标移动（M14-1）：±1 环绕，其余钳制（与左 pane 语义一致）。
    pub fn move_cursor(&mut self, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        let len = self.entries.len();
        match delta {
            1 => self.cursor = (self.cursor + 1) % len,
            -1 => self.cursor = (self.cursor + len - 1) % len,
            _ => {
                let max = len as isize - 1;
                let next = self.cursor as isize + delta;
                self.cursor = next.clamp(0, max) as usize;
            }
        }
    }

    pub fn cursor_top(&mut self) {
        self.cursor = 0;
    }

    pub fn cursor_bottom(&mut self) {
        if !self.entries.is_empty() {
            self.cursor = self.entries.len() - 1;
        }
    }

    /// 进入光标条目（M14-1）：仅物理目录；文件/归档 no-op（SIMPLIFIED）。
    pub fn enter_selected(&mut self, config: Config) -> std::io::Result<()> {
        if let Location::Physical(base) = &self.location {
            let Some(item) = self.entries.get(self.cursor) else {
                return Ok(());
            };
            if item.is_dir {
                self.location = Location::Physical(base.join(&item.name));
                self.cursor = 0;
                self.scroll = 0;
                return self.refresh(config);
            }
        }
        Ok(())
    }

    /// 返回父目录（M14-1，h）。
    pub fn return_parent(&mut self, config: Config) -> std::io::Result<()> {
        if let Location::Physical(base) = &self.location
            && let Some(parent) = base.parent().filter(|p| !p.as_os_str().is_empty())
        {
            self.location = Location::Physical(parent.to_path_buf());
            self.cursor = 0;
            self.scroll = 0;
            return self.refresh(config);
        }
        Ok(())
    }

    /// 重读当前目录条目并钳制光标。
    pub fn refresh(&mut self, config: Config) -> std::io::Result<()> {
        if let Location::Physical(dir) = &self.location {
            self.entries = load_entries(dir, &config)?;
            if self.cursor >= self.entries.len() {
                self.cursor = self.entries.len().saturating_sub(1);
            }
        }
        Ok(())
    }
}

/// 焦点 pane（M14-1）：左 = 活动工作区（完整能力）；右 = 导航 pane。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    #[default]
    Left,
    Right,
}

/// 每标签独立状态：位置、条目、光标、滚动、搜索、历史（SPEC §4.2）。
/// `visual` 为可视选择锚点（M3-1）；`tags` 为文件标记（M3-2）；`scroll` 为
/// 显式偏移预留（视口滚动当前由 `ListState` 从 cursor 派生，显式存储待 M10）。
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Tab {
    pub location: Location,
    pub entries: Vec<FileItem>,
    pub cursor: usize,
    pub scroll: usize,
    pub search: Option<SearchState>,
    pub history: Vec<Location>,
    /// 可视选择锚点（`v` 进入时的 cursor；None = 非可视模式，M3-1）。
    pub visual: Option<usize>,
    /// glob 过滤模式（M4-2，`=` 设置；None = 不过滤）。
    pub filter: Option<String>,
    /// `t` 标记的文件名集合（tag：文件级标记，供 M3 批量操作。
    /// **tag ≠ mark**——mark 是路径跳转标记，本项目不实现，见 SPEC OUT_OF_SCOPE）。
    pub tags: HashSet<String>,
    /// 首字母快速搜索状态（M13-1）：上次 (字符, 正向?)；分号同向重复、
    /// 逗号反向重复。
    pub char_search: Option<(char, bool)>,
    /// 树视图模式（M13-7）：Some(depth) = 当前列表为当前目录的静态树
    /// （只读浏览，`:tree` 再执行恢复平铺）。
    pub tree: Option<u32>,
    /// view 模式（M13-9/M14-1）：右 pane 显示活动 pane 光标文件的完整
    /// 内容预览（false = 右 pane 显示目录列表）。
    pub view_mode: bool,
    /// 右 pane（M14-1）：独立目录列表（独立 Location/光标/滚动）。
    pub right: Pane,
    /// 焦点 pane（M14-1）：Tab 键切换；导航与文件操作作用于焦点 pane。
    pub focus: Focus,
    /// FEAT-2：焦点在列表顶部 `..` 虚拟行（渲染层条目，不占用 cursor 空间、
    /// 不进入 entries/搜索/过滤/标记集合）。`l`/Enter = return_parent；
    /// 写操作被守卫拒绝。
    pub on_parent_row: bool,
    /// M16 NAV-3：当前视图在 `history` 中的下标（history 从 M16 起为
    /// **完整访问轨迹**，含当前视图；新导航截断 forward）。
    pub hist_pos: usize,
    /// M17 SRCH：搜索结果虚拟目录（:find/:grep；沿 :tree 同构——静态
    /// 条目 + 只读守卫，items/entries 平行表）。
    pub results: Option<crate::search::SearchResults>,
    /// M17 VIEW-1：交互式查看器状态（view_mode 激活时右 pane）。
    pub viewer: Option<ViewerState>,
}

/// M17 VIEW-1：查看器状态（行号滚动 + 搜索定位）。
#[derive(Debug, Clone, Default)]
pub struct ViewerState {
    /// 视口首行（0-based）。
    pub scroll: usize,
    /// 文件总行数（激活时一次性读取，滚动钳制用）。
    pub line_count: usize,
    /// `/` 搜索词（None = 未搜索）。
    pub query: Option<String>,
    /// 当前匹配行（0-based）。
    pub match_line: usize,
}

impl Tab {
    /// 从物理目录构建标签：按配置加载并排序条目（M5-2 起 options 生效）。
    pub fn from_physical(dir: &Path, config: Config) -> std::io::Result<Self> {
        Ok(Self {
            location: Location::Physical(dir.to_path_buf()),
            entries: load_entries(dir, &config)?,
            cursor: 0,
            scroll: 0,
            search: None,
            history: vec![Location::Physical(dir.to_path_buf())],
            hist_pos: 0,
            results: None,
            viewer: None,
            visual: None,
            filter: None,
            tags: HashSet::new(),
            char_search: None,
            tree: None,
            view_mode: false,
            // M14-1：右 pane 默认同目录（vifm 启动行为），独立导航。
            right: Pane::from_physical(dir, config)?,
            focus: Focus::Left,
            on_parent_row: false,
        })
    }

    /// 任意 Location 的条目加载（NAV-3：历史跳转后恢复条目列表用）。
    fn load_for(location: &Location, config: Config) -> std::io::Result<Vec<FileItem>> {
        match location {
            Location::Physical(p) => load_entries(p, &config),
            Location::Archive { host, inner } => {
                load_archive_entries(host, &inner.to_string_lossy())
            }
        }
    }

    /// M16 NAV-3：导航到新视图——截断 forward（游标之后全部丢弃），
    /// 追加并落位。cursor/scroll/`..` 焦点重置，条目按新位置加载。
    /// 注意：进入目录/归档后需调用方补 `on_parent_row = has_parent_row()`
    /// （落在 ".." 行的 vifm 语义）。
    pub fn navigate_to(&mut self, new: Location, config: Config) -> std::io::Result<()> {
        // 先加载，成功才变更状态（损坏归档等失败路径无副作用，M7-4 契约）。
        let entries = Self::load_for(&new, config)?;
        self.history.truncate(self.hist_pos + 1);
        self.history.push(new.clone());
        self.hist_pos = self.history.len() - 1;
        self.location = new;
        self.entries = entries;
        self.cursor = 0;
        self.scroll = 0;
        self.on_parent_row = false;
        Ok(())
    }

    /// M16 NAV-2：从历史移除第 idx 条（菜单内 dd）。当前视图不可移除；
    /// 移除游标之前的条目时 hist_pos 前移。返回是否移除。
    pub fn history_remove(&mut self, idx: usize) -> bool {
        if idx >= self.history.len() || idx == self.hist_pos {
            return false;
        }
        self.history.remove(idx);
        if idx < self.hist_pos {
            self.hist_pos -= 1;
        }
        true
    }

    /// M16 NAV-3：`histprev`/`histnext`——沿当前 tab 的视图历史后退/前进。
    /// 返回是否移动；边界（无更早/更新）返回 false 且不产生副作用。
    pub fn hist_jump(&mut self, forward: bool, config: Config) -> std::io::Result<bool> {
        let new_pos = if forward {
            self.hist_pos + 1
        } else {
            self.hist_pos.saturating_sub(1)
        };
        if new_pos >= self.history.len() || (!forward && self.hist_pos == 0) {
            return Ok(false);
        }
        let loc = self.history[new_pos].clone();
        let entries = Self::load_for(&loc, config)?;
        self.hist_pos = new_pos;
        self.location = loc;
        self.entries = entries;
        self.cursor = 0;
        self.scroll = 0;
        self.on_parent_row = false;
        Ok(true)
    }

    /// FEAT-2：列表顶部是否渲染 `..` 行——物理目录存在父目录时显示；
    /// 归档视图恒显示（`..`/`h` 返回归档文件所在物理目录，M7-5）。
    pub fn has_parent_row(&self) -> bool {
        match &self.location {
            Location::Physical(p) => p
                .parent()
                .is_some_and(|parent| !parent.as_os_str().is_empty()),
            Location::Archive { .. } => true,
        }
    }

    /// 焦点 pane 的基准目录（M14-1）：exec 写命令作用于焦点 pane。
    pub fn focus_base(&self) -> &Path {
        match self.focus {
            Focus::Right => self.right.location.base_dir(),
            Focus::Left => self.location.base_dir(),
        }
    }

    /// 光标移动（M1-1）：`delta` 为正向下。空目录为 no-op，两端钳制不越界。
    pub fn move_cursor(&mut self, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        // FEAT-2：Ctrl-u/d/滚轮离开 `..` 行；向上停留不动。
        if self.on_parent_row {
            if delta < 0 {
                return;
            }
            self.on_parent_row = false;
        }
        let max = self.entries.len() as isize - 1;
        let next = self.cursor as isize + delta;
        self.cursor = next.clamp(0, max) as usize;
    }

    /// 光标环绕移动（M13-4，`j`/`k`）：尾行向下环绕到首行、首行向上环绕到
    /// 尾行（官方 issue #874）；`delta` 仅支持 ±1，其余回退钳制语义。
    /// 空目录为 no-op。
    pub fn move_cursor_wrap(&mut self, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        // FEAT-2：k 在首条目时上到 `..` 行（有父行时替代"首→尾"环绕）；
        // j 从 `..` 行回首条目。
        if delta == -1 {
            if self.on_parent_row {
                return; // 已在 `..` 行，停留。
            }
            if self.cursor == 0 && self.has_parent_row() {
                self.on_parent_row = true;
                return;
            }
        }
        if delta == 1 && self.on_parent_row {
            self.on_parent_row = false;
            return;
        }
        let len = self.entries.len();
        match delta {
            1 => self.cursor = (self.cursor + 1) % len,
            -1 => self.cursor = (self.cursor + len - 1) % len,
            _ => self.move_cursor(delta),
        }
    }

    /// 跳到首项（M1-3，`gg`）。FEAT-2：gg 落在 `..` 行（vifm 语义）。
    pub fn cursor_top(&mut self) {
        self.cursor = 0;
        self.on_parent_row = self.has_parent_row();
    }

    /// 跳到末项（M1-3，`G`）。空目录停在 0。
    pub fn cursor_bottom(&mut self) {
        self.cursor = self.entries.len().saturating_sub(1);
        self.on_parent_row = false;
    }

    /// 环绕查找匹配条目（M1-7）：大小写不敏感子串；`start` 为起始扫描索引。
    pub fn find_match(&self, query: &str, start: usize, forward: bool) -> Option<usize> {
        if self.entries.is_empty() || query.is_empty() {
            return None;
        }
        let len = self.entries.len();
        for i in 0..len {
            let idx = if forward {
                (start + i) % len
            } else {
                (start + len - i % len) % len
            };
            if query_matches(&self.entries[idx].name, query) {
                return Some(idx);
            }
        }
        None
    }

    /// 首字母快速搜索（M13-1）：从 `start` 沿方向查找**文件名首字符**为
    /// `c` 的条目（环绕列表首尾，官方 f/F 语义）。区分大小写。
    pub fn find_first_char(&self, c: char, start: usize, forward: bool) -> Option<usize> {
        if self.entries.is_empty() {
            return None;
        }
        let len = self.entries.len();
        for i in 0..len {
            let idx = if forward {
                (start + i) % len
            } else {
                (start + len - i % len) % len
            };
            if self.entries[idx].name.starts_with(c) {
                return Some(idx);
            }
        }
        None
    }

    /// 跳到下一个首字符匹配（M13-1）：更新 `char_search` 状态；
    /// 无匹配时光标不动并返回 `false`。
    pub fn jump_first_char(&mut self, c: char, forward: bool) -> bool {
        self.char_search = Some((c, forward));
        let start = if forward {
            self.cursor + 1
        } else {
            self.cursor + self.entries.len().saturating_sub(1)
        };
        match self.find_first_char(c, start, forward) {
            Some(idx) => {
                self.cursor = idx;
                self.on_parent_row = false;
                true
            }
            None => false,
        }
    }

    /// 从当前位置沿 `forward` 方向移动到下一个匹配（M1-7，`n`/`N`）。
    /// 未找到时 cursor 不变并返回 `false`。
    pub fn search_move(&mut self, query: &str, forward: bool) -> bool {
        let start = if forward {
            self.cursor + 1
        } else {
            self.cursor + self.entries.len().saturating_sub(1)
        };
        match self.find_match(query, start, forward) {
            Some(idx) => {
                self.cursor = idx;
                self.on_parent_row = false;
                true
            }
            None => false,
        }
    }

    /// 搜索匹配计数（FIX-1）：`(当前匹配序号, 总匹配数)`；光标项不匹配时
    /// 序号为 0（调用方据此显示 `i/N` 或 `0/0`）。
    pub fn search_status(&self, query: &str) -> (usize, usize) {
        let matched = |item: &FileItem| query_matches(&item.name, query);
        let total = self.entries.iter().filter(|item| matched(item)).count();
        let cur = match self.entries.get(self.cursor) {
            Some(item) if matched(item) => self.entries[..=self.cursor]
                .iter()
                .filter(|item| matched(item))
                .count(),
            _ => 0,
        };
        (cur, total)
    }

    /// 重读当前物理目录条目并钳制光标（删除/撤销后刷新，M2-2）。
    /// 归档位置为 no-op（M7 接入）。
    pub fn refresh(&mut self, config: Config) -> std::io::Result<()> {
        let Location::Physical(dir) = &self.location else {
            return Ok(());
        };
        let mut entries = load_entries(dir, &config)?;
        if let Some(pat) = &self.filter {
            entries.retain(|item| glob_match(pat, &item.name));
        }
        self.entries = entries;
        self.cursor = self.cursor.min(self.entries.len().saturating_sub(1));
        self.visual = self
            .visual
            .map(|a| a.min(self.entries.len().saturating_sub(1)));
        Ok(())
    }

    /// 应用/清除 glob 过滤（M4-2，`=`）。空 pattern 清除过滤；应用后光标与选区归零。
    pub fn apply_filter(&mut self, pattern: &str, config: Config) {
        let pat = pattern.trim().to_owned();
        self.filter = if pat.is_empty() { None } else { Some(pat) };
        if self.refresh(config).is_ok() {
            self.cursor = 0;
            self.visual = None;
        }
    }

    /// 进入/退出可视选择（M3-1，`v`）。空目录为 no-op。
    pub fn toggle_visual(&mut self) {
        // FEAT-2：`..` 行不可作为可视选择起点。
        if self.on_parent_row {
            return;
        }
        self.visual = match self.visual {
            Some(_) => None,
            None if !self.entries.is_empty() => Some(self.cursor),
            None => None,
        };
    }

    /// 退出可视选择（M3-1，Esc）。
    pub fn exit_visual(&mut self) {
        self.visual = None;
    }

    /// 可视选择范围（含锚点与光标的闭区间）；非可视模式返回 None。
    pub fn visual_range(&self) -> Option<(usize, usize)> {
        self.visual
            .map(|a| (a.min(self.cursor), a.max(self.cursor)))
    }

    /// 切换光标所在条目的 tag 标记（M3-2，`t`）。空目录为 no-op。
    /// tag 按文件名记录；目录重读后仍按名匹配（改名/删除的条目在批量操作时跳过）。
    pub fn toggle_tag(&mut self) {
        // FEAT-2：`..` 行无 tag 语义。
        if self.on_parent_row {
            return;
        }
        if let Some(item) = self.entries.get(self.cursor) {
            let name = item.name.clone();
            if !self.tags.remove(&name) {
                self.tags.insert(name);
            }
        }
    }

    /// 已标记数量（M3-2 状态栏与 M3-3 批量操作用）。
    pub fn tagged_count(&self) -> usize {
        self.tags.len()
    }

    /// 清空全部标记（M3-4：批量操作完成后调用；`:unselect` 无参同此语义）。
    pub fn clear_tags(&mut self) {
        self.tags.clear();
    }

    /// 按 pattern 批量选择/取消（M13-2，对齐 vifm `:select`/`:unselect`）：
    /// glob 匹配当前列表条目名，`select` 加入 tag 集合、`unselect` 移除。
    /// 返回本次受影响的条目数（用于状态栏提示）。
    /// pattern 缺失 = 仅当前光标条目（对齐官方无 range 时选当前文件）。
    pub fn select_by_pattern(&mut self, pattern: Option<&str>, select: bool) -> usize {
        let mut affected = 0;
        match pattern {
            Some(pat) => {
                for item in &self.entries {
                    if crate::tab::glob_match(pat, &item.name) {
                        if select {
                            if self.tags.insert(item.name.clone()) {
                                affected += 1;
                            }
                        } else if self.tags.remove(&item.name) {
                            affected += 1;
                        }
                    }
                }
            }
            None => {
                if let Some(item) = self.entries.get(self.cursor) {
                    let name = item.name.clone();
                    if select {
                        if self.tags.insert(name) {
                            affected += 1;
                        }
                    } else if self.tags.remove(&name) {
                        affected += 1;
                    }
                }
            }
        }
        affected
    }

    /// 构建静态树条目（M13-7）：以 `root` 为根递归收集 `depth` 层内的
    /// 条目，`name` 为相对路径（渲染按层缩进）；目录优先 + 字典序。
    /// 条目总数超过 `MAX_TREE_ITEMS` 截断（防超大目录爆炸）。
    /// SIMPLIFIED：静态快照（不做 FS 状态自动同步），树模式为只读浏览。
    pub fn build_tree_entries(root: &Path, depth: u32) -> std::io::Result<Vec<FileItem>> {
        const MAX_TREE_ITEMS: usize = 10_000;
        let mut out = Vec::new();
        let mut truncated = false;
        fn walk(
            dir: &Path,
            prefix: &str,
            level: u32,
            depth: u32,
            out: &mut Vec<FileItem>,
            truncated: &mut bool,
        ) {
            if *truncated {
                return;
            }
            let mut items: Vec<(String, std::fs::FileType)> = match std::fs::read_dir(dir) {
                Ok(rd) => rd
                    .filter_map(|e| e.ok())
                    .filter_map(|e| {
                        let ft = e.file_type().ok()?;
                        Some((e.file_name().to_string_lossy().into_owned(), ft))
                    })
                    .collect(),
                Err(_) => return,
            };
            items.sort_by(|a, b| {
                let ad = a.1.is_dir();
                let bd = b.1.is_dir();
                bd.cmp(&ad)
                    .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
            });
            for (name, ft) in items {
                if *truncated {
                    return;
                }
                let rel = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}/{name}")
                };
                let full = dir.join(&name);
                let is_dir = ft.is_dir();
                if level < depth {
                    out.push(FileItem {
                        icon: crate::icons::icon_for_active(&full),
                        name: rel.clone(),
                        is_dir,
                        size: if is_dir {
                            0
                        } else {
                            std::fs::metadata(&full).map(|m| m.len()).unwrap_or(0)
                        },
                        perm: String::new(),
                    });
                    if out.len() >= MAX_TREE_ITEMS {
                        *truncated = true;
                        return;
                    }
                    if is_dir {
                        walk(&full, &rel, level + 1, depth, out, truncated);
                    }
                }
            }
        }
        walk(root, "", 0, depth, &mut out, &mut truncated);
        Ok(out)
    }

    /// 切换 view 模式（M13-9）：预览窗格在摘要预览与完整内容视图间切换。
    /// 返回切换后的状态（true = 完整视图开启）。
    pub fn toggle_view(&mut self) -> bool {
        self.view_mode = !self.view_mode;
        if self.view_mode {
            // M17 VIEW-1：激活查看器；grep 命中定位到命中行（上移 2 行上下文）。
            let hit_line = self
                .results
                .as_ref()
                .and_then(|r| r.items.get(self.cursor))
                .and_then(|i| i.line);
            let line_count = self.viewer_lines().map(|l| l.len()).unwrap_or(0);
            let scroll = hit_line
                .map(|l| l.saturating_sub(2))
                .unwrap_or(0)
                .min(line_count.saturating_sub(1));
            self.viewer = Some(ViewerState {
                scroll,
                line_count,
                query: None,
                match_line: 0,
            });
        } else {
            self.viewer = None;
        }
        self.view_mode
    }

    /// M17 VIEW-1：查看器内容行（激活时一次性读取；上限 10 万行）。
    pub fn viewer_lines(&self) -> Option<Vec<String>> {
        let path = self.preview_path()?;
        let text = std::fs::read_to_string(path).ok()?;
        Some(text.lines().take(100_000).map(|l| l.to_owned()).collect())
    }

    /// M17 VIEW-1：查看器滚动（delta 行，钳制到内容范围）。
    pub fn viewer_scroll(&mut self, delta: isize, half: usize) {
        let Some(v) = self.viewer.as_mut() else {
            return;
        };
        let max = v.line_count.saturating_sub(1);
        let delta = match delta {
            i if i.abs() > 1 => {
                let sign = if i < 0 { -1 } else { 1 };
                sign * half as isize
            }
            i => i,
        };
        let next = v.scroll as isize + delta;
        v.scroll = next.clamp(0, max as isize) as usize;
    }

    /// M17 VIEW-1：查看器跳顶/跳底（gg/G）。
    pub fn viewer_goto(&mut self, top: bool) {
        let Some(v) = self.viewer.as_mut() else {
            return;
        };
        v.scroll = if top {
            0
        } else {
            v.line_count.saturating_sub(1)
        };
    }

    /// M17 VIEW-1：查看器搜索提交——定位第一个匹配（自当前滚动行起，
    /// 环绕）；空查询清除。
    pub fn viewer_search_commit(&mut self, query: &str) {
        if query.is_empty() {
            if let Some(v) = self.viewer.as_mut() {
                v.query = None;
            }
            return;
        }
        let Some(lines) = self.viewer_lines() else {
            return;
        };
        let Some(v) = self.viewer.as_mut() else {
            return;
        };
        v.query = Some(query.to_owned());
        let lower = query.to_lowercase();
        let start = v.scroll;
        let len = lines.len();
        let mut found = None;
        for i in 0..len {
            let idx = (start + i) % len;
            if lines[idx].to_lowercase().contains(&lower) {
                found = Some(idx);
                break;
            }
        }
        if let Some(idx) = found {
            v.match_line = idx;
            v.scroll = idx.saturating_sub(2);
        }
    }

    /// M17 VIEW-1：查看器 n/N——上一/下一匹配（环绕）。
    pub fn viewer_jump_match(&mut self, forward: bool) -> bool {
        let Some(v) = self.viewer.clone() else {
            return false;
        };
        let Some(query) = &v.query else { return false };
        let Some(lines) = self.viewer_lines() else {
            return false;
        };
        if lines.is_empty() {
            return false;
        }
        let lower = query.to_lowercase();
        let len = lines.len();
        let start = if forward {
            v.match_line + 1
        } else {
            v.match_line + len - 1
        };
        for i in 0..len {
            let idx = (start + i) % len;
            if lines[idx].to_lowercase().contains(&lower) {
                if let Some(vv) = self.viewer.as_mut() {
                    vv.match_line = idx;
                    vv.scroll = idx.saturating_sub(2).min(len.saturating_sub(1));
                }
                return true;
            }
        }
        false
    }

    /// 进入树视图（M13-7）：记录深度并把 entries 替换为静态树。
    pub fn enter_tree(&mut self, depth: u32) -> std::io::Result<()> {
        let base = self.location.base_dir().to_path_buf();
        let entries = Self::build_tree_entries(&base, depth)?;
        self.entries = entries;
        self.tree = Some(depth);
        self.cursor = 0;
        self.scroll = 0;
        Ok(())
    }

    /// 退出树视图（M13-7）：恢复平铺目录列表。
    pub fn exit_tree(&mut self) -> std::io::Result<()> {
        self.tree = None;
        let cfg = Config::default();
        let _ = self.refresh(cfg);
        Ok(())
    }

    /// 树视图守卫（M13-7 只读边界）：树模式下拒绝写操作/导航操作。
    pub fn in_tree(&self) -> bool {
        self.tree.is_some()
    }

    /// M17 SRCH-1：进入搜索结果虚拟目录——items/entries 平行表，
    /// entries 的 name 为显示串（find=全路径；grep=路径:行号: 内容）。
    pub fn enter_results(
        &mut self,
        kind: crate::search::SearchKind,
        query: String,
        root: PathBuf,
        items: Vec<crate::search::ResultItem>,
        truncated: bool,
    ) {
        self.entries = items
            .iter()
            .map(|item| {
                // 显示层统一 /（AGENTS §7；Windows display 为反斜杠）。
                let shown = item.path.display().to_string().replace('\\', "/");
                let name = match item.line {
                    Some(line) => format!("{}:{}: {}", shown, line, item.text),
                    None => shown,
                };
                FileItem {
                    icon: crate::icons::icon_for(&item.path, crate::icons::IconTheme::Dark),
                    name,
                    is_dir: false,
                    size: 0,
                    perm: String::new(),
                }
            })
            .collect();
        self.results = Some(crate::search::SearchResults {
            kind,
            query,
            root,
            truncated,
            items,
        });
        self.cursor = 0;
        self.scroll = 0;
        self.on_parent_row = false;
        self.tree = None;
    }

    /// M17 SRCH-1：退出结果视图——恢复原目录列表。
    pub fn exit_results(&mut self, config: Config) {
        self.results = None;
        let _ = self.refresh(config);
    }

    /// M17 SRCH：结果虚拟目录守卫（只读边界）。
    pub fn in_results(&self) -> bool {
        self.results.is_some()
    }

    /// M17 SRCH：预览/操作的目标文件路径——结果模式取命中项，
    /// 物理视图取 base.join(name)；归档视图 None（预览未接入）。
    pub fn preview_path(&self) -> Option<PathBuf> {
        if let Some(r) = &self.results {
            return r.items.get(self.cursor).map(|i| i.path.clone());
        }
        match &self.location {
            Location::Physical(base) => self.entries.get(self.cursor).map(|i| base.join(&i.name)),
            Location::Archive { .. } => None,
        }
    }

    /// M17 SRCH-1：定位当前命中文件——退出结果视图 + 导航到其父目录
    /// + 光标停在该文件（navigate_to 入历史）。
    pub fn locate_result(&mut self, config: Config) -> std::io::Result<()> {
        let Some(path) = self.result_path_at(self.cursor) else {
            return Ok(());
        };
        let parent = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.results = None;
        self.navigate_to(Location::Physical(parent), config)?;
        if let Some(idx) = self.entries.iter().position(|i| i.name == name) {
            self.cursor = idx;
        }
        Ok(())
    }

    /// M17 SRCH：结果条目的文件路径。
    pub fn result_path_at(&self, cursor: usize) -> Option<PathBuf> {
        self.results
            .as_ref()?
            .items
            .get(cursor)
            .map(|i| i.path.clone())
    }

    /// 进入光标选中的条目（M1-2 目录；M7-4 归档虚拟目录）。
    /// `l`/`Enter`。目录重读即刷新；读取失败时返回 Err 且不产生副作用
    /// （错误经 M6-1 提示进状态栏）。普通文件 no-op（打开走 M11 opener）。
    pub fn enter_selected(&mut self, config: Config) -> std::io::Result<()> {
        let Some(item) = self.entries.get(self.cursor) else {
            return Ok(());
        };
        let name = item.name.clone();
        let is_dir = item.is_dir;
        match self.location.clone() {
            Location::Physical(base) => {
                let target = base.join(&name);
                if is_dir {
                    self.navigate_to(Location::Physical(target), config)?;
                    // FEAT-2：进入目录后光标落在 `..` 行（vifm 语义）。
                    self.on_parent_row = self.has_parent_row();
                    return Ok(());
                }
                // 归档文件：进入虚拟根目录（M7-4）；不支持的格式给出解压提示（M7-9）。
                if crate::archive::is_unsupported_archive(&name) {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Unsupported,
                        "该归档格式不支持虚拟浏览，请解压后浏览",
                    ));
                }
                if crate::archive::is_archive_file(&name) {
                    self.navigate_to(
                        Location::Archive {
                            host: target,
                            inner: PathBuf::new(),
                        },
                        config,
                    )?;
                    // FEAT-2：进入归档后光标落在 `..` 行。
                    self.on_parent_row = self.has_parent_row();
                    return Ok(());
                }
                // 普通文件：由调用方（handle_key M11-7）决定是否调用系统 opener。
                Ok(())
            }
            Location::Archive { host, inner } => {
                if !is_dir {
                    return Ok(()); // 归档内文件：只读，打开在 M11 接入
                }
                let inner_str = inner.to_string_lossy().into_owned();
                let new_inner = if inner_str.is_empty() {
                    name
                } else {
                    format!("{inner_str}/{name}")
                };
                self.navigate_to(
                    Location::Archive {
                        host,
                        inner: PathBuf::from(crate::archive::normalize_inner(&new_inner)),
                    },
                    config,
                )?;
                self.on_parent_row = self.has_parent_row();
                Ok(())
            }
        }
    }

    /// 返回父目录（M1-2，`h`）。
    /// 物理目录：已在文件系统根时为 no-op（ACCEPTANCE §3）。
    /// 归档内（M7-4）：上一层虚拟目录；归档根处返回宿主目录。
    /// 读取失败时返回 Err 且不产生副作用（提示经 M6-1 进状态栏）。
    pub fn return_parent(&mut self, config: Config) -> std::io::Result<()> {
        match self.location.clone() {
            Location::Archive { host, inner } => {
                let inner_str = inner.to_string_lossy().into_owned();
                if inner_str.is_empty() {
                    // 归档根：`h` 返回归档文件所在的物理目录（host 的父目录）。
                    let host_dir = host.parent().unwrap_or(host.as_path()).to_path_buf();
                    self.navigate_to(Location::Physical(host_dir), config)?;
                    return Ok(());
                }
                // 归档内：上一层虚拟目录（parent 为空串 = 归档根，仍留在归档内）。
                let parent = crate::archive::normalize_inner(
                    inner_str.rsplit_once('/').map(|(p, _)| p).unwrap_or(""),
                );
                self.navigate_to(
                    Location::Archive {
                        host,
                        inner: PathBuf::from(parent),
                    },
                    config,
                )?;
                Ok(())
            }
            Location::Physical(base) => {
                let Some(parent) = base.parent() else {
                    return Ok(());
                };
                if parent == base {
                    return Ok(());
                }
                self.navigate_to(Location::Physical(parent.to_path_buf()), config)?;
                Ok(())
            }
        }
    }
}

/// 列出归档虚拟目录条目（M7-4）：`ArchiveEntry` → `FileItem`
/// （名字取叶子段；perm 在归档视图留空）。打开失败（损坏/加密）原样上抛。
fn load_archive_entries(host: &Path, inner: &str) -> std::io::Result<Vec<FileItem>> {
    let fs = crate::archive::open_dispatch(host)?;
    let entries = fs.list(&crate::archive::normalize_inner(inner))?;
    Ok(entries
        .into_iter()
        .map(|e| {
            let name = e
                .path
                .rsplit_once('/')
                .map(|(_, l)| l.to_owned())
                .unwrap_or(e.path.clone());
            // 归档内路径为虚拟路径：icon 按叶子名/目录推断。
            let icon_path = if e.is_dir {
                format!("/{}/", name)
            } else {
                format!("/{name}")
            };
            let icon = crate::icons::icon_for_active(Path::new(&icon_path));
            FileItem {
                icon,
                name,
                is_dir: e.is_dir,
                size: e.size,
                perm: String::new(),
            }
        })
        .collect())
}

/// 智能大小写匹配（M4-1）：查询含大写字母时区分大小写，否则不区分（vim/vifm smartcase）。
fn query_matches(name: &str, query: &str) -> bool {
    if query.chars().any(char::is_uppercase) {
        name.contains(query)
    } else {
        name.to_lowercase().contains(query)
    }
}

/// `name` 中查询子串的字节区间（FIX-1 渲染高亮用；smart-case 同 M4-1）。
/// 查询为空返回 `None`；不区分大小写匹配若改变字节布局（非常规 Unicode
/// 大小写映射）则放弃高亮，返回 `None`（匹配判定仍以 `query_matches` 为准）。
pub fn match_range(name: &str, query: &str) -> Option<(usize, usize)> {
    if query.is_empty() {
        return None;
    }
    if query.chars().any(char::is_uppercase) {
        return name.find(query).map(|pos| (pos, pos + query.len()));
    }
    let lower_name = name.to_lowercase();
    let lower_query = query.to_lowercase();
    let pos = lower_name.find(&lower_query)?;
    (lower_name.len() == name.len()).then_some((pos, pos + lower_query.len()))
}

/// 简单 glob 匹配（M4-2）：支持 `*`（任意序列）与 `?`（单字符），不区分大小写。
pub(crate) fn glob_match(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().flat_map(char::to_lowercase).collect();
    let s: Vec<char> = name.chars().flat_map(char::to_lowercase).collect();
    fn m(p: &[char], s: &[char]) -> bool {
        if p.is_empty() {
            return s.is_empty();
        }
        match p[0] {
            '*' => (0..=s.len()).any(|i| m(&p[1..], &s[i..])),
            '?' => !s.is_empty() && m(&p[1..], &s[1..]),
            c => !s.is_empty() && s[0] == c && m(&p[1..], &s[1..]),
        }
    }
    m(&p, &s)
}

/// 读取并排序目录条目：按配置决定隐藏文件与排序（M5-2）。
fn load_entries(dir: &Path, config: &Config) -> std::io::Result<Vec<FileItem>> {
    let mut items = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let is_dir = entry.file_type()?.is_dir();
        let name = entry.file_name().to_string_lossy().into_owned();
        // show_hidden = false 时跳过 `.` 开头的隐藏条目。
        if !config.show_hidden && name.starts_with('.') {
            continue;
        }
        let meta = entry.metadata()?;
        let size = meta.len();
        let perm = perm_string(&meta);
        let icon = crate::icons::icon_for_active(&entry.path());
        items.push(FileItem {
            icon,
            name,
            is_dir,
            size,
            perm,
        });
    }
    items.sort_by(|a, b| {
        let dir_cmp = if config.sort_dirs_first {
            b.is_dir.cmp(&a.is_dir)
        } else {
            std::cmp::Ordering::Equal
        };
        let name_cmp = if config.ignore_case_sort {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        } else {
            a.name.cmp(&b.name)
        };
        dir_cmp.then(name_cmp)
    });
    Ok(items)
}

/// 权限摘要：unix 用 3 位八进制；Windows 仅区分只读。
#[cfg(unix)]
pub(crate) fn perm_string(meta: &std::fs::Metadata) -> String {
    use std::os::unix::fs::PermissionsExt;
    format!("{:03o}", meta.permissions().mode() & 0o777)
}

/// 权限摘要：Windows 仅区分只读。
#[cfg(windows)]
pub(crate) fn perm_string(meta: &std::fs::Metadata) -> String {
    if meta.permissions().readonly() {
        "ro".to_owned()
    } else {
        "rw".to_owned()
    }
}

/// 字节数格式化：`B` / `KiB` / `MiB` / `GiB`（1 位小数）。
pub(crate) fn format_size(size: u64) -> String {
    const KIB: f64 = 1024.0;
    let s = size as f64;
    if size < 1024 {
        format!("{size} B")
    } else if size < 1024_u64.pow(2) {
        format!("{:.1} KiB", s / KIB)
    } else if size < 1024_u64.pow(3) {
        format!("{:.1} MiB", s / KIB / KIB)
    } else {
        format!("{:.1} GiB", s / KIB / KIB / KIB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn move_cursor_wrap_round_trips_ends() {
        // M13-4：j/k 环绕——尾行向下回首行、首行向上回尾行；非 ±1 回退钳制。
        let mut tab = Tab::from_physical(std::path::Path::new("."), Config::default()).unwrap();
        tab.entries = vec![
            FileItem {
                name: "a".to_owned(),
                ..make_item("a", false)
            },
            FileItem {
                name: "b".to_owned(),
                ..make_item("b", false)
            },
        ];
        tab.cursor = 1;
        tab.move_cursor_wrap(1);
        assert_eq!(tab.cursor, 0, "尾行向下环绕到首行");
        tab.move_cursor_wrap(-1);
        assert_eq!(tab.cursor, 1, "首行向上环绕到尾行");
        tab.move_cursor(5); // 钳制语义保持（半页等非环绕路径不受影响）
        assert_eq!(tab.cursor, 1);
    }

    #[test]
    fn find_first_char_wraps_and_respects_direction() {
        // M13-1：f/F 首字母搜索——环绕列表首尾；区分方向。
        let mut tab = Tab::from_physical(std::path::Path::new("."), Config::default()).unwrap();
        tab.entries = vec![
            FileItem {
                name: "alpha".to_owned(),
                ..make_item("alpha", true)
            },
            FileItem {
                name: "beta".to_owned(),
                ..make_item("beta", false)
            },
            FileItem {
                name: "Beta2".to_owned(),
                ..make_item("Beta2", false)
            },
        ];

        // f 'b' 正向：从 0 出发 → 1（beta）。
        assert_eq!(tab.find_first_char('b', 0, true), Some(1));
        // 区分大小写：'B' 正向 → 2（Beta2），跳过 beta。
        assert_eq!(tab.find_first_char('B', 0, true), Some(2));
        // 环绕：从 2 正向找 'a' → 0（alpha）。
        assert_eq!(tab.find_first_char('a', 2, true), Some(0));
        // F 反向：从 1 出发找 'a' → 0。
        assert_eq!(tab.find_first_char('a', 1, false), Some(0));
        // 无匹配。
        assert_eq!(tab.find_first_char('z', 0, true), None);
        // jump_first_char 更新状态并移动光标。
        assert!(tab.jump_first_char('b', true));
        assert_eq!(tab.cursor, 1);
        assert_eq!(tab.char_search, Some(('b', true)));
    }

    fn make_item(name: &str, is_dir: bool) -> FileItem {
        FileItem {
            icon: crate::icons::icon_for(std::path::Path::new(name), crate::icons::IconTheme::Dark),
            name: name.to_owned(),
            is_dir,
            size: 0,
            perm: String::new(),
        }
    }

    #[test]
    fn archive_navigation_roundtrip() {
        // M7-4：l 进 zip 虚拟目录 → 进子目录 → h 逐层返回 → 根处 h 回宿主目录。
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("t.zip");
        crate::archive::make_test_zip(&zip_path);
        fs::write(tmp.path().join("host.txt"), "H").unwrap();
        let mut tab = Tab::from_physical(tmp.path(), Config::default()).unwrap();

        // 排序：host.txt(0) / t.zip(1)。
        tab.cursor = 1;
        tab.enter_selected(Config::default()).unwrap();
        eprintln!(
            "DBG loc={:?} names={:?}",
            tab.location,
            tab.entries
                .iter()
                .map(|i| (&i.name, i.is_dir))
                .collect::<Vec<_>>()
        );
        match &tab.location {
            Location::Archive { host, inner } => {
                assert_eq!(host, &zip_path);
                assert!(inner.as_os_str().is_empty(), "根 inner 为空");
            }
            other => panic!("expected Archive, got {other:?}"),
        }
        let names: Vec<&str> = tab.entries.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["deep", "readme.md", "src"]);

        // 进入 src/ 虚拟子目录。
        tab.cursor = 2;
        tab.enter_selected(Config::default()).unwrap();
        match &tab.location {
            Location::Archive { inner, .. } => {
                assert_eq!(inner, &PathBuf::from("src"))
            }
            other => panic!("expected Archive, got {other:?}"),
        }
        let names: Vec<&str> = tab.entries.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["main.rs"]);

        // h → 回归档根；h → 回宿主目录。
        tab.return_parent(Config::default()).unwrap();
        assert_eq!(
            tab.location,
            Location::Archive {
                host: zip_path.clone(),
                inner: PathBuf::new()
            }
        );
        tab.return_parent(Config::default()).unwrap();
        assert_eq!(tab.location, Location::Physical(tmp.path().to_path_buf()));
        assert!(tab.entries.iter().any(|i| i.name == "host.txt"));
    }

    #[test]
    fn corrupt_archive_enter_returns_error() {
        // M7-4：损坏归档进入返回 Err（M6-1 提示进状态栏），位置不变。
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("bad.zip");
        fs::write(&p, b"PK junk bytes").unwrap();
        let mut tab = Tab::from_physical(tmp.path(), Config::default()).unwrap();
        tab.cursor = 0; // bad.zip
        assert!(tab.enter_selected(Config::default()).is_err());
        assert_eq!(tab.location, Location::Physical(tmp.path().to_path_buf()));
    }

    #[test]
    fn physical_display_is_path() {
        let loc = Location::Physical(PathBuf::from("C:/tmp"));
        assert_eq!(loc.display_path(), "C:/tmp");
    }

    #[test]
    fn archive_display_uses_host_inner_form() {
        let loc = Location::Archive {
            host: PathBuf::from("host.zip"),
            inner: PathBuf::from("src/main"),
        };
        assert_eq!(loc.display_path(), "host.zip:/src/main");
    }

    #[test]
    fn from_physical_loads_sorted_entries() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("zdir")).unwrap();
        fs::write(tmp.path().join("apple.txt"), "").unwrap();

        let tab = Tab::from_physical(tmp.path(), Config::default()).unwrap();
        assert_eq!(tab.cursor, 0);
        assert!(tab.search.is_none());
        // M16 NAV-3：history 为完整轨迹（含当前视图），hist_pos 指向末尾。
        assert_eq!(tab.history.len(), 1);
        assert_eq!(tab.hist_pos, 0);
        assert_eq!(tab.location, Location::Physical(tmp.path().to_path_buf()));
        let names: Vec<&str> = tab.entries.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["zdir", "apple.txt"]);
    }

    #[test]
    fn move_cursor_clamps_at_bounds() {
        let tmp = TempDir::new().unwrap();
        for name in ["a", "b", "c"] {
            fs::write(tmp.path().join(name), "").unwrap();
        }
        let mut tab = Tab::from_physical(tmp.path(), Config::default()).unwrap();
        assert_eq!(tab.cursor, 0);
        tab.move_cursor(1);
        assert_eq!(tab.cursor, 1);
        tab.move_cursor(10);
        assert_eq!(tab.cursor, 2); // 末尾钳制
        tab.move_cursor(-10);
        assert_eq!(tab.cursor, 0); // 顶部钳制
        tab.move_cursor(-1);
        assert_eq!(tab.cursor, 0);
    }

    #[test]
    fn move_cursor_is_noop_on_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let mut tab = Tab::from_physical(tmp.path(), Config::default()).unwrap();
        tab.move_cursor(1);
        tab.move_cursor(-1);
        assert_eq!(tab.cursor, 0);
    }

    #[test]
    fn query_matches_smart_case() {
        // 全小写查询：不区分大小写。
        assert!(query_matches("Beta2.txt", "beta"));
        assert!(query_matches("beta.txt", "beta"));
        // 含大写查询：区分大小写。
        assert!(query_matches("Beta2.txt", "Beta"));
        assert!(!query_matches("beta.txt", "Beta"));
        assert!(!query_matches("BETA.txt", "Beta"));
        // 子串语义保持。
        assert!(query_matches("report-2026.txt", "2026"));
    }

    #[test]
    fn format_size_units() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(2048), "2.0 KiB");
        assert_eq!(format_size(1024 * 1024), "1.0 MiB");
        assert_eq!(format_size(3 * 1024 * 1024 * 1024), "3.0 GiB");
    }

    #[test]
    fn entries_carry_size_and_perm() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("hello.txt"), "hello").unwrap(); // 5 字节
        let tab = Tab::from_physical(tmp.path(), Config::default()).unwrap();
        let f = &tab.entries[0];
        assert_eq!(f.name, "hello.txt");
        assert_eq!(f.size, 5);
        // perm 只需与同一元数据的 perm_string 一致；Unix 八进制随 umask
        // 变化（022→644、002→664），不能硬编码。
        let meta = fs::metadata(tmp.path().join("hello.txt")).unwrap();
        assert_eq!(f.perm, perm_string(&meta));
    }

    #[test]
    fn enter_selected_navigates_and_records_history() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        fs::write(tmp.path().join("sub").join("inner.txt"), "").unwrap();
        fs::write(tmp.path().join("file.txt"), "").unwrap();
        let mut tab = Tab::from_physical(tmp.path(), Config::default()).unwrap();

        // 排序后 "sub/" 在第 0 位（目录优先）。
        tab.enter_selected(Config::default()).unwrap();
        assert_eq!(tab.location, Location::Physical(tmp.path().join("sub")));
        let names: Vec<&str> = tab.entries.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["inner.txt"]);
        assert_eq!(tab.cursor, 0);
        // M16 NAV-3：完整轨迹 [tmp, sub]，pos 指向当前（sub）。
        assert_eq!(
            tab.history,
            vec![
                Location::Physical(tmp.path().to_path_buf()),
                Location::Physical(tmp.path().join("sub")),
            ]
        );
        assert_eq!(tab.hist_pos, 1);
    }

    #[test]
    fn enter_selected_ignores_files_and_missing_dirs() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("file.txt"), "").unwrap();
        fs::create_dir(tmp.path().join("gone")).unwrap();
        let mut tab = Tab::from_physical(tmp.path(), Config::default()).unwrap();

        // 光标落在文件上：no-op。
        tab.cursor = 1; // "file.txt"（"gone" 为目录在前）
        tab.enter_selected(Config::default()).unwrap();
        assert_eq!(tab.location, Location::Physical(tmp.path().to_path_buf()));
        assert_eq!(tab.history.len(), 1);

        // 目录在加载后被删除：读取失败返回 Err，不导航（M6-1 错误提示接入）。
        fs::remove_dir(tmp.path().join("gone")).unwrap();
        tab.cursor = 0;
        assert!(tab.enter_selected(Config::default()).is_err());
        assert_eq!(tab.location, Location::Physical(tmp.path().to_path_buf()));
        assert_eq!(tab.history.len(), 1, "失败无副作用");
    }

    #[test]
    fn return_parent_goes_up_and_noop_at_root() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        let mut tab = Tab::from_physical(&tmp.path().join("sub"), Config::default()).unwrap();

        tab.return_parent(Config::default()).unwrap();
        assert_eq!(tab.location, Location::Physical(tmp.path().to_path_buf()));
        // M16 NAV-3：完整轨迹 [sub, tmp]。
        assert_eq!(tab.history.len(), 2);
        assert_eq!(tab.hist_pos, 1);
        // histprev 回 sub；histnext 前进回 tmp；再 prev 到底 false。
        assert!(tab.hist_jump(false, Config::default()).unwrap());
        assert_eq!(tab.location, Location::Physical(tmp.path().join("sub")));
        assert!(tab.hist_jump(true, Config::default()).unwrap());
        assert_eq!(tab.location, Location::Physical(tmp.path().to_path_buf()));
        assert!(!tab.hist_jump(true, Config::default()).unwrap());
        assert!(tab.hist_jump(false, Config::default()).unwrap());
        assert!(!tab.hist_jump(false, Config::default()).unwrap());

        // 根目录：parent() 为 None（Windows 盘根同理），no-op。
        // 根路径平台相关：Windows 盘根 "C:\"，Unix "/"。
        let root = if cfg!(windows) {
            PathBuf::from("C:\\")
        } else {
            PathBuf::from("/")
        };
        let mut root_tab = Tab {
            location: Location::Physical(root.clone()),
            entries: Vec::new(),
            cursor: 0,
            scroll: 0,
            search: None,
            history: Vec::new(),
            hist_pos: 0,
            results: None,
            viewer: None,
            visual: None,
            filter: None,
            tags: HashSet::new(),
            char_search: None,
            tree: None,
            view_mode: false,
            right: Pane::from_physical(&root, Config::default()).unwrap(),
            focus: Focus::Left,
            on_parent_row: false,
        };
        let _ = root_tab.return_parent(Config::default()); // 根目录 no-op 返回 Ok
        assert_eq!(root_tab.location, Location::Physical(root));
        assert!(root_tab.history.is_empty());
    }

    #[test]
    fn nav3_forward_truncated_on_new_navigation() {
        // 嵌套 a/b/c：逐层进入 → 历史四条；后退两步到 a 再进 b → c 被截断。
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("a").join("b").join("c")).unwrap();
        let mut tab = Tab::from_physical(tmp.path(), Config::default()).unwrap();
        for _ in 0..3 {
            tab.cursor = 0; // 唯一条目即下一层目录
            tab.enter_selected(Config::default()).unwrap();
        }
        assert_eq!(tab.history.len(), 4);
        assert_eq!(tab.hist_pos, 3);
        // 后退两步到 a（entries=[b]），再进 b → forward（c）被截断。
        assert!(tab.hist_jump(false, Config::default()).unwrap());
        assert!(tab.hist_jump(false, Config::default()).unwrap());
        assert_eq!(tab.location, Location::Physical(tmp.path().join("a")));
        let b_idx = tab.entries.iter().position(|i| i.name == "b").unwrap();
        tab.cursor = b_idx;
        tab.enter_selected(Config::default()).unwrap();
        assert_eq!(tab.history.len(), 3, "forward 已截断");
        assert_eq!(tab.hist_pos, 2);
        assert!(!tab.hist_jump(true, Config::default()).unwrap(), "无前进");
    }

    #[test]
    fn nav3_history_restores_archive_locations() {
        // 归档位置在历史中往返：条目按归档索引重新加载。
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("t.zip");
        crate::archive::make_test_zip(&zip_path);
        fs::write(tmp.path().join("host.txt"), "H").unwrap();
        let mut tab = Tab::from_physical(tmp.path(), Config::default()).unwrap();
        tab.cursor = tab.entries.iter().position(|i| i.name == "t.zip").unwrap();
        tab.enter_selected(Config::default()).unwrap();
        assert!(matches!(tab.location, Location::Archive { .. }));
        tab.return_parent(Config::default()).unwrap();
        assert!(matches!(tab.location, Location::Physical(_)));
        // histprev 回归档：条目重新从归档索引加载。
        assert!(tab.hist_jump(false, Config::default()).unwrap());
        assert!(matches!(tab.location, Location::Archive { .. }));
        assert!(!tab.entries.is_empty(), "归档条目已恢复");
    }

    #[test]
    fn nav2_history_remove_adjusts_cursor() {
        // M16 NAV-2：dd 移除历史条目——当前视图不可删；删游标前条目时 pos 前移。
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("a").join("b")).unwrap();
        let mut tab = Tab::from_physical(tmp.path(), Config::default()).unwrap();
        tab.cursor = 0;
        tab.enter_selected(Config::default()).unwrap(); // → a（pos 1）
        tab.cursor = 0;
        tab.enter_selected(Config::default()).unwrap(); // → b（pos 2）
        assert_eq!(tab.hist_pos, 2);
        // 当前视图（idx 2）不可删。
        assert!(!tab.history_remove(2));
        // 删 idx 0（tmp）→ pos 前移到 1。
        assert!(tab.history_remove(0));
        assert_eq!(tab.hist_pos, 1);
        assert_eq!(tab.history.len(), 2);
        assert_eq!(
            tab.history[1],
            Location::Physical(tmp.path().join("a").join("b"))
        );
    }

    #[test]
    fn match_range_smart_case() {
        // FIX-1：渲染高亮用匹配区间，smart-case 同 M4-1。
        // 小写查询：不区分大小写。
        assert_eq!(match_range("Beta2.txt", "ta2"), Some((2, 5)));
        assert_eq!(match_range("beta.txt", "eta"), Some((1, 4)));
        // 大写查询：区分大小写。
        assert_eq!(match_range("Beta2.txt", "B"), Some((0, 1)));
        assert_eq!(match_range("beta.txt", "B"), None);
        assert_eq!(match_range("beta.txt", "Eta"), None);
        // 不匹配 / 空查询。
        assert_eq!(match_range("alpha", "zz"), None);
        assert_eq!(match_range("alpha", ""), None);
    }

    #[test]
    fn search_status_counts_current_and_total() {
        let tmp = TempDir::new().unwrap();
        for name in ["alpha", "src.txt", "srconly", "beta"] {
            fs::write(tmp.path().join(name), "").unwrap();
        }
        let mut tab = Tab::from_physical(tmp.path(), Config::default()).unwrap();
        // 排序后：alpha(0) beta(1) src.txt(2) srconly(3)；"src" 匹配 2、3。
        assert_eq!(tab.entries[2].name, "src.txt");
        assert_eq!(tab.cursor, 0);
        assert_eq!(tab.search_status("src"), (0, 2), "光标项非匹配 → 序号 0");
        tab.cursor = 2;
        assert_eq!(tab.search_status("src"), (1, 2));
        tab.cursor = 3;
        assert_eq!(tab.search_status("src"), (2, 2));
        tab.cursor = 1;
        assert_eq!(tab.search_status("src"), (0, 2));
        assert_eq!(tab.search_status("zzz"), (0, 0), "无匹配 → 0/0");
    }
}
