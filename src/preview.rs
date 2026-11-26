//! 预览渲染器（M8-8；SPEC §4.9）。
//!
//! `PreviewRenderer` 是 TUI 与 `pick` **共用**的预览抽象——不允许为
//! pick 单独实现一套（NOTES §4.8）。文本/目录内建；图像为占位
//! （M11-1/M11-2 接入 `ratatui-image` 协议探测 + halfblocks 回退）。
//!
//! M8-8 仅固化共享渲染入口与文本/目录实现；消费方：M11-1/M11-2
//! （图像渲染）、M11-9（Office 接入）——届时 TUI 接入预览窗格。

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// M61-2：带阈值的 viewer 载荷——超阈值本地文件不整文件读取，只估算行数。
pub fn load_viewer_payload_at(path: &Path, threshold: u64) -> ViewerPayload {
    if crate::viewport::use_for(path, threshold) {
        let hex = crate::tab::looks_binary(path);
        let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        return ViewerPayload {
            path: path.to_path_buf(),
            lines: Vec::new(),
            // 估算值（约 80 字节/行），仅用于滚动条/钳制；真实行由视口读取器给出。
            total: (size / 80) as usize,
            hex,
        };
    }
    load_viewer_payload(path)
}

/// M60-1：后台 viewer 载荷（路径 + 行 + 完整行数 + hex 判定）。
#[derive(Debug, Clone)]
pub struct ViewerPayload {
    pub path: PathBuf,
    pub lines: Vec<String>,
    pub total: usize,
    pub hex: bool,
}

/// M60-1：在后台线程读取/判定文件内容（纯函数，不触碰 AppState/redb）。
pub fn load_viewer_payload(path: &Path) -> ViewerPayload {
    let hex = crate::tab::looks_binary(path);
    if hex {
        let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let total = size.div_ceil(16) as usize;
        return ViewerPayload {
            path: path.to_path_buf(),
            lines: Vec::new(),
            total,
            hex: true,
        };
    }
    let text = fs::read_to_string(path).unwrap_or_default();
    let total = text.lines().count();
    let lines = text.lines().take(100_000).map(str::to_owned).collect();
    ViewerPayload {
        path: path.to_path_buf(),
        lines,
        total,
        hex: false,
    }
}

/// M60-4：后台计算 Office/归档/PDF 文本的预览行（只读；不碰 Frame/redb）。
pub fn load_special_preview_lines(path: &Path, external_enabled: bool) -> Vec<String> {
    match preview_kind(path) {
        PreviewKind::Archive => {
            let cache = ArchivePreviewCache::default();
            archive_preview_cached_with_config(path, 200, &cache, external_enabled).line_texts()
        }
        _ => render_preview(path, 200).line_texts(),
    }
}

/// 预览内容的最大读取字节数（防止大文件撑爆内存/渲染）。
const MAX_PREVIEW_BYTES: u64 = 512 * 1024;

/// M46-8：归档 `w` 预览的成员列表缓存（内存态，进程重启重建）。
/// 键 = (路径, mtime, size)：文件未变则直接复用，避免每帧重复 `open_dispatch`
/// 并对压缩 tar（尤其 xz/bz2）整文件解码。
pub type ArchivePreviewCache =
    std::cell::RefCell<std::collections::HashMap<(PathBuf, u64, u64), ArchivePreviewData>>;

/// M46-8：缓存值——成员列表 + 是否因过大而跳过索引。
#[derive(Debug, Clone)]
pub struct ArchivePreviewData {
    pub members: Vec<(String, bool)>,
    pub too_large: bool,
}

/// M46-8：超过该压缩体积不生成成员列表（如 firefox tarball），只给
/// 「按 l 进入浏览」提示——vifm 的快速返回体验，杜绝卡顿。
const ARCHIVE_PREVIEW_MAX_BYTES: u64 = 64 * 1024 * 1024;
/// 轻量成员上限（zip 中央目录可能有数万项，列表只取前若干）。
const ARCHIVE_PREVIEW_MAX_MEMBERS: usize = 4000;

/// M48-7：外部列目录命令（按归档类型，参数固定，不经 shell）。
fn archive_listing_tool(path: &Path) -> Option<(&'static str, &'static [&'static str])> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if name.ends_with(".zip")
        || name.ends_with(".jar")
        || name.ends_with(".war")
        || name.ends_with(".apk")
        || name.ends_with(".docx")
        || name.ends_with(".xlsx")
        || name.ends_with(".pptx")
    {
        Some(("unzip", &["-Z1"]))
    } else if name.ends_with(".tar.gz")
        || name.ends_with(".tgz")
        || name.ends_with(".tar.bz2")
        || name.ends_with(".tbz2")
        || name.ends_with(".tar.xz")
        || name.ends_with(".txz")
        || name.ends_with(".tar.zst")
        || name.ends_with(".tar")
    {
        Some(("tar", &["-tf"]))
    } else if name.ends_with(".deb") {
        // M53-7：deb 是 ar 容器，`tar -tf` 失败 → 用 dpkg-deb -c 直列成员，
        // 避免大包回退内置 VFS 后触发 64MB「归档较大」提示。
        Some(("dpkg-deb", &["-c"]))
    } else if name.ends_with(".7z") || name.ends_with(".rar") {
        Some(("7z", &["l", "-ba"]))
    } else {
        None
    }
}

/// M53-7：解析不同列目录工具的输出行 → (成员路径, 是否目录)。
fn parse_listing_line(tool: &str, raw: &str) -> Option<(String, bool)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = if tool == "dpkg-deb" || tool == "7z" {
        // `dpkg-deb -c` / `7z l -ba` 输出为多列表格，路径是最后一列。
        trimmed.split_whitespace().last()?
    } else {
        trimmed
    };
    let path = path.strip_prefix("./").unwrap_or(path).replace('\\', "/");
    let is_dir = path.ends_with('/');
    Some((path.trim_end_matches('/').to_owned(), is_dir))
}

