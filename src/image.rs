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
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::{FontSize, Image, Resize};

/// M45-4a：字体尺寸兜底/合理性钳制。原生协议查询失败时 ratatui-image 回退
/// 到 10×20；若探测到 0 或离谱比例（宽 > 高、超出常见终端范围），按格子数
/// 推导的图片高度会暴涨并溢出预览窗格。这里把尺寸钳到合理区间（纯函数，
/// 便于测试隔离——不读宿主终端）。
fn safe_font_size(picker: &Picker) -> FontSize {
    let fs = picker.font_size();
    let plausible = fs.width >= 4
        && fs.width <= 64
        && fs.height >= 8
        && fs.height <= 128
        && fs.height >= fs.width;
    if plausible { fs } else { FontSize::new(10, 20) }
}

/// M45-4a / M46-1：虚拟高度上限（单位：格）。超高图片只保留顶部一屏，
/// 不再向 `SlicedProtocol` 提交 3 屏虚拟图形——原生协议的绝对定位图形不受
/// widget 裁剪约束，会在窗格外（甚至窗口顶部）残留。纯函数便于测试。
const MAX_SLICE_SCREENS: u32 = 3;
fn clamp_slice_height(dh: u16, pane_height: u16) -> u16 {
    let max = (pane_height as u32)
        .saturating_mul(MAX_SLICE_SCREENS)
        .min(u16::MAX as u32) as u16;
    dh.min(max).max(1)
}

/// M46-1：纯几何规划。产出图片最终绘制矩形与（超高时）源图顶部裁剪像素
/// 高度。关键不变量：`rect` 严格落在调用方传入的预览窗格 `area` 内，且协议
/// 尺寸 == `rect` 尺寸——从源头杜绝"虚拟图形/绝对定位图形超出预览区"。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ImagePlan {
    rect: Rect,
    crop_px: Option<u32>,
}

fn plan_image(area: Rect, img_w: u32, img_h: u32, fw: f64, fh: f64) -> ImagePlan {
    let iw = img_w.max(1) as f64;
    let ih = img_h.max(1) as f64;
    let cell_w = (iw / fw).ceil().max(1.0);
    let cell_h = (ih / fh).ceil().max(1.0);
    // 原尺寸放得下：不放大，居中。
    if cell_w <= area.width as f64 && cell_h <= area.height as f64 {
        let w = (cell_w as u16).clamp(1, area.width);
        let h = (cell_h as u16).clamp(1, area.height);
        return ImagePlan {
            rect: Rect {
                x: area.x + (area.width - w) / 2,
                y: area.y + (area.height - h) / 2,
                width: w,
                height: h,
            },
            crop_px: None,
        };
    }
    // 超出：fit-width。
    let scale = (area.width as f64 * fw) / iw;
    let dh = ((ih * scale / fh).ceil() as u16).max(1);
    if dh <= area.height {
        return ImagePlan {
            rect: Rect {
                x: area.x,
                y: area.y + (area.height - dh) / 2,
                width: area.width,
                height: dh,
            },
            crop_px: None,
        };
    }
    // 超高：只取顶部一屏，源图裁到对应像素高度；协议尺寸恰好等于窗格尺寸，
    // 不再生成 3 屏虚拟图形（M46-1 真修复）。
    let rows = clamp_slice_height(dh, area.height).min(area.height).max(1);
    let crop = ((rows as f64 * fh) / scale).ceil() as u32;
    ImagePlan {
        rect: area,
        crop_px: Some(crop.clamp(1, img_h.max(1))),
    }
}

/// 支持图片预览的扩展名（小写比较）。
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp"];

/// 是否为可预览的图片文件（按扩展名判断）。
/// M60-3：后台解码（纯 IO + 解码，不碰 Frame；协议编码仍在 UI 线程）。
pub fn decode(path: &Path) -> std::io::Result<image::DynamicImage> {
    image::ImageReader::open(path)?
        .with_guessed_format()?
        .decode()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// M62-1/M85：后台预缩放（显式 Nearest，速度优先）。把大图先缩到
/// `max_side` 以内，后台再做精确视口 resize+编码（M85 ThreadProtocol）。
/// `fast_image_resize` 6.x 默认是 Lanczos3 卷积，旧的"默认 Nearest"注释
/// 是错的；这里显式 `ResizeAlg::Nearest`，避免大图预缩放成为首帧瓶颈。
pub const PREVIEW_MAX_SIDE: u32 = 1600;

pub fn decode_for_preview(path: &Path, max_side: u32) -> std::io::Result<image::DynamicImage> {
    let img = decode(path)?;
    let longest = img.width().max(img.height());
    if longest <= max_side {
        return Ok(img);
    }
    let scale = max_side as f64 / longest as f64;
    let width = ((img.width() as f64) * scale).max(1.0) as u32;
    let height = ((img.height() as f64) * scale).max(1.0) as u32;
    let src = img.to_rgba8();
    let mut resizer = fast_image_resize::Resizer::new();
    let src_view = fast_image_resize::images::Image::from_vec_u8(
        src.width(),
        src.height(),
        src.into_raw(),
        fast_image_resize::PixelType::U8x4,
    )
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let mut dst_view =
        fast_image_resize::images::Image::new(width, height, fast_image_resize::PixelType::U8x4);
    let options =
        fast_image_resize::ResizeOptions::new().resize_alg(fast_image_resize::ResizeAlg::Nearest);
    resizer
        .resize(&src_view, &mut dst_view, Some(&options))
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let resized = image::RgbaImage::from_raw(width, height, dst_view.into_vec())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "缩放缓冲大小不符"))?;
    Ok(image::DynamicImage::ImageRgba8(resized))
}

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

