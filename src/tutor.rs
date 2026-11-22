//! M42：`:tutor` 导师模式——数据模型与内嵌模块加载。
//!
//! 单一模式（学习与自验合一，SPEC M42）：模块内容以 TOML 内嵌
//! （`include_str!`，先例 help.txt/BUNDLED_THEMES）；进度持久化随 M42-2
//! （`data_dir()/tutor_progress.toml`）。无外部报告文件（用户裁剪，见
//! NOTES 2026-11-22）。Tutor 键位硬编码（y/n/q/Esc），不进 ModeLayers。
//!
//! M42-2：进度持久化（`data_dir()/tutor_progress.toml`，缺失/损坏静默降级）
//! 与错题本——未掌握（skipped）项下次进入优先复习；掌握/未掌握互斥迁移；
//! `:tutor reset <module_id>` 清空。

use serde::Deserialize;

/// 模块内单个自验/学习条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TutorItem {
    /// 条目标识（进度记录与错题本键）。
    pub id: String,
    /// 操作指引（左侧面板首行）。
    pub prompt: String,
    /// 预期结果（左侧面板说明行）。
    pub expected: String,
    /// M43-2：环境依赖标注（逗号分隔 token：editor|sudo|sftp|git|osc52|
    /// graphic|tmux）——Panel 显示提示行，无此环境可 Ctrl+N 跳过；
    /// 不做环境探测、不过滤条目（SPEC M43 SIMPLIFIED）。
    pub requires: Option<String>,
}

/// 一个导师模块（如 core_nav）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TutorModule {
    /// 模块标识（`:tutor <id>` 入口键）。
    pub id: String,
    /// 显示名（Panel 标题与列表行）。
    pub name: String,
    /// 一句话说明（模块列表展示）。
    pub description: String,
    /// 线性条目序列。
    pub items: Vec<TutorItem>,
}

/// 内嵌模块 TOML 文件形状（`[module]` 元信息 + `[[items]]` 数组）。
#[derive(Debug, Deserialize)]
struct TutorModuleFile {
    module: TutorModuleMeta,
    #[serde(default)]
    items: Vec<TutorItem>,
}

#[derive(Debug, Deserialize)]
struct TutorModuleMeta {
    id: String,
    name: String,
    #[serde(default)]
    description: String,
}

impl<'de> Deserialize<'de> for TutorItem {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            id: String,
            prompt: String,
            expected: String,
            requires: Option<String>,
        }
        let raw = Raw::deserialize(deserializer)?;
        Ok(TutorItem {
            id: raw.id,
            prompt: raw.prompt,
            expected: raw.expected,
            requires: raw.requires,
        })
    }
}

/// 内嵌模块表（编译期 include_str!；M42-3 三模块——core_nav/file_ops/
/// search_frecency；新增模块在此登记）。
const BUILTIN_TUTOR_MODULES: &[(&str, &str)] = &[
    ("core_nav", include_str!("../assets/tutor/core_nav.toml")),
    ("file_ops", include_str!("../assets/tutor/file_ops.toml")),
    (
        "search_frecency",
        include_str!("../assets/tutor/search_frecency.toml"),
    ),
    (
        "search_filter",
        include_str!("../assets/tutor/search_filter.toml"),
    ),
    (
        "visual_tag",
        include_str!("../assets/tutor/visual_tag.toml"),
    ),
    (
        "marks_frecency",
        include_str!("../assets/tutor/marks_frecency.toml"),
    ),
    (
        "tabs_history",
        include_str!("../assets/tutor/tabs_history.toml"),
    ),
    (
        "panes_sort",
        include_str!("../assets/tutor/panes_sort.toml"),
    ),
    (
        "archive_ops",
        include_str!("../assets/tutor/archive_ops.toml"),
    ),
    (
        "maintenance",
        include_str!("../assets/tutor/maintenance.toml"),
    ),
    (
        "command_line",
        include_str!("../assets/tutor/command_line.toml"),
    ),
    (
        "clipboard_git",
        include_str!("../assets/tutor/clipboard_git.toml"),
    ),
    (
        "remote_sftp",
        include_str!("../assets/tutor/remote_sftp.toml"),
    ),
    (
        "tutor_meta",
        include_str!("../assets/tutor/tutor_meta.toml"),
    ),
];