/// M48-7：用外部只读列目录命令快速取成员（vifm fileviewer 同思路）。
/// stdout 有硬上限；失败/超限/空结果返回 None 由调用方回退内置 VFS。
fn external_archive_members(path: &Path) -> Option<Vec<(String, bool)>> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let (tool, args) = archive_listing_tool(path)?;
    let mut child = Command::new(tool)
        .args(args)
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .ok()?;
    let cap = 4 * 1024 * 1024usize;
    let mut out = Vec::new();
    let stdout = child.stdout.take()?;
    stdout.take((cap + 1) as u64).read_to_end(&mut out).ok()?;
    let _ = child.wait();
    if out.len() > cap {
        return None;
    }
    let text = String::from_utf8_lossy(&out);
    let mut members: Vec<(String, bool)> = text
        .lines()
        .filter_map(|raw| parse_listing_line(tool, raw))
        .filter(|(s, _)| !s.is_empty())
        .filter(|(s, _)| !s.starts_with('/') && !s.split('/').any(|p| p == ".."))
        .collect();
    if members.is_empty() {
        return None;
    }
    members.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    members.truncate(ARCHIVE_PREVIEW_MAX_MEMBERS);
    Some(members)
}

fn archive_cache_key(path: &Path) -> (PathBuf, u64, u64) {
    let meta = fs::metadata(path).ok();
    let mtime = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let size = meta.map(|m| m.len()).unwrap_or(0);
    (path.to_path_buf(), mtime, size)
}

/// 预览结果：可直接交给 ratatui 渲染的行列表。
pub struct Preview {
    lines: Vec<Line<'static>>,
}

impl Preview {
    /// 转换为 `Paragraph`（TUI 渲染用）。
    pub fn into_paragraph(self) -> Paragraph<'static> {
        Paragraph::new(self.lines)
    }

    /// 行数（测试与截断判断用）。
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// 行文本（测试与 pick 共用入口）。
    pub fn line_texts(&self) -> Vec<String> {
        self.lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.clone()).collect())
            .collect()
    }
}

/// 预览内容类型（M11-2；SPEC §4.9 `PreviewContent` 的分流语义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewKind {
    /// 目录成员列表。
    Directory,
    /// 图片文件（由 `ImageEngine` 渲染到预览区域）。
    Image,
    /// Office 文档（DOCX/XLSX/PPTX 等，纯 Rust 文本提取）。
    Office,
    /// PDF 文档（pdf-inspector 文本提取 → 视觉路径回退提示）。
    Pdf,
    /// 归档文件（M45-4b）：显示成员名列表（复用 ArchiveFs::list）。
    /// 注意 Office/PDF 判定必须先于本项——docx/xlsx 是 zip 容器。
    Archive,
    /// 其余按文本预览。
    Text,
}

/// 预览分流入口（M11-2）：TUI 与 `pick` 共用，保证同源渲染。
pub fn preview_kind(path: &Path) -> PreviewKind {
    if path.is_dir() {
        PreviewKind::Directory
    } else if crate::image::is_image_file(path) {
        PreviewKind::Image
    } else if crate::office::is_office_file(path) {
        PreviewKind::Office
    } else if crate::pdf::is_pdf_file(path) {
        PreviewKind::Pdf
    } else if path
        .file_name()
        .map(|n| {
            let name = n.to_string_lossy();
            crate::archive::is_archive_file(&name) || crate::archive::is_unsupported_archive(&name)
        })
        .unwrap_or(false)
    {
        // M45-4b：Office/PDF 已在上面命中并提前返回——zip 容器
        // docx/xlsx/pptx 不会被归到归档；jar/apk/war 走归档成员列表；
        // .rar（unsupported）也归归档，由 render_preview 回退信息面板。
        PreviewKind::Archive
    } else {
        PreviewKind::Text
    }
}

/// 为 `path` 生成预览（M8-8）：
/// - 目录 → 成员列表（前若干项，不递归）；
/// - Office 文档 → `office_oxide` 文本提取（M11-9），失败降级提示；
/// - 普通文件 → 文本内容（截断保护）；二进制/读取失败 → 友好提示；
/// - 图像文件 → 友好提示行（真实渲染走 `ImageEngine`；此分支服务
///   解码失败降级与无图形能力的调用方）。
pub fn render_preview(path: &Path, max_lines: usize) -> Preview {
    if path.is_dir() {
        return directory_preview(path, max_lines);
    }
    // M39-2：CSV/TSV 表格对齐预览（naive 分隔——引号内分隔符不解析，
    // SIMPLIFIED；TUI 预览与 pick 同源）。
    let ext_lower = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if matches!(ext_lower.as_str(), "csv" | "tsv") {
        return csv_preview(path, max_lines, &ext_lower);
    }
    if crate::office::is_office_file(path) {
        return match crate::office::extract_preview_text(path, max_lines) {
            Ok(body) => text_preview(&body),
            Err(e) => text_preview(&format!(
                "[Office 文档解析失败] {}\n{e}\n按 Enter 用系统默认程序打开。",
                path.display()
            )),
        };
    }
    if crate::pdf::is_pdf_file(path) {
        // M11-11：Poppler-free——干净环境（无 Poppler/MuPDF）只走
        // pdf-inspector 文本提取；视觉路径回退为提示行（SPEC 边界：
        // 不保证零配置视觉预览）。
        return match crate::pdf::preview_pdf(path) {
            crate::pdf::PdfPreview::Text(md) => {
                let body: String = md.lines().take(max_lines).collect::<Vec<_>>().join("\n");
                text_preview(&body)
            }
            crate::pdf::PdfPreview::NeedsVisual => text_preview(&format!(
                "[PDF 文本层不足] {}\n\n视觉预览需要启用 pdf-visual feature\n或安装 pdftoppm/mutool（可选外部工具）。\n按 Enter 用系统默认程序打开。",
                path.display()
            )),
            crate::pdf::PdfPreview::Unavailable(e) => text_preview(&format!(
                "[PDF 文本提取失败] {}\n{e}\n按 Enter 用系统默认程序打开。",
                path.display()
            )),
        };
    }
    if crate::image::is_image_file(path) {
        return text_preview(&format!(
            "[图像预览] {}\n\n图形渲染不可用（协议探测失败或解码错误）。\n按 Enter 用系统默认程序打开。",
            path.display()
        ));
    }
    if preview_kind(path) == PreviewKind::Archive {
        return archive_preview(path, max_lines);
    }
    text_preview(&read_text_preview(path, max_lines))
}

