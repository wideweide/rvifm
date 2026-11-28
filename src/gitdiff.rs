//! M69：git 统一补丁解析与渲染（只读，不执行任何 git 写操作）。
//!
//! 项目坚持 git CLI 零编译期依赖路线：diff 由 `git diff` 产出，这里只做
//! 结构化解析与 Ratatui 行渲染；词级差异用纯 Rust `similar`（M82）。

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffLineKind {
    Context,
    Add,
    Del,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffLineKind,
    pub text: String,
    pub old_no: Option<usize>,
    pub new_no: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

/// 单个文件的 diff 数据（后台 worker 产出，UI 线程只读）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffFileData {
    pub path: String,
    pub hunks: Vec<DiffHunk>,
    pub old_full: Vec<String>,
    pub new_full: Vec<String>,
    pub staged: bool,
}

/// 解析 `git diff --no-color -U3` 输出（忽略文件头与 "\ No newline" 行）。
pub fn parse_unified(input: &str) -> Vec<DiffHunk> {
    let mut hunks = Vec::new();
    let mut current: Option<DiffHunk> = None;
    let (mut old_no, mut new_no) = (0usize, 0usize);
    for raw in input.lines() {
        if raw.starts_with("@@") {
            if let Some(done) = current.take() {
                hunks.push(done);
            }
            let mut old = 0usize;
            let mut new = 0usize;
            for part in raw.split_whitespace() {
                let nums = part.trim_start_matches(['-', '+']);
                let first = nums.split(',').next().unwrap_or("0").parse().unwrap_or(0);
                if part.starts_with('-') {
                    old = first;
                } else if part.starts_with('+') {
                    new = first;
                }
            }
            old_no = old;
            new_no = new;
            current = Some(DiffHunk {
                header: raw.to_owned(),
                lines: Vec::new(),
            });
            continue;
        }
        if raw.starts_with("---") || raw.starts_with("+++") || raw.starts_with("\\ No newline") {
            continue;
        }
        let Some(hunk) = current.as_mut() else {
            continue;
        };
        let (kind, text, old, new) = match raw.as_bytes().first() {
            Some(b'+') => (DiffLineKind::Add, &raw[1..], None, Some(new_no)),
            Some(b'-') => (DiffLineKind::Del, &raw[1..], Some(old_no), None),
            Some(b' ') => (DiffLineKind::Context, &raw[1..], Some(old_no), Some(new_no)),
            _ => continue,
        };
        hunk.lines.push(DiffLine {
            kind,
            text: text.to_owned(),
            old_no: old,
            new_no: new,
        });
        if old.is_some() {
            old_no += 1;
        }
        if new.is_some() {
            new_no += 1;
        }
    }
    if let Some(hunk) = current {
        hunks.push(hunk);
    }
    hunks
}

/// 软换行（默认开启）：按字符宽度折行，续行带缩进标记。
fn wrap_line(text: &str, width: usize, prefix: &str) -> Vec<String> {
    if width == 0 {
        return vec![text.to_owned()];
    }
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= width {
        return vec![format!("{prefix}{text}")];
    }
    let mut out = Vec::new();
    for (idx, chunk) in chars.chunks(width).enumerate() {
        out.push(format!(
            "{}{}",
            if idx == 0 { prefix } else { "  " },
            chunk.iter().collect::<String>()
        ));
    }
    out
}

/// 统一视图渲染：返回渲染行与每个 hunk 的起始行（供 J/K 跳转）。
pub fn render_unified(hunks: &[DiffHunk], wrap: bool, width: usize) -> (Vec<String>, Vec<usize>) {
    let mut rows = Vec::new();
    let mut starts = Vec::new();
    for hunk in hunks {
        starts.push(rows.len());
        rows.push(hunk.header.clone());
        for line in &hunk.lines {
            let marker = match line.kind {
                DiffLineKind::Add => "+",
                DiffLineKind::Del => "-",
                DiffLineKind::Context => " ",
            };
            let text = if wrap {
                wrap_line(&line.text, width.saturating_sub(2), marker)
            } else {
                vec![format!("{marker}{}", line.text)]
            };
            rows.extend(text);
        }
    }
    (rows, starts)
}

