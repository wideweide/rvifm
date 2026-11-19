//! 文件操作与撤销执行（M2-2 起）。
//!
//! 删除走**应用内暂存**保证 `u` 精确可逆（决策见 docs/NOTES.md §4.2）：
//! move 到暂存目录（跨盘回退 copy+delete），undo 即移回原位。

use std::fs;
use std::path::{Path, PathBuf};

use crate::state::UndoableOp;

/// 应用内暂存根目录：Windows `%LOCALAPPDATA%\rvifm\trash`；
/// Unix `$XDG_DATA_HOME/rvifm/trash`，缺省 `~/.local/share/rvifm/trash`。
pub fn trash_dir() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share"))
            })
            .unwrap_or_else(|| PathBuf::from("."))
    };
    base.join("rvifm").join("trash")
}

/// 跨盘安全的移动：优先 `rename`，失败回退 copy + 删除。
fn move_path(from: &Path, to: &Path) -> std::io::Result<()> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    if from.is_dir() {
        copy_dir(from, to)?;
        fs::remove_dir_all(from)
    } else {
        fs::copy(from, to)?;
        fs::remove_file(from)
    }
}

/// 递归复制目录（跨盘回退用）。
fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

/// 把 `path` 移入 `trash_root`（唯一命名防碰撞），返回暂存路径。
fn stage_into(trash_root: &Path, path: &Path) -> std::io::Result<PathBuf> {
    fs::create_dir_all(trash_root)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "unnamed".to_owned());
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let staged = trash_root.join(format!("{nanos}-{name}"));
    move_path(path, &staged)?;
    Ok(staged)
}

/// 可恢复删除（M2-2，`dd`）：移入应用内暂存并生成撤销记录。
pub fn delete_to_trash(tab_id: usize, path: &Path) -> std::io::Result<UndoableOp> {
    let staged = stage_into(&trash_dir(), path)?;
    record_staging(&trash_dir(), &staged, path);
    Ok(UndoableOp::Delete {
        tab_id,
        path: path.to_path_buf(),
        staged,
    })
}

// ── M13-10：暂存区索引（orig 路径持久化，供 `:trash` 菜单恢复） ─────────────

/// 暂存区一条记录：staged（暂存位置）→ orig（删除前原路径）。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TrashEntry {
    pub staged: String,
    pub orig: String,
}

fn trash_index_dir() -> PathBuf {
    trash_dir().join("index")
}

/// 追加一条暂存记录：每条独立 `.json` 文件（staged 名唯一 → 无跨进程
/// 写竞争；失败静默——索引是增强，不影响主流程）。
fn record_staging(trash_root: &Path, staged: &Path, orig: &Path) {
    let dir = trash_root.join("index");
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let entry = TrashEntry {
        staged: staged.to_string_lossy().into_owned(),
        orig: orig.to_string_lossy().into_owned(),
    };
    let file = staged
        .file_name()
        .map(|n| dir.join(format!("{}.json", n.to_string_lossy())))
        .unwrap_or_else(|| dir.join(format!("{}.json", std::process::id())));
    if let Ok(json) = serde_json::to_string(&entry) {
        let _ = fs::write(file, json);
    }
}

/// 读取暂存区索引：仅返回 staged 仍存在的条目（已恢复/已撤销的过滤）。
pub fn read_trash_index() -> Vec<TrashEntry> {
    let dir = trash_index_dir();
    let Ok(rd) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for e in rd.filter_map(|e| e.ok()) {
        if let Ok(json) = fs::read_to_string(e.path())
            && let Ok(entry) = serde_json::from_str::<TrashEntry>(&json)
            && Path::new(&entry.staged).exists()
        {
            out.push(entry);
        }
    }
    out
}

/// 从索引移除指定 staged 记录（恢复成功后删除对应索引文件）。
fn remove_trash_index(trash_root: &Path, staged: &Path) {
    let name = match staged.file_name() {
        Some(n) => n.to_string_lossy().into_owned(),
        None => return,
    };
    let _ = fs::remove_file(trash_root.join("index").join(format!("{name}.json")));
}

/// 恢复暂存文件到原路径（M13-10，`:trash` 菜单）：staged 存在且 orig
/// 不存在时移动；返回恢复后的原路径。冲突/缺失返回 Err。
pub fn restore_from_trash(entry: &TrashEntry) -> std::io::Result<PathBuf> {
    let staged = Path::new(&entry.staged);
    let orig = Path::new(&entry.orig);
    if !staged.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "暂存文件已不存在",
        ));
    }
    if orig.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("原位置已存在同名文件: {}", orig.display()),
        ));
    }
    move_path(staged, orig)?;
    remove_trash_index(&trash_dir(), staged);
    Ok(orig.to_path_buf())
}

/// 重命名（M2-3，`:rename <新名字>`）：仅改文件名，不允许路径分隔符。
pub fn rename_to(tab_id: usize, path: &Path, new_name: &str) -> std::io::Result<UndoableOp> {
    if new_name.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "新名字为空",
        ));
    }
    if new_name.contains('/') || new_name.contains('\\') {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "重命名不能包含路径分隔符",
        ));
    }
    let to = path.with_file_name(new_name);
    if to.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("目标已存在: {}", to.display()),
        ));
    }
    fs::rename(path, &to)?;
    Ok(UndoableOp::Rename {
        tab_id,
        from: path.to_path_buf(),
        to,
    })
}