/// M45-4b：归档成员预览——打开 ArchiveFs 列根目录，复用 entries_preview
/// 渲染成员名列表。损坏/加密/.rar → 结构化信息面板（不 panic）。
fn archive_preview(path: &Path, max_lines: usize) -> Preview {
    match archive_members(path) {
        Ok(data) => archive_lines_from_data(path, &data, max_lines),
        Err(e) => text_preview(&e),
    }
}

/// M46-8：生成（或复用缓存的）归档预览。缓存键含 mtime+size，文件变化后
/// 自动重建；大归档只显示提示，不触发整文件解压。
pub fn archive_preview_cached(
    path: &Path,
    max_lines: usize,
    cache: &ArchivePreviewCache,
) -> Preview {
    archive_preview_cached_with_config(path, max_lines, cache, true)
}

/// M48-7：带配置的归档预览（外部列目录可关闭）。
pub fn archive_preview_cached_with_config(
    path: &Path,
    max_lines: usize,
    cache: &ArchivePreviewCache,
    external_enabled: bool,
) -> Preview {
    let key = archive_cache_key(path);
    if let Ok(data) = cache.try_borrow()
        && let Some(hit) = data.get(&key)
    {
        return archive_lines_from_data(path, hit, max_lines);
    }
    let data = match archive_members_with_config(path, external_enabled) {
        Ok(d) => d,
        Err(e) => return text_preview(&e),
    };
    if let Ok(mut cache) = cache.try_borrow_mut() {
        // 只保留最近少量条目（路径切换不会无限增长）。
        if cache.len() >= 16 {
            cache.clear();
        }
        cache.insert(key, data.clone());
    }
    archive_lines_from_data(path, &data, max_lines)
}

/// M46-8：只读成员列表——加输入大小护栏与成员上限，避免大 xz/bz2 预览卡死。
fn archive_members(path: &Path) -> Result<ArchivePreviewData, String> {
    archive_members_with_config(path, true)
}

fn archive_members_with_config(
    path: &Path,
    external_enabled: bool,
) -> Result<ArchivePreviewData, String> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if crate::archive::is_unsupported_archive(&name) {
        return Err(file_info_text(path));
    }
    // M48-7：外部列目录先试（对大归档也生效——这正是内置 64MB 护栏绕过的场景）。
    if external_enabled && let Some(members) = external_archive_members(path) {
        return Ok(ArchivePreviewData {
            members,
            too_large: false,
        });
    }
    if fs::metadata(path).map(|m| m.len()).unwrap_or(0) > ARCHIVE_PREVIEW_MAX_BYTES {
        return Ok(ArchivePreviewData {
            members: Vec::new(),
            too_large: true,
        });
    }
    let fs_archive = crate::archive::open_dispatch(path).map_err(|e| {
        format!(
            "[归档打开失败] {}\n{e}\n{}",
            path.display(),
            file_info_text(path)
        )
    })?;
    let entries = fs_archive.list("").map_err(|e| {
        format!(
            "[归档成员读取失败] {}\n{e}\n按 Enter 用系统默认程序打开。",
            path.display()
        )
    })?;
    let mut items: Vec<(String, bool)> = entries
        .into_iter()
        .take(ARCHIVE_PREVIEW_MAX_MEMBERS)
        .map(|e| (e.path, e.is_dir))
        .collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Ok(ArchivePreviewData {
        members: items,
        too_large: false,
    })
}

/// M47-1：预览条目的图标缩进。按路径层级逐级缩进（基础一级 2 空格），
/// 只作用于显示层，不改变条目内容、排序或导航。
fn preview_indent(entry_path: &str) -> String {
    let depth = entry_path
        .split('/')
        .filter(|part| !part.is_empty())
        .count()
        .saturating_sub(1);
    "  ".repeat(1 + depth)
}

/// M46-8：把缓存的成员数据渲染成行（廉价，可每帧执行）。
fn archive_lines_from_data(path: &Path, data: &ArchivePreviewData, max_lines: usize) -> Preview {
    if data.too_large {
        return text_preview(&format!(
            "归档 {} 较大，为保持界面响应未展开成员列表。\n按 l / Enter 进入虚拟目录浏览（只读），或按 Enter 用系统默认程序打开。",
            path.display()
        ));
    }
    let items = &data.members;
    let mut lines = vec![Line::from(format!(
        "归档 {}（{} 项，只读）",
        path.display(),
        items.len()
    ))];
    for (p, is_dir) in items.iter().take(max_lines.saturating_sub(1)) {
        let label = if *is_dir { format!("{p}/") } else { p.clone() };
        let icon = crate::icons::icon_for_active(std::path::Path::new(p));
        lines.push(Line::from(format!(
            "{} {} {label}",
            preview_indent(p),
            icon.glyph
        )));
    }
    if items.len() > max_lines.saturating_sub(1) {
        lines.push(Line::from(format!(
            "  …（共 {} 项，仅显示前 {}）",
            items.len(),
            max_lines.saturating_sub(1)
        )));
    }
    Preview { lines }
}

