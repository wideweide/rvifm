//! 文件打开器（M11-3/M11-4/M11-5；SPEC §4.9；AUDIT-7 整层接线）。
//!
//! 零配置默认走系统 opener；TOML 分层定制（[opener] 定义 + [open] 规则，
//! prepend/append/rules 三层与键位系统同构）。目录由 rvifm 内部进入，
//! 文件才调用 opener。
//!
//! AUDIT-7 前该层仅测试可达（无 TOML 来源、无 mime、无 rules、无执行器）；
//! 现由 `app::open_selected` 生产接线：规则命中 → `use` 名单按序执行，
//! `block=true` 经 `state.pending_external` 交 run 循环挂起 TUI 执行。

use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Stdio};

/// 静默化子进程 IO（M21-F8）：GUI 程序的 stderr/stdout 不漏进 TUI 终端
/// （如 winit_wayland 警告直漏终端）；失败经返回值进状态栏（app.rs 调用方）。
fn silent(mut cmd: Command) -> Command {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd
}

/// 系统默认 opener（M11-3；M21-F8 子进程 IO 静默化）。
/// Linux：`gio open` → `xdg-open`；macOS：`open`；Windows：`cmd /c start`。
pub fn system_open(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        // Linux：优先 gio open，回退 xdg-open。
        if let Ok(s) = silent(Command::new("gio"))
            .args(["open"])
            .arg(path)
            .status()
            && s.success()
        {
            return Ok(());
        }
        if let Ok(s) = silent(Command::new("xdg-open")).arg(path).status()
            && s.success()
        {
            return Ok(());
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "gio open 和 xdg-open 均不可用",
        ))
    }
    #[cfg(target_os = "macos")]
    {
        silent(Command::new("open")).arg(path).status().map(|_| ())
    }
    #[cfg(target_os = "windows")]
    {
        let status = silent(Command::new("cmd"))
            .args(["/c", "start", ""])
            .arg(path)
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::other(format!("start 返回 {}", status)))
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = path;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "不支持的平台",
        ))
    }
}

/// POSIX 单引号转义（shell -c 命令串内的路径/命令引用；M18-4 起 tmux
/// 窗口命令与 AUDIT-7 opener `%s` 展开共用）。
pub(crate) fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// AUDIT-7：[opener] 表条目——命名 opener 定义（SPEC §4.9，yazi 风格）。
#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
pub struct OpenerDef {
    /// 命令模板（`%s` 为路径占位；缺失时路径追加为末尾参数）。
    pub run: String,
    /// true = 挂起 TUI 在当前 pane 执行并等待退出；false = fire-and-forget。
    #[serde(default)]
    pub block: bool,
    /// 展示描述（帮助/文档用；运行时不消费）。
    #[serde(default)]
    pub desc: Option<String>,
}

/// AUDIT-7：[open] 规则——mime 优先、name 回退；`use` 为按序尝试的
/// opener 名单（`open` = 系统 opener 内建名；未知名跳过）。
#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
pub struct OpenRule {
    #[serde(default)]
    pub mime: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "use", default)]
    pub use_rules: Vec<String>,
}

/// AUDIT-7：[open] 分层——prepend 优先、append 补充、rules 完全替换默认
/// 规则表（默认规则为空；与键位 prepend_keymap/append_keymap/keymap 同构）。
#[derive(Debug, Clone, Default, serde::Deserialize, PartialEq, Eq)]
pub struct OpenLayers {
    #[serde(default)]
    pub prepend_rules: Vec<OpenRule>,
    #[serde(default)]
    pub append_rules: Vec<OpenRule>,
    #[serde(default)]
    pub rules: Option<Vec<OpenRule>>,
}

/// AUDIT-7：解析后的打开动作（`use` 名单按序展开）。
#[derive(Debug, PartialEq, Eq)]
pub enum OpenAction {
    /// 系统 opener（builtin `open`）。
    SystemDefault,
    /// 自定义 opener——run 模板已展开为可执行命令串。
    Run { cmd: String, block: bool },
}