/// 移动（M2-4，`:move <目标目录>`）：把 `path` 移入 `target_dir`（保留原名）。
pub fn move_to(tab_id: usize, path: &Path, target_dir: &Path) -> std::io::Result<UndoableOp> {
    if !target_dir.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("目标目录不存在: {}", target_dir.display()),
        ));
    }
    let name = path
        .file_name()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "无效路径"))?;
    let to = target_dir.join(name);
    if to.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("目标已存在: {}", to.display()),
        ));
    }
    // 防自嵌套：把目录移动到其自身内部会让跨盘回退的 copy 无限递归。
    if to.starts_with(path) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "不能把目录移动到其自身内部",
        ));
    }
    move_path(path, &to)?;
    Ok(UndoableOp::Move {
        tab_id,
        from: path.to_path_buf(),
        to,
    })
}

/// 复制（M2-4b，`:copy <目标目录>`；SPEC §5 M2 MUST）：复制到 `target_dir`
/// （保留原名）。undo = 删除生成的副本（`Create` 语义）。
pub fn copy_to(tab_id: usize, path: &Path, target_dir: &Path) -> std::io::Result<UndoableOp> {
    if !target_dir.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("目标目录不存在: {}", target_dir.display()),
        ));
    }
    let name = path
        .file_name()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "无效路径"))?;
    let to = target_dir.join(name);
    if to.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("目标已存在: {}", to.display()),
        ));
    }
    // 防自嵌套：把目录复制到其自身内部会无限递归。
    if to.starts_with(path) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "不能把目录复制到其自身内部",
        ));
    }
    if path.is_dir() {
        copy_dir(path, &to)?;
        Ok(UndoableOp::Create {
            tab_id,
            path: to,
            recursive: true,
        })
    } else {
        fs::copy(path, &to)?;
        Ok(UndoableOp::Create {
            tab_id,
            path: to,
            recursive: false,
        })
    }
}

/// 创建符号链接（M28-1，`:link <dest>`）。`target` 为链接指向的条目
/// （绝对路径），`link` 为新建链接路径。撤销 = 移除链接本身（
/// [`UndoableOp::Create`]，undo_op 已按 symlink_metadata 判链接类型）。
pub fn create_link(tab_id: usize, target: &Path, link: &Path) -> std::io::Result<UndoableOp> {
    if fs::symlink_metadata(link).is_ok() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("链接目标已存在: {}", link.display()),
        ));
    }
    let is_dir = fs::symlink_metadata(target)
        .map(|m| m.is_dir())
        .unwrap_or(false);
    fs_symlink(target, link, is_dir)?;
    Ok(UndoableOp::Create {
        tab_id,
        path: link.to_path_buf(),
        recursive: false,
    })
}

/// 平台链接原语：unix symlink；Windows 文件用 symlink_file、目录用
/// symlink_dir——权限不足（未开发者模式/未提权，os error 1314 归类
/// PermissionDenied）时目录回退目录联接（junction，`mklink /J`，
/// 无需特权；参数化传递、IO 静默）。产物判定沿用 M26-4 外部 CLI 口径
/// （成功退出 + 产物存在）。
fn fs_symlink(target: &Path, link: &Path, is_dir: bool) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let _ = is_dir;
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        let result = if is_dir {
            std::os::windows::fs::symlink_dir(target, link)
        } else {
            std::os::windows::fs::symlink_file(target, link)
        };
        match result {
            Ok(()) => Ok(()),
            Err(e) if is_dir && e.kind() == std::io::ErrorKind::PermissionDenied => {
                // 目录联接回退：junction 无特权要求；经 cmd 内建 mklink /J。
                let out = std::process::Command::new("cmd")
                    .args(["/c", "mklink", "/J"])
                    .arg(link.as_os_str())
                    .arg(target.as_os_str())
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()?;
                if out.success() && link.is_dir() {
                    Ok(())
                } else {
                    Err(std::io::Error::other(format!(
                        "符号链接创建失败且目录联接回退失败（mklink /J 退出码 {}）",
                        out.code().unwrap_or(-1)
                    )))
                }
            }
            Err(e) => Err(e),
        }
    }
}

/// 新建名字校验（M2-5）：仅当前目录下的合法新名字。
fn validate_new_name(base: &Path, name: &str) -> std::io::Result<PathBuf> {
    if name.is_empty() || name.contains('/') || name.contains('\\') {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "名字为空或包含路径分隔符",
        ));
    }
    let path = base.join(name);
    if path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("目标已存在: {}", path.display()),
        ));
    }
    Ok(path)
}

// ── M12-1：写操作权限不足检测 ─────────────────────────────────────────────
//
// 提权触发条件：复制/移动/删除/重命名因 EACCES/EPERM 失败。
// Windows（UAC 机制不同，SIMPLIFIED 尽力而为）：只识别
// `ErrorKind::PermissionDenied`；提权执行路径整体 cfg(unix) 门控。
// 消费方：M12-3（TUI 提权触发接线）。

/// 判断写操作错误是否为权限不足（提权候选）。
#[allow(dead_code)]
pub fn is_permission_denied(err: &std::io::Error) -> bool {
    if err.kind() == std::io::ErrorKind::PermissionDenied {
        return true;
    }
    #[cfg(unix)]
    {
        // errno: EACCES = 13，EPERM = 1。
        matches!(err.raw_os_error(), Some(13 | 1))
    }
    #[cfg(not(unix))]
    {
        false
    }
}