/// 左右对照渲染：false = 仅差异 hunk；true = 完整文件双列（不做精细配对）。
pub fn render_side_by_side(
    data: &DiffFileData,
    full: bool,
    wrap: bool,
    width: usize,
) -> (Vec<(String, String)>, Vec<usize>) {
    let half = width / 2;
    let mut rows: Vec<(String, String)> = Vec::new();
    let mut starts = Vec::new();
    if full {
        starts.push(0);
        let total = data.old_full.len().max(data.new_full.len());
        for i in 0..total {
            let left = data
                .old_full
                .get(i)
                .map(|s| {
                    if wrap {
                        wrap_line(s, half, " ")
                    } else {
                        vec![s.clone()]
                    }
                })
                .unwrap_or_default();
            let right = data
                .new_full
                .get(i)
                .map(|s| {
                    if wrap {
                        wrap_line(s, half, " ")
                    } else {
                        vec![s.clone()]
                    }
                })
                .unwrap_or_default();
            for j in 0..left.len().max(right.len()) {
                rows.push((
                    left.get(j).cloned().unwrap_or_default(),
                    right.get(j).cloned().unwrap_or_default(),
                ));
            }
        }
        return (rows, starts);
    }
    for hunk in &data.hunks {
        starts.push(rows.len());
        let dels: Vec<_> = hunk
            .lines
            .iter()
            .filter(|l| l.kind != DiffLineKind::Add)
            .collect();
        let adds: Vec<_> = hunk
            .lines
            .iter()
            .filter(|l| l.kind != DiffLineKind::Del)
            .collect();
        for i in 0..dels.len().max(adds.len()) {
            let left = dels
                .get(i)
                .map(|l| {
                    let m = if l.kind == DiffLineKind::Del {
                        "-"
                    } else {
                        " "
                    };
                    if wrap {
                        wrap_line(&l.text, half, m)
                    } else {
                        vec![format!("{m}{}", l.text)]
                    }
                })
                .unwrap_or_default();
            let right = adds
                .get(i)
                .map(|l| {
                    let m = if l.kind == DiffLineKind::Add {
                        "+"
                    } else {
                        " "
                    };
                    if wrap {
                        wrap_line(&l.text, half, m)
                    } else {
                        vec![format!("{m}{}", l.text)]
                    }
                })
                .unwrap_or_default();
            for j in 0..left.len().max(right.len()) {
                rows.push((
                    left.get(j).cloned().unwrap_or_default(),
                    right.get(j).cloned().unwrap_or_default(),
                ));
            }
        }
    }
    (rows, starts)
}

/// M71：单个 hunk 的确认预览文本。
pub fn hunk_preview(hunks: &[DiffHunk], index: usize) -> String {
    let Some(hunk) = hunks.get(index) else {
        return "（无差异块）".to_owned();
    };
    let mut out = hunk.header.clone();
    for line in &hunk.lines {
        let marker = match line.kind {
            DiffLineKind::Add => "+",
            DiffLineKind::Del => "-",
            DiffLineKind::Context => " ",
        };
        out.push('\n');
        out.push_str(marker);
        out.push_str(&line.text);
    }
    out
}

/// M82：词级差异（左删红底，右增绿底），用于左右对照。
pub fn word_spans(old: &str, new: &str, left: bool) -> Vec<Span<'static>> {
    let diff = similar::TextDiff::from_words(old, new);
    let mut spans = Vec::new();
    for change in diff.iter_all_changes() {
        let text = change.value().to_owned();
        if text.is_empty() {
            continue;
        }
        match change.tag() {
            similar::ChangeTag::Equal => spans.push(Span::from(text)),
            similar::ChangeTag::Delete if left => spans.push(Span::styled(
                text,
                Style::new().bg(Color::LightRed).fg(Color::Black),
            )),
            similar::ChangeTag::Insert if !left => spans.push(Span::styled(
                text,
                Style::new().bg(Color::LightGreen).fg(Color::Black),
            )),
            _ => {}
        }
    }
    spans
}