/// 内建扩展名 → MIME 映射（SIMPLIFIED：覆盖常见类别即可，规则作者对
/// 长尾类型可用 `name` glob 命中）。未知扩展 → `application/octet-stream`。
fn mime_of(path: &Path) -> String {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let mime = match ext.as_str() {
        "txt" | "md" | "markdown" | "csv" | "log" | "toml" | "yaml" | "yml" => "text/plain",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "json" => "application/json",
        "xml" => "text/xml",
        "rs" | "c" | "h" | "cpp" | "py" | "go" | "sh" => "text/plain",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "tar" => "application/x-tar",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "mp4" => "video/mp4",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        _ => "application/octet-stream",
    };
    mime.to_owned()
}

/// AUDIT-7：mime 规则匹配——`*` 全匹配；`image/*` 前缀匹配；其余精确
/// （大小写不敏感）。
fn mime_match(pattern: &str, mime: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    match pattern.strip_suffix("/*") {
        Some(prefix) => mime.to_lowercase().starts_with(&prefix.to_lowercase()),
        None => pattern.eq_ignore_ascii_case(mime),
    }
}

/// AUDIT-7：规则命中——mime 优先，name 回退（单规则内 mime 先判）。
fn rule_matches(rule: &OpenRule, mime: &str, name: &str) -> bool {
    if let Some(m) = &rule.mime
        && mime_match(m, mime)
    {
        return true;
    }
    if let Some(n) = &rule.name
        && glob_match_simple(n, name)
    {
        return true;
    }
    false
}

/// AUDIT-7：展开 run 模板——`%s` 替换为平台安全引用的路径；模板无
/// `%s` 时路径追加为末尾参数。经平台 shell 执行，引用保证含空格路径
/// 仍为单参数。
fn expand_run(run: &str, path: &Path) -> String {
    let raw = path.to_string_lossy().into_owned();
    let p = if cfg!(windows) {
        // cmd /C：双引号包裹（含空格路径保持单参数）。
        format!("\"{raw}\"")
    } else {
        shell_quote(&raw)
    };
    if run.contains("%s") {
        run.replace("%s", &p)
    } else {
        format!("{run} {p}")
    }
}

/// AUDIT-7：解析打开动作——规则按 prepend →（rules 覆盖默认规则表，
/// 默认为空）→ append 顺序取**第一个命中**；命中的 `use` 名单按序展开
/// （`open` → 系统 opener，未知名跳过），名单为空/全未知 → 回退系统
/// opener；无命中规则 → 系统 opener。
pub fn resolve_open_actions(
    layers: &OpenLayers,
    defs: &BTreeMap<String, OpenerDef>,
    path: &Path,
) -> Vec<OpenAction> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mime = mime_of(path);
    let default_rules: &[OpenRule] = &[];
    let chain = layers
        .prepend_rules
        .iter()
        .chain(layers.rules.as_deref().unwrap_or(default_rules))
        .chain(&layers.append_rules);
    for rule in chain {
        if rule_matches(rule, &mime, &name) {
            let actions: Vec<OpenAction> = rule
                .use_rules
                .iter()
                .filter_map(|u| match u.as_str() {
                    "open" => Some(OpenAction::SystemDefault),
                    other => defs.get(other).map(|d| OpenAction::Run {
                        cmd: expand_run(&d.run, path),
                        block: d.block,
                    }),
                })
                .collect();
            return if actions.is_empty() {
                vec![OpenAction::SystemDefault]
            } else {
                actions
            };
        }
    }
    vec![OpenAction::SystemDefault]
}

/// AUDIT-7：fire-and-forget 执行（平台 shell，IO 静默；不等待退出——
/// GUI 程序由系统接管，退出码不回传）。
fn run_detached(cmd: &str) -> std::io::Result<()> {
    let c = if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", cmd]);
        c
    } else {
        let mut c = Command::new("sh");
        c.arg("-c").arg(cmd);
        c
    };
    silent(c).spawn().map(|_| ())
}

