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
    Ok(UndoableOp::Delete {
        tab_id,
        path: path.to_path_buf(),
        staged,
    })
}

/// 撤销执行（`u`）：按操作类型逆操作。仅 Delete 在 M2-2 接入。
pub fn undo_op(op: &UndoableOp) -> std::io::Result<()> {
    match op {
        UndoableOp::Delete { path, staged, .. } => {
            // 防覆盖：原位已被占用时拒绝恢复（ACCEPTANCE §4“目标已存在有错误提示”）。
            if path.exists() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!("撤销目标已存在: {}", path.display()),
                ));
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            move_path(staged, path)
        }
        // Rename/Move/Create 的 undo 在 M2-3～M2-5 接入。
        UndoableOp::Rename { .. } | UndoableOp::Move { .. } | UndoableOp::Create { .. } => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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
}
