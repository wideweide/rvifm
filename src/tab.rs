//! `Location` 与 `Tab`：浏览位置模型与每标签独立状态（SPEC §4.1 / §4.2）。
//!
//! 归档浏览行为在 M7 接入；`Location::Archive` 的数据形状现在固定，
//! 避免后期改动所有 tab 相关代码。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::config::Config;

/// 浏览位置：物理目录或归档内虚拟路径（只读）。
/// M45-3e：派生 `Hash` 以作 per-location 光标记忆的键。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Location {
    Physical(PathBuf),
    /// `host` 为归档文件的物理路径，`inner` 为归档内路径；构造与导航在 M7 接入
    /// （形状由 SPEC §4.1 现在固定）。
    #[allow(dead_code)]
    Archive {
        host: PathBuf,
        inner: PathBuf,
    },
    /// M19-2：远端位置——`remote` 为会话名（remotes.toml / ssh config
    /// Host 别名），`path` 为远端绝对路径（`/` 分隔）。
    #[allow(dead_code)]
    Remote {
        remote: String,
        path: PathBuf,
    },
}

/// M45-1a：HOME 下物理路径缩写为 `~/…`（显示层专用，bash/fish 习惯）。
/// 纯函数：`home` 注入便于测试隔离（AGENTS §7：测试不得受宿主 HOME 影响）。
/// 仅整段路径组件匹配（`/home/u2/x` 不会被 `/home/u` 误缩写）。
pub fn abbreviate_home(path: &Path, home: Option<&Path>) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    if let Some(home) = home {
        let h = home.to_string_lossy().replace('\\', "/");
        let h = h.trim_end_matches('/');
        if !h.is_empty() {
            if normalized == h {
                return "~".to_owned();
            }
            if let Some(rest) = normalized.strip_prefix(&format!("{h}/")) {
                return format!("~/{rest}");
            }
        }
    }
    normalized
}

/// 当前 HOME（Unix `HOME` / Windows `USERPROFILE` 回退），供显示层缩写用。
fn home_for_display() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

impl Location {
    /// 状态栏/面包屑显示。归档位置显示为 `host:/inner/path`（SPEC §4.1）；
    /// 远端位置显示为 `remote:/path`（M19-2）。物理路径在 HOME 下缩写为
    /// `~/…`（M45-1a，仅显示层；base_dir/导航仍用真实绝对路径）。
    pub fn display_path(&self) -> String {
        match self {
            Location::Physical(p) => {
                let home = home_for_display();
                abbreviate_home(p, home.as_deref())
            }
            Location::Archive { host, inner } => format!(
                "{}:/{}",
                host.display(),
                inner.to_string_lossy().replace('\\', "/")
            ),
            Location::Remote { remote, path } => {
                format!("{}:{}", remote, path.to_string_lossy().replace('\\', "/"))
            }
        }
    }

    /// `:tabnew` 的落点目录：Physical 为该目录；Archive 为宿主目录
    /// （归档内新建标签复用宿主，M7 再细化）；Remote 为远端当前路径
    /// （新标签仍在该远端会话，M19-2）。
    pub fn base_dir(&self) -> &Path {
        match self {
            Location::Physical(p) => p,
            Location::Archive { host, .. } => host,
            Location::Remote { path, .. } => path,
        }
    }

    /// M20-1：远端会话名（非远端位置返回 None）。
    pub fn remote_name(&self) -> Option<String> {
        match self {
            Location::Remote { remote, .. } => Some(remote.clone()),
            _ => None,
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
    /// 是否符号链接（M21-F1 类型着色；unix/Windows 通用 `file_type().is_symlink()`）。
    pub is_link: bool,
    /// 是否可执行（M21-F1 类型着色；unix 按执行位，其余按扩展名近似）。
    pub is_exec: bool,
    /// 文件字节数（目录同样记录底层值，状态栏对目录省略显示）。
    pub size: u64,
    /// 平台相关权限摘要：unix 为 3 位八进制（如 `644`），Windows 为 `ro`/`rw`。
    pub perm: String,
    /// 修改时间（Unix 秒；归档/结果条目可能为 0）。
    pub mtime: u64,
}

/// 文件类型分类（M21-F1；yazi filetype 语义 + vifm 配色参考）。
/// 分类顺序即优先级：目录 > 符号链接 > 可执行 > 归档 > 图片 > 视频 > 代码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileTypeClass {
    Directory,
    Executable,
    Symlink,
    Archive,
    Image,
    Video,
    Code,
    Other,
}

impl FileTypeClass {
    /// 状态栏/详情的类型标签（M21-F1）。
    pub fn label(self) -> &'static str {
        match self {
            FileTypeClass::Directory => "目录",
            FileTypeClass::Executable => "可执行",
            FileTypeClass::Symlink => "链接",
            FileTypeClass::Archive => "归档",
            FileTypeClass::Image => "图片",
            FileTypeClass::Video => "视频",
            FileTypeClass::Code => "代码",
            FileTypeClass::Other => "文件",
        }
    }
}

// 扩展名表（M21-F1）：与 yazi filetype 对齐的常用集合，取小写后匹配。
const IMAGE_EXTS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "tif", "tiff", "svg", "avif", "heic",
];
const VIDEO_EXTS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "webm", "flv", "wmv", "m4v", "mpg", "mpeg", "ts", "3gp",
];
const CODE_EXTS: &[&str] = &[
    "rs", "py", "js", "mjs", "cjs", "ts", "jsx", "tsx", "c", "h", "cpp", "hpp", "cc", "go", "java",
    "rb", "php", "sh", "bash", "zsh", "fish", "lua", "pl", "swift", "kt", "scala", "cs", "vb", "r",
    "m", "sql", "html", "css", "scss", "less", "vue", "svelte", "astro", "dart", "ex", "exs", "hs",
    "ml", "clj", "nim", "zig", "v", "gradle", "cmake", "make", "ps1", "bat", "cmd",
];
/// Windows 可执行扩展名（无执行位可用，按扩展名近似；含脚本类）。
const EXEC_EXTS: &[&str] = &["exe", "bat", "cmd", "com", "scr", "msi", "ps1"];

/// 取小写扩展名（无扩展名返回空串）。
fn ext_lower(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, e)| e.to_lowercase())
        .unwrap_or_default()
}

fn ext_in(name: &str, exts: &[&str]) -> bool {
    let e = ext_lower(name);
    !e.is_empty() && exts.contains(&e.as_str())
}

/// unix：按执行位（任一 x）判定；非 unix：按扩展名近似（M21-F1）。
#[cfg(unix)]
fn is_executable(meta: &std::fs::Metadata, name: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0 || ext_in(name, EXEC_EXTS)
}

/// 非 unix（Windows）：无执行位可用，按扩展名近似。
#[cfg(not(unix))]
fn is_executable(meta: &std::fs::Metadata, name: &str) -> bool {
    let _ = meta;
    ext_in(name, EXEC_EXTS)
}

/// 按扩展名近似可执行判定——无执行位信息的来源使用
/// （远端 SFTP 元数据、归档内条目；M21-F1）。
pub fn is_exec_name(name: &str) -> bool {
    ext_in(name, EXEC_EXTS)
}

impl FileItem {
    /// 文件类型分类（M21-F1）。归档判定复用 M7 的扩展名探测
    /// （支持 + 不支持虚拟浏览的并集都算归档着色）。
    pub fn filetype_class(&self) -> FileTypeClass {
        if self.is_dir {
            FileTypeClass::Directory
        } else if self.is_link {
            FileTypeClass::Symlink
        } else if self.is_exec {
            FileTypeClass::Executable
        } else if crate::archive::is_archive_file(&self.name)
            || crate::archive::is_unsupported_archive(&self.name)
        {
            FileTypeClass::Archive
        } else if ext_in(&self.name, IMAGE_EXTS) {
            FileTypeClass::Image
        } else if ext_in(&self.name, VIDEO_EXTS) {
            FileTypeClass::Video
        } else if ext_in(&self.name, CODE_EXTS) {
            FileTypeClass::Code
        } else {
            FileTypeClass::Other
        }
    }
}

/// Unix 秒 → `yyyy-mm-dd HH:mm:ss`（本地时区，jiff；M21-F1 底部详情）。
/// 0（归档/结果条目无时间）或超范围显示 `-`。
pub fn format_mtime(secs: u64) -> String {
    if secs == 0 {
        return "-".to_owned();
    }
    let Ok(ts) = jiff::Timestamp::from_second(secs as i64) else {
        return "-".to_owned();
    };
    let z = ts.to_zoned(jiff::tz::TimeZone::system());
    let d = z.date();
    let t = z.time();
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        d.year(),
        d.month(),
        d.day(),
        t.hour(),
        t.minute(),
        t.second()
    )
}

/// M24-1：分钟精度时间格式——`yyyy-mm-dd HH:mm`（16 列，列表属性列用）。
pub fn format_mtime_min(secs: u64) -> String {
    let full = format_mtime(secs);
    if full.len() >= 16 {
        full[..16].to_owned()
    } else {
        full
    }
}

/// M24-2：自然排序比较器——分段解析数字/非数字，数字段按 u64 值比较、
/// 非数字段按字典序；大小写不敏感（`Config.ignore_case_sort` 在接线处
/// 分流到 [`natural_cmp_case`]）。"1.txt" < "2.txt" < "10.txt"。
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    natural_cmp_case(&a.to_lowercase(), &b.to_lowercase())
}