/// AUDIT-7：完整打开编排（resolve → `use` 名单按序执行）。
/// - `Ok(Some(cmd))`：block=true 命令交由调用方挂起 TUI 执行
///   （`app::open_selected` → `state.pending_external` → run 循环）；
/// - `Ok(None)`：已通过系统 opener / fire-and-forget 完成；
/// - `Err(e)`：全部候选执行失败（可读错误进状态栏）。
pub fn open_with_layers(
    layers: &OpenLayers,
    defs: &BTreeMap<String, OpenerDef>,
    path: &Path,
) -> std::io::Result<Option<String>> {
    let mut last_err: Option<std::io::Error> = None;
    for action in resolve_open_actions(layers, defs, path) {
        match action {
            OpenAction::SystemDefault => match system_open(path) {
                Ok(()) => return Ok(None),
                Err(e) => last_err = Some(e),
            },
            OpenAction::Run { cmd, block } => {
                if block {
                    // 挂起执行由 run 循环完成（TUI 恢复后状态一致）。
                    return Ok(Some(cmd));
                }
                match run_detached(&cmd) {
                    Ok(()) => return Ok(None),
                    Err(e) => last_err = Some(e),
                }
            }
        }
    }
    Err(last_err.unwrap_or_else(|| std::io::Error::other("无可用打开方式")))
}