/// M60/M59 用户确认：默认做一次带超时的原生协议查询（tmux/screen 下也查），
/// 失败由 ratatui-image 回退 halfblocks。仅 `RVIFM_IMAGE_PROTOCOL_QUERY=0/false/off`
/// 显式禁用（避免复用器中无响应时的启动等待）。纯函数便于测试。
fn should_query_protocol(
    _tmux: Option<&std::ffi::OsStr>,
    _sty: Option<&std::ffi::OsStr>,
    allow: bool,
    _passthrough: bool,
) -> bool {
    allow
}

fn tmux_passthrough_on() -> bool {
    std::process::Command::new("tmux")
        .args(["show-options", "-gv", "allow-passthrough"])
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim() == "on")
        .unwrap_or(false)
}

impl std::fmt::Debug for ImageEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageEngine")
            .field("protocol", &self.protocol_name())
            .finish()
    }
}

impl ImageEngine {
    /// 启动时探测最佳协议：TTY 下查询终端；查询失败或非 TTY
    /// 回退 halfblocks（无条件可用，保证零配置不出空白）。
    /// 非 TTY（CI、管道、测试）**不做 stdio 查询**——查询会向
    /// stdout 写转义序列并阻塞等待响应。
    pub fn detect() -> Self {
        Self::detect_with(true)
    }

    /// M85：`[preview] image_protocol_query=false` 时直接 halfblocks，
    /// 连 2s 协议查询都不发起（配置在 AppState 构造前即可决定）。
    pub fn detect_with(config_allow: bool) -> Self {
        use ratatui::crossterm::tty::IsTty;
        if !config_allow || !std::io::stdout().is_tty() {
            return Self::halfblocks();
        }
        // 默认做一次协议查询（ratatui-image 内置 2s 超时），tmux/screen 下也查；
        // 若终端不回传响应则回退 halfblocks。可用
        // `RVIFM_IMAGE_PROTOCOL_QUERY=0` 显式跳过查询（避免启动等待）。
        let tmux = std::env::var_os("TMUX");
        let sty = std::env::var_os("STY");
        let allow = match std::env::var_os("RVIFM_IMAGE_PROTOCOL_QUERY") {
            Some(v) => {
                let v = v.to_string_lossy().to_ascii_lowercase();
                !matches!(v.as_str(), "0" | "false" | "off")
            }
            None => true,
        };
        let allow_query = should_query_protocol(
            tmux.as_deref(),
            sty.as_deref(),
            allow,
            tmux_passthrough_on(),
        );
        if !allow_query {
            // halfblocks 无条件可用；passthrough 未开启时原生查询拿不到响应。
            return Self::halfblocks();
        }
        let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
        Self { picker }
    }

    /// 强制 halfblocks（非 TTY 环境：CI / 测试 / 管道）。
    pub fn halfblocks() -> Self {
        Self {
            picker: Picker::halfblocks(),
        }
    }

    /// M85：配置关闭协议查询时立即切 halfblocks（无需重启）。
    pub fn force_halfblocks(&mut self) {
        self.picker = Picker::halfblocks();
    }

