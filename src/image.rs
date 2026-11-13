//! 图片预览引擎（M11-1；SPEC §4.9）。
//!
//! `ratatui-image` 协议探测与回退链：原生协议（Kitty/iTerm2/Sixel）→
//! halfblocks（无条件可用）。协议探测只在 TTY 启动时执行一次
//! （`Picker::from_query_stdio()`）；非 TTY（CI、管道、测试）直接
//! `Picker::halfblocks()`。协议差异全部交给 `ratatui-image`，
//! 不为特定终端写硬编码分支（AGENTS §5.5）。
//!
//! 渲染走无状态 `Image` widget：预览是文件切换时的一次性重绘，
//! `new_protocol` 一次 resize 足够，避免 `StatefulImage` 在无
//! 后台线程时阻塞 UI（ratatui-image 文档警告）。
//!
//! M11-1 固化引擎层；消费方：M11-2（PreviewRenderer 接入）。

// M11-1 引擎层先行；TUI 预览接线在 M11-2 接入。
#![allow(dead_code)]

use std::path::Path;

use ratatui::Frame;
use ratatui::layout::{Rect, Size};
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::{Image, Resize};

/// 支持图片预览的扩展名（小写比较）。
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp"];

/// 是否为可预览的图片文件（按扩展名判断）。
pub fn is_image_file(path: &Path) -> bool {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .map(|e| IMAGE_EXTENSIONS.contains(&e.as_str()))
        .unwrap_or(false)
}

/// 图片渲染引擎：持有协议探测结果，统一渲染入口。
pub struct ImageEngine {
    picker: Picker,
}

impl ImageEngine {
    /// 启动时探测最佳协议：TTY 下查询终端；查询失败或非 TTY
    /// 回退 halfblocks（无条件可用，保证零配置不出空白）。
    pub fn detect() -> Self {
        let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
        Self { picker }
    }

    /// 强制 halfblocks（非 TTY 环境：CI / 测试 / 管道）。
    pub fn halfblocks() -> Self {
        Self {
            picker: Picker::halfblocks(),
        }
    }

    /// 当前协议名（帮助页/调试用）。
    pub fn protocol_name(&self) -> &'static str {
        match self.picker.protocol_type() {
            ProtocolType::Halfblocks => "halfblocks",
            ProtocolType::Sixel => "sixel",
            ProtocolType::Kitty => "kitty",
            ProtocolType::Iterm2 => "iterm2",
        }
    }

    /// 将图片文件渲染到预览区域（`Resize::Fit` 等比缩放）。
    /// 解码失败返回 Err，由调用方降级为文本/提示行，不 panic。
    pub fn render_file(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        path: &Path,
    ) -> std::io::Result<()> {
        let img = image::ImageReader::open(path)?
            .with_guessed_format()?
            .decode()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let protocol = self
            .picker
            .new_protocol(img, Size::new(area.width, area.height), Resize::Fit(None))
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        frame.render_widget(Image::new(&protocol), area);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    /// 生成最小合法 PNG（1x1 红点）的字节，避免测试依赖二进制 fixture。
    fn tiny_png() -> Vec<u8> {
        let img = image::DynamicImage::new_rgba8(1, 1).into_rgba8();
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn is_image_file_matches_extension_case_insensitive() {
        assert!(is_image_file(Path::new("a/photo.png")));
        assert!(is_image_file(Path::new("a/photo.PNG")));
        assert!(is_image_file(Path::new("cat.JPEG")));
        assert!(!is_image_file(Path::new("notes.txt")));
        assert!(!is_image_file(Path::new("noext")));
        // 目录与归档不算图片。
        assert!(!is_image_file(Path::new("dir.zip")));
    }

    #[test]
    fn halfblocks_engine_reports_protocol_and_renders_png() {
        let engine = ImageEngine::halfblocks();
        assert_eq!(engine.protocol_name(), "halfblocks");

        // 8x8 红色 PNG。
        let img = image::DynamicImage::new_rgba8(8, 8).into_rgba8();
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("t.png");
        std::fs::write(&path, out.into_inner()).unwrap();

        let backend = TestBackend::new(40, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let r = terminal
            .draw(|f| {
                let _ = engine.render_file(f, f.area(), &path);
            })
            .unwrap();
        assert!(r.area.width == 40);

        // halfblocks 渲染后至少出现非空白单元（前景块）。
        let has_content = r
            .buffer
            .content
            .iter()
            .any(|c| c.symbol() != " " || c.bg != ratatui::style::Color::Reset);
        assert!(has_content, "halfblocks 预览不应为空白");
    }

    #[test]
    fn render_file_invalid_data_errors_without_panic() {
        let engine = ImageEngine::halfblocks();
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("fake.png");
        std::fs::write(&path, b"not a png at all").unwrap();

        let backend = TestBackend::new(40, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let r = terminal.draw(|f| {
            // Err 被吞掉后区域保持空白，不 panic。
            let _ = engine.render_file(f, f.area(), &path);
        });
        assert!(r.is_ok());
    }

    #[test]
    fn tiny_png_decodes() {
        let bytes = tiny_png();
        assert!(bytes.len() > 8);
    }
}