/// 目录预览：成员名列表（目录后缀 `/`，按名排序，截断到 max_lines）。
fn directory_preview(path: &Path, max_lines: usize) -> Preview {
    let mut lines: Vec<Line<'static>> = Vec::new();
    match std::fs::read_dir(path) {
        Ok(iter) => {
            let mut names: Vec<(String, bool)> = iter
                .filter_map(|e| e.ok())
                .map(|e| {
                    let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    (e.file_name().to_string_lossy().into_owned(), is_dir)
                })
                .collect();
            names.sort_by(|a, b| {
                b.1.cmp(&a.1)
                    .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
            });
            lines.push(Line::from(format!(
                "目录 {}（{} 项）",
                path.display(),
                names.len()
            )));
            for (name, is_dir) in names.iter().take(max_lines.saturating_sub(1)) {
                let label = if *is_dir {
                    format!("{name}/")
                } else {
                    name.clone()
                };
                // M9-4：归档/预览共用同一 devicons 图标 + 激活主题。
                let full = path.join(name);
                let icon = crate::icons::icon_for_active(&full);
                lines.push(Line::from(format!(
                    "{} {} {label}",
                    preview_indent(name),
                    icon.glyph
                )));
            }
            if names.len() > max_lines.saturating_sub(1) {
                lines.push(Line::from(format!(
                    "  …（共 {} 项，仅显示前 {}）",
                    names.len(),
                    max_lines.saturating_sub(1)
                )));
            }
        }
        Err(e) => {
            lines.push(Line::from(format!("目录读取失败: {e}")));
        }
    }
    Preview { lines }
}

/// 文本文件预览：读取（截断保护），按行渲染。
fn text_preview(body: &str) -> Preview {
    let lines: Vec<Line<'static>> = body.lines().map(|l| Line::from(l.to_owned())).collect();
    Preview { lines }
}

/// 读取文件为预览文本：二进制探测 / 大文件截断 / 错误提示。
pub(crate) fn read_text_preview(path: &Path, max_lines: usize) -> String {
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) => return format!("读取失败: {e}"),
    };
    if meta.len() > MAX_PREVIEW_BYTES {
        return format!(
            "[文件过大] {}（{} 字节，仅预览前 {} KB）",
            path.display(),
            meta.len(),
            MAX_PREVIEW_BYTES / 1024
        );
    }
    let bytes = match fs::read(path) {
        Ok(b) => b,
        Err(e) => return format!("读取失败: {e}"),
    };
    // 二进制探测：首 1KB 含 NUL 即按二进制处理（M34-7：结构化信息面板）。
    let probe = &bytes[..bytes.len().min(1024)];
    if probe.contains(&0) {
        return file_info_text(path);
    }
    // M34-7：非 UTF-8（无 NUL）同样面板化——替代 lossy 乱码渲染。
    let text = match String::from_utf8(bytes) {
        Ok(t) => t,
        Err(_) => return file_info_text(path),
    };
    let mut kept: Vec<&str> = text.lines().take(max_lines).collect();
    if kept.is_empty() {
        kept.push("（空文件）");
    }
    kept.join("\n")
}

/// M39-2：CSV/TSV 表格对齐预览——前 200 行按分隔符切列，列宽 = 视口内
/// 最大显示宽（字符数，上限 40；CJK 计 1——SIMPLIFIED）；列数取最大，
/// 短行右侧留空；输出截断到 max_lines。
fn csv_preview(path: &Path, max_lines: usize, ext: &str) -> Preview {
    let delim = if ext == "tsv" { '\t' } else { ',' };
    let Ok(text) = fs::read_to_string(path) else {
        return text_preview(&format!("[无法读取] {}", path.display()));
    };
    let rows: Vec<Vec<&str>> = text
        .lines()
        .take(200)
        .map(|l| l.split(delim).collect())
        .collect();
    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    if cols == 0 {
        return text_preview(&text);
    }
    let widths: Vec<usize> = (0..cols)
        .map(|c| {
            rows.iter()
                .filter_map(|r| r.get(c))
                .map(|s| s.chars().count())
                .max()
                .unwrap_or(0)
                .min(40)
        })
        .collect();
    let mut out = String::new();
    for row in &rows {
        let line = (0..cols)
            .map(|c| {
                let cell = row.get(c).copied().unwrap_or("");
                let w = widths[c];
                format!("{cell:<w$}  ")
            })
            .collect::<String>();
        out.push_str(line.trim_end());
        out.push('\n');
        if out.lines().count() >= max_lines {
            break;
        }
    }
    text_preview(out.trim_end())
}

/// M39-2：图片容器级摘要（尺寸 + 格式）——解析文件头，不引依赖、不整读
/// （PNG IHDR / JPEG SOF 扫描 / GIF 逻辑屏幕 / BMP 头）。非图像或解析
/// 失败返回 None（EXIF 全字段 OUT_OF_SCOPE，SIMPLIFIED）。
pub fn image_dimensions(path: &Path) -> Option<(u32, u32, &'static str)> {
    use std::io::Read;
    let mut f = fs::File::open(path).ok()?;
    let mut head = vec![0u8; 128];
    let n = f.read(&mut head).ok()?;
    let h = &head[..n];
    // PNG：8 字节签名 + 4 长度 + "IHDR" + 宽(4 BE) + 高(4 BE)。
    if h.len() >= 24 && &h[..8] == b"\x89PNG\r\n\x1a\n" && &h[12..16] == b"IHDR" {
        let w = u32::from_be_bytes([h[16], h[17], h[18], h[19]]);
        let ht = u32::from_be_bytes([h[20], h[21], h[22], h[23]]);
        return Some((w, ht, "PNG"));
    }
    // GIF：签名 + 逻辑屏幕宽（LE u16 @6）/ 高（@8）。
    if h.len() >= 10 && (&h[..6] == b"GIF87a" || &h[..6] == b"GIF89a") {
        let w = u16::from_le_bytes([h[6], h[7]]) as u32;
        let ht = u16::from_le_bytes([h[8], h[9]]) as u32;
        return Some((w, ht, "GIF"));
    }
    // BMP："BM" + 宽（LE i32 @18）/ 高（@22，DIB 头 ≥ 40 字节；top-down
    // 负高取绝对值）。
    if h.len() >= 26 && &h[..2] == b"BM" {
        let w = u32::from_le_bytes([h[18], h[19], h[20], h[21]]);
        let ht = i32::from_le_bytes([h[22], h[23], h[24], h[25]]).unsigned_abs();
        return Some((w, ht, "BMP"));
    }
    // JPEG：FF D8 起，扫描 SOF0–SOF3/SOF9–SOF11（C0-C3/C9-CA/CB）段——
    // 段长 2 字节 BE；SOF 段内 高@+5、宽@+7（各 2 字节 BE）。
    if h.len() >= 4 && &h[..2] == b"\xff\xd8" {
        let mut i = 2usize;
        while i + 4 <= h.len() {
            if h[i] != 0xff {
                break;
            }
            let marker = h[i + 1];
            if marker == 0xd8 || (0xd0..=0xd7).contains(&marker) {
                i += 2; // 无长度段
                continue;
            }
            let len = u16::from_be_bytes([h[i + 2], h[i + 3]]) as usize;
            let sof = matches!(marker, 0xc0 | 0xc1 | 0xc2 | 0xc3 | 0xc9 | 0xca | 0xcb);
            if sof && i + 9 <= h.len() {
                let ht = u16::from_be_bytes([h[i + 5], h[i + 6]]) as u32;
                let w = u16::from_be_bytes([h[i + 7], h[i + 8]]) as u32;
                return Some((w, ht, "JPEG"));
            }
            if len == 0 {
                break;
            }
            i += 2 + len;
        }
    }
    None
}