fn highlighter_for(path: &str, sample: &str) -> Option<crate::highlight::Highlighter> {
    let dark = crate::theme::active_variant() != opaline::schema::ThemeVariant::Light;
    crate::highlight::Highlighter::for_path(std::path::Path::new(path), sample, dark)
}

fn kind_style(kind: DiffLineKind) -> Style {
    match kind {
        DiffLineKind::Add => Style::new().bg(Color::Rgb(20, 80, 20)),
        DiffLineKind::Del => Style::new().bg(Color::Rgb(80, 20, 20)),
        DiffLineKind::Context => Style::new(),
    }
}

/// 按字节区间裁剪语法片段（保留样式）；无片段时回退原文切片。
fn token_slice(
    tokens: &[(String, Style)],
    text: &str,
    start: usize,
    end: usize,
) -> Vec<Span<'static>> {
    if tokens.is_empty() {
        return vec![Span::from(text.get(start..end).unwrap_or("").to_owned())];
    }
    let mut out = Vec::new();
    let mut pos = 0usize;
    for (tok, style) in tokens {
        let len = tok.len();
        let s = pos;
        let e = pos + len;
        pos = e;
        if e <= start || s >= end {
            continue;
        }
        let a = start.saturating_sub(s).min(len);
        let b = end.saturating_sub(s).min(len);
        out.push(Span::styled(tok[a..b].to_owned(), *style));
    }
    out
}

/// 把带语法着色的行按宽度切块并保留 diff 标记前缀（H1/H2 回归）。
fn styled_chunks(
    tokens: &[(String, Style)],
    text: &str,
    wrap: bool,
    width: usize,
    marker: &str,
) -> Vec<Vec<Span<'static>>> {
    let prefix = marker.chars().count();
    let eff = width.saturating_sub(prefix + 1).max(1);
    let chars: Vec<char> = text.chars().collect();
    let n = if wrap {
        chars.len().div_ceil(eff).max(1)
    } else {
        1
    };
    let mut rows = Vec::with_capacity(n);
    for idx in 0..n {
        let (byte_a, byte_b) = if wrap {
            let a = (idx * eff).min(chars.len());
            let b = ((idx + 1) * eff).min(chars.len());
            let ba: usize = chars[..a].iter().map(|c| c.len_utf8()).sum();
            let bb: usize = chars[..b].iter().map(|c| c.len_utf8()).sum();
            (ba, bb)
        } else {
            (0, text.len())
        };
        let mut spans = vec![Span::from(if idx == 0 {
            marker.to_owned()
        } else {
            " ".repeat(prefix)
        })];
        spans.extend(token_slice(tokens, text, byte_a, byte_b));
        rows.push(spans);
    }
    rows
}

/// M82：带语法高亮的统一补丁渲染。
pub fn render_unified_styled(
    hunks: &[DiffHunk],
    path: &str,
    wrap: bool,
    width: usize,
) -> (Vec<Line<'static>>, Vec<usize>) {
    let mut rows = Vec::new();
    let mut starts = Vec::new();
    let sample = hunks
        .first()
        .and_then(|h| h.lines.first())
        .map(|l| l.text.as_str())
        .unwrap_or("");
    let mut hl = highlighter_for(path, sample);
    for hunk in hunks {
        starts.push(rows.len());
        rows.push(Line::from(Span::styled(
            hunk.header.clone(),
            Style::new().add_modifier(ratatui::style::Modifier::BOLD),
        )));
        for line in &hunk.lines {
            let marker = match line.kind {
                DiffLineKind::Add => "+",
                DiffLineKind::Del => "-",
                DiffLineKind::Context => " ",
            };
            let tokens = hl
                .as_mut()
                .map(|h| h.highlight_line(&line.text))
                .unwrap_or_default();
            for spans in styled_chunks(&tokens, &line.text, wrap, width, marker) {
                rows.push(Line::from(spans).style(kind_style(line.kind)));
            }
        }
    }
    (rows, starts)
}