/// 新建目录（M2-5，`:mkdir <名字>`）。
pub fn create_dir(tab_id: usize, base: &Path, name: &str) -> std::io::Result<UndoableOp> {
    let path = validate_new_name(base, name)?;
    fs::create_dir(&path)?;
    Ok(UndoableOp::Create {
        tab_id,
        path,
        recursive: false,
    })
}

/// 新建空文件（M2-5，`:touch <名字>`）。
pub fn create_file(tab_id: usize, base: &Path, name: &str) -> std::io::Result<UndoableOp> {
    let path = validate_new_name(base, name)?;
    fs::File::create(&path)?;
    Ok(UndoableOp::Create {
        tab_id,
        path,
        recursive: false,
    })
}

/// 归档创建格式（M25-3 + M26-2 7z + M26-4 外部 CLI 六形态）。**写端边界**：
/// zip/tar/tar.gz/7z 为纯 Rust 内建；rar/cab/lzh/tar.xz/tar.bz2/tar.zst 的
/// 主流编码器为 C 绑定或专有工具 → M26-4 经外部 CLI 降级创建（PATH 探测
/// 7z/rar/tar，缺失优雅终止）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    Tar,
    TarGz,
    SevenZ,
    Rar,
    Cab,
    Lzh,
    TarXz,
    TarBz2,
    TarZst,
}

impl ArchiveFormat {
    /// 对应扩展名（含点；tar.gz 为 `.tar.gz`）。
    pub fn ext(self) -> &'static str {
        match self {
            ArchiveFormat::Zip => ".zip",
            ArchiveFormat::Tar => ".tar",
            ArchiveFormat::TarGz => ".tar.gz",
            ArchiveFormat::SevenZ => ".7z",
            ArchiveFormat::Rar => ".rar",
            ArchiveFormat::Cab => ".cab",
            ArchiveFormat::Lzh => ".lzh",
            ArchiveFormat::TarXz => ".tar.xz",
            ArchiveFormat::TarBz2 => ".tar.bz2",
            ArchiveFormat::TarZst => ".tar.zst",
        }
    }

    /// 从格式名解析（`:archive <格式> <名>`）。
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "zip" => Some(ArchiveFormat::Zip),
            "tar" => Some(ArchiveFormat::Tar),
            "tar.gz" => Some(ArchiveFormat::TarGz),
            "7z" => Some(ArchiveFormat::SevenZ),
            "rar" => Some(ArchiveFormat::Rar),
            "cab" => Some(ArchiveFormat::Cab),
            "lzh" => Some(ArchiveFormat::Lzh),
            "tar.xz" => Some(ArchiveFormat::TarXz),
            "tar.bz2" => Some(ArchiveFormat::TarBz2),
            "tar.zst" => Some(ArchiveFormat::TarZst),
            _ => None,
        }
    }

    /// 从归档名扩展名推断（`:archive <名>` 无显式格式时）。
    pub fn infer(name: &str) -> Option<Self> {
        let lower = name.to_lowercase();
        if lower.ends_with(".zip") {
            Some(ArchiveFormat::Zip)
        } else if lower.ends_with(".tar.gz") {
            Some(ArchiveFormat::TarGz)
        } else if lower.ends_with(".7z") {
            Some(ArchiveFormat::SevenZ)
        } else if lower.ends_with(".rar") {
            Some(ArchiveFormat::Rar)
        } else if lower.ends_with(".cab") {
            Some(ArchiveFormat::Cab)
        } else if lower.ends_with(".lzh") {
            Some(ArchiveFormat::Lzh)
        } else if lower.ends_with(".tar.xz") {
            Some(ArchiveFormat::TarXz)
        } else if lower.ends_with(".tar.bz2") {
            Some(ArchiveFormat::TarBz2)
        } else if lower.ends_with(".tar.zst") {
            Some(ArchiveFormat::TarZst)
        } else if lower.ends_with(".tar") {
            Some(ArchiveFormat::Tar)
        } else {
            None
        }
    }
}

/// 归档创建（M25-3，`:archive [zip|tar|tar.gz] <名>`）：把 `names`（当前
/// 目录条目名，目录递归打包、软链接按链接本身收录）打包为 `base` 下 `<名>`
/// （缺扩展名自动补齐）。同名已存在报错不覆盖；任一源缺失整批中止；
/// 写入失败清理半成品（M20-3 教训）；产物入撤销栈（Create，`u` 可逆）。
pub fn create_archive(
    tab_id: usize,
    base: &Path,
    name: &str,
    format: ArchiveFormat,
    names: &[String],
) -> std::io::Result<UndoableOp> {
    let final_name = if name.to_lowercase().ends_with(format.ext()) {
        name.to_owned()
    } else {
        format!("{name}{}", format.ext())
    };
    let dest = validate_new_name(base, &final_name)?;
    let mut sources = Vec::new();
    for n in names {
        let p = base.join(n);
        fs::symlink_metadata(&p)
            .map_err(|e| std::io::Error::new(e.kind(), format!("源条目不存在: {n}——整批中止")))?;
        sources.push((n.as_str().to_owned(), p));
    }
    let result = match format {
        ArchiveFormat::Zip => pack_zip(&dest, &sources),
        ArchiveFormat::Tar => pack_tar(&dest, &sources, false),
        ArchiveFormat::TarGz => pack_tar(&dest, &sources, true),
        ArchiveFormat::SevenZ => pack_7z(&dest, &sources),
        // M26-4：外部 CLI 降级（PATH 探测 → Command → 失败按退出码/stderr
        // 可读错误；输出 .output() 捕获不进 TUI；失败由下方统一清理半成品）。
        ArchiveFormat::Rar
        | ArchiveFormat::Cab
        | ArchiveFormat::Lzh
        | ArchiveFormat::TarXz
        | ArchiveFormat::TarBz2
        | ArchiveFormat::TarZst => pack_cli(format, &dest, &sources, base),
    };
    if let Err(e) = result {
        let _ = fs::remove_file(&dest);
        return Err(e);
    }
    Ok(UndoableOp::Create {
        tab_id,
        path: dest,
        recursive: false,
    })
}