/// 解析模块 TOML 为 [`TutorModule`]。
pub fn parse_module(text: &str) -> Result<TutorModule, String> {
    let file: TutorModuleFile =
        toml::from_str(text).map_err(|e| format!("tutor 模块解析失败: {e}"))?;
    Ok(TutorModule {
        id: file.module.id,
        name: file.module.name,
        description: file.module.description,
        items: file.items,
    })
}

/// 全部内嵌模块（解析失败的条目静默跳过——内嵌内容由测试固化，不 panic）。
pub fn builtin_modules() -> Vec<TutorModule> {
    BUILTIN_TUTOR_MODULES
        .iter()
        .filter_map(|(_, text)| parse_module(text).ok())
        .collect()
}

/// 按模块 id 查找内嵌模块。
pub fn module_by_id(id: &str) -> Option<TutorModule> {
    builtin_modules().into_iter().find(|m| m.id == id)
}

/// 可用模块 id 列表（错误提示与列表覆盖层共用）。
pub fn builtin_module_ids() -> Vec<String> {
    builtin_modules().into_iter().map(|m| m.id).collect()
}

// ── M43-1：导师会话（覆盖层状态）──────────────────────────────────────────

/// 导师会话（M43-1：由 `AppState.tutor` 持有——覆盖层状态而非 InputMode，
/// 命令/帮助/确认等任意模式切换不丢会话；练习键照常放行，标记键
/// Ctrl+Y/N/Q 在事件分发前置拦截）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TutorSession {
    pub module: TutorModule,
    /// 当前条目下标（done 后 == items.len()）。
    pub idx: usize,
    /// 已作答条目（item_id, mastered）——advance 时即时落盘。
    pub marks: Vec<(String, bool)>,
    /// 结算态（末项作答后置位；Summary 持续显示至 Ctrl+Q）。
    pub done: bool,
    /// 复习模式（错题本 skipped 项排前）。
    pub review: bool,
}

impl TutorSession {
    /// Ctrl+Y/N：作答并推进；末项作答后进入结算态；done 后为 no-op。
    pub fn advance(&mut self, mastered: bool) {
        if self.done {
            return;
        }
        if let Some(item) = self.module.items.get(self.idx) {
            self.marks.push((item.id.clone(), mastered));
        }
        self.idx += 1;
        if self.idx >= self.module.items.len() {
            self.done = true;
        }
    }

    /// 掌握计数（Panel 进度条与 Summary）。
    pub fn mastered_count(&self) -> usize {
        self.marks.iter().filter(|(_, ok)| *ok).count()
    }

    /// 未掌握计数。
    pub fn skipped_count(&self) -> usize {
        self.marks.len() - self.mastered_count()
    }
}

// ── M42-2：进度持久化与错题本 ─────────────────────────────────────────────

/// 单模块进度（错题本）。mastered/skipped 互斥：同一 item 只在其一。
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ModuleProgress {
    #[serde(default)]
    pub mastered: Vec<String>,
    #[serde(default)]
    pub skipped: Vec<String>,
    /// 最近一次作答时间（epoch 秒；仅记录用途，无格式化依赖）。
    #[serde(default)]
    pub last_accessed: Option<u64>,
}

/// 全部模块进度（文件形状：`[<module_id>]` 小节）。
pub type TutorProgress = std::collections::BTreeMap<String, ModuleProgress>;

/// 进度文件路径（M8-3 既有数据目录 `data_dir()/tutor_progress.toml`）。
pub fn tutor_progress_path() -> std::path::PathBuf {
    crate::config::data_dir().join("tutor_progress.toml")
}

/// 读取进度：缺失或损坏 → 全新进度（静默降级，SPEC M42 SIMPLIFIED）。
pub fn load_progress_from(path: &std::path::Path) -> TutorProgress {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

/// 读取真实进度文件（run 循环用）。
pub fn load_progress() -> TutorProgress {
    load_progress_from(&tutor_progress_path())
}

/// 写进度文件（自动建父目录；失败由调用方提示）。
pub fn save_progress_to(path: &std::path::Path, progress: &TutorProgress) -> std::io::Result<()> {
    let text =
        toml::to_string_pretty(progress).map_err(|e| std::io::Error::other(e.to_string()))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, text)
}

