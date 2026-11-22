//! M42：`:tutor` 导师模式——数据模型与内嵌模块加载。
//!
//! 单一模式（学习与自验合一，SPEC M42）：模块内容以 TOML 内嵌
//! （`include_str!`，先例 help.txt/BUNDLED_THEMES）；进度持久化随 M42-2
//! （`data_dir()/tutor_progress.toml`）。无外部报告文件（用户裁剪，见
//! NOTES 2026-11-22）。Tutor 键位硬编码（y/n/q/Esc），不进 ModeLayers。

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
        }
        let raw = Raw::deserialize(deserializer)?;
        Ok(TutorItem {
            id: raw.id,
            prompt: raw.prompt,
            expected: raw.expected,
        })
    }
}

/// 内嵌模块表（编译期 include_str!；M42-3 扩充 file_ops/search_frecency）。
const BUILTIN_TUTOR_MODULES: &[(&str, &str)] =
    &[("core_nav", include_str!("../assets/tutor/core_nav.toml"))];

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
            }
        }
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