/// 校验文件类型（M25-4；md5 默认。md5sum/sha256sum 兼容行格式
/// `<hex>  <文件名>`，`md5sum -c` / `sha256sum -c` 可直接校验）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Checksum {
    Md5,
    Sha256,
    None,
}

impl Checksum {
    /// 从配置/参数值解析（`[archive] checksum` 与 `checksum=` 参数同源）。
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "md5" => Some(Checksum::Md5),
            "sha256" => Some(Checksum::Sha256),
            "none" => Some(Checksum::None),
            _ => None,
        }
    }

    /// 校验文件扩展名（None → None，不生成文件）。
    pub fn ext(self) -> Option<&'static str> {
        match self {
            Checksum::Md5 => Some("md5"),
            Checksum::Sha256 => Some("sha256"),
            Checksum::None => None,
        }
    }
}

/// 计算文件摘要十六进制串（M25-4；64KiB 分块喂入防大文件占内存）。
pub fn digest_hex(path: &Path, kind: Checksum) -> std::io::Result<String> {
    use md5::Digest as _;
    use std::io::Read as _;
    let mut file = fs::File::open(path)?;
    let hex =
        |bytes: &[u8]| -> String { bytes.iter().map(|b| format!("{b:02x}")).collect::<String>() };
    let mut buf = vec![0u8; 64 * 1024];
    match kind {
        Checksum::Md5 => {
            let mut h = md5::Md5::new();
            loop {
                let n = file.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                h.update(&buf[..n]);
            }
            Ok(hex(&h.finalize()))
        }
        Checksum::Sha256 => {
            let mut h = sha2::Sha256::new();
            loop {
                let n = file.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                h.update(&buf[..n]);
            }
            Ok(hex(&h.finalize()))
        }
        Checksum::None => Ok(String::new()),
    }
}

/// 生成校验文件 `<归档名>.<ext>`（M25-4）。返回校验文件路径
/// （[`Checksum::None`] → None）。失败时调用方已持有归档的撤销记录。
pub fn write_checksum(path: &Path, kind: Checksum) -> std::io::Result<Option<PathBuf>> {
    let Some(ext) = kind.ext() else {
        return Ok(None);
    };
    let hex = digest_hex(path, kind)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut cpath = path.to_string_lossy().into_owned();
    cpath.push('.');
    cpath.push_str(ext);
    let cpath = PathBuf::from(cpath);
    fs::write(&cpath, format!("{hex}  {name}\n"))?;
    Ok(Some(cpath))
}

/// zip 打包（zip crate deflate；目录递归、软链接 add_symlink）。
fn pack_zip(dest: &Path, sources: &[(String, PathBuf)]) -> std::io::Result<()> {
    let file = fs::File::create(dest)?;
    let mut w = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let pack_result = (|| {
        for (name, path) in sources {
            pack_zip_entry(&mut w, &opts, name.clone(), path)?;
        }
        w.finish().map_err(zip_err)?;
        Ok(())
    })();
    // 任何失败：半成品文件由调用方清理（M20-3 教训）。
    pack_result
}

/// zip 单条目递归打包（目录 → add_directory + 子项递归；软链接 →
/// add_symlink 按链接本身收录；文件 → start_file + 内容拷贝）。
fn pack_zip_entry(
    w: &mut zip::ZipWriter<fs::File>,
    opts: &zip::write::SimpleFileOptions,
    arc: String,
    path: &Path,
) -> std::io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.is_dir() {
        w.add_directory(arc.clone(), *opts).map_err(zip_err)?;
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let leaf = entry.file_name().to_string_lossy().into_owned();
            let sub_arc = format!("{arc}/{leaf}");
            pack_zip_entry(w, opts, sub_arc, &entry.path())?;
        }
        Ok(())
    } else if meta.is_symlink() {
        let target = fs::read_link(path)?;
        w.add_symlink(arc, target.to_string_lossy().into_owned(), *opts)
            .map_err(zip_err)
    } else {
        w.start_file(arc, *opts).map_err(zip_err)?;
        let mut src = fs::File::open(path)?;
        std::io::copy(&mut src, w)?;
        Ok(())
    }
}

fn zip_err(e: zip::result::ZipError) -> std::io::Error {
    std::io::Error::other(e)
}

/// tar / tar.gz 打包（flate2 GzEncoder；目录 append_dir_all、
/// 软链接 append_link 按链接本身收录）。
/// 7z 编码错误 → 可读 io 错误（sevenz-rust Error 不带 io kind）。
fn sevenz_err(e: sevenz_rust::Error) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
}

// ── M26-4：外部 CLI 归档创建降级 ──────────────────────────────────────────

/// 各 CLI 格式的候选工具（按优先序 PATH 探测；M25-6 find_git_tui 同款模式）。
/// cab/lzh 由 7z 尝试创建——7z 实际不支持 cab/lzh 打包时会给出自身错误，
/// 经 M26-4 的错误上浮为可读提示（工具探测与失败链路按 SPEC 落实）。
pub(crate) fn cli_tool_candidates(format: ArchiveFormat) -> &'static [&'static str] {
    match format {
        ArchiveFormat::Rar => &["rar"],
        ArchiveFormat::Cab | ArchiveFormat::Lzh => &["7z", "7za", "7zr"],
        ArchiveFormat::TarXz | ArchiveFormat::TarBz2 | ArchiveFormat::TarZst => &["tar"],
        _ => &[],
    }
}