/// M34-7：二进制/不可读文件的结构化信息面板——类型（魔数嗅探 + 扩展名）、
/// 大小、修改时间；**不执行文件**（无子进程、无内容解析执行）。替代旧的
/// 单行错误提示与 lossy 乱码渲染；TUI 预览与 pick 同源（read_text_preview
/// 与查看器读取失败分支共用）。M39-2：图像文件追加容器级尺寸摘要。
pub fn file_info_text(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut out = vec![format!("[二进制/不可读预览] {}", path.display())];
    // 魔数嗅探：只读前 1KB（有界读取，不整文件加载）。
    let head = (|| -> std::io::Result<Vec<u8>> {
        use std::io::Read;
        let mut f = fs::File::open(path)?;
        let mut buf = vec![0u8; 1024];
        let n = f.read(&mut buf)?;
        buf.truncate(n);
        Ok(buf)
    })()
    .ok();
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let kind = if crate::archive::is_archive_file(&name) {
        format!("归档（{ext}）")
    } else if let Some(m) = head.as_deref().and_then(magic_kind) {
        m.to_owned()
    } else if !ext.is_empty() {
        format!("{ext} 文件（二进制数据）")
    } else {
        "二进制数据".to_owned()
    };
    out.push(format!("  类型: {kind}"));
    // M39-2：图像容器摘要（尺寸 + 格式；头解析，不引依赖）。
    if let Some((w, h, fmt)) = image_dimensions(path) {
        out.push(format!("  尺寸: {w}×{h}（{fmt}）"));
    }
    match fs::metadata(path) {
        Ok(m) => {
            out.push(format!("  大小: {}", crate::tab::format_size(m.len())));
            let mtime = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);
            out.push(format!("  修改: {}", crate::tab::format_mtime_min(mtime)));
        }
        Err(e) => out.push(format!("  元数据读取失败: {e}")),
    }
    out.join("\n")
}

/// 魔数 → 类型描述（M34-7：嗅探首字节；粒度到"归档/图像/文档/可执行"）。
fn magic_kind(head: &[u8]) -> Option<&'static str> {
    const MAGICS: &[(&[u8], &str)] = &[
        (b"%PDF", "PDF 文档"),
        (b"PK\x03\x04", "zip 容器（jar/war 同类）"),
        (b"\x1f\x8b", "gzip 压缩"),
        (b"7z\xbc\xaf\x27\x1c", "7z 归档"),
        (b"Rar!", "RAR 归档"),
        (b"!<arch>\n", "deb/ar 容器"),
        (b"\x89PNG", "PNG 图像"),
        (b"\xff\xd8\xff", "JPEG 图像"),
        (b"GIF8", "GIF 图像"),
        (b"\x7fELF", "ELF 可执行"),
        (b"MZ", "Windows PE 可执行"),
    ];
    MAGICS
        .iter()
        .find(|(m, _)| head.starts_with(m))
        .map(|(_, d)| *d)
}

/// 供 `pick` 使用的快捷入口：给路径渲染预览行（Span 形式，M8-8 共用证明）。
pub fn preview_lines(path: &Path, max_lines: usize) -> Vec<Span<'static>> {
    let preview = render_preview(path, max_lines);
    // Paragraph 的行文本提取（共享渲染证明；M11 接入图像后同样适用）。
    preview.line_texts().into_iter().map(Span::from).collect()
}

/// M39-2：合成最小 PNG 头（IHDR 尺寸）——供 image_dimensions 断言。
#[cfg(test)]
fn write_png_head(path: &std::path::Path, w: u32, h: u32) {
    use std::io::Write as _;
    let mut b = std::fs::File::create(path).unwrap();
    let _ = b.write_all(b"\x89PNG\r\n\x1a\n");
    let _ = b.write_all(&0u32.to_be_bytes());
    let _ = b.write_all(b"IHDR");
    let _ = b.write_all(&w.to_be_bytes());
    let _ = b.write_all(&h.to_be_bytes());
}