/// 会话结束合并进度（纯函数）：掌握项迁入 mastered 并移出 skipped，
/// 未掌握反之；时间戳刷新。
pub fn merge_session(
    mut progress: TutorProgress,
    module_id: &str,
    marks: &[(String, bool)],
) -> TutorProgress {
    let entry = progress.entry(module_id.to_owned()).or_default();
    for (id, ok) in marks {
        if *ok {
            entry.skipped.retain(|s| s != id);
            if !entry.mastered.iter().any(|m| m == id) {
                entry.mastered.push(id.clone());
            }
        } else {
            entry.mastered.retain(|m| m != id);
            if !entry.skipped.iter().any(|s| s == id) {
                entry.skipped.push(id.clone());
            }
        }
    }
    entry.last_accessed = Some(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    );
    progress
}

/// `:tutor reset <module_id>`：移除该模块进度（纯函数）。
pub fn reset_module(progress: TutorProgress, module_id: &str) -> TutorProgress {
    progress
        .into_iter()
        .filter(|(k, _)| k != module_id)
        .collect()
}

/// 复习排序（纯函数）：skipped（原序）→ 未见过（原序）→ mastered（原序）。
/// 无 skipped 记录时返回 None（非复习模式，保持原顺序）。
pub fn review_order(
    progress: &TutorProgress,
    module_id: &str,
    items: &[TutorItem],
) -> Option<Vec<TutorItem>> {
    let entry = progress.get(module_id)?;
    if entry.skipped.is_empty() {
        return None;
    }
    let mut ordered: Vec<TutorItem> = Vec::new();
    // 桶序：skipped → 未见过 → mastered（各桶内保持原顺序）。
    let in_list = |id: &str, list: &[String]| list.iter().any(|x| x == id);
    for it in items {
        if in_list(&it.id, &entry.skipped) {
            ordered.push(it.clone());
        }
    }
    for it in items {
        if !in_list(&it.id, &entry.skipped) && !in_list(&it.id, &entry.mastered) {
            ordered.push(it.clone());
        }
    }
    for it in items {
        if in_list(&it.id, &entry.mastered) {
            ordered.push(it.clone());
        }
    }
    Some(ordered)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 内嵌内容必须全部可解析且 id 唯一（内嵌内容为编译期事实，测试固化）。
    #[test]
    fn builtin_modules_parse_and_unique() {
        let modules = builtin_modules();
        assert!(!modules.is_empty(), "至少内嵌一个导师模块");
        let mut ids: Vec<&str> = modules.iter().map(|m| m.id.as_str()).collect();
        ids.sort();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n, "模块 id 不得重复");
        for m in &modules {
            assert!(!m.items.is_empty(), "模块 {} 条目为空", m.id);
            for item in &m.items {
                assert!(!item.prompt.is_empty());
                assert!(!item.expected.is_empty());
                if let Some(r) = &item.requires {
                    for token in r.split(',') {
                        assert!(
                            matches!(
                                token,
                                "editor" | "sudo" | "sftp" | "git" | "osc52" | "graphic" | "tmux"
                            ),
                            "模块 {} 条目 {} 非法 requires token: {token}",
                            m.id,
                            item.id
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn m43_2_requires_field_parsing() {
        // M43-2：requires 可选（缺省 None）、多 token 逗号分隔。
        let text = "[module]\nid = \"x\"\nname = \"X\"\n\n[[items]]\nid = \"a\"\n\
prompt = \"p\"\nexpected = \"e\"\n[[items]]\nid = \"b\"\nprompt = \"p\"\n\
expected = \"e\"\nrequires = \"editor,sudo\"\n";
        let m = parse_module(text).unwrap();
        assert_eq!(m.items[0].requires, None, "缺省无依赖");
        assert_eq!(
            m.items[1].requires.as_deref(),
            Some("editor,sudo"),
            "多 token 保留原文（展示层拆分）"
        );
    }

    #[test]
    fn parse_module_reports_error_for_broken_text() {
        let err = parse_module("not [ valid toml").unwrap_err();
        assert!(err.contains("解析失败"), "err: {err}");
        // 缺 items 字段（serde default）→ 空条目模块仍可解析，但测试固化
        // 的内嵌内容不会出现该形态。
        let m = parse_module("[module]\nid = \"x\"\nname = \"X\"").unwrap();
        assert!(m.items.is_empty());
    }

    #[test]
    fn module_by_id_lookup() {
        assert!(module_by_id("core_nav").is_some());
        assert!(module_by_id("no_such_module").is_none());
        assert!(builtin_module_ids().contains(&"core_nav".to_owned()));
    }
}