/// 构造 CLI 参数（纯函数，测试用）。`dest` 为绝对路径，`names` 相对 base；
/// 调用方以 `current_dir(base)` 执行，`.output()` 捕获输出（不进 TUI）。
pub(crate) fn cli_create_args(format: ArchiveFormat, dest: &Path, names: &[String]) -> Vec<String> {
    let dest_str = dest.to_string_lossy().into_owned();
    let mut args: Vec<String> = match format {
        ArchiveFormat::Rar => vec!["a".to_owned(), "-ep1".to_owned(), dest_str],
        ArchiveFormat::Cab | ArchiveFormat::Lzh => vec!["a".to_owned(), dest_str],
        ArchiveFormat::TarXz => vec!["-cJf".to_owned(), dest_str],
        ArchiveFormat::TarBz2 => vec!["-cjf".to_owned(), dest_str],
        ArchiveFormat::TarZst => vec!["--zstd".to_owned(), "-cf".to_owned(), dest_str],
        _ => Vec::new(),
    };
    args.extend(names.iter().cloned());
    args
}

/// 外部 CLI 归档创建（M26-4）：PATH 探测候选工具 → Command（cwd=base，
/// 输出捕获）→ 非零退出取 stderr 尾部给可读错误；目标缺失/失败由调用方
/// 清理半成品。
fn pack_cli(
    format: ArchiveFormat,
    dest: &Path,
    sources: &[(String, PathBuf)],
    base: &Path,
) -> std::io::Result<()> {
    let candidates = cli_tool_candidates(format);
    let Some(tool) = candidates.iter().find(|t| crate::pdf::which_ok(t)) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "创建 {} 需要 {}（按 PATH 探测），均未在系统中找到",
                format.ext(),
                candidates.join("/")
            ),
        ));
    };
    let names: Vec<String> = sources.iter().map(|(n, _)| n.clone()).collect();
    let args = cli_create_args(format, dest, &names);
    let out = std::process::Command::new(tool)
        .args(&args)
        .current_dir(base)
        .output()
        .map_err(|e| std::io::Error::new(e.kind(), format!("{tool} 执行失败: {e}")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let tail: Vec<&str> = stderr
            .lines()
            .filter(|l| !l.trim().is_empty())
            .rev()
            .take(3)
            .collect();
        let tail = tail.into_iter().rev().collect::<Vec<_>>().join("; ");
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "{tool} 退出码 {}{}",
                out.status
                    .code()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "信号终止".to_owned()),
                if tail.is_empty() {
                    String::new()
                } else {
                    format!(": {tail}")
                }
            ),
        ));
    }
    if !dest.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "{tool} 未产出目标文件（退出码 {}）",
                out.status.code().map(|c| c.to_string()).unwrap_or_default()
            ),
        ));
    }
    Ok(())
}

/// 7z 单条目递归打包（M26-2）：目录 → `is_directory` 条目 + 子项递归；
/// 文件/软链接 → 读内容为流（SIMPLIFIED：软链接按目标内容收录，
/// zip 写端的"按链接本身收录"语义在 7z 写端不可表达）。
fn pack_7z_entry<W: std::io::Write + std::io::Seek>(
    w: &mut sevenz_rust::SevenZWriter<W>,
    arc: String,
    path: &Path,
) -> std::io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.is_dir() {
        let mut e = sevenz_rust::SevenZArchiveEntry::new();
        e.name = format!("{arc}/");
        e.is_directory = true;
        w.push_archive_entry(e, None::<std::io::Empty>)
            .map_err(sevenz_err)?;
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let leaf = entry.file_name().to_string_lossy().into_owned();
            let sub_arc = format!("{arc}/{leaf}");
            pack_7z_entry(w, sub_arc, &entry.path())?;
        }
        Ok(())
    } else {
        // 软链接 fs::read 跟随目标（与 tar.gz 写端 read 语义一致）。
        let data = fs::read(path)?;
        let mut e = sevenz_rust::SevenZArchiveEntry::new();
        e.name = arc;
        w.push_archive_entry(e, Some(std::io::Cursor::new(data)))
            .map_err(sevenz_err)?;
        Ok(())
    }
}

/// 7z 归档打包（M26-2，sevenz-rust 0.6 纯 Rust 写端；LZMA2 默认方法）。
/// 任何失败：半成品文件由调用方清理（M20-3 教训）。
fn pack_7z(dest: &Path, sources: &[(String, PathBuf)]) -> std::io::Result<()> {
    let mut w = sevenz_rust::SevenZWriter::create(dest).map_err(sevenz_err)?;
    for (name, path) in sources {
        pack_7z_entry(&mut w, name.clone(), path)?;
    }
    w.finish()?; // finish 返回 io::Result（非 sevenz Error）
    Ok(())
}

fn pack_tar(dest: &Path, sources: &[(String, PathBuf)], gzip: bool) -> std::io::Result<()> {
    let file = fs::File::create(dest)?;
    let buf = std::io::BufWriter::new(file);
    if gzip {
        let enc = flate2::write::GzEncoder::new(buf, flate2::Compression::default());
        pack_tar_into(enc, sources)
    } else {
        pack_tar_into(buf, sources)
    }
}