    /// M85：把已解码图包装为可后台 resize+编码的协议状态。构造本身不编码，
    /// 真正的缩放/编码由 ThreadProtocol 交给 worker 线程。
    pub(crate) fn resize_protocol(&self, img: image::DynamicImage) -> StatefulProtocol {
        self.picker.new_resize_protocol(img)
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

    /// 将图片文件渲染到预览区域（M34-9 尺寸语义）：
    /// 原始尺寸 ≤ 窗格 → 不放大（`Resize::Fit` 天然行为）且在窗格内居中；
    /// 超出窗格 → 缩放至窗格**宽度**（fit-width），高度超出部分只显示顶部
    /// （普通预览窗格不滚动；SlicedImage 顶部切片）。解码失败返回 Err，
    /// 由调用方降级为文本/提示行，不 panic。
    pub fn render_file(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        path: &Path,
    ) -> std::io::Result<()> {
        let img = decode(path)?;
        self.render_dynamic(frame, area, img)
    }

    /// M60-3：已解码图像的尺寸适配渲染（供后台解码结果在 UI 线程只做协议编码）。
    pub(crate) fn render_dynamic(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        img: image::DynamicImage,
    ) -> std::io::Result<()> {
        let (protocol, rect) = self.encode_for_area(img, area, frame.area())?;
        frame.render_widget(Image::new(&protocol), rect);
        Ok(())
    }

    /// M62-2：把已解码图编码为协议 + 绘制矩形（可缓存复用，避免每帧重编码）。
    pub(crate) fn encode_for_area(
        &self,
        img: image::DynamicImage,
        area: Rect,
        frame_area: Rect,
    ) -> std::io::Result<(ratatui_image::protocol::Protocol, Rect)> {
        // M46-1：只与调用方传入的预览 pane 相交（不再拿整帧兜底）。
        let area = area.intersection(frame_area);
        if area.width == 0 || area.height == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "空预览区",
            ));
        }
        // M45-4a：字体尺寸合理性钳制（原生协议探测兜底 10×20）。
        let fs = safe_font_size(&self.picker);
        let plan = plan_image(
            area,
            img.width(),
            img.height(),
            fs.width.max(1) as f64,
            fs.height.max(1) as f64,
        );
        // 超高图：把源图裁到与窗格等高的像素带，协议尺寸 == 绘制矩形，
        // 原生协议不再生成超出窗格的绝对定位图形（M46-1）。
        let img = match plan.crop_px {
            Some(h) => img.crop_imm(0, 0, img.width(), h.min(img.height())),
            None => img,
        };
        let protocol = self
            .picker
            .new_protocol(
                img,
                Size::new(plan.rect.width, plan.rect.height),
                Resize::Fit(None),
            )
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok((protocol, plan.rect))
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
    fn m621_decode_for_preview_downscales_in_worker() {
        // M62-1：后台预缩放把大图缩到目标边长以内（SIMD 路径）。
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("big.png");
        image::DynamicImage::new_rgba8(200, 100)
            .save(&path)
            .unwrap();
        let small = decode_for_preview(&path, 20).unwrap();
        assert_eq!(small.width(), 20);
        assert_eq!(small.height(), 10);
        let unchanged = decode_for_preview(&path, 512).unwrap();
        assert_eq!(unchanged.width(), 200);
    }

    #[test]
    fn m60_default_protocol_query_even_in_multiplexer() {
        use std::ffi::OsStr;
        // 用户确认：默认在 tmux/screen 下也做一次带超时的协议查询。
        assert!(should_query_protocol(
            Some(OsStr::new("tmux")),
            None,
            true,
            false
        ));
        assert!(should_query_protocol(None, None, true, false));
        // 仅显式 opt-out 关闭。
        assert!(!should_query_protocol(
            Some(OsStr::new("tmux")),
            None,
            false,
            true
        ));
    }

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

    /// 生成不透明纯色 PNG（半块渲染必然出内容，供尺寸断言）。
    fn opaque_png(w: u32, h: u32) -> Vec<u8> {
        let mut rgba = image::RgbaImage::new(w, h);
        for px in rgba.pixels_mut() {
            *px = image::Rgba([255, 0, 0, 255]);
        }
        let img = image::DynamicImage::ImageRgba8(rgba);
        let mut out = std::io::Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    /// 收集非空白单元（symbol 非空格或背景非默认）坐标。
    fn content_cells(buf: &ratatui::buffer::Buffer) -> Vec<(u16, u16)> {
        buf.content
            .iter()
            .enumerate()
            .filter(|(_, c)| c.symbol() != " " || c.bg != ratatui::style::Color::Reset)
            .map(|(i, _)| {
                let x = i as u16 % buf.area().width;
                let y = i as u16 / buf.area().width;
                (x, y)
            })
            .collect()
    }

    #[test]
    fn m34_9_small_image_renders_centered_not_upscaled() {
        // M34-9：小于窗格 → 原尺寸（不放大）居中——内容不再顶格。
        let engine = ImageEngine::halfblocks();
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("small.png");
        std::fs::write(&path, opaque_png(8, 8)).unwrap();
        let backend = TestBackend::new(40, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let _ = engine.render_file(f, f.area(), &path);
            })
            .unwrap();
        let cells = content_cells(terminal.backend().buffer());
        assert!(!cells.is_empty(), "内容存在");
        let min_x = cells.iter().map(|c| c.0).min().unwrap();
        assert!(min_x >= 10, "居中：最左内容列 {min_x}（旧实现顶格为 0）");
        assert!(
            cells
                .iter()
                .all(|(x, y)| (10..30).contains(x) && (2..10).contains(y)),
            "全部内容落在中央区: {cells:?}"
        );
    }

    #[test]
    fn m454a_font_size_and_slice_height_guards() {
        // M45-4a：字体尺寸异常回退 10×20；合理值保留。
        // FontSize 未实现 PartialEq，逐字段断言。
        let fs_eq = |a: FontSize, w: u16, h: u16| (a.width, a.height) == (w, h);
        let normal = Picker::halfblocks();
        assert!(fs_eq(safe_font_size(&normal), 10, 20));
        #[allow(deprecated)]
        let zero = Picker::from_fontsize(FontSize::new(0, 0));
        assert!(fs_eq(safe_font_size(&zero), 10, 20), "0 尺寸回退");
        #[allow(deprecated)]
        let weird = Picker::from_fontsize(FontSize::new(40, 10)); // 宽 > 高
        assert!(fs_eq(safe_font_size(&weird), 10, 20), "比例离谱回退");
        #[allow(deprecated)]
        let valid = Picker::from_fontsize(FontSize::new(9, 18));
        assert!(fs_eq(safe_font_size(&valid), 9, 18), "合理值保留");

        // 虚拟高度钳制：至多 3 屏，至少 1 行。
        assert_eq!(clamp_slice_height(1000, 12), 36);
        assert_eq!(clamp_slice_height(5, 12), 5);
        assert_eq!(clamp_slice_height(0, 12), 1);
    }

    #[test]
    fn m34_9_large_image_fit_width_top_slice() {
        // M34-9：大于窗格 → fit-width（内容到达右缘；旧 Fit 受高度约束缩在
        // 中部）+ 超高部分顶部切片不越界。
        let engine = ImageEngine::halfblocks();
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("big.png");
        std::fs::write(&path, opaque_png(600, 600)).unwrap();
        let backend = TestBackend::new(40, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let _ = engine.render_file(f, f.area(), &path);
            })
            .unwrap();
        let cells = content_cells(terminal.backend().buffer());
        assert!(!cells.is_empty(), "内容存在");
        assert!(
            cells.iter().any(|(x, _)| *x >= 38),
            "fit-width：内容到达右缘"
        );
        assert!(
            cells.iter().all(|(x, y)| *x < 40 && *y < 12),
            "切片不越界: {:?}",
            cells.iter().max().unwrap()
        );
    }

    /// M46-1：几何规划产出矩形严格落在预览 pane 内；超高图改为等高裁剪，
    /// 不再产生超过窗格高度的虚拟图形。
    #[test]
    fn m461_image_plan_stays_within_pane() {
        let pane = Rect::new(20, 5, 30, 10);
        // 小图：居中且在 pane 内。
        let small = plan_image(pane, 8, 8, 10.0, 20.0);
        assert!(small.crop_px.is_none());
        assert!(
            small.rect.x >= pane.x
                && small.rect.y >= pane.y
                && small.rect.right() <= pane.right()
                && small.rect.bottom() <= pane.bottom()
        );
        // 大图（fit-width 后仍超高）：目标矩形 == pane，源图被裁剪。
        let huge = plan_image(pane, 600, 600, 10.0, 20.0);
        assert_eq!(huge.rect, pane);
        let crop = huge.crop_px.expect("超高图必须裁剪源图");
        assert!((1..=600).contains(&crop), "crop={crop}");
        // 中等图：fit-width 后高度放得下，不裁剪。
        let medium = plan_image(pane, 600, 60, 10.0, 20.0);
        assert!(medium.crop_px.is_none());
        assert!(medium.rect.width == pane.width && medium.rect.height <= pane.height);
    }

    /// M46-1：图片渲染只落在传入的预览子矩形内，不向窗口其它区域溢出。
    #[test]
    fn m461_render_into_subrect_does_not_leak() {
        let engine = ImageEngine::halfblocks();
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("big.png");
        std::fs::write(&path, opaque_png(600, 600)).unwrap();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let pane = Rect::new(20, 5, 30, 10);
        terminal
            .draw(|f| {
                let _ = engine.render_file(f, pane, &path);
            })
            .unwrap();
        let cells = content_cells(terminal.backend().buffer());
        assert!(!cells.is_empty());
        assert!(
            cells
                .iter()
                .all(|(x, y)| (20..50).contains(x) && (5..15).contains(y)),
            "图片内容不得越出预览子矩形: {cells:?}"
        );
        assert!(
            cells.iter().all(|(_, y)| *y >= 5),
            "图片顶部不得出现在窗口顶部（y<5）"
        );
    }
}