/// M82：带词级高亮的左右对照渲染（两个 Block 各占一列，分隔线连续）。
pub fn render_side_by_side_styled(
    data: &DiffFileData,
    full: bool,
    wrap: bool,
    width: usize,
) -> (Vec<(Line<'static>, Line<'static>)>, Vec<usize>) {
    let half = width / 2;
    let mut rows = Vec::new();
    let mut starts = Vec::new();
    if full {
        starts.push(0);
        let sample = data
            .old_full
            .first()
            .or_else(|| data.new_full.first())
            .map(String::as_str)
            .unwrap_or("");
        // 左右各用一个高亮器，跨行状态独立推进（H2）。
        let mut hl_l = highlighter_for(&data.path, sample);
        let mut hl_r = highlighter_for(&data.path, sample);
        for i in 0..data.old_full.len().max(data.new_full.len()) {
            let left = data.old_full.get(i).cloned().unwrap_or_default();
            let right = data.new_full.get(i).cloned().unwrap_or_default();
            let tl = hl_l
                .as_mut()
                .map(|h| h.highlight_line(&left))
                .unwrap_or_default();
            let tr = hl_r
                .as_mut()
                .map(|h| h.highlight_line(&right))
                .unwrap_or_default();
            let lrows = styled_chunks(&tl, &left, wrap, half, "");
            let rrows = styled_chunks(&tr, &right, wrap, half, "");
            // M106-4：完整文件视图不能丢掉增删高亮——行不同则左红右绿，
            // 与补丁视图的 DiffLineKind 配色同源。
            let changed = left != right;
            let base_l = if changed {
                Style::new().bg(Color::Rgb(80, 20, 20))
            } else {
                Style::new()
            };
            let base_r = if changed {
                Style::new().bg(Color::Rgb(20, 80, 20))
            } else {
                Style::new()
            };
            for k in 0..lrows.len().max(rrows.len()) {
                let l = lrows.get(k).cloned().unwrap_or_default();
                let r = rrows.get(k).cloned().unwrap_or_default();
                rows.push((Line::from(l).style(base_l), Line::from(r).style(base_r)));
            }
        }
        return (rows, starts);
    }
    for hunk in &data.hunks {
        starts.push(rows.len());
        let dels: Vec<_> = hunk
            .lines
            .iter()
            .filter(|l| l.kind != DiffLineKind::Add)
            .collect();
        let adds: Vec<_> = hunk
            .lines
            .iter()
            .filter(|l| l.kind != DiffLineKind::Del)
            .collect();
        for i in 0..dels.len().max(adds.len()) {
            let left_line = dels.get(i);
            let right_line = adds.get(i);
            let (old, new) = (
                left_line.map(|l| l.text.as_str()).unwrap_or(""),
                right_line.map(|l| l.text.as_str()).unwrap_or(""),
            );
            let lspans = if wrap {
                wrap_line(
                    old,
                    half,
                    if left_line.is_some_and(|l| l.kind == DiffLineKind::Del) {
                        "-"
                    } else {
                        " "
                    },
                )
                .into_iter()
                .map(Span::from)
                .collect()
            } else {
                // M4：词级模式也保留 +/- 标记，与 plain/wrap 一致。
                let mut spans = vec![Span::from(
                    if left_line.is_some_and(|l| l.kind == DiffLineKind::Del) {
                        "-".to_owned()
                    } else {
                        " ".to_owned()
                    },
                )];
                spans.extend(word_spans(old, new, true));
                spans
            };
            let rspans = if wrap {
                wrap_line(
                    new,
                    half,
                    if right_line.is_some_and(|l| l.kind == DiffLineKind::Add) {
                        "+"
                    } else {
                        " "
                    },
                )
                .into_iter()
                .map(Span::from)
                .collect()
            } else {
                let mut spans = vec![Span::from(
                    if right_line.is_some_and(|l| l.kind == DiffLineKind::Add) {
                        "+".to_owned()
                    } else {
                        " ".to_owned()
                    },
                )];
                spans.extend(word_spans(old, new, false));
                spans
            };
            let lstyle = left_line.map(|l| kind_style(l.kind)).unwrap_or_default();
            let rstyle = right_line.map(|l| kind_style(l.kind)).unwrap_or_default();
            rows.push((
                Line::from(lspans).style(lstyle),
                Line::from(rspans).style(rstyle),
            ));
        }
    }
    (rows, starts)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -1,3 +1,3 @@\n keep\n-old\n+new\n last\n";

    #[test]
    fn gitdiff_parses_hunks_and_line_numbers() {
        let hunks = parse_unified(SAMPLE);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].lines.len(), 4);
        assert_eq!(hunks[0].lines[1].kind, DiffLineKind::Del);
        assert_eq!(hunks[0].lines[1].old_no, Some(2));
        assert_eq!(hunks[0].lines[2].kind, DiffLineKind::Add);
        assert_eq!(hunks[0].lines[2].new_no, Some(2));
    }

    #[test]
    fn m1064_full_view_keeps_diff_highlight() {
        let data = DiffFileData {
            path: "f.txt".to_owned(),
            hunks: Vec::new(),
            old_full: vec!["keep".to_owned(), "old".to_owned()],
            new_full: vec!["keep".to_owned(), "new".to_owned()],
            staged: false,
        };
        let (rows, _) = render_side_by_side_styled(&data, true, false, 80);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0.style.bg, None, "相同行不应着色");
        assert_eq!(
            rows[1].0.style.bg,
            Some(Color::Rgb(80, 20, 20)),
            "差异行左侧红底"
        );
        assert_eq!(
            rows[1].1.style.bg,
            Some(Color::Rgb(20, 80, 20)),
            "差异行右侧绿底"
        );
    }

    #[test]
    fn gitdiff_renders_both_views() {
        let hunks = parse_unified(SAMPLE);
        let (rows, starts) = render_unified(&hunks, true, 40);
        assert_eq!(starts, vec![0]);
        assert!(rows.iter().any(|r| r.starts_with('-')));
        let data = DiffFileData {
            path: "f.txt".to_owned(),
            hunks: hunks.clone(),
            old_full: vec!["keep".into(), "old".into(), "last".into()],
            new_full: vec!["keep".into(), "new".into(), "last".into()],
            staged: false,
        };
        let (side, _) = render_side_by_side(&data, false, false, 80);
        assert!(
            side.iter()
                .any(|(l, r)| l.starts_with('-') && r.starts_with('+'))
        );
        let (full, _) = render_side_by_side(&data, true, false, 80);
        assert!(full.len() >= 3);
    }

    #[test]
    fn audit_styled_diff_keeps_markers_and_full_highlight() {
        // H1/H2：语法高亮不得吞掉 +/- 标记，`=` 完整视图也要着色。
        let hunks = parse_unified(SAMPLE);
        let (rows, _) = render_unified_styled(&hunks, "f.txt", false, 40);
        let texts: Vec<String> = rows
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect();
        assert!(
            texts.iter().any(|t| t.starts_with('-')),
            "保留 -: {texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.starts_with('+')),
            "保留 +: {texts:?}"
        );

        let long = DiffHunk {
            header: "@@ -1 +1 @@".to_owned(),
            lines: vec![DiffLine {
                kind: DiffLineKind::Add,
                text: "abcdefghijklmnopqrstuvwxyz".to_owned(),
                old_no: None,
                new_no: Some(1),
            }],
        };
        let (wrapped, _) = render_unified_styled(&[long], "f.rs", true, 8);
        assert!(wrapped.len() > 1, "软换行应产生多行: {}", wrapped.len());
        assert!(
            wrapped
                .iter()
                .all(|l| { l.spans.first().is_some_and(|s| !s.content.is_empty()) })
        );

        let data = DiffFileData {
            path: "f.rs".to_owned(),
            hunks,
            old_full: vec!["fn old() {}".to_owned()],
            new_full: vec!["fn new() {}".to_owned()],
            staged: false,
        };
        let (full, _) = render_side_by_side_styled(&data, true, false, 80);
        assert!(
            full[0].0.spans.iter().any(|s| s.style.fg.is_some()),
            "完整视图应着色"
        );
    }
}