fn pack_tar_into<W: std::io::Write>(w: W, sources: &[(String, PathBuf)]) -> std::io::Result<()> {
    let mut b = tar::Builder::new(w);
    for (name, path) in sources {
        let meta = fs::symlink_metadata(path)?;
        if meta.is_dir() {
            b.append_dir_all(name, path)?;
        } else if meta.is_symlink() {
            let mut header = tar::Header::new_gnu();
            header.set_size(0);
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_mode(0o777);
            b.append_link(&mut header, name, fs::read_link(path)?)?;
        } else {
            b.append_path_with_name(path, name)?;
        }
    }
    b.finish()?;
    Ok(())
}

/// 撤销执行（`u`）：按操作类型逆操作。
/// 逆操作源已不存在时按成功处理（幂等，避免过期操作卡住撤销栈）。
pub fn undo_op(op: &UndoableOp) -> std::io::Result<()> {
    match op {
        // M20-1：远端撤销由 app 层经会话执行（undo_last 分派），不经此函数。
        UndoableOp::Remote { .. } => Ok(()),

        UndoableOp::Delete { path, staged, .. } => {
            // 防覆盖：原位已被占用时拒绝恢复（ACCEPTANCE §4“目标已存在有错误提示”）。
            if path.exists() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!("撤销目标已存在: {}", path.display()),
                ));
            }
            if !staged.exists() {
                return Ok(()); // 暂存已消失：等同已撤销
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            move_path(staged, path)
        }
        UndoableOp::Rename { from, to, .. } => {
            if from.exists() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!("撤销目标已存在: {}", from.display()),
                ));
            }
            if !to.exists() {
                return Ok(()); // 目标已消失：等同已撤销
            }
            fs::rename(to, from)
        }
        UndoableOp::Move { from, to, .. } => {
            if from.exists() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!("撤销目标已存在: {}", from.display()),
                ));
            }
            if !to.exists() {
                return Ok(()); // 目标已消失：等同已撤销
            }
            if let Some(parent) = from.parent() {
                fs::create_dir_all(parent)?;
            }
            move_path(to, from)
        }
        UndoableOp::Create {
            path, recursive, ..
        } => {
            // M28-1：链接产物（符号链接/目录联接）按链接自身移除——
            // `path.is_dir()` 会穿越链接（unix 上 remove_dir 对链接报
            // ENOTDIR；断链 exists() 为 false 会被误判"已撤销"），因此
            // 先按 symlink_metadata 判链接类型。断链 = 只删链接本身。
            let Ok(meta) = fs::symlink_metadata(path) else {
                return Ok(()); // 已不存在：等同已撤销
            };
            if meta.file_type().is_symlink() {
                fs::remove_file(path)
            } else if path.is_dir() {
                if *recursive {
                    // 目录树整体为新建产物（copy 目录）：整树移除。
                    fs::remove_dir_all(path)
                } else {
                    // 非空目录保护性报错，避免连带删除用户后续放入的内容。
                    fs::remove_dir(path)
                }
            } else {
                fs::remove_file(path)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn is_permission_denied_matches_error_kinds_and_errno() {
        // M12-1：提权候选判定。
        let denied = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        assert!(is_permission_denied(&denied));
        let not_found = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        assert!(!is_permission_denied(&not_found));
        let other = std::io::Error::other("boom");
        assert!(!is_permission_denied(&other));
        #[cfg(unix)]
        {
            // 原始 errno 路径：EACCES(13)/EPERM(1)。
            let eacces = std::io::Error::from_raw_os_error(13);
            assert!(is_permission_denied(&eacces));
            let eperm = std::io::Error::from_raw_os_error(1);
            assert!(is_permission_denied(&eperm));
            let enoent = std::io::Error::from_raw_os_error(2);
            assert!(!is_permission_denied(&enoent));
        }
    }

    #[cfg(unix)]
    #[test]
    fn permission_denied_write_detected_as_eacces() {
        // M12-1：真实 EACCES 场景——只读目录内创建文件。
        // root 下写入会成功（root 无视 0o555）：此时跳过断言。
        use std::os::unix::fs::PermissionsExt;
        let tmp = TempDir::new().unwrap();
        let ro_dir = tmp.path().join("ro");
        std::fs::create_dir(&ro_dir).unwrap();
        let meta = std::fs::metadata(&ro_dir).unwrap();
        let mut perms = meta.permissions();
        perms.set_mode(0o555); // r-xr-xr-x
        std::fs::set_permissions(&ro_dir, perms).unwrap();

        match fs::write(ro_dir.join("new.txt"), "x") {
            Ok(()) => { /* root 环境：无法触发 EACCES，跳过 */ }
            Err(err) => assert!(is_permission_denied(&err), "expected EACCES, got {err}"),
        }
        // 恢复权限让 TempDir 清理成功。
        let mut perms = std::fs::metadata(&ro_dir).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&ro_dir, perms).unwrap();
    }

    #[test]
    fn delete_and_undo_roundtrip_file() {
        let tmp = TempDir::new().unwrap();
        let trash = TempDir::new().unwrap();
        let file = tmp.path().join("x.txt");
        fs::write(&file, "hello").unwrap();

        let staged = stage_into(trash.path(), &file).unwrap();
        assert!(!file.exists());
        assert!(staged.is_dir() || staged.is_file());
        assert_eq!(fs::read_to_string(&staged).unwrap(), "hello");

        // undo：移回原位。
        undo_op(&UndoableOp::Delete {
            tab_id: 0,
            path: file.clone(),
            staged: staged.clone(),
        })
        .unwrap();
        assert!(file.exists());
        assert_eq!(fs::read_to_string(&file).unwrap(), "hello");
        assert!(!staged.exists());
    }

    #[test]
    fn delete_and_undo_roundtrip_dir() {
        let tmp = TempDir::new().unwrap();
        let trash = TempDir::new().unwrap();
        let dir = tmp.path().join("sub");
        fs::create_dir_all(dir.join("nested")).unwrap();
        fs::write(dir.join("nested").join("a.txt"), "A").unwrap();

        let staged = stage_into(trash.path(), &dir).unwrap();
        assert!(!dir.exists());

        undo_op(&UndoableOp::Delete {
            tab_id: 0,
            path: dir.clone(),
            staged,
        })
        .unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("nested").join("a.txt")).unwrap(),
            "A"
        );
    }

    #[test]
    fn rename_to_and_undo_restores_name() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("old.txt");
        fs::write(&file, "R").unwrap();

        let op = rename_to(2, &file, "new.txt").unwrap();
        assert!(matches!(op, UndoableOp::Rename { tab_id: 2, .. }));
        assert!(!file.exists());
        let to = tmp.path().join("new.txt");
        assert_eq!(fs::read_to_string(&to).unwrap(), "R");

        undo_op(&op).unwrap();
        assert!(!to.exists());
        assert_eq!(fs::read_to_string(&file).unwrap(), "R");
    }

    #[test]
    fn rename_rejects_invalid_targets() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("a.txt");
        fs::write(&file, "A").unwrap();
        fs::write(tmp.path().join("b.txt"), "B").unwrap();

        // 目标已存在。
        assert!(rename_to(0, &file, "b.txt").is_err());
        // 路径分隔符。
        assert!(rename_to(0, &file, "sub/c.txt").is_err());
        assert!(rename_to(0, &file, "sub\\c.txt").is_err());
        // 空名字。
        assert!(rename_to(0, &file, "").is_err());
        // 原文件未被改动。
        assert_eq!(fs::read_to_string(&file).unwrap(), "A");
    }

    #[test]
    fn move_to_and_undo_restores() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir(&sub).unwrap();
        let file = tmp.path().join("m.txt");
        fs::write(&file, "M").unwrap();

        let op = move_to(1, &file, &sub).unwrap();
        assert!(matches!(op, UndoableOp::Move { tab_id: 1, .. }));
        assert!(!file.exists());
        assert_eq!(fs::read_to_string(sub.join("m.txt")).unwrap(), "M");

        undo_op(&op).unwrap();
        assert!(!sub.join("m.txt").exists());
        assert_eq!(fs::read_to_string(&file).unwrap(), "M");
    }

    #[test]
    fn move_rejects_missing_dir_and_collision() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("m.txt");
        fs::write(&file, "M").unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("m.txt"), "X").unwrap();

        // 目标目录不存在。
        assert!(move_to(0, &file, &tmp.path().join("nope")).is_err());
        // 目标同名冲突。
        assert!(move_to(0, &file, &sub).is_err());
        // 原文件未被改动。
        assert_eq!(fs::read_to_string(&file).unwrap(), "M");
    }

    #[test]
    fn create_dir_and_touch_with_undo() {
        let tmp = TempDir::new().unwrap();

        let dir_op = create_dir(0, tmp.path(), "newdir").unwrap();
        assert!(tmp.path().join("newdir").is_dir());
        undo_op(&dir_op).unwrap();
        assert!(!tmp.path().join("newdir").exists());
        // 幂等：再次撤销同一操作返回成功。
        undo_op(&dir_op).unwrap();

        let file_op = create_file(0, tmp.path(), "t.txt").unwrap();
        assert_eq!(fs::read(tmp.path().join("t.txt")).unwrap(), b"");
        undo_op(&file_op).unwrap();
        assert!(!tmp.path().join("t.txt").exists());
    }

    #[test]
    fn create_rejects_invalid_names_and_protects_nonempty_dir() {
        let tmp = TempDir::new().unwrap();
        // 非法名字。
        assert!(create_dir(0, tmp.path(), "a/b").is_err());
        assert!(create_file(0, tmp.path(), "").is_err());
        assert!(create_file(0, tmp.path(), "x\\y").is_err());
        // 目标已存在。
        fs::write(tmp.path().join("t.txt"), "X").unwrap();
        assert!(create_file(0, tmp.path(), "t.txt").is_err());

        // 非空目录的 undo 被拒绝（保护内容），目录保留。
        let dir_op = create_dir(0, tmp.path(), "d").unwrap();
        fs::write(tmp.path().join("d").join("keep.txt"), "K").unwrap();
        assert!(undo_op(&dir_op).is_err());
        assert!(tmp.path().join("d").is_dir());
    }

    #[test]
    fn copy_to_and_undo_removes_copy() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir(&sub).unwrap();
        let file = tmp.path().join("c.txt");
        fs::write(&file, "C").unwrap();

        // 文件复制：源保留，副本出现；undo 移除副本。
        let op = copy_to(0, &file, &sub).unwrap();
        assert!(matches!(op, UndoableOp::Create { .. }));
        assert!(file.exists(), "复制不移动源");
        assert_eq!(fs::read_to_string(sub.join("c.txt")).unwrap(), "C");
        undo_op(&op).unwrap();
        assert!(!sub.join("c.txt").exists());
        assert!(file.exists());
    }

    #[test]
    fn copy_dir_and_undo_removes_tree() {
        let tmp = TempDir::new().unwrap();
        let dst = TempDir::new().unwrap();
        let dir = tmp.path().join("tree");
        fs::create_dir_all(dir.join("inner")).unwrap();
        fs::write(dir.join("inner").join("a.txt"), "A").unwrap();

        let op = copy_to(0, &dir, dst.path()).unwrap();
        assert_eq!(
            fs::read_to_string(dst.path().join("tree").join("inner").join("a.txt")).unwrap(),
            "A"
        );
        undo_op(&op).unwrap();
        assert!(!dst.path().join("tree").exists());
    }

    #[cfg(unix)]
    #[test]
    fn create_link_file_and_undo_removes_link_not_target() {
        // M28-1：文件符号链接创建 + 撤销——只删链接，不动目标。
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("src.txt");
        fs::write(&target, "S").unwrap();
        let link = tmp.path().join("src-link");
        let op = create_link(0, &target, &link).unwrap();
        assert!(link.is_file(), "链接可读");
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        undo_op(&op).unwrap();
        assert!(!link.exists(), "链接已移除");
        assert!(target.is_file(), "目标不受影响");
    }

    #[cfg(unix)]
    #[test]
    fn create_link_dir_and_undo_removes_link_itself() {
        // M28-1 回归：链接指向目录——undo 必须删链接本身（旧实现
        // `path.is_dir()` 穿越链接走 remove_dir，unix 报 ENOTDIR）。
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("dir");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("a.txt"), "A").unwrap();
        let link = tmp.path().join("dir-link");
        let op = create_link(0, &target, &link).unwrap();
        assert!(link.is_dir(), "链接可进入");
        undo_op(&op).unwrap();
        assert!(!link.exists(), "链接已移除");
        assert!(
            target.is_dir() && target.join("a.txt").is_file(),
            "目标不受影响"
        );
    }

    #[cfg(unix)]
    #[test]
    fn create_link_broken_target_and_undo_removes_broken_link() {
        // M28-1：断链目标——创建成功（指向不存在路径）、链接仍可删
        //（exists() 穿越语义为 false，undo 不得误判"已撤销"）。
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("no-such-target");
        let link = tmp.path().join("broken-link");
        let op = create_link(0, &target, &link).unwrap();
        assert!(fs::symlink_metadata(&link).is_ok());
        assert!(!link.exists(), "断链 exists() 为 false（穿越语义）");
        undo_op(&op).unwrap();
        assert!(fs::symlink_metadata(&link).is_err(), "断链已移除");
    }

    #[cfg(unix)]
    #[test]
    fn create_link_duplicate_rejected() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("t.txt");
        fs::write(&target, "T").unwrap();
        let link = tmp.path().join("l.txt");
        create_link(0, &target, &link).unwrap();
        let err = create_link(0, &target, &link).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn copy_rejects_missing_dir_collision_and_self_nest() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("c.txt");
        fs::write(&file, "C").unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("c.txt"), "X").unwrap();

        assert!(copy_to(0, &file, &tmp.path().join("nope")).is_err());
        assert!(copy_to(0, &file, &sub).is_err()); // 同名冲突
        assert!(copy_to(0, &sub, &sub).is_err()); // 自嵌套
        assert_eq!(fs::read_to_string(&file).unwrap(), "C");
    }

    #[test]
    fn undo_recreates_missing_parent_dirs() {
        let tmp = TempDir::new().unwrap();
        let trash = TempDir::new().unwrap();
        let file = tmp.path().join("gone").join("x.txt");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "V").unwrap();

        let staged = stage_into(trash.path(), &file).unwrap();
        fs::remove_dir_all(file.parent().unwrap()).unwrap(); // 父目录也被删掉

        undo_op(&UndoableOp::Delete {
            tab_id: 0,
            path: file.clone(),
            staged,
        })
        .unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "V");
    }

    #[test]
    fn m264_cli_tool_candidates_and_args() {
        // M26-4：CLI 降级——候选工具表与参数构造（纯函数；探测注入在
        // pack_cli 之前的 PATH which_ok）。
        assert_eq!(cli_tool_candidates(ArchiveFormat::Rar), &["rar"]);
        assert_eq!(
            cli_tool_candidates(ArchiveFormat::Cab),
            &["7z", "7za", "7zr"]
        );
        assert_eq!(
            cli_tool_candidates(ArchiveFormat::Lzh),
            &["7z", "7za", "7zr"]
        );
        assert_eq!(cli_tool_candidates(ArchiveFormat::TarXz), &["tar"]);
        assert_eq!(
            cli_tool_candidates(ArchiveFormat::Zip),
            &[] as &[&str],
            "内建格式无 CLI 候选"
        );
        let args = cli_create_args(
            ArchiveFormat::Rar,
            Path::new("C:/t/o.rar"),
            &["a.txt".to_owned()],
        );
        assert_eq!(args, vec!["a", "-ep1", "C:/t/o.rar", "a.txt"]);
        let args = cli_create_args(
            ArchiveFormat::TarXz,
            Path::new("/t/o.tar.xz"),
            &["a.txt".to_owned()],
        );
        assert_eq!(args, vec!["-cJf", "/t/o.tar.xz", "a.txt"]);
        let args = cli_create_args(
            ArchiveFormat::TarZst,
            Path::new("/t/o.tar.zst"),
            &["b".to_owned()],
        );
        assert_eq!(args, vec!["--zstd", "-cf", "/t/o.tar.zst", "b"]);
    }
}
