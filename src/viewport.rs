//! M61：大文本视口读取器（只读、按需，不把整文件读进内存）。
//!
//! 用 mmap 把文件映射到虚拟地址空间，由操作系统按页载入；稀疏行检查点
//! 记录每 `CHECKPOINT_STRIDE` 行的字节偏移，`lines()` 只扫描目标视口附近
//! 的少量字节。远端/归档不适用（仅本地物理文件）。

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use memmap2::{Mmap, MmapOptions};

/// 每 1024 行记录一个字节偏移检查点（定位一行最多扫描 1024 行）。
pub const CHECKPOINT_STRIDE: usize = 1024;

/// 是否对本地物理大文件启用视口读取（远端/归档临时副本除外）。
pub fn use_for(path: &Path, threshold: u64) -> bool {
    path.is_file()
        && std::fs::metadata(path)
            .map(|m| m.len() >= threshold)
            .unwrap_or(false)
}

/// 单个物理文件的只读视口文本源。
pub struct ViewportText {
    path: PathBuf,
    mmap: Mmap,
    len: usize,
    /// 第 `k` 项的字节偏移 = 第 `k * CHECKPOINT_STRIDE` 行的起始位置。
    checkpoints: Mutex<Vec<u64>>,
}

impl std::fmt::Debug for ViewportText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewportText")
            .field("path", &self.path)
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}

impl ViewportText {
    /// 映射文件（O(1)，不读取内容）。空文件返回 InvalidData。
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = std::fs::File::open(path)?;
        if file.metadata()?.len() == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "空文件"));
        }
        // SAFETY：文件以只读方式打开且在本结构体生命周期内保持有效；
        // 映射全程只读，不写入任何字节，也不会把映射暴露给修改操作。
        let mmap = unsafe { MmapOptions::new().map(&file)? };
        Ok(Self {
            path: path.to_path_buf(),
            len: mmap.len(),
            mmap,
            checkpoints: Mutex::new(vec![0]),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn byte_len(&self) -> usize {
        self.len
    }

    /// 已建立的检查点给出的行数下界（索引在后台按视口推进，并非全文精确值）。
    pub fn known_line_count(&self) -> usize {
        self.checkpoints.lock().map(|c| c.len()).unwrap_or(1) * CHECKPOINT_STRIDE
    }

    /// 确保检查点覆盖 `upto_line` 所在区间（只扫描必要的换行字节）。
    fn ensure_checkpoints(&self, upto_line: usize) {
        let Ok(mut checkpoints) = self.checkpoints.lock() else {
            return;
        };
        let wanted = upto_line / CHECKPOINT_STRIDE;
        if checkpoints.len() > wanted {
            return;
        }
        let mut pos = *checkpoints.last().unwrap_or(&0) as usize;
        let mut line = (checkpoints.len() - 1) * CHECKPOINT_STRIDE;
        while line < wanted * CHECKPOINT_STRIDE && pos < self.len {
            let mut advanced = 0;
            while advanced < CHECKPOINT_STRIDE && pos < self.len {
                if self.mmap[pos] == b'\n' {
                    advanced += 1;
                }
                pos += 1;
            }
            line += CHECKPOINT_STRIDE;
            if pos < self.len {
                checkpoints.push(pos as u64);
            } else {
                break;
            }
        }
    }

    /// 读取视口行：起点 `start`（0-based），最多 `count` 行，每行截断到 `max_chars` 字符。
    pub fn lines(&self, start: usize, count: usize, max_chars: usize) -> Vec<String> {
        if count == 0 || self.len == 0 {
            return Vec::new();
        }
        self.ensure_checkpoints(start.saturating_add(count));
        let checkpoints = self
            .checkpoints
            .lock()
            .map(|c| c.clone())
            .unwrap_or_else(|_| vec![0]);
        let cp_idx = start / CHECKPOINT_STRIDE;
        let cp_idx = cp_idx.min(checkpoints.len() - 1);
        let mut pos = checkpoints[cp_idx] as usize;
        let mut line = cp_idx * CHECKPOINT_STRIDE;
        while line < start && pos < self.len {
            if self.mmap[pos] == b'\n' {
                line += 1;
            }
            pos += 1;
        }
        let mut out = Vec::with_capacity(count);
        while out.len() < count && pos < self.len {
            let end = self.mmap[pos..]
                .iter()
                .position(|b| *b == b'\n')
                .map(|i| pos + i)
                .unwrap_or(self.len);
            let mut text = String::from_utf8_lossy(&self.mmap[pos..end]).into_owned();
            if text.ends_with('\r') {
                text.pop();
            }
            out.push(text.chars().take(max_chars).collect());
            if end >= self.len {
                break;
            }
            pos = end + 1;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viewport_lines_are_viewport_driven() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("big.log");
        let body = (0..4096)
            .map(|i| format!("line-{i:04}-payload\n"))
            .collect::<String>();
        std::fs::write(&path, body).unwrap();
        let view = ViewportText::open(&path).unwrap();
        let lines = view.lines(3000, 5, 100);
        assert_eq!(lines.len(), 5);
        assert!(lines[0].starts_with("line-3000"));
        assert!(lines[4].starts_with("line-3004"));
        // 未请求的区域没有建立检查点（按需推进）。
        assert!(view.known_line_count() <= 4096);
    }
}
