# CHANGELOG

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)；
版本号遵循 [SemVer](https://semver.org/lang/zh-CN/)。

## [0.11.1] — 2026-11-21

### 新增
- harness 一致性检查脚本 `scripts/harness_check.py`（七条规则，CI required check）。
- `CHANGELOG.md` 本文件（0.8.0 → 0.11.0 用户可见变化回填）。
- REVIEW 评估报告归档制（`docs/reviews/REVIEW-<日期>.md`）。

### 修复
- 文档状态漂移六处（AGENTS §10 过期任务指针、SPEC 7z/rar 能力三处、STATE/TASKS/PLAN/README 状态句矛盾）。
- 依赖卫生：移除零引用 `tokio-stream`；清理测试调试输出。
- 文档测试计数与实际代码一致（437）。

## [0.11.0] — 2026-11-21

### 新增
- 默认主题 gruvbox-dark；捆绑 snowwhite 主题（`:theme` 直接可用，39 + 捆绑内置主题）。
- 右 pane `/` 搜索（焦点 pane 感知，Esc 恢复）。
- 对侧预览统一：右焦点 `:view`/预览不再错位（preview_path 焦点修复）。
- frecency 选择器视觉回显（标题、全路径、底部 query）。
- `.jar` 归档虚拟浏览。
- 二进制文件信息面板（1KB 有界读取 + 10 类魔数，不执行文件）。
- `docs/GUIDE.md` 用户手册（快速上手/键位/命令速查）。
- 图片/PDF 预览尺寸适配（小图居中不放大、超宽 fit-width、超高顶部切片）。

### 平台
- Windows CI 测试与 clippy 全绿（本版本起 cfg(unix) 测试隔离纪律固化为 CI 口径）。

## [0.10.0] — 2026-11-21（GitHub Release；crates.io 跳过）

### 新增
- `:link` 符号链接创建（Windows 目录联接回退；`u` 可撤销）。
- `:rename` 结构化批量重命名（`s/old/new/[i]`、`prefix=`、`suffix=`、`seq=start,step,width`；预览结果视图 y=应用/Esc=取消）。
- `:jobs` 任务视图增强：`p` 暂停/恢复、Enter 详情（进度/速率/时长）、状态列。
- `:compare bycontents`（SHA-256 内容判定，单文件 64MB 护栏）；对比结果视图 `dp`/`do` 同步（可恢复）。
- frecency cmd topic：`:` 命令历史跨会话 frecency 排序、Tab 补全加权、`:cmdmark` 命令书签。
- SFTP 连接类错误自动重连一次；传输失败保留 `.part` 现场。
- 会话恢复归档/远端标签（失败跳过 + 状态栏提示）。
- `[visual]`/`[view]` 键位分层 TOML（进入模式时用户绑定生效）。

### 修复
- 符号链接目录判定（DirEntry lstat 误判，M27-1 残留缺陷）。
- 归档会话路径 Windows 盘符冒号切分。

### 变更
- frecency 清理策略固化为"仅容量裁剪，无分数下限"（对齐同级项目不变量）。
- `:compare` 结果视图 `compare` 命令可重执行（结果替换）。

## [0.9.0] — 跳过发布（2026-11-21 用户决策；变更并入 0.10.0 说明）

- M24–M27 / AUDIT 18 项 / M25-2 marks / M26 归档创建扩展 / MISC-1 rv 别名的变更
  随 0.10.0 发布面世；版本序列 0.8.0 → 0.10.0 → 0.11.0。

## [0.8.0] — 2026-11-19

- SFTP 远端浏览/写/安全补全（M19–M22）、frecency 对齐增强与 Pane 模式（M23）、
  UI/UX 打磨（M21）、任务视图/对比/命令历史/补全（M24）。