/// M39-2：合成最小 JPEG 头（SOF0 段）——供 image_dimensions 断言。
#[cfg(test)]
fn write_jpeg_head(path: &std::path::Path, w: u16, h: u16) {
    use std::io::Write as _;
    let mut b = std::fs::File::create(path).unwrap();
    let _ = b.write_all(b"\xff\xd8");
    let _ = b.write_all(&[0xff, 0xc0]); // SOF0
    let _ = b.write_all(&[0x00, 0x0b]); // 段长 11
    let _ = b.write_all(&[8]); // 精度
    let _ = b.write_all(&h.to_be_bytes());
    let _ = b.write_all(&w.to_be_bytes());
    let _ = b.write_all(&[1]); // 分量数
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;
    use tempfile::TempDir;

    #[test]
    fn directory_preview_lists_sorted_with_dirs_first() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join("zdir")).unwrap();
        std::fs::create_dir(tmp.path().join("adir")).unwrap();
        std::fs::write(tmp.path().join("f.txt"), "").unwrap();

        let preview = render_preview(tmp.path(), 20);
        let texts = preview.line_texts();
        assert!(texts[0].contains("目录"), "first line: {}", texts[0]);
        assert!(texts.iter().any(|t| t.contains("adir/")));
        assert!(texts.iter().any(|t| t.contains("f.txt")));
    }

    #[test]
    fn m471_preview_icons_are_indented_by_depth() {
        // M47-1：图标前必须有随层级递增的缩进（基础一级 2 空格）。
        assert_eq!(preview_indent("f.txt"), "  ");
        assert_eq!(preview_indent("src/main.rs"), "    ");
        assert_eq!(preview_indent("a/b/c.txt"), "      ");
        let tmp = TempDir::new().unwrap();
        let zip = tmp.path().join("nested.zip");
        crate::archive::make_test_zip(&zip);
        let texts = render_preview(&zip, 20).line_texts();
        assert!(
            texts
                .iter()
                .any(|t| t.starts_with("  ") && !t.starts_with("    ") && t.contains("readme.md")),
            "顶层成员基础缩进: {texts:?}"
        );
        let nested = archive_lines_from_data(
            std::path::Path::new("nested.zip"),
            &ArchivePreviewData {
                members: vec![("src/main.rs".to_owned(), false)],
                too_large: false,
            },
            20,
        );
        assert!(
            nested
                .line_texts()
                .iter()
                .any(|t| t.starts_with("    ") && t.contains("src/main.rs")),
            "嵌套成员更深缩进: {:?}",
            nested.line_texts()
        );
    }

    #[test]
    fn text_preview_reads_and_truncates() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("a.txt");
        let mut f = fs::File::create(&p).unwrap();
        for i in 0..100 {
            writeln!(f, "line {i}").unwrap();
        }
        let preview = render_preview(&p, 5);
        assert_eq!(preview.line_count(), 5); // 截断到 max_lines

        // 空文件。
        let empty = tmp.path().join("empty.txt");
        fs::File::create(&empty).unwrap();
        let preview = render_preview(&empty, 5);
        let texts = preview.line_texts();
        assert!(texts.iter().any(|t| t.contains("空文件")));
    }

    #[test]
    fn binary_file_and_oversize_get_friendly_hint() {
        let tmp = TempDir::new().unwrap();
        let bin = tmp.path().join("x.bin");
        let mut f = fs::File::create(&bin).unwrap();
        f.write_all(&[0x89, 0x50, 0x4e, 0x47, 0x00, 0x01]).unwrap(); // 含 NUL

        let preview = render_preview(&bin, 10);
        let texts = preview.line_texts();
        // M34-7：结构化信息面板（PNG 魔数嗅探 + 大小/修改），非单一错误行。
        assert!(texts.iter().any(|t| t.contains("二进制/不可读预览")));
        assert!(texts.iter().any(|t| t.contains("PNG 图像")));
        assert!(texts.iter().any(|t| t.contains("大小:")));
        assert!(texts.iter().any(|t| t.contains("修改:")));

        // 大文件（>512KB）→ 截断提示。
        let big = tmp.path().join("big.txt");
        let mut f = fs::File::create(&big).unwrap();
        let chunk = vec![b'a'; 64 * 1024];
        for _ in 0..10 {
            f.write_all(&chunk).unwrap();
        }
        let preview = render_preview(&big, 10);
        let texts = preview.line_texts();
        assert!(texts.iter().any(|t| t.contains("文件过大")));
    }

    #[test]
    fn image_files_get_friendly_hint_when_graphics_unavailable() {
        // M11-2：图像文件预览行 = 解码失败/无图形能力时的降级提示。
        let tmp = TempDir::new().unwrap();
        let img = tmp.path().join("x.png");
        fs::write(&img, b"not-a-real-png").unwrap();
        assert_eq!(preview_kind(&img), PreviewKind::Image);
        let preview = render_preview(&img, 10);
        let texts = preview.line_texts();
        assert!(texts.iter().any(|t| t.contains("图像预览")));
    }

    #[test]
    fn m454b_archive_preview_members_and_fallbacks() {
        // M45-4b：归档预览显示成员列表；损坏/.rar 回退信息面板；
        // Office 文件名（docx = zip 容器）不得被误判为 Archive。
        let tmp = TempDir::new().unwrap();

        // zip：成员列表（src/、readme.md、deep/）。
        let zip = tmp.path().join("bundle.zip");
        crate::archive::make_test_zip(&zip);
        assert_eq!(preview_kind(&zip), PreviewKind::Archive);
        let texts = render_preview(&zip, 20).line_texts();
        let joined = texts.join("\n");
        assert!(joined.contains("归档"), "标题: {joined}");
        assert!(joined.contains("readme.md"), "成员: {joined}");
        assert!(joined.contains("src/"), "目录成员: {joined}");
        assert!(joined.contains("只读"), "只读提示: {joined}");

        // Office 优先：docx 是 zip 容器但 preview_kind 必须为 Office。
        let docx = tmp.path().join("report.docx");
        crate::archive::make_test_zip(&docx);
        assert_eq!(preview_kind(&docx), PreviewKind::Office, "Office 先于归档");

        // 损坏归档 → 信息面板，不 panic。
        let broken = tmp.path().join("broken.zip");
        fs::write(&broken, b"not a zip").unwrap();
        let texts = render_preview(&broken, 20).line_texts();
        assert!(
            texts.iter().any(|t| t.contains("归档打开失败")),
            "损坏归档回退: {texts:?}"
        );

        // .rar → 信息面板。
        let rar = tmp.path().join("disk.rar");
        fs::write(&rar, b"Rar!\x1a\x07").unwrap();
        assert_eq!(preview_kind(&rar), PreviewKind::Archive);
        let texts = render_preview(&rar, 20).line_texts();
        assert!(
            texts.iter().any(|t| t.contains("二进制/不可读预览")),
            ".rar 回退信息面板: {texts:?}"
        );
    }

    #[test]
    fn m34_7_non_utf8_panel_and_magic_sniff() {
        let tmp = TempDir::new().unwrap();
        // 非 UTF-8（无 NUL）→ 信息面板，不渲染 lossy 乱码。
        let bad = tmp.path().join("bad.txt");
        fs::write(&bad, [0xff, 0xfe, b'a', b'b']).unwrap();
        let texts = render_preview(&bad, 10).line_texts();
        assert!(texts.iter().any(|t| t.contains("二进制/不可读预览")));
        assert!(texts.iter().all(|t| !t.contains('\u{fffd}')), "无乱码");
        // NUL 二进制 + PDF 魔数 → 类型嗅探。
        let pdf = tmp.path().join("doc.bin");
        fs::write(&pdf, b"%PDF-1.4\x00\x01").unwrap();
        let texts = render_preview(&pdf, 10).line_texts();
        assert!(texts.iter().any(|t| t.contains("PDF 文档")));
        // magic_kind 直接断言（含 zip 容器）。
        assert_eq!(
            magic_kind(b"PK\x03\x04rest"),
            Some("zip 容器（jar/war 同类）")
        );
        assert_eq!(magic_kind(b"hello"), None);
    }

    #[test]
    fn preview_kind_classifies_dir_image_text() {
        // M11-2：TUI 与 pick 共用的分流入口。
        let tmp = TempDir::new().unwrap();
        assert_eq!(preview_kind(tmp.path()), PreviewKind::Directory);

        let img = tmp.path().join("photo.JPEG");
        fs::write(&img, b"\x89PNG").unwrap();
        assert_eq!(preview_kind(&img), PreviewKind::Image);

        let txt = tmp.path().join("a.txt");
        fs::write(&txt, "hello").unwrap();
        assert_eq!(preview_kind(&txt), PreviewKind::Text);

        let noext = tmp.path().join("Makefile");
        fs::write(&noext, "all:").unwrap();
        assert_eq!(preview_kind(&noext), PreviewKind::Text);
    }

    #[test]
    fn office_preview_extracts_text_via_shared_renderer() {
        // M11-9：Office 分支接入共享 PreviewRenderer（TUI 与 pick 同源）。
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("report.docx");
        office_oxide::create::create_from_markdown(
            "# 报告标题\n\n正文段落。",
            office_oxide::format::DocumentFormat::Docx,
            &p,
        )
        .unwrap();
        assert_eq!(preview_kind(&p), PreviewKind::Office);

        let preview = render_preview(&p, 10);
        let texts = preview.line_texts();
        assert!(texts.iter().any(|t| t.contains("报告标题")));
    }

    #[test]
    fn office_preview_corrupt_file_gets_friendly_hint() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("bad.xlsx");
        fs::write(&p, b"junk").unwrap();
        assert_eq!(preview_kind(&p), PreviewKind::Office);
        let preview = render_preview(&p, 10);
        let texts = preview.line_texts();
        assert!(texts.iter().any(|t| t.contains("Office 文档解析失败")));
    }

    #[test]
    fn pdf_preview_extracts_text_without_poppler() {
        // M11-11：干净环境（无 Poppler/MuPDF/外部工具）下含文本层的
        // PDF 走 pdf-inspector 文本提取预览。
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("doc.pdf");
        let content = "%PDF-1.4\n1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>\nendobj\n4 0 obj\n<< /Length 46 >>\nstream\nBT /F1 24 Tf 72 700 Td (Poppler free) Tj ET\nendstream\nendobj\n5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\ntrailer\n<< /Root 1 0 R /Size 6 >>\n%%EOF\n";
        fs::write(&p, content).unwrap();
        assert_eq!(preview_kind(&p), PreviewKind::Pdf);

        let preview = render_preview(&p, 10);
        let texts = preview.line_texts();
        assert!(
            texts.iter().any(|t| t.contains("Poppler")),
            "pdf text extraction: {texts:?}"
        );
    }

    #[test]
    fn pdf_preview_corrupt_file_gets_friendly_hint() {
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("bad.pdf");
        fs::write(&p, b"junk bytes").unwrap();
        let preview = render_preview(&p, 10);
        let texts = preview.line_texts();
        assert!(texts.iter().any(|t| t.contains("PDF 文本提取失败")));
        // 不 panic 且提示系统 opener 回退。
        assert!(texts.iter().any(|t| t.contains("系统默认程序")));
    }

    #[test]
    fn preview_lines_shared_helper() {
        // M8-8：TUI 与 pick 共用的入口（返回 Span 行）。
        let tmp = TempDir::new().unwrap();
        let p = tmp.path().join("shared.txt");
        fs::write(&p, "hello shared").unwrap();
        let spans = preview_lines(&p, 10);
        let joined: String = spans.iter().map(|s| s.content.clone()).collect();
        assert!(joined.contains("hello shared"));
    }

    #[test]
    fn m39_2_csv_tsv_table_alignment() {
        // CSV/TSV 表格对齐：列宽 = 最长单元格；短行留空；TSV 用 Tab。
        let tmp = TempDir::new().unwrap();
        let csv = tmp.path().join("t.csv");
        fs::write(&csv, "name,age\nalice,30\nbob,\n").unwrap();
        let p = render_preview(&csv, 10);
        let texts = p.line_texts();
        let joined: String = texts.concat();
        assert!(joined.contains("name   age"), "对齐列宽: {joined}");
        assert!(joined.contains("alice  30"), "第二列跟随最宽列");
        assert_eq!(
            texts.iter().position(|l| l == "bob"),
            Some(2),
            "短行尾随填充被裁剪"
        );
        // TSV：Tab 分隔。
        let tsv = tmp.path().join("t.tsv");
        fs::write(&tsv, "a\tbb\nccc\td\n").unwrap();
        let p = render_preview(&tsv, 10);
        let joined: String = p.line_texts().concat();
        assert!(joined.contains("a    bb"), "TSV 列宽按最长: {joined}");
        assert!(joined.contains("ccc  d"));
    }

    #[test]
    fn m39_2_image_dimensions_and_info_panel() {
        // 容器级摘要：PNG IHDR / JPEG SOF；BMP/GIF 头解析；非图像 None。
        let tmp = TempDir::new().unwrap();
        let png = tmp.path().join("t.png");
        write_png_head(&png, 1920, 1080);
        assert_eq!(
            image_dimensions(&png),
            Some((1920, 1080, "PNG")),
            "PNG IHDR"
        );
        let jpg = tmp.path().join("t.jpg");
        write_jpeg_head(&jpg, 640, 480);
        assert_eq!(image_dimensions(&jpg), Some((640, 480, "JPEG")));
        let gif = tmp.path().join("t.gif");
        let mut g = vec![b'G', b'I', b'F', b'8', b'9', b'a'];
        g.extend(320u16.to_le_bytes());
        g.extend(200u16.to_le_bytes());
        fs::write(&gif, &g).unwrap();
        assert_eq!(image_dimensions(&gif), Some((320, 200, "GIF")));
        let bmp = tmp.path().join("t.bmp");
        let mut m = vec![b'B', b'M'];
        m.extend(vec![0u8; 16]);
        m.extend(100i32.to_le_bytes());
        m.extend(50i32.to_le_bytes());
        fs::write(&bmp, &m).unwrap();
        assert_eq!(image_dimensions(&bmp), Some((100, 50, "BMP")));
        let txt = tmp.path().join("t.txt");
        fs::write(&txt, "plain").unwrap();
        assert_eq!(image_dimensions(&txt), None);
        // 信息面板追加尺寸行（file_info_text 同源接线）。
        let info = file_info_text(&png);
        assert!(info.contains("尺寸: 1920×1080（PNG）"), "info: {info}");
    }

    /// M46-8：缓存命中时直接使用已有成员列表（无需打开归档）；键含
    /// (路径, mtime, size)，文件变化后自动失效重建。
    #[test]
    fn m537_deb_listing_parse_and_tool_mapping() {
        assert_eq!(
            archive_listing_tool(std::path::Path::new("big.deb")),
            Some(("dpkg-deb", &["-c"][..]))
        );
        let line = "-rw-r--r-- root/root  12345 2026-01-01 00:00 ./usr/bin/app";
        assert_eq!(
            parse_listing_line("dpkg-deb", line),
            Some(("usr/bin/app".to_owned(), false))
        );
    }

    #[test]
    fn m487_archive_listing_tool_mapping_and_guard() {
        // M48-7：按归档类型映射到固定只读命令，不经 shell；未知类型无工具。
        assert_eq!(
            archive_listing_tool(std::path::Path::new("a.zip")),
            Some(("unzip", &["-Z1"][..]))
        );
        assert!(archive_listing_tool(std::path::Path::new("a.tar.gz")).is_some());
        assert!(archive_listing_tool(std::path::Path::new("a.7z")).is_some());
        assert!(archive_listing_tool(std::path::Path::new("a.unknown")).is_none());

        // 外部工具缺失/失败时归档预览仍不 panic，走内置回退（损坏归档给可读提示）。
        let tmp = TempDir::new().unwrap();
        let broken = tmp.path().join("broken.zip");
        fs::write(&broken, b"not a zip").unwrap();
        let texts = render_preview(&broken, 10).line_texts();
        assert!(
            texts.iter().any(|t| t.contains("归档打开失败")),
            "{texts:?}"
        );
    }

    #[test]
    fn m468_archive_preview_cache_hit_and_invalidate() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("bundle.zip");
        crate::archive::make_test_zip(&path);
        let cache: ArchivePreviewCache = std::cell::RefCell::new(std::collections::HashMap::new());
        let key = archive_cache_key(&path);

        // 预置哨兵成员：若缓存命中，输出必定出现该名字（即使归档实际不含）。
        cache.borrow_mut().insert(
            key.clone(),
            ArchivePreviewData {
                members: vec![("CACHED.bin".to_owned(), false)],
                too_large: false,
            },
        );
        let p1 = archive_preview_cached(&path, 10, &cache);
        assert!(p1.line_texts().iter().any(|t| t.contains("CACHED.bin")));

        // 修改文件（size 变化）→ 键变化 → 缓存未命中 → 真实成员列表。
        fs::write(&path, b"x").unwrap();
        assert_ne!(archive_cache_key(&path), key, "mtime/size 变化应使缓存失效");
        let p2 = archive_preview_cached(&path, 10, &cache);
        assert!(!p2.line_texts().iter().any(|t| t.contains("CACHED.bin")));
    }

    /// M46-8：超过大小护栏的归档只给快速提示，不做成员索引。
    #[test]
    fn m468_archive_preview_size_guard_hint() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("huge.tar.xz");
        // 只写一个标记内容（远小于阈值时为 normal 路径；这里直接验证 data 层
        // 的 too_large 分支渲染）。
        fs::write(&path, b"not really huge").unwrap();
        let data = ArchivePreviewData {
            members: Vec::new(),
            too_large: true,
        };
        let pv = archive_lines_from_data(&path, &data, 10);
        let text = pv.line_texts().join("\n");
        assert!(
            text.contains("按 l / Enter 进入虚拟目录浏览"),
            "text: {text}"
        );
    }
}