/// M24-2：大小写敏感的自然排序变体（`ignore_case_sort = false` 时使用）。
/// M40-3：重写为**分块比较**——字符串切分为数字段/文本段交替序列，逐段
/// 比较（数字段按 u64 值、文本段按字典序、数字段 < 文本段）。原逐字符
/// 实现在"一侧有数字段另一侧无"时丢弃数字信息，导致 Equal 非传递
/// （"1a" == "a" 但 "1a" < "2" < "a"）——std sort 检测到非全序比较器会
/// panic（M40-3 proptest 抓到的真实缺陷，见 NOTES）。
pub fn natural_cmp_case(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    /// 切分为 (是否数字段, 段内容) 交替序列。
    fn chunks(s: &str) -> Vec<(bool, String)> {
        let mut out: Vec<(bool, String)> = Vec::new();
        for c in s.chars() {
            let num = c.is_ascii_digit();
            match out.last_mut() {
                Some((n, buf)) if *n == num => buf.push(c),
                _ => out.push((num, c.to_string())),
            }
        }
        out
    }

    let ca = chunks(a);
    let cb = chunks(b);
    let len = ca.len().max(cb.len());
    for i in 0..len {
        match (ca.get(i), cb.get(i)) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some((na, sa)), Some((nb, sb))) => {
                if *na && *nb {
                    // 数字段按 u64 值（前导零等值继续下一段；超长段同回退
                    // u64::MAX 继续下一段，保证全序）。
                    let av: u64 = sa.parse().unwrap_or(u64::MAX);
                    let bv: u64 = sb.parse().unwrap_or(u64::MAX);
                    if av != bv {
                        return av.cmp(&bv);
                    }
                } else if !*na && !*nb {
                    let o = sa.cmp(sb);
                    if o != Ordering::Equal {
                        return o;
                    }
                } else {
                    // 数字段 < 文本段（与既有"数字在前"行为一致）。
                    return if *na {
                        Ordering::Less
                    } else {
                        Ordering::Greater
                    };
                }
            }
        }
    }
    Ordering::Equal
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
    /// 焦点在右栏时是否选中渲染层 `..` 行。
    pub on_parent_row: bool,
    /// M45-3e：per-location 光标记忆（离开记下、重返还原）。仅内存。
    pub cursor_memory: std::collections::HashMap<Location, usize>,
    /// M47-2：右 pane 独立的 glob 过滤模式（`=` 设置；None = 不过滤）。
    pub filter: Option<String>,
    /// M47-8：右 pane 独立的 `v` 可视选择锚点与 `t` 标记集合。
    pub visual: Option<usize>,
    pub tags: HashSet<String>,
    /// M47-4：右 pane 独立的 `:sort` 规格（刷新后重应用）。
    pub sort: Option<Vec<SortSpec>>,
}

impl Pane {
    /// M45-3e：离开当前位置前记忆光标。
    pub fn remember_cursor(&mut self) {
        self.cursor_memory
            .insert(self.location.clone(), self.cursor);
    }

    /// M45-3e：进入位置后还原记忆的光标（按当前条目数钳制）。
    pub fn restore_cursor(&mut self) {
        if let Some(&saved) = self.cursor_memory.get(&self.location) {
            self.cursor = saved.min(self.entries.len().saturating_sub(1));
        }
    }

    /// 从物理目录构建 pane（加载排序条目，M5-2 options 生效）。
    pub fn from_physical(dir: &Path, config: Config) -> std::io::Result<Self> {
        Ok(Self {
            location: Location::Physical(dir.to_path_buf()),
            entries: load_entries(dir, &config)?,
            cursor: 0,
            scroll: 0,
            on_parent_row: false,
            cursor_memory: std::collections::HashMap::new(),
            filter: None,
            visual: None,
            tags: HashSet::new(),
            sort: None,
        })
    }

    /// 光标移动（M14-1）：±1 环绕，其余钳制（与左 pane 语义一致）。
    pub fn move_cursor(&mut self, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        if self.on_parent_row {
            if delta < 0 {
                return;
            }
            self.on_parent_row = false;
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
        self.on_parent_row = self.has_parent_row();
    }

    pub fn cursor_bottom(&mut self) {
        if !self.entries.is_empty() {
            self.cursor = self.entries.len() - 1;
        }
        self.on_parent_row = false;
    }

    /// 进入光标条目（M14-1 物理目录；M45-6c 裁决：右 pane 归档文件/归档内
    /// 虚拟目录也可进入，文件仍 no-op）。M45-3e：离开前记忆、进入后还原。
    pub fn enter_selected(&mut self, config: Config) -> std::io::Result<()> {
        let Some(item) = self.entries.get(self.cursor) else {
            return Ok(());
        };
        let name = item.name.clone();
        let is_dir = item.is_dir;
        let target = match self.location.clone() {
            Location::Physical(base) => {
                let full = base.join(&name);
                if is_dir {
                    Some(Location::Physical(full))
                } else if crate::archive::is_unsupported_archive(&name) {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::Unsupported,
                        "该归档格式不支持虚拟浏览，请解压后浏览",
                    ));
                } else if crate::archive::is_archive_file(&name) {
                    Some(Location::Archive {
                        host: full,
                        inner: std::path::PathBuf::new(),
                    })
                } else {
                    None // 普通文件 no-op（M41-3 裁决）
                }
            }
            Location::Archive { host, inner } => {
                if is_dir {
                    let inner_str = inner.to_string_lossy().replace('\\', "/");
                    let joined = if inner_str.is_empty() {
                        name.clone()
                    } else {
                        format!("{inner_str}/{name}")
                    };
                    Some(Location::Archive {
                        host,
                        inner: std::path::PathBuf::from(crate::archive::normalize_inner(&joined)),
                    })
                } else {
                    None // 归档内文件 no-op
                }
            }
            Location::Remote { .. } => None,
        };
        if let Some(target) = target {
            // M46-4：先判断目标位置是否有记忆，再进入/还原；有记忆时保留
            // 还原的光标，不再无条件高亮 `..` 行。
            let had_memory = self.cursor_memory.contains_key(&target);
            self.remember_cursor();
            self.location = target;
            self.cursor = 0;
            self.scroll = 0;
            self.refresh(config)?;
            self.restore_cursor();
            self.on_parent_row = if had_memory {
                false
            } else {
                self.has_parent_row()
            };
        }
        Ok(())
    }

    /// 返回父目录（M14-1，h；M21-F4 返回后光标定位在来源目录条目上）。
    /// M45-3e：离开前记忆子目录光标（重返时还原）；父目录光标仍按 M21-F4
    /// 定位在来源条目。M45-6c：归档虚拟目录/h 继续上溯（根回物理宿主目录）。
    pub fn return_parent(&mut self, config: Config) -> std::io::Result<()> {
        let (target, leaf) = match self.location.clone() {
            Location::Physical(base) => {
                let Some(parent) = base.parent().filter(|p| !p.as_os_str().is_empty()) else {
                    return Ok(());
                };
                (
                    Location::Physical(parent.to_path_buf()),
                    base.file_name().map(|n| n.to_string_lossy().into_owned()),
                )
            }
            Location::Archive { host, inner } => {
                let inner_str = inner.to_string_lossy().replace('\\', "/");
                if inner_str.is_empty() {
                    // 归档根 → 宿主归档文件所在物理目录。
                    let host_dir = host.parent().unwrap_or(host.as_path()).to_path_buf();
                    (
                        Location::Physical(host_dir),
                        host.file_name().map(|n| n.to_string_lossy().into_owned()),
                    )
                } else {
                    let (parent, leaf) = match inner_str.rsplit_once('/') {
                        Some((p, l)) => (p.to_owned(), l.to_owned()),
                        None => (String::new(), inner_str.clone()),
                    };
                    (
                        Location::Archive {
                            host,
                            inner: std::path::PathBuf::from(crate::archive::normalize_inner(
                                &parent,
                            )),
                        },
                        Some(leaf),
                    )
                }
            }
            Location::Remote { .. } => return Ok(()),
        };
        self.remember_cursor();
        self.location = target;
        self.cursor = 0;
        self.scroll = 0;
        self.refresh(config)?;
        if let Some(name) = leaf
            && let Some(idx) = locate_entry(&self.entries, &name)
        {
            self.cursor = idx;
        }
        self.on_parent_row = false;
        Ok(())
    }

    /// 重读当前位置条目并钳制光标。M45-6c：右 pane 进入归档虚拟目录后
    /// 也需按 Location 装载成员（原仅 Physical）。远端仍由会话驱动、no-op。
    pub fn refresh(&mut self, config: Config) -> std::io::Result<()> {
        match self.location {
            Location::Remote { .. } => {}
            Location::Physical(_) | Location::Archive { .. } => {
                self.entries = Tab::load_for(&self.location, config.clone())?;
                // M47-2：右 pane 独立过滤在自身刷新时应用（与 Tab::refresh 同源）。
                if let Some(pat) = &self.filter {
                    self.entries.retain(|item| filter_matches(pat, &item.name));
                }
                // M47-4：右 pane 的 :sort 在刷新后重应用（与左 pane 对称）。
                if let Some(specs) = self.sort.clone() {
                    sort_entries_specs(&mut self.entries, &specs, &config);
                }
                if self.cursor >= self.entries.len() {
                    self.cursor = self.entries.len().saturating_sub(1);
                }
            }
        }
        if !self.has_parent_row() {
            self.on_parent_row = false;
        }
        Ok(())
    }

    pub fn has_parent_row(&self) -> bool {
        match &self.location {
            Location::Physical(p) => p
                .parent()
                .is_some_and(|parent| !parent.as_os_str().is_empty()),
            Location::Archive { .. } | Location::Remote { .. } => true,
        }
    }

    pub fn move_cursor_wrap(&mut self, delta: isize) {
        if self.entries.is_empty() {
            if delta < 0 && self.has_parent_row() {
                self.on_parent_row = true;
            } else if delta > 0 {
                self.on_parent_row = false;
            }
            return;
        }
        if delta == -1 {
            if self.on_parent_row {
                return;
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
    /// M42：预览窗格焦点——`w`/`:view` 开启后仍为 false（焦点留在原
    /// pane，j/k 移动光标、对侧预览跟随）；`Tab` 显式进入预览窗格后为
    /// true，此时按键路由给查看器（滚动/搜索），`Tab` 返回原 pane。
    pub preview_focus: bool,
    /// 右 pane（M14-1）：独立目录列表（独立 Location/光标/滚动）。
    pub right: Pane,
    /// 焦点 pane（M14-1）：Tab 键切换；导航与文件操作作用于焦点 pane。
    pub focus: Focus,
    /// FEAT-2：焦点在列表顶部 `..` 虚拟行（渲染层条目，不占用 cursor 空间、
    /// 不进入 entries/搜索/过滤/标记集合）。`l`/Enter = return_parent；
    /// 写操作被守卫拒绝。
    pub on_parent_row: bool,
    /// M39-6：命名标签（`:tabname <名>`；None = 标签栏显示目录名）。
    /// 仅内存（会话恢复不保存，SIMPLIFIED）。
    pub name: Option<String>,
    /// M16 NAV-3：当前视图在 `history` 中的下标（history 从 M16 起为
    /// **完整访问轨迹**，含当前视图；新导航截断 forward）。
    pub hist_pos: usize,
    /// M17 SRCH：搜索结果虚拟目录（:find/:grep；沿 :tree 同构——静态
    /// 条目 + 只读守卫，items/entries 平行表）。
    pub results: Option<crate::search::SearchResults>,
    /// M28-2：`:rename` 结构化预览待应用计划（(旧路径, 新名) 对；仅含将
    /// 重命名的条目）。进入 RenamePreview 结果视图时设置，退出/应用时清空。
    pub pending_rename: Option<Vec<(PathBuf, String)>>,
    /// M18-1/M18-2：运行时排序状态（`S` 对话框 / `:sort`；None = 默认顺序；
    /// 多字段经 :sort，对话框写单元素）。
    pub sort: Option<Vec<SortSpec>>,
    /// M17 VIEW-1：交互式查看器状态（view_mode 激活时右 pane）。
    pub viewer: Option<ViewerState>,
    /// M23-6：焦点 pane 的 Frecency 内嵌搜索（Pane 模式，非浮层）。
    pub pane_frecency: Option<FrecencyPane>,
    /// M19-2：进入远端前的本地位置（`h` 在远端根目录时返回此地）。
    pub pre_remote: Option<Location>,
    /// M45-3e：per-location 光标记忆（离开记下、重返还原，`h`→`l` 往返）。
    /// 仅内存，不入会话序列化（SIMPLIFIED）；键为 Location 本身。
    pub cursor_memory: std::collections::HashMap<Location, usize>,
}

/// M23-6：Frecency Pane 状态（焦点 pane 内嵌搜索，非浮层）。
#[derive(Debug, Clone)]
pub struct FrecencyPane {
    /// 打开时一次性读入的候选项。
    pub entries: Vec<crate::frecency::ScoredEntry>,
    /// 搜索词（M23-1 多词分级过滤）。
    pub query: String,
    /// 可见结果光标。
    pub cursor: usize,
    /// Normal 子态（Esc 一次后）；可打印字符回搜索态。
    pub normal: bool,
    /// gg 待决。
    pub g_pending: bool,
    /// M49-1：预览区是否获得焦点（Tab 切换，列表始终可见）。
    pub preview_focus: bool,
    /// M49-1：预览滚动与搜索态（复用 ViewerState 渲染链）。
    pub preview_scroll: usize,
    pub preview_query: Option<String>,
    pub preview_match: usize,
    /// M49-15：`dd` 删除记录待决 / 最近删除项（供 `u` 恢复）。
    pub d_pending: bool,
    pub y_pending: bool,
    pub last_deleted: Option<(String, String)>,
}

/// M18-1：运行时排序字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortField {
    Name,
    /// M24-2：自然排序（数字感知，1.txt < 2.txt < 10.txt）。
    Natural,
    Mtime,
    Size,
    Ext,
}

/// M18-1：排序规格（字段 + 方向）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortSpec {
    pub field: SortField,
    pub desc: bool,
}

