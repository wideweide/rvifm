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

/// 新建目录（M2-5，`:mkdir <名字>`）。
pub fn create_dir(tab_id: usize, base: &Path, name: &str) -> std::io::Result<UndoableOp> {
    let path = validate_new_name(base, name)?;
    fs::create_dir(&path)?;
    Ok(UndoableOp::Create { tab_id, path })
}

/// 新建空文件（M2-5，`:touch <名字>`）。
pub fn create_file(tab_id: usize, base: &Path, name: &str) -> std::io::Result<UndoableOp> {
    let path = validate_new_name(base, name)?;
    fs::File::create(&path)?;
    Ok(UndoableOp::Create { tab_id, path })
}

/// 撤销执行（`u`）：按操作类型逆操作。
/// 逆操作源已不存在时按成功处理（幂等，避免过期操作卡住撤销栈）。
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
        UndoableOp::Create { path, .. } => {
            if !path.exists() {
                return Ok(()); // 已不存在：等同已撤销
            }
            // 非空目录保护性报错，避免连带删除用户后续放入的内容。
            if path.is_dir() {
                fs::remove_dir(path)
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