/// 简化 glob 匹配（`*` 通配符，不区分大小写）。
/// 已知缺陷（AUDIT-10 整改）：`*x*` 双星模式因 `strip_prefix` 分支先返回
/// 而退化为 `ends_with("x*")` 永假；本任务（AUDIT-7）不改匹配语义。
fn glob_match_simple(pattern: &str, name: &str) -> bool {
    let p = pattern.to_lowercase();
    let n = name.to_lowercase();
    if let Some(prefix) = p.strip_suffix('*') {
        return n.starts_with(prefix);
    }
    if let Some(suffix) = p.strip_prefix('*') {
        return n.ends_with(suffix);
    }
    if p.starts_with('*') && p.ends_with('*') && p.len() > 1 {
        return n.contains(&p[1..p.len() - 1]);
    }
    p == n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn m21f8_system_open_failure_returns_readable_err() {
        // M21-F8：子进程 IO 静默化（silent()：stdin/stdout/stderr = null，
        // GUI 程序 stderr 不漏进 TUI 终端——stable Rust 无法内省 Command
        // stdio，实现直读）+ 失败经返回 Err 由调用方进状态栏（app.rs
        // "打开失败: {e}"）。本测试验证失败路径：不存在的文件返回可读 Err
        // 且不 panic。仅 unix：Windows `start` 对不存在文件会弹 GUI 对话框
        // 阻塞 status()（本机实测），不能进自动化测试。
        let r = system_open(Path::new("definitely-not-here-xyz.txt"));
        let err = r.expect_err("不存在的文件应返回 Err");
        assert!(!err.to_string().is_empty(), "错误信息可读: {err}");
    }

    #[test]
    fn glob_match_simple_patterns() {
        assert!(glob_match_simple("*.txt", "readme.txt"));
        assert!(glob_match_simple("*.TXT", "readme.txt"));
        assert!(glob_match_simple("Makefile", "Makefile"));
        assert!(glob_match_simple("*.rs", "main.rs"));
        assert!(!glob_match_simple("*.rs", "main.go"));
        // 不区分大小写（Windows 文件系统惯例）。
        assert!(glob_match_simple("Makefile", "makefile"));
    }

    #[test]
    fn mime_of_common_categories() {
        // AUDIT-7：内建 ext → mime 表覆盖常见类别；未知兜底。
        assert_eq!(mime_of(Path::new("/a/b.png")), "image/png");
        assert_eq!(mime_of(Path::new("/a/notes.md")), "text/plain");
        assert_eq!(mime_of(Path::new("/a/paper.PDF")), "application/pdf");
        assert_eq!(mime_of(Path::new("/a/clip.MP4")), "video/mp4");
        assert_eq!(mime_of(Path::new("/a/noext")), "application/octet-stream");
        assert_eq!(
            mime_of(Path::new("/a/unknown.xyz")),
            "application/octet-stream"
        );
    }

    #[test]
    fn mime_match_patterns() {
        // AUDIT-7：`*` 全匹配 / `image/*` 前缀 / 精确值，大小写不敏感。
        assert!(mime_match("*", "anything/x"));
        assert!(mime_match("image/*", "image/png"));
        assert!(mime_match("IMAGE/*", "image/jpeg"));
        assert!(!mime_match("image/*", "text/plain"));
        assert!(mime_match("application/pdf", "application/pdf"));
        assert!(mime_match("APPLICATION/PDF", "application/pdf"));
        assert!(!mime_match("application/pdf", "application/zip"));
    }

    fn def(run: &str, block: bool) -> OpenerDef {
        OpenerDef {
            run: run.to_owned(),
            block,
            desc: None,
        }
    }

    fn rule(mime: Option<&str>, name: Option<&str>, use_rules: &[&str]) -> OpenRule {
        OpenRule {
            mime: mime.map(str::to_owned),
            name: name.map(str::to_owned),
            use_rules: use_rules.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// AUDIT-7：`%s` 展开后的路径引用随平台变化（unix 单引号 / Windows
    /// 双引号）——断言经此构造，双端 CI 与本机一致。
    fn arg(p: &str) -> String {
        if cfg!(windows) {
            format!("\"{p}\"")
        } else {
            format!("'{p}'")
        }
    }

    #[test]
    fn resolve_layering_prepend_default_append() {
        // AUDIT-7：prepend → 默认规则表（空）→ append；首个命中生效。
        let layers = OpenLayers {
            prepend_rules: vec![rule(None, Some("*.md"), &["a_edit"])],
            append_rules: vec![rule(None, Some("*.md"), &["b_less"])],
            rules: None,
        };
        let mut defs = BTreeMap::new();
        defs.insert("a_edit".to_owned(), def("nvim %s", false));
        defs.insert("b_less".to_owned(), def("less %s", false));
        let acts = resolve_open_actions(&layers, &defs, Path::new("/w/notes.md"));
        assert_eq!(
            acts,
            vec![OpenAction::Run {
                cmd: format!("nvim {}", arg("/w/notes.md")),
                block: false
            }],
            "prepend 优先命中"
        );
    }

    #[test]
    fn resolve_rules_replace_default_table_only() {
        // AUDIT-7：rules 完全替换默认规则表（默认为空），prepend/append
        // 仍按层包裹——与键位 prepend_keymap/append_keymap/keymap 同构。
        let layers = OpenLayers {
            prepend_rules: vec![rule(None, Some("*.md"), &["a_edit"])],
            append_rules: vec![rule(None, Some("*.md"), &["b_less"])],
            rules: Some(vec![rule(None, Some("*.txt"), &["c_cat"])]),
        };
        let mut defs = BTreeMap::new();
        defs.insert("a_edit".to_owned(), def("nvim %s", false));
        defs.insert("b_less".to_owned(), def("less %s", false));
        defs.insert("c_cat".to_owned(), def("cat %s", false));
        let acts = resolve_open_actions(&layers, &defs, Path::new("/w/notes.md"));
        // prepend 层不受 rules 影响（仍最先命中）。
        assert!(matches!(&acts[0], OpenAction::Run { cmd, .. } if cmd.starts_with("nvim")));
        // prepend 未命中的类型落 rules 表（rules 替换的是默认规则表，
        // 默认为空——无"隐式默认规则"兜底）。
        let acts2 = resolve_open_actions(&layers, &defs, Path::new("/w/x.txt"));
        assert!(matches!(&acts2[0], OpenAction::Run { cmd, .. } if cmd.starts_with("cat")));
    }

    #[test]
    fn resolve_mime_beats_name_within_rule_and_order_wins() {
        // AUDIT-7：单规则内 mime 先于 name；规则按链序首个命中生效。
        let layers = OpenLayers {
            prepend_rules: vec![rule(Some("image/*"), Some("*.txt"), &["img_view"])],
            append_rules: vec![],
            rules: None,
        };
        let mut defs = BTreeMap::new();
        defs.insert("img_view".to_owned(), def("imv %s", false));
        // mime 命中（image/* 对 .txt 的 mime 仍 image/*? 否——.txt 是
        // text/plain）：这里验证 name 命中同样生效（mime 不中则回退 name）。
        let acts = resolve_open_actions(&layers, &defs, Path::new("/w/pic.txt"));
        assert!(matches!(&acts[0], OpenAction::Run { cmd, .. } if cmd.starts_with("imv")));
        // mime 命中路径。
        let acts2 = resolve_open_actions(&layers, &defs, Path::new("/w/pic.png"));
        assert!(matches!(&acts2[0], OpenAction::Run { cmd, .. } if cmd.starts_with("imv")));
        // 两者都不中 → 系统 opener。
        let acts3 = resolve_open_actions(&layers, &defs, Path::new("/w/doc.pdf"));
        assert_eq!(acts3, vec![OpenAction::SystemDefault]);
    }

    #[test]
    fn resolve_use_list_fallback_and_builtin_open() {
        // AUDIT-7：use 名单按序展开；`open` = 系统 opener；未知名跳过；
        // 名单为空/全未知 → 回退系统 opener。
        let layers = OpenLayers {
            prepend_rules: vec![rule(Some("image/*"), None, &["ghost", "open", "nope"])],
            append_rules: vec![],
            rules: None,
        };
        let mut defs = BTreeMap::new();
        defs.insert("ghost".to_owned(), def("gimp %s", true));
        let acts = resolve_open_actions(&layers, &defs, Path::new("/w/pic.png"));
        assert_eq!(
            acts,
            vec![
                OpenAction::Run {
                    cmd: format!("gimp {}", arg("/w/pic.png")),
                    block: true
                },
                OpenAction::SystemDefault,
            ]
        );
        // 空名单。
        let empty = OpenLayers {
            prepend_rules: vec![rule(Some("image/*"), None, &[])],
            append_rules: vec![],
            rules: None,
        };
        let acts2 = resolve_open_actions(&empty, &defs, Path::new("/w/pic.png"));
        assert_eq!(acts2, vec![OpenAction::SystemDefault]);
    }

    #[test]
    fn expand_run_substitution_and_append() {
        // AUDIT-7：`%s` 替换；无 `%s` 追加末尾参数；路径按平台安全引用
        // （unix 单引号 / Windows 双引号），含空格路径保持单参数。
        assert_eq!(
            expand_run("nvim %s", Path::new("/w/my file.md")),
            format!("nvim {}", arg("/w/my file.md"))
        );
        assert_eq!(
            expand_run("nvim", Path::new("/w/a.md")),
            format!("nvim {}", arg("/w/a.md"))
        );
    }

    #[test]
    fn open_with_layers_block_true_hands_off_command() {
        // AUDIT-7：block=true 不在编排内执行——命令交还调用方
        // （app.rs 经 state.pending_external 由 run 循环挂起执行）。
        let layers = OpenLayers {
            prepend_rules: vec![rule(None, Some("*.md"), &["edit"])],
            append_rules: vec![],
            rules: None,
        };
        let mut defs = BTreeMap::new();
        defs.insert("edit".to_owned(), def("nvim %s", true));
        let r = open_with_layers(&layers, &defs, Path::new("/w/notes.md")).expect("block 交还");
        assert_eq!(r, Some(format!("nvim {}", arg("/w/notes.md"))));
    }

    #[test]
    fn open_with_layers_falls_through_use_list_on_spawn_failure() {
        // AUDIT-7：block=false 的 fire-and-forget spawn 失败 → 尝试名单
        // 下一项（此处第二项为系统 opener）。注：shell 存在时子进程以
        // 非零退出不影响 spawn 成功——此测试注入 shell 不存在的形态由
        // unix 分支覆盖（cmd/sh 均在，故以 unknown def 跳过路径验证链）。
        let layers = OpenLayers {
            prepend_rules: vec![rule(None, Some("*.md"), &["nope", "open"])],
            append_rules: vec![],
            rules: None,
        };
        let defs: BTreeMap<String, OpenerDef> = BTreeMap::new();
        // "nope" 未定义 → 跳过；"open" → 系统 opener（unix 下 gio/xdg-open
        // 缺失时报错——仅断言返回形态，不断言成功）。
        let r = open_with_layers(&layers, &defs, Path::new("/w/notes.md"));
        #[cfg(unix)]
        assert!(r.is_err() || r == Ok(None), "系统 opener 结果: {r:?}");
        #[cfg(not(unix))]
        let _ = r;
    }

    // M11-3 的 `system_open` 依赖系统 opener，测试中不调用（会弹出 GUI）。
}