impl SortField {
    /// 中文名（对话框显示）。
    pub fn label(self) -> &'static str {
        match self {
            SortField::Name => "名称",
            SortField::Natural => "自然",
            SortField::Mtime => "修改时间",
            SortField::Size => "大小",
            SortField::Ext => "扩展名",
        }
    }

    /// 文件扩展名（小写；无扩展名 = 空串）。
    fn ext_of(name: &str) -> String {
        name.rsplit_once('.')
            .map(|(_, e)| e.to_lowercase())
            .unwrap_or_default()
    }
}

/// M17 VIEW-1：查看器状态（行号滚动 + 搜索定位）。
#[derive(Debug, Clone, Default)]
pub struct ViewerState {
    /// 视口首行（0-based）。
    pub scroll: usize,
    /// 文件总行数（激活时一次性读取，滚动钳制用；超过 100,000 行封顶，
    /// 完整行数在 `total_lines`）。hex 模式 = 16 字节/行的总行数。
    pub line_count: usize,
    /// 完整行数（M21-E3 截断提示用；未截断时与 `line_count` 相等）。
    pub total_lines: usize,
    /// `/` 搜索词（None = 未搜索）。
    pub query: Option<String>,
    /// 当前匹配行（0-based）。
    pub match_line: usize,
    /// M39-1：hex 模式——二进制文件的完整视图（16 字节/行，分块读取）。
    pub hex: bool,
    /// M39-2：PDF 查看器当前页（0-based；`[`/`]` 翻页）。
    pub page: u32,
    /// M42：当前查看的目标文件（预览跟随：光标换文件时据此判断重建）。
    pub path: Option<std::path::PathBuf>,
}

/// M39-1：二进制判定——前 8KB 含 NUL 字节即视为二进制（hex 查看器进入）。
pub(crate) fn looks_binary(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let mut buf = vec![0u8; 8192];
    let n = f.read(&mut buf).unwrap_or(0);
    buf[..n].contains(&0)
}

/// M47-4：左右 pane 共用的排序比较器（目录优先 + 逐字段链 + 名称平手）。
fn sort_entries_specs(entries: &mut [FileItem], specs: &[SortSpec], config: &Config) {
    let dirs_first = config.sort_dirs_first;
    entries.sort_by(|a, b| {
        let dir_cmp = if dirs_first {
            b.is_dir.cmp(&a.is_dir)
        } else {
            std::cmp::Ordering::Equal
        };
        dir_cmp.then_with(|| {
            let mut ord = std::cmp::Ordering::Equal;
            for spec in specs {
                let base = match spec.field {
                    SortField::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    // M24-2：自然排序受 ignore_case_sort 影响（大小写敏感变体）。
                    SortField::Natural => {
                        if config.ignore_case_sort {
                            natural_cmp(&a.name, &b.name)
                        } else {
                            natural_cmp_case(&a.name, &b.name)
                        }
                    }
                    SortField::Mtime => a.mtime.cmp(&b.mtime),
                    SortField::Size => a.size.cmp(&b.size),
                    SortField::Ext => SortField::ext_of(&a.name).cmp(&SortField::ext_of(&b.name)),
                };
                ord = ord.then_with(|| if spec.desc { base.reverse() } else { base });
            }
            ord.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        })
    });
}

impl Tab {
    /// M45-3e：离开当前位置前记忆光标。
    fn remember_cursor(&mut self) {
        self.cursor_memory
            .insert(self.location.clone(), self.cursor);
    }

    /// M45-3e：进入位置后还原记忆的光标（按当前条目数钳制）。
    fn restore_cursor(&mut self) {
        if let Some(&saved) = self.cursor_memory.get(&self.location) {
            self.cursor = saved.min(self.entries.len().saturating_sub(1));
        }
    }

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
            view_mode: false,
            preview_focus: false,
            // M14-1：右 pane 默认同目录（vifm 启动行为），独立导航。
            right: Pane::from_physical(dir, config)?,
            focus: Focus::Left,
            on_parent_row: false,
            name: None,
            cursor_memory: std::collections::HashMap::new(),
        })
    }

    /// 任意 Location 的条目加载（NAV-3：历史跳转后恢复条目列表用）。
    /// M19-2：远端位置由会话驱动（remote_navigate），此处不支持。
    pub(crate) fn load_for(location: &Location, config: Config) -> std::io::Result<Vec<FileItem>> {
        match location {
            Location::Physical(p) => load_entries(p, &config),
            Location::Archive { host, inner } => {
                load_archive_entries(host, &inner.to_string_lossy())
            }
            Location::Remote { .. } => Err(std::io::Error::other("远端条目由会话驱动加载")),
        }
    }

    /// M16 NAV-3：导航到新视图——截断 forward（游标之后全部丢弃），
    /// 追加并落位。cursor/scroll/`..` 焦点重置，条目按新位置加载。
    /// 注意：进入目录/归档后需调用方补 `on_parent_row = has_parent_row()`
    /// （落在 ".." 行的 vifm 语义）。
    pub fn navigate_to(&mut self, new: Location, config: Config) -> std::io::Result<()> {
        // 先加载，成功才变更状态（损坏归档等失败路径无副作用，M7-4 契约）。
        let entries = Self::load_for(&new, config)?;
        // M45-3e：离开前记忆当前光标（失败路径无副作用——记忆亦不写）。
        self.remember_cursor();
        self.history.truncate(self.hist_pos + 1);
        self.history.push(new.clone());
        self.hist_pos = self.history.len() - 1;
        self.location = new;
        self.entries = entries;
        self.cursor = 0;
        self.scroll = 0;
        // M45-3e：重返已访问位置时还原其记忆光标（首次进入仍为 0）。
        self.restore_cursor();
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
    /// 归档视图恒显示（`..`/`h` 返回归档文件所在物理目录，M7-5）；
    /// 远端视图恒显示（`..`/`h` 返回上级或本地目录，M19-2）。
    pub fn has_parent_row(&self) -> bool {
        match &self.location {
            Location::Physical(p) => p
                .parent()
                .is_some_and(|parent| !parent.as_os_str().is_empty()),
            Location::Archive { .. } | Location::Remote { .. } => true,
        }
    }

    /// M41-4：视口偏移同步（每帧输入处理后、绘制前调用）——左/右 pane
    /// 各自按光标与既有偏移重算：光标在窗口内 → 偏移不变（列表不动、
    /// 高亮上移）；越界 → 光标贴顶/贴底。左 pane `..` 行占用首行
    /// （结果视图除外，与渲染同口径）；左右 pane 均计入可见的 `..` 行。
    /// `list_h` 为单 pane 内区行数（左右同高）。
    pub fn sync_viewport_scroll(&mut self, list_h: usize) {
        let parent_taken = self.has_parent_row() && !self.in_results();
        let left_h = list_h.saturating_sub(parent_taken as usize);
        let left_cur = if self.on_parent_row { 0 } else { self.cursor };
        self.scroll = list_offset_sticky(left_cur, left_h, self.scroll);
        let right_h = list_h.saturating_sub((self.right.has_parent_row()) as usize);
        let right_cur = if self.right.on_parent_row {
            0
        } else {
            self.right.cursor
        };
        self.right.scroll = list_offset_sticky(right_cur, right_h, self.right.scroll);
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
            if delta < 0 && self.has_parent_row() {
                self.on_parent_row = true;
            } else if delta > 0 {
                self.on_parent_row = false;
            }
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
    /// M34-3：作用对象 = 焦点 pane；两侧 pane 均维护 `..` 行状态。
    pub fn search_move(&mut self, query: &str, forward: bool) -> bool {
        let (entries, cursor) = match self.focus {
            Focus::Right => (&mut self.right.entries, &mut self.right.cursor),
            Focus::Left => (&mut self.entries, &mut self.cursor),
        };
        let start = if forward {
            *cursor + 1
        } else {
            *cursor + entries.len().saturating_sub(1)
        };
        match find_match_in(entries, query, start, forward) {
            Some(idx) => {
                *cursor = idx;
                if self.focus == Focus::Left {
                    self.on_parent_row = false;
                }
                true
            }
            None => false,
        }
    }

    /// 搜索匹配计数（FIX-1）：`(当前匹配序号, 总匹配数)`；光标项不匹配时
    /// 序号为 0（调用方据此显示 `i/N` 或 `0/0`）。
    /// M34-3：计数对象 = 焦点 pane。
    pub fn search_status(&self, query: &str) -> (usize, usize) {
        let (entries, cursor) = match self.focus {
            Focus::Right => (&self.right.entries, &self.right.cursor),
            Focus::Left => (&self.entries, &self.cursor),
        };
        let matched = |item: &FileItem| query_matches(&item.name, query);
        let total = entries.iter().filter(|item| matched(item)).count();
        let cur = match entries.get(*cursor) {
            Some(item) if matched(item) => entries[..=*cursor]
                .iter()
                .filter(|item| matched(item))
                .count(),
            _ => 0,
        };
        (cur, total)
    }

    /// M18-1：就地应用单字段排序（对话框入口；光标保持在同一文件上）。
    pub fn apply_sort(&mut self, spec: SortSpec, config: Config) {
        self.apply_sort_multi(vec![spec], config);
    }

    /// M18-2：就地应用多字段排序（按序比较；目录优先保持 + 名称平手）；
    /// 光标保持在同一文件上。
    pub fn apply_sort_multi(&mut self, specs: Vec<SortSpec>, config: Config) {
        if specs.is_empty() {
            return;
        }
        self.sort = Some(specs);
        let keep = self.entries.get(self.cursor).map(|i| i.name.clone());
        self.sort_entries(&config);
        if let Some(name) = keep
            && let Some(idx) = self.entries.iter().position(|i| i.name == name)
        {
            self.cursor = idx;
        }
    }

    /// 排序比较器（目录优先 + 逐字段链 + 名称平手，确定性）。
    fn sort_entries(&mut self, config: &Config) {
        sort_entries_specs(
            &mut self.entries,
            self.sort.as_deref().unwrap_or(&[]),
            config,
        );
    }

    /// M47-4：右 pane 的 `:sort`——复用左 pane 的 `apply_sort_multi`，临时
    /// 投影右栏条目/光标/排序状态后交换回来，保证比较器只有一份实现。
    pub fn apply_sort_multi_focused(&mut self, specs: Vec<SortSpec>, config: Config) {
        std::mem::swap(&mut self.entries, &mut self.right.entries);
        std::mem::swap(&mut self.cursor, &mut self.right.cursor);
        std::mem::swap(&mut self.sort, &mut self.right.sort);
        self.apply_sort_multi(specs, config);
        std::mem::swap(&mut self.entries, &mut self.right.entries);
        std::mem::swap(&mut self.cursor, &mut self.right.cursor);
        std::mem::swap(&mut self.sort, &mut self.right.sort);
    }

    /// 重读当前物理目录条目并钳制光标（删除/撤销后刷新，M2-2）。
    /// M18-1：刷新后重应用 tab 排序，光标尽量保持在同一文件。
    /// 归档位置为 no-op（M7 接入）。
    pub fn refresh(&mut self, config: Config) -> std::io::Result<()> {
        let Location::Physical(dir) = &self.location else {
            return Ok(());
        };
        let mut entries = load_entries(dir, &config)?;
        if let Some(pat) = &self.filter {
            entries.retain(|item| filter_matches(pat, &item.name));
        }
        self.entries = entries;
        let keep_name = self.entries.get(self.cursor).map(|i| i.name.clone());
        if self.sort.is_some() {
            self.sort_entries(&config);
        }
        if let Some(name) = keep_name
            && let Some(idx) = self.entries.iter().position(|i| i.name == name)
        {
            self.cursor = idx;
        }
        self.cursor = self.cursor.min(self.entries.len().saturating_sub(1));
        self.visual = self
            .visual
            .map(|a| a.min(self.entries.len().saturating_sub(1)));
        Ok(())
    }

    /// 应用/清除 glob 过滤（M4-2，`=`；M47-2 焦点化）。空 pattern 清除过滤；
    /// 应用后光标与选区归零。焦点在右时写入右 pane 的独立过滤状态。
    pub fn apply_filter(&mut self, pattern: &str, config: Config) {
        let pat = pattern.trim().to_owned();
        let filter = if pat.is_empty() { None } else { Some(pat) };
        if self.focus == Focus::Right {
            self.right.filter = filter;
            if self.right.refresh(config).is_ok() {
                self.right.cursor = 0;
                self.right.visual = None;
            }
        } else {
            self.filter = filter;
            if self.refresh(config).is_ok() {
                self.cursor = 0;
                self.visual = None;
            }
        }
    }

    /// M47-3/4：焦点 pane 的条目/光标/选择集合——命令层（不经键位投影）
    /// 用它把源文件、批量集合等从"恒左栏"改为焦点栏。
    pub fn focused_entries(&self) -> &[FileItem] {
        if self.focus == Focus::Right {
            &self.right.entries
        } else {
            &self.entries
        }
    }

    pub fn focused_cursor(&self) -> usize {
        if self.focus == Focus::Right {
            self.right.cursor
        } else {
            self.cursor
        }
    }

    pub fn focused_location(&self) -> &Location {
        if self.focus == Focus::Right {
            &self.right.location
        } else {
            &self.location
        }
    }

    pub fn focused_on_parent_row(&self) -> bool {
        if self.focus == Focus::Right {
            self.right.on_parent_row
        } else {
            self.on_parent_row
        }
    }

    /// M48-3：焦点栏的进入/返回上级——右栏走 Pane 自身实现（无 Tab 历史栈）。
    pub fn focused_enter_selected(&mut self, config: Config) -> std::io::Result<()> {
        if self.focus == Focus::Right {
            self.right.enter_selected(config)
        } else {
            self.enter_selected(config)
        }
    }

    pub fn focused_return_parent(&mut self, config: Config) -> std::io::Result<()> {
        if self.focus == Focus::Right {
            self.right.return_parent(config)
        } else {
            self.return_parent(config)
        }
    }

    pub fn focused_tags(&self) -> &HashSet<String> {
        if self.focus == Focus::Right {
            &self.right.tags
        } else {
            &self.tags
        }
    }

    pub fn focused_visual_range(&self) -> Option<(usize, usize)> {
        if self.focus == Focus::Right {
            self.right
                .visual
                .map(|a| (a.min(self.right.cursor), a.max(self.right.cursor)))
        } else {
            self.visual_range()
        }
    }

    /// M47-4：`:select`/`:unselect` 作用于焦点 pane 的条目与标记集合。
    pub fn select_by_pattern_focused(&mut self, pattern: Option<&str>, select: bool) -> usize {
        if self.focus == Focus::Right {
            std::mem::swap(&mut self.entries, &mut self.right.entries);
            std::mem::swap(&mut self.cursor, &mut self.right.cursor);
            std::mem::swap(&mut self.tags, &mut self.right.tags);
            let affected = self.select_by_pattern(pattern, select);
            std::mem::swap(&mut self.entries, &mut self.right.entries);
            std::mem::swap(&mut self.cursor, &mut self.right.cursor);
            std::mem::swap(&mut self.tags, &mut self.right.tags);
            affected
        } else {
            self.select_by_pattern(pattern, select)
        }
    }

    pub fn clear_tags_focused(&mut self) {
        if self.focus == Focus::Right {
            self.right.tags.clear();
        } else {
            self.tags.clear();
        }
    }

    pub fn tagged_count_focused(&self) -> usize {
        self.focused_tags().len()
    }

    /// M47-2：焦点 pane 当前是否处于过滤态（Normal 模式 Esc 清除用）。
    pub fn focused_filter(&self) -> Option<&str> {
        let filter = if self.focus == Focus::Right {
            &self.right.filter
        } else {
            &self.filter
        };
        filter.as_deref()
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
            let mut items: Vec<(String, bool, bool)> = match std::fs::read_dir(dir) {
                Ok(rd) => rd
                    .filter_map(|e| e.ok())
                    .filter_map(|e| {
                        let ft = e.file_type().ok()?;
                        let is_link = ft.is_symlink();
                        // M27-1：符号链接跟随目标判定目录（同 load_entries；
                        // path 版 std::fs::metadata 穿越，e.metadata() 是
                        // lstat——M27-3 验证修正）。
                        let is_dir = if is_link {
                            std::fs::metadata(e.path())
                                .map(|m| m.is_dir())
                                .unwrap_or(false)
                        } else {
                            ft.is_dir()
                        };
                        Some((
                            e.file_name().to_string_lossy().into_owned(),
                            is_dir,
                            is_link,
                        ))
                    })
                    .collect(),
                Err(_) => return,
            };
            items.sort_by(|a, b| {
                let bd_cmp = b.1.cmp(&a.1);
                bd_cmp.then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
            });
            for (name, is_dir, is_link) in items {
                if *truncated {
                    return;
                }
                let rel = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}/{name}")
                };
                let full = dir.join(&name);
                if level < depth {
                    out.push(FileItem {
                        icon: crate::icons::icon_for_active(&full),
                        name: rel.clone(),
                        is_dir,
                        is_link,
                        is_exec: !is_dir
                            && std::fs::metadata(&full)
                                .map(|m| is_executable(&m, &name))
                                .unwrap_or(false),
                        size: if is_dir {
                            0
                        } else {
                            std::fs::metadata(&full).map(|m| m.len()).unwrap_or(0)
                        },
                        mtime: std::fs::metadata(&full)
                            .ok()
                            .and_then(|m| m.modified().ok())
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs())
                            .unwrap_or(0),
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
    /// M42：开关预览不改变焦点 pane——焦点留在原 pane，`Tab` 才显式进入
    /// 预览窗格；这样原 pane 的 j/k 仍切换文件，对侧预览自动跟随。
    pub fn toggle_view(&mut self) -> bool {
        self.view_mode = !self.view_mode;
        self.preview_focus = false;
        if self.view_mode {
            self.activate_viewer();
        } else {
            self.viewer = None;
        }
        self.view_mode
    }

    /// M42：按当前焦点 pane 的光标文件初始化/重建查看器（toggle 开启与
    /// 光标换文件时共用）。
    fn activate_viewer(&mut self) {
        let target = self.preview_path();
        // M17 VIEW-1：激活查看器；grep 命中定位到命中行（上移 2 行上下文）。
        // M39-1：二进制文件（前 8KB 含 NUL）→ hex 模式（16 字节/行，
        // 总行数 = ceil(size/16)，滚动走同一行机制、内容分块读取）。
        let hex = target.as_ref().is_some_and(|p| looks_binary(p));
        let hit_line = if hex {
            None
        } else {
            self.results
                .as_ref()
                .and_then(|r| r.items.get(self.cursor))
                .and_then(|i| i.line)
        };
        let (capped, total) = if hex {
            let size = target
                .as_ref()
                .and_then(|p| std::fs::metadata(p).ok())
                .map(|m| m.len())
                .unwrap_or(0);
            let total = size.div_ceil(16) as usize;
            (total, total)
        } else {
            self.viewer_lines()
                .map(|(lines, total)| (lines.len(), total))
                .unwrap_or((0, 0))
        };
        let scroll = hit_line
            .map(|l| l.saturating_sub(2))
            .unwrap_or(0)
            .min(capped.saturating_sub(1));
        self.viewer = Some(ViewerState {
            scroll,
            line_count: capped,
            total_lines: total,
            query: None,
            match_line: 0,
            hex,
            page: 0,
            path: target,
        });
    }

    /// M42：预览跟随——焦点留在原 pane 时，光标移动/目录切换后把对侧预览
    /// 切到当前光标文件；同一文件只钳制滚动位置（保留滚动）。
    pub fn refresh_viewer_follow(&mut self) {
        if !self.view_mode || self.preview_focus {
            return;
        }
        let target = self.preview_path();
        if self.viewer.as_ref().and_then(|v| v.path.clone()) == target {
            let max = self
                .viewer
                .as_ref()
                .map(|v| v.line_count.saturating_sub(1))
                .unwrap_or(0);
            if let Some(v) = self.viewer.as_mut() {
                v.scroll = v.scroll.min(max);
            }
        } else {
            self.activate_viewer();
        }
    }

    /// M17 VIEW-1：查看器内容行（激活时一次性读取；上限 10 万行）。
    /// 查看器内容行（封顶 100,000 行）与完整行数（M21-E3 截断提示）。
    pub fn viewer_lines(&self) -> Option<(Vec<String>, usize)> {
        let path = self.preview_path()?;
        let text = std::fs::read_to_string(path).ok()?;
        let total = text.lines().count();
        let lines = text.lines().take(100_000).map(|l| l.to_owned()).collect();
        Some((lines, total))
    }

    /// M39-1：hex 查看器内容行——分块读取（仅视口对应窗口，单次读 ≤1MB），
    /// 16 字节/行：偏移 + 十六进制 + ASCII 侧栏。非 hex 模式返回 None。
    pub fn viewer_hex_lines(&self, height: usize) -> Option<Vec<String>> {
        let v = self.viewer.as_ref()?;
        if !v.hex {
            return None;
        }
        let path = self.preview_path()?;
        let mut file = std::fs::File::open(&path).ok()?;
        use std::io::{Read, Seek, SeekFrom};
        let start = v.scroll.min(v.line_count.saturating_sub(1)) * 16;
        file.seek(SeekFrom::Start(start as u64)).ok()?;
        let want = height.saturating_mul(16).min(1024 * 1024);
        let mut buf = vec![0u8; want];
        let mut filled = 0usize;
        while filled < want {
            let n = file.read(&mut buf[filled..]).ok()?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        let mut out = Vec::with_capacity(height);
        for row in 0..height {
            let base = row * 16;
            if base >= filled {
                break;
            }
            let chunk = &buf[base..(base + 16).min(filled)];
            let mut hexs = String::with_capacity(49);
            let mut ascii = String::with_capacity(16);
            for i in 0..16 {
                match chunk.get(i) {
                    Some(&b) => {
                        hexs.push_str(&format!("{b:02x}"));
                        ascii.push(if (0x20..0x7f).contains(&b) {
                            b as char
                        } else {
                            '.'
                        });
                    }
                    None => hexs.push_str("  "),
                }
                if i == 7 {
                    hexs.push(' ');
                }
                hexs.push(' ');
            }
            out.push(format!("{:08x}  {}|{}|", start + base, hexs, ascii));
        }
        Some(out)
    }

    /// M39-1：`:goto <偏移>`——hex 查看器跳到偏移（16 字节行对齐、钳制）。
    pub fn viewer_goto_offset(&mut self, offset: u64) {
        let Some(v) = self.viewer.as_mut() else {
            return;
        };
        let line = (offset / 16) as usize;
        v.scroll = line.min(v.line_count.saturating_sub(1));
    }

    /// M39-2：PDF 查看器翻页——`[` 上一页（第 1 页停留）、`]` 下一页
    /// （越界页渲染失败由查看器显示失败提示，用户 `[` 退回）。
    pub fn viewer_prev_page(&mut self) {
        if let Some(v) = self.viewer.as_mut()
            && v.page > 0
        {
            v.page -= 1;
        }
    }

    pub fn viewer_next_page(&mut self) {
        if let Some(v) = self.viewer.as_mut() {
            v.page += 1;
        }
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
        let Some((lines, _total)) = self.viewer_lines() else {
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
        let Some((lines, _total)) = self.viewer_lines() else {
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
    /// entries 的 name 为显示串（find=全路径；grep=路径:行号: 内容；
    /// M24-8 compare=前缀标记 + 文件名，size 透传到属性列）。
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
                let name = if kind == crate::search::SearchKind::RenamePreview {
                    // M28-2：预览行——新名（text）为普通名时显示 "旧 → 新"；
                    // 跳过原因（（…）形态）时显示 "旧 （原因）"。
                    let old_name = item
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if item.text.starts_with('（') {
                        format!("{old_name} {}", item.text)
                    } else {
                        format!("{old_name} → {}", item.text)
                    }
                } else if kind == crate::search::SearchKind::Compare {
                    let file = item
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    format!("{} {}", item.text, file)
                } else {
                    match item.line {
                        Some(line) => format!("{}:{}: {}", shown, line, item.text),
                        None => shown,
                    }
                };
                FileItem {
                    icon: crate::icons::icon_for(&item.path, crate::icons::IconTheme::Dark),
                    name,
                    is_dir: false,
                    is_link: false,
                    is_exec: false,
                    size: item.size,
                    perm: String::new(),
                    mtime: 0,
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
        self.pending_rename = None;
        let _ = self.refresh(config);
    }

    /// M17 SRCH：结果虚拟目录守卫（只读边界）。
    pub fn in_results(&self) -> bool {
        self.results.is_some()
    }
    /// M38-1：find/grep 结果视图可操作（Delete/Yank/Rename 映射真实
    /// 路径）；compare/rename-preview 保持只读。
    pub fn results_actions_enabled(&self) -> bool {
        self.results
            .as_ref()
            .map(|r| {
                matches!(
                    r.kind,
                    crate::search::SearchKind::Find | crate::search::SearchKind::Grep
                )
            })
            .unwrap_or(false)
    }

    /// M17 SRCH：预览/操作的目标文件路径——结果模式取命中项，
    /// 物理视图取 base.join(name)；归档视图 None（预览未接入）；
    /// 远端视图 None（预览经 RemoteFs read 的链路在 M20 后接入）。
    /// M52-1：恒取焦点栏光标文件（`:view` 对侧预览语义）；frecency 预览走显式路径。
    pub fn preview_path(&self) -> Option<PathBuf> {
        if let Some(r) = &self.results {
            return r.items.get(self.cursor).map(|i| i.path.clone());
        }
        let (base, entries, cursor) = match self.focus {
            Focus::Right => (
                &self.right.location,
                &self.right.entries,
                &self.right.cursor,
            ),
            Focus::Left => (&self.location, &self.entries, &self.cursor),
        };
        match base {
            Location::Physical(p) => entries.get(*cursor).map(|i| p.join(&i.name)),
            Location::Archive { .. } | Location::Remote { .. } => None,
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
        self.pending_rename = None;
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
            // M19-2：远端导航由 app 层会话驱动（remote_navigate）。
            Location::Remote { .. } => Ok(()),
            Location::Physical(base) => {
                let target = base.join(&name);
                if is_dir {
                    let target = Location::Physical(target);
                    // M46-1/M46-4：重返已记忆位置时保留还原的光标；只有首次
                    // 进入（无记忆）才落 `..` 行（FEAT-2）。旧实现无条件置
                    // true，视觉上高亮 `..` 覆盖了还原值。
                    let had_memory = self.cursor_memory.contains_key(&target);
                    self.navigate_to(target, config)?;
                    self.on_parent_row = if had_memory {
                        false
                    } else {
                        self.has_parent_row()
                    };
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
                    let target = Location::Archive {
                        host: target,
                        inner: PathBuf::new(),
                    };
                    let had_memory = self.cursor_memory.contains_key(&target);
                    self.navigate_to(target, config)?;
                    self.on_parent_row = if had_memory {
                        false
                    } else {
                        self.has_parent_row()
                    };
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
                let target = Location::Archive {
                    host,
                    inner: PathBuf::from(crate::archive::normalize_inner(&new_inner)),
                };
                let had_memory = self.cursor_memory.contains_key(&target);
                self.navigate_to(target, config)?;
                self.on_parent_row = if had_memory {
                    false
                } else {
                    self.has_parent_row()
                };
                Ok(())
            }
        }
    }

    /// 返回父目录（M1-2，`h`）。
    /// 物理目录：已在文件系统根时为 no-op（ACCEPTANCE §3）。
    /// 归档内（M7-4）：上一层虚拟目录；归档根处返回宿主目录。
    /// M19-2：远端视图的 h 由 app 层 remote_up 驱动，此处 no-op。
    /// 读取失败时返回 Err 且不产生副作用（提示经 M6-1 进状态栏）。
    /// 返回父目录（h；M21-F4 返回后光标定位在来源条目上——子目录返回落在
    /// 该目录条目，归档根返回物理目录落在归档文件条目）。
    pub fn return_parent(&mut self, config: Config) -> std::io::Result<()> {
        match self.location.clone() {
            Location::Remote { .. } => Ok(()),
            Location::Archive { host, inner } => {
                let inner_str = inner.to_string_lossy().into_owned();
                if inner_str.is_empty() {
                    // 归档根：`h` 返回归档文件所在的物理目录（host 的父目录），
                    // 光标落在归档文件条目上（M21-F4）。
                    let leaf = host.file_name().map(|n| n.to_string_lossy().into_owned());
                    let host_dir = host.parent().unwrap_or(host.as_path()).to_path_buf();
                    self.navigate_to(Location::Physical(host_dir), config)?;
                    if let Some(name) = leaf
                        && let Some(idx) = locate_entry(&self.entries, &name)
                    {
                        self.cursor = idx;
                    }
                    return Ok(());
                }
                // 归档内：上一层虚拟目录（parent 为空串 = 归档根，仍留在归档内），
                // 光标落在来源虚拟目录条目上（M21-F4）。
                let (parent, leaf) = match inner_str.rsplit_once('/') {
                    Some((p, leaf)) => (crate::archive::normalize_inner(p), leaf.to_owned()),
                    None => (String::new(), inner_str.clone()),
                };
                self.navigate_to(
                    Location::Archive {
                        host,
                        inner: PathBuf::from(parent),
                    },
                    config,
                )?;
                if let Some(idx) = locate_entry(&self.entries, &leaf) {
                    self.cursor = idx;
                }
                Ok(())
            }
            Location::Physical(base) => {
                let Some(parent) = base.parent() else {
                    return Ok(());
                };
                if parent == base {
                    return Ok(());
                }
                // M21-F4：返回后光标落在来源目录条目上。
                let leaf = base.file_name().map(|n| n.to_string_lossy().into_owned());
                self.navigate_to(Location::Physical(parent.to_path_buf()), config)?;
                if let Some(name) = leaf
                    && let Some(idx) = locate_entry(&self.entries, &name)
                {
                    self.cursor = idx;
                }
                Ok(())
            }
        }
    }
}

/// M21-F4：在条目列表中定位名称对应的索引（渲染层的 `/` 后缀不进 name，
/// 直接按原名匹配；条目被隐藏/过滤时返回 None，调用方回落 cursor 0）。
fn locate_entry(entries: &[FileItem], name: &str) -> Option<usize> {
    entries.iter().position(|i| i.name == name)
}

/// FEAT-1/M41-4：列表视口偏移——保证光标可见（offset ≤ cursor <
/// offset+height）。渲染（ListState.with_offset）与鼠标命中测试共用同一
/// 推导，避免偏差。
/// M41-4 修复：**有状态**——光标在既有窗口 [prev, prev+height) 内时保持
/// prev（列表不动、高亮上移）；越出上界光标贴顶、越出下界光标贴底。prev
/// 取 `Tab.scroll` / `Pane.scroll`（每帧 `sync_viewport_scroll` 落盘）；
/// 原无状态推导每帧由 cursor 重算，会把光标钉在视口底行。
pub(crate) fn list_offset_sticky(cursor: usize, height: usize, prev: usize) -> usize {
    if height == 0 {
        return 0;
    }
    if cursor >= prev && cursor < prev + height {
        prev
    } else if cursor < prev {
        cursor
    } else {
        cursor + 1 - height
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
            // 归档内无执行位信息：按扩展名近似（M21-F1）。
            let is_exec = !e.is_dir && is_exec_name(&name);
            FileItem {
                icon,
                name,
                is_dir: e.is_dir,
                is_link: false,
                is_exec,
                size: e.size,
                perm: String::new(),
                mtime: 0,
            }
        })
        .collect())
}

/// 智能大小写匹配（M4-1）：查询含大写字母时区分大小写，否则不区分（vim/vifm smartcase）。
/// 环绕查找切片级实现（M34-3 抽出）：大小写不敏感子串；`start` 为起始
/// 扫描索引。`Tab::find_match`（左 pane）与右 pane 搜索共用。
pub(crate) fn find_match_in(
    entries: &[FileItem],
    query: &str,
    start: usize,
    forward: bool,
) -> Option<usize> {
    if entries.is_empty() || query.is_empty() {
        return None;
    }
    let len = entries.len();
    for i in 0..len {
        let idx = if forward {
            (start + i) % len
        } else {
            (start + len - i % len) % len
        };
        if query_matches(&entries[idx].name, query) {
            return Some(idx);
        }
    }
    None
}

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

/// `=` 过滤匹配（M21-E7）：含通配符（`*`/`?`）按 glob；无通配符时隐式
/// `*模式*`（vifm 语义：`src` 等效 `*src*`）。大小写不敏感。
pub fn filter_matches(pattern: &str, name: &str) -> bool {
    if pattern.contains('*') || pattern.contains('?') {
        glob_match(pattern, name)
    } else {
        let p: String = pattern.chars().flat_map(char::to_lowercase).collect();
        let s: String = name.chars().flat_map(char::to_lowercase).collect();
        s.contains(&p)
    }
}

/// 读取并排序目录条目：按配置决定隐藏文件与排序（M5-2）。
pub fn load_entries(dir: &Path, config: &Config) -> std::io::Result<Vec<FileItem>> {
    let mut items = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let is_link = ft.is_symlink();
        // M27-1：符号链接跟随目标判定目录（file_type/DirEntry::metadata 均
        // 是 lstat 语义、不穿越链接——link-to-dir 会误判为文件，M27-3 验证
        // 修正为 path 版 std::fs::metadata 穿越）——`l`/Enter 进入目标目录
        // 而非误开系统程序；断链（目标不存在）回退链接自身语义
        // （is_dir=false，列表不失败）。
        let is_dir = if is_link {
            std::fs::metadata(entry.path())
                .map(|m| m.is_dir())
                .unwrap_or(false)
        } else {
            ft.is_dir()
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        // show_hidden = false 时跳过 `.` 开头的隐藏条目。
        if !config.show_hidden && name.starts_with('.') {
            continue;
        }
        // 跟随目标取元数据（path 版穿越链接）；断链回退链接自身
        // （列表加载不受断链影响）。
        let meta =
            std::fs::metadata(entry.path()).or_else(|_| std::fs::symlink_metadata(entry.path()))?;
        let size = meta.len();
        let perm = perm_string(&meta);
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let is_exec = !is_dir && is_executable(&meta, &name);
        // M45-2b：类型感知图标——devicons 默认 `*` 时按可执行/文本/二进制推断，
        // jar/deb 给专用图标（替代无后缀 `*`）。
        let icon =
            crate::icons::icon_for_entry(&entry.path(), is_exec, crate::icons::active_theme());
        items.push(FileItem {
            icon,
            name,
            is_dir,
            is_link,
            is_exec,
            size,
            perm,
            mtime,
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
    use std::cmp::Ordering;
    use std::fs;

    #[test]
    fn m40_3_natural_cmp_total_order_regression() {
        // M40-3 proptest 抓到的真实缺陷回归：原逐字符实现的 Equal 非传递
        // （"1a" == "a" 但 "1a" < "2" < "a"）→ std sort panic。分块实现下
        // "1a" < "2"（数字段 < 文本段）且 "1a" < "a"，全序成立。
        // 精确触发序列（std sort 抽样输入）：
        let v = vec![
            "a", "0", "a", "0a", "n", "800", "0g", "a", "000", "000", "0", "a", "0", "a", "a", "0",
            "1", "a", "0", "0", "0", "0", "a", "a", "0", "0", "0", "0", "0", "0", "a", "a", "a",
            "a",
        ];
        let mut sorted = v.clone();
        sorted.sort_by(|a, b| natural_cmp(a, b));
        let once = sorted.clone();
        sorted.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(once, sorted, "排序幂等（全序）");
        // 语义断言：数字段 < 文本段；"1a" 与 "a" 不再相等。
        assert_eq!(natural_cmp("1a", "2"), Ordering::Less);
        assert_eq!(natural_cmp("1a", "a"), Ordering::Less);
        assert_eq!(natural_cmp("a", "2"), Ordering::Greater);
        assert_eq!(natural_cmp("00", "0"), Ordering::Equal, "前导零等值保持");
        assert_eq!(natural_cmp("a0", "a"), Ordering::Greater, "后缀数字段");
    }

    #[test]
    fn m24_2_natural_sort_digit_aware() {
        // ACCEPTANCE §23 M24-2：1.txt < 2.txt < 10.txt（数字感知）。
        assert_eq!(natural_cmp("1.txt", "2.txt"), Ordering::Less);
        assert_eq!(natural_cmp("2.txt", "10.txt"), Ordering::Less);
        assert_eq!(natural_cmp("10.txt", "2.txt"), Ordering::Greater);
        assert_eq!(natural_cmp("1.5.txt", "2.txt"), Ordering::Less, "小数段");
        assert_eq!(natural_cmp("abc", "abd"), Ordering::Less, "纯字母");
        assert_eq!(natural_cmp("same", "same"), Ordering::Equal);
        // 前导零：01 == 1（数值相同，继续下一段）。
        assert_eq!(natural_cmp("file01.txt", "file1.txt"), Ordering::Equal);
        // 混合：字母段不同 → 字典序。
        assert_eq!(natural_cmp("a1", "b1"), Ordering::Less);
    }

    #[test]
    fn m24_2_natural_cmp_case_variant() {
        // ACCEPTANCE §23 M24-2：受 ignore_case_sort 影响——接线处按配置分流。
        // 默认（ignore_case_sort=true）：大小写不敏感。
        assert_eq!(natural_cmp("a.txt", "B.txt"), Ordering::Less);
        assert_eq!(natural_cmp("B.txt", "a.txt"), Ordering::Greater);
        // ignore_case_sort=false：字节序 B(66) < a(97)。
        assert_eq!(natural_cmp_case("B.txt", "a.txt"), Ordering::Less);
        assert_eq!(natural_cmp_case("a.txt", "B.txt"), Ordering::Greater);
        // 敏感变体数字感知不变。
        assert_eq!(natural_cmp_case("2.txt", "10.txt"), Ordering::Less);
        assert_eq!(
            natural_cmp_case("File02", "file10"),
            Ordering::Less,
            "F(70) < f(102) 且数值段 2 < 10"
        );
    }
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
            is_link: false,
            is_exec: false,
            size: 0,
            perm: String::new(),
            mtime: 0,
        }
    }

    #[test]
    fn filetype_class_priority_and_extensions() {
        // M21-F1：分类优先级 目录 > 链接 > 可执行 > 归档 > 图片 > 视频 > 代码。
        assert_eq!(
            make_item("d", true).filetype_class(),
            FileTypeClass::Directory
        );

        let mut item = make_item("link", false);
        item.is_link = true;
        assert_eq!(item.filetype_class(), FileTypeClass::Symlink);

        let mut item = make_item("tool", false);
        item.is_exec = true;
        assert_eq!(item.filetype_class(), FileTypeClass::Executable);

        // 归档：支持虚拟浏览（zip/tar.gz）与不支持（7z）都按归档着色。
        assert_eq!(
            make_item("a.zip", false).filetype_class(),
            FileTypeClass::Archive
        );
        assert_eq!(
            make_item("a.tar.gz", false).filetype_class(),
            FileTypeClass::Archive
        );
        assert_eq!(
            make_item("a.7z", false).filetype_class(),
            FileTypeClass::Archive
        );
        assert_eq!(
            make_item("pic.PNG", false).filetype_class(),
            FileTypeClass::Image
        );
        assert_eq!(
            make_item("clip.mkv", false).filetype_class(),
            FileTypeClass::Video
        );
        assert_eq!(
            make_item("main.rs", false).filetype_class(),
            FileTypeClass::Code
        );
        assert_eq!(
            make_item("notes.txt", false).filetype_class(),
            FileTypeClass::Other
        );
    }

    #[test]
    fn is_exec_name_by_extension() {
        // M21-F1：扩展名近似可执行（Windows 本地/远端/归档内共用）。
        assert!(is_exec_name("tool.EXE"));
        assert!(is_exec_name("run.bat"));
        assert!(!is_exec_name("readme.md"));
        assert!(!is_exec_name("noext"));
    }

    #[test]
    fn format_mtime_shapes_local_and_zero() {
        // M21-F1：0 → "-"；正常值输出 `yyyy-mm-dd HH:mm:ss`（本地时区，
        // 期望值用 jiff 同源计算，跨时区可复现）。
        assert_eq!(format_mtime(0), "-");
        let expected = {
            let z = jiff::Timestamp::from_second(86_400)
                .unwrap()
                .to_zoned(jiff::tz::TimeZone::system());
            let (d, t) = (z.date(), z.time());
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                d.year(),
                d.month(),
                d.day(),
                t.hour(),
                t.minute(),
                t.second()
            )
        };
        assert_eq!(format_mtime(86_400), expected);
        assert_eq!(format_mtime(86_400).len(), 19);
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
    fn m451a_abbreviate_home_display_only() {
        // M45-1a：HOME 下缩写 ~/…，组件边界精确，仅显示（不修改 Location）。
        let home = Path::new("/home/u");
        assert_eq!(
            super::abbreviate_home(Path::new("/home/u/Downloads"), Some(home)),
            "~/Downloads"
        );
        assert_eq!(
            super::abbreviate_home(Path::new("/home/u"), Some(home)),
            "~"
        );
        assert_eq!(
            super::abbreviate_home(Path::new("/home/u2/shx"), Some(home)),
            "/home/u2/shx",
            "不得把 /home/u2 误缩写"
        );
        assert_eq!(
            super::abbreviate_home(Path::new("/tmp/x"), Some(home)),
            "/tmp/x"
        );
        assert_eq!(
            super::abbreviate_home(Path::new("/home/u/x"), None),
            "/home/u/x",
            "无 HOME 时原样"
        );
        // 真实 Location 的 base_dir 不受缩写影响。
        let loc = Location::Physical(PathBuf::from("/home/u/Downloads"));
        assert_eq!(loc.base_dir(), Path::new("/home/u/Downloads"));
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
    fn m453e_cursor_restored_on_h_l_roundtrip() {
        // M45-3e：在子目录移动光标 → h 回父 → l 重返，光标还原到离开时的位置。
        let tmp = TempDir::new().unwrap();
        let child = tmp.path().join("child");
        fs::create_dir(&child).unwrap();
        for i in 0..4 {
            fs::write(child.join(format!("f{i}.txt")), "").unwrap();
        }
        fs::write(tmp.path().join("sibling.txt"), "").unwrap();
        let cfg = Config::default();
        let mut tab = Tab::from_physical(tmp.path(), cfg.clone()).unwrap();

        // 进入 child（首次 → cursor 0）。
        let child_idx = tab.entries.iter().position(|e| e.name == "child").unwrap();
        tab.cursor = child_idx;
        tab.enter_selected(cfg.clone()).unwrap();
        assert_eq!(tab.location, Location::Physical(child.clone()));
        assert_eq!(tab.cursor, 0, "首次进入落 0");
        assert!(tab.on_parent_row, "首次进入无记忆 → 落 `..` 行（FEAT-2）");
        assert!(
            !tab.cursor_memory
                .contains_key(&Location::Physical(child.clone()))
        );

        // 在 child 内移动光标到 2。
        tab.cursor = 2;
        // h 回父（记忆 child=2；父目录光标按 M21-F4 落在 child 条目）。
        tab.return_parent(cfg.clone()).unwrap();
        assert_eq!(tab.location, Location::Physical(tmp.path().to_path_buf()));
        assert_eq!(tab.cursor_memory[&Location::Physical(child.clone())], 2);

        // l 重返 child → 还原 2。
        let child_idx = tab.entries.iter().position(|e| e.name == "child").unwrap();
        tab.cursor = child_idx;
        tab.enter_selected(cfg.clone()).unwrap();
        assert_eq!(tab.cursor, 2, "h→l 往返还原光标");
        assert!(
            !tab.on_parent_row,
            "M46-4：h→l 往返后不得高亮 `..` 覆盖还原的光标"
        );
    }

    #[test]
    fn m456c_pane_enters_archive_virtual_dir_and_returns() {
        // M45-6c：右 pane 归档文件 `l`/Enter 进入虚拟目录；h 上溯；文件 no-op。
        let tmp = TempDir::new().unwrap();
        let zip_path = tmp.path().join("bundle.zip");
        crate::archive::make_test_zip(&zip_path);
        fs::write(tmp.path().join("plain.txt"), "x").unwrap();
        let cfg = Config::default();
        let mut pane = Pane::from_physical(tmp.path(), cfg.clone()).unwrap();

        let zip_idx = pane
            .entries
            .iter()
            .position(|e| e.name == "bundle.zip")
            .unwrap();
        pane.cursor = zip_idx;
        pane.enter_selected(cfg.clone()).unwrap();
        assert_eq!(
            pane.location,
            Location::Archive {
                host: zip_path.clone(),
                inner: PathBuf::new()
            },
            "进入归档虚拟根"
        );
        let names: Vec<&str> = pane.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"readme.md") && names.contains(&"src"),
            "{names:?}"
        );

        // 归档内虚拟目录继续进入。
        let src_idx = pane.entries.iter().position(|e| e.name == "src").unwrap();
        pane.cursor = src_idx;
        pane.enter_selected(cfg.clone()).unwrap();
        assert_eq!(
            pane.location,
            Location::Archive {
                host: zip_path.clone(),
                inner: PathBuf::from("src")
            }
        );
        assert!(pane.entries.iter().any(|e| e.name == "main.rs"));

        // h 上溯回归档根。
        pane.return_parent(cfg.clone()).unwrap();
        assert_eq!(
            pane.location,
            Location::Archive {
                host: zip_path.clone(),
                inner: PathBuf::new()
            }
        );
        // 再 h 回物理宿主目录。
        pane.return_parent(cfg.clone()).unwrap();
        assert_eq!(pane.location, Location::Physical(tmp.path().to_path_buf()));

        // 归档内普通文件 no-op。
        let zip_idx = pane
            .entries
            .iter()
            .position(|e| e.name == "bundle.zip")
            .unwrap();
        pane.cursor = zip_idx;
        pane.enter_selected(cfg.clone()).unwrap();
        let file_idx = pane
            .entries
            .iter()
            .position(|e| e.name == "readme.md")
            .unwrap();
        let before = pane.location.clone();
        pane.cursor = file_idx;
        pane.enter_selected(cfg.clone()).unwrap();
        assert_eq!(pane.location, before, "文件 no-op");
    }

    #[test]
    fn m453e_cursor_restored_clamped_and_pane() {
        // 钳制：记忆的光标超出新条目数时取末尾；右 pane 同源。
        let tmp = TempDir::new().unwrap();
        let child = tmp.path().join("child");
        fs::create_dir(&child).unwrap();
        fs::write(child.join("only.txt"), "").unwrap(); // 仅 1 项
        fs::write(tmp.path().join("sibling.txt"), "").unwrap();
        let cfg = Config::default();

        // 左 pane：手工注入超限记忆。
        let mut tab = Tab::from_physical(tmp.path(), cfg.clone()).unwrap();
        tab.cursor_memory
            .insert(Location::Physical(child.clone()), 99);
        let child_idx = tab.entries.iter().position(|e| e.name == "child").unwrap();
        tab.cursor = child_idx;
        tab.enter_selected(cfg.clone()).unwrap();
        assert_eq!(tab.cursor, 0, "99 钳制到仅 1 项的末位 0");
        assert!(!tab.on_parent_row, "有记忆时不落 `..` 行（M46-4）");

        // 右 pane：h→l 往返同样还原。
        let mut pane = Pane::from_physical(tmp.path(), cfg.clone()).unwrap();
        let child_idx = pane.entries.iter().position(|e| e.name == "child").unwrap();
        pane.cursor = child_idx;
        pane.enter_selected(cfg.clone()).unwrap();
        assert_eq!(pane.location, Location::Physical(child.clone()));
        pane.cursor = 0; // 唯一条目
        pane.return_parent(cfg.clone()).unwrap();
        assert_eq!(pane.cursor_memory[&Location::Physical(child.clone())], 0);
        let child_idx = pane.entries.iter().position(|e| e.name == "child").unwrap();
        pane.cursor = child_idx;
        pane.enter_selected(cfg.clone()).unwrap();
        assert_eq!(pane.cursor, 0, "右 pane 还原");
        assert!(!pane.on_parent_row, "右 pane 重返不落 `..` 行（M46-4）");
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
            view_mode: false,
            preview_focus: false,
            right: Pane::from_physical(&root, Config::default()).unwrap(),
            focus: Focus::Left,
            on_parent_row: false,
            name: None,
            cursor_memory: std::collections::HashMap::new(),
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
    fn m181_apply_sort_fields_and_cursor_retention() {
        // 手工条目：mtime/size 显式可控（不依赖真实 fs 时间戳）。
        let mk = |name: &str, is_dir: bool, size: u64, mtime: u64| FileItem {
            icon: crate::icons::icon_for(std::path::Path::new(name), crate::icons::IconTheme::Dark),
            name: name.to_owned(),
            is_dir,
            is_link: false,
            is_exec: false,
            size,
            perm: String::new(),
            mtime,
        };
        let mut tab = Tab {
            location: Location::Physical(PathBuf::from("/x")),
            entries: vec![
                mk("b.txt", false, 2, 100),
                mk("a.doc", false, 0, 200),
                mk("c.md", false, 15, 300),
            ],
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
            view_mode: false,
            preview_focus: false,
            right: Pane::from_physical(Path::new("."), Config::default()).unwrap(),
            focus: Focus::Left,
            on_parent_row: false,
            name: None,
            cursor_memory: std::collections::HashMap::new(),
        };
        let cfg = Config::default(); // sort_dirs_first 默认 true（无目录，不影响）

        // 名称升序 → 光标跟随原文件（b.txt）。
        tab.apply_sort(
            SortSpec {
                field: SortField::Name,
                desc: false,
            },
            cfg.clone(),
        );
        let names: Vec<&str> = tab.entries.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["a.doc", "b.txt", "c.md"]);
        assert_eq!(tab.entries[tab.cursor].name, "b.txt", "光标跟随");

        // 名称降序。
        tab.apply_sort(
            SortSpec {
                field: SortField::Name,
                desc: true,
            },
            cfg.clone(),
        );
        let names: Vec<&str> = tab.entries.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["c.md", "b.txt", "a.doc"]);
        assert_eq!(tab.entries[tab.cursor].name, "b.txt");

        // 大小升序：a.doc(0) < b.txt(2) < c.md(15)。
        tab.apply_sort(
            SortSpec {
                field: SortField::Size,
                desc: false,
            },
            cfg.clone(),
        );
        let names: Vec<&str> = tab.entries.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["a.doc", "b.txt", "c.md"]);

        // mtime 降序：c.md(300) 最前。
        tab.apply_sort(
            SortSpec {
                field: SortField::Mtime,
                desc: true,
            },
            cfg.clone(),
        );
        assert_eq!(tab.entries[0].name, "c.md");

        // 扩展名升序：doc < md < txt。
        tab.apply_sort(
            SortSpec {
                field: SortField::Ext,
                desc: false,
            },
            cfg.clone(),
        );
        let names: Vec<&str> = tab.entries.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["a.doc", "c.md", "b.txt"]);
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
    fn m34_3_right_pane_search_move_and_status() {
        // M34-3：focus=Right 时搜索作用于 right pane（左 pane 不动）。
        let tmp = tempfile::TempDir::new().unwrap();
        for n in ["alpha.txt", "beta.txt", "gamma.txt"] {
            std::fs::write(tmp.path().join(n), "x").unwrap();
        }
        let mut tab = Tab::from_physical(tmp.path(), Config::default()).unwrap();
        tab.right = Pane::from_physical(tmp.path(), Config::default()).unwrap();
        tab.focus = Focus::Right;
        assert_eq!(tab.search_status("gam"), (0, 1), "光标在 alpha → 序号 0");
        assert!(tab.search_move("gam", true));
        assert_eq!(tab.right.cursor, 2);
        assert_eq!(tab.cursor, 0, "左 pane 光标不动");
        assert_eq!(tab.search_status("gam"), (1, 1));
        // 单匹配自回绕（M1-7 既有语义）：反向下一个仍是 gamma。
        assert!(tab.search_move("gam", false));
        assert_eq!(tab.right.cursor, 2);
        // 多匹配查询（三个文件名均含 'a'）：反向 → beta（第 2 个匹配）。
        assert!(tab.search_move("a", false));
        assert_eq!(tab.right.cursor, 1);
        assert_eq!(tab.search_status("a"), (2, 3));
    }

    #[test]
    fn m34_4_preview_path_follows_focus() {
        // M34-4：preview_path = 焦点 pane 光标文件（修复右焦点预览错位）。
        let left = tempfile::TempDir::new().unwrap();
        std::fs::write(left.path().join("left-a.txt"), "L").unwrap();
        let right = tempfile::TempDir::new().unwrap();
        std::fs::write(right.path().join("right-b.txt"), "R").unwrap();
        let mut tab = Tab::from_physical(left.path(), Config::default()).unwrap();
        tab.right = Pane::from_physical(right.path(), Config::default()).unwrap();
        assert_eq!(tab.preview_path(), Some(left.path().join("left-a.txt")));
        tab.focus = Focus::Right;
        assert_eq!(tab.preview_path(), Some(right.path().join("right-b.txt")));
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

/// M40-3：属性测试（proptest）——glob 匹配 / 自然排序不变量。
#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// P1：`*` 匹配任意名称（任意字符含空串）。
        #[test]
        fn glob_star_matches_anything(name in "[^\n]{0,40}") {
            prop_assert!(glob_match("*", &name));
        }

        /// P2：无通配符 pattern → 匹配当且仅当小写相等。
        #[test]
        fn glob_literal_pattern_is_equality(
            p in "[a-z0-9._]{0,12}",
            n in "[a-z0-9._]{0,12}",
        ) {

            let expected = p.to_lowercase() == n.to_lowercase();
            prop_assert_eq!(glob_match(&p, &n), expected);
        }

        /// P3：`*p*` 等价包含语义（p 不含通配符）。
        #[test]
        fn glob_star_p_star_is_contains(
            p in "[a-z0-9._]{1,12}",
            n in "[a-z0-9._]{0,20}",
        ) {
            let contains = n.to_lowercase().contains(&p.to_lowercase());
            prop_assert_eq!(glob_match(&format!("*{p}*"), &n), contains);
        }

        /// P4：`?` 计数——n 个 `?` 匹配当且仅当长度为 n（按字符）。
        #[test]
        fn glob_question_marks_count_length(
            qs in 1usize..6,
            n in "[a-z0-9]{0,10}",
        ) {
            let pattern = "?".repeat(qs);
            prop_assert_eq!(glob_match(&pattern, &n), n.chars().count() == qs);
        }

        /// P5：自然排序对称性——cmp(a,b) 与 cmp(b,a) 严格互逆。
        #[test]
        fn natural_cmp_symmetric(a in "[a-z0-9]{0,12}", b in "[a-z0-9]{0,12}") {
            let ab = natural_cmp(&a, &b);
            let ba = natural_cmp(&b, &a);
            prop_assert_eq!(ab, ba.reverse());
        }

        /// P6：数字感知——同数字前缀下 m < n 必排在前（含 10 vs 9 陷阱）。
        #[test]
        fn natural_cmp_numeric_aware(
            prefix in "[a-z]{1,6}",
            m in 0u64..1_000_000,
            n in 0u64..1_000_000,
        ) {
            prop_assume!(m < n);
            let a = format!("{prefix}{m}.txt");
            let b = format!("{prefix}{n}.txt");
            prop_assert_eq!(natural_cmp(&a, &b), std::cmp::Ordering::Less);
        }

        /// P7：排序幂等——自然排序两次结果一致。
        #[test]
        fn natural_sort_idempotent(names in proptest::collection::vec("[a-z0-9]{1,10}", 0..200)) {
            let mut v = names;
            v.sort_by(|a, b| natural_cmp(a, b));
            let once = v.clone();
            v.sort_by(|a, b| natural_cmp(a, b));
            prop_assert_eq!(once, v);
        }
    }
}
