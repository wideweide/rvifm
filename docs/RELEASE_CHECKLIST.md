# RELEASE_CHECKLIST — 发布前人工验证清单

> **规则（REL-1，2026-11-22 起）**：GitHub Release 自动发布（tag `v*` → CI 全绿 →
> 四归档 + SHA256SUMS）；**crates.io publish 为人工验证门后动作**——本清单对应
> 版本勾选完成是唯一放行条件（crates.io 版本号不可回收，GitHub 产物可逆）。
> 本文件随版本更新（版本号/日期/代码事实）；历史入口 ACCEPTANCE §16（v0.6.0）
> 与 §35-A（0.11.0）已收编至此。
>
> **v1.0.0 特别说明**：crates.io 1.0.0 已于 2026-11-22 发布（发布前完成
> CHANGELOG/CI/四归档门禁）。本清单对 1.0.0 为**发布后全量实测验收**（0.9–1.0
> 功能从未人工实测过）；自下一版本起转为发布前门禁。

---

## 版本：v1.0.0（2026-11-22）

- 适用产物：GitHub Release `v1.0.0` 四归档（windows-x64 / linux-x64-gnu /
  linux-x64-musl / linux-aarch64-gnu）或 `cargo install rvifm@1.0.0`。
- 门禁必做组：**A / B / C / D / E / H**（TUI 核心 + CLI 管道，单终端可串行跑完）。
- 选做组：F / G / I / J / K（环境依赖——远端、tmux、提权、特殊终端，有条件则验）。
- 工作流：单终端串行执行；失败项在结论段记录现象 → `docs/NOTES.md` 走 FIX 登记
  → 修复重跑失败项；全部通过即视为放行 crates.io。

## 环境准备（一次性）

1. 解包对应平台归档至工作目录（或 `cargo install rvifm --version 1.0.0`）。
2. 建测试目录 `rvifm-test/`，内含：
   - 3 个文本文件（其一 >100 行）、1 张 PNG、1 张损坏图片（截断的 PNG 即可）、
     1 个 DOCX、1 个 CSV、1 个含文本层 PDF、1 个二进制文件（`head -c 4096
     /dev/urandom > blob.bin`）、1 个大图（> 400 万像素，可选）；
   - `demo.zip` / `demo.tar.gz` / `demo.7z`（各含子目录与文件）、1 个 `.docx`
     改名副本（非 zip 内容，验损坏拒绝）；
   - 子目录 `sub/`（内含 2 个文件）。
3. 真实终端（Windows Terminal / foot / Alacritty / Kitty 任一），进入
   `rvifm-test/`。

## A. 核心回归

- [ ] **A1** `rvifm --version` 输出 `rvifm 1.0.0`；`rv --version` 正常退出；
      `--help` 列出子命令。
- [ ] **A2** 启动 TUI：默认 `gruvbox-dark` 主题、顶部标签栏、双栏列表、底部
      状态栏；`q` 干净退出无残留；再次启动恢复上会话 tabs 与光标。
- [ ] **A3** 导航：`j/k`（尾→首环绕）、`l/Enter` 进 `sub/`、`h` 返回且光标
      定位在原子目录、`gg/G`、`Ctrl-u/d` 半页、`3j` 计数移动、`↑↓←→` 等价
      k/j/h/l。
- [ ] **A4** 搜索：`/txt` 实时跳转+高亮+计数；`n/N` 环绕；Esc 全清。
      `=*.txt` 过滤（状态栏 `[filter: …]`）；Esc 清除。
- [ ] **A5** 选择与标记：`v`/`V` 行可视 + `d`/`y` 对区域生效；`t` 标记两个
      文件（状态栏计数）；`:tag copy sub` 批量复制后标记清空。
- [ ] **A6** 操作与撤销闭环：`yy` → `sub/` 内 `p` 复制 → `u` 撤销消失；
      `dd` → 确认 → 进暂存 → `u` 恢复；`P` 移动语义；跨 tab 撤销正确定位；
      `DD` 强确认提示。
- [ ] **A7** 重命名族：`cw` 批量（`$EDITOR`）；`I/cc/A` 单文件就地编辑；
      `:rename s/old/new/` 预览 + `y` 应用整批，`u` 逐项可逆。
- [ ] **A8** 排序：`S` 对话框切换字段/方向；`:sort name,size desc` 多字段；
      数字文件名自然排序（`a2` 在 `a10` 前）。
- [ ] **A9** 对比：左右 pane 各放一个目录，`:compare`（按名）与
      `:compare bycontents`；结果虚拟目录 `dp`/`do` 同步 + `u`。
- [ ] **A10** 归档只读：`demo.zip`/`demo.tar.gz`/`demo.7z` `l` 进入虚拟目录
      （面包屑 `zip:/…`），`dd` 拒绝提示不 panic，`h` 回物理目录；
      `.docx` 直接 `l` 可浏览（zip 容器识别）。
- [ ] **A11** 任务视图：`:archive` / `:tag copy` 大目录后台化；`:jobs` 列表
      （`p` 暂停恢复 / Enter 详情 / `x` 取消）。
- [ ] **A12** git 列（在任一 git 仓库内）：状态列着色、状态栏分支、`gi` 探测
      gitui/lazygit（装有任一即验）。

## B. 双栏与标签（M41 语义）

- [ ] **B1** `Tab` 切焦点（非焦点行 DIM）；左右 pane 独立浏览目录与光标。
- [ ] **B2** 右 pane 焦点：`j/k/gg/G/l/h` 生效；`:` 进入命令模式，命令作用于
      右 pane 目录（如 `:mkdir r-d` 建在右 pane 路径下）；`gt`/`gT`/`2gt` 切
      标签与左 pane 一致。
- [ ] **B3** 右 pane 边界：`dd`/`D`/`c` 仍拒绝（提示），不崩溃。
- [ ] **B4** 钉底：右 pane 光标在视口末行按 `k`，列表不动、高亮上移；鼠标
      点击右 pane 条目 → 焦点+高亮跟随；已聚焦行再击 = 打开；滚轮作用于
      焦点 pane；右键菜单仍限左 pane。
- [ ] **B5** `w` 对侧预览：焦点左 → 查看器占右 pane；焦点右 → 查看器占左
      pane 且右 pane 保持列表；再按 `w` 恢复。
- [ ] **B6** `Space` 切换活动窗格；`:tabname <名>` 命名标签显示于标签栏；
      标签溢出时 `‹›` 滚动窗口。

## C. 搜索结果与查看器（M17/M38/M39）

- [ ] **C1** `:find <词>` 递归结果虚拟目录（上限 1 万提示）；`l` 定位原文件
      并光标跟随；Esc 退出；结果上 `yy`/`dd` 作用于真实路径（非虚拟路径）。
- [ ] **C2** `:find` 过滤器（`--k=v` 与 `--k v` 双形态）：`--size +1k` /
      `--size -1k`（上下限）、`--mtime -7`（7 天内）或 `--mtime +7`（更旧）、
      `--type f|d`、`--depth 1` 任一组合生效；非法值（如 `--size +5x`）可读
      报错；模式后多余位置参数报错。
- [ ] **C3** `:grep <正则>`：`路径:行号:内容` 条目；二进制跳过；`w` 打开
      查看器并定位命中行。
- [ ] **C4** 查看器：行号、j/k 滚动、`/` 搜索 + n/N、q/Esc 退出状态还原。
- [ ] **C5** 二进制 `blob.bin` 按 `w`：hex 视图（偏移 + ASCII 侧栏）；
      `:goto 0x100` 行对齐跳转；`:goto 256` 等价。
- [ ] **C6** CSV 预览列对齐（TSV 同源）；PDF 查看器 `[`/`]` 翻页；
      图片信息面板含尺寸摘要。
- [ ] **C7** 非 zip 内容的伪 docx：`l` 可读拒绝（魔数校验），不 panic。

## D. 文件运维与传输（M36/M37）

- [ ] **D1** `:chmod 0600 <文件>`（Unix；符号模式 `u+x` 同验）。
- [ ] **D2** `:extract demo.zip`：解压 job 化、zip-slip 防护（构造绝对路径
      成员条目可选验）、`u` 逐成员可逆。
- [ ] **D3** `:sync <左目录> <右目录>`：缺省 dry-run 列差异；`--apply` 执行；
      `--delete` 需显式。
- [ ] **D4** `:du`：top-5 占用视图，可取消；`:dup`：同内容分组报告；
      `:checksum <文件>`：md5 sidecar 生成与校验。
- [ ] **D5** 大文件跨 pane 复制（>100MB）：进度在 `:jobs` 可见、UI 不卡死；
      入队显示 `:queue`。
- [ ] **D6** 冲突策略 `[transfer] conflict_policy`（默认 `ask` 交互询问）：
      改 `"skip"` 同名目标不覆盖；改 `"overwrite"` 覆盖；恢复默认后同名粘贴
      出询问。
- [ ] **D7** 目录递归复制（含子目录+权限保留）；撤销恢复完整目录。
- [ ] **D8** `:archive zip out.zip`（可带 `checksum=sha256`）：后台完成入撤销
      栈，`u` 删除归档；`:archive 7z out.7z` 同验；`demo.7z`/`demo.zip` 用
      `:extract` 往返校验。

## E. frecency / marks / 复制序列

- [ ] **E1** `o` 合一选择器（图标全路径行、✦keyword、底部 `/query` 回显）；
      `:j`/`:J` 仅目录/仅文件；`:recent` 近期视角；选择器纯只读（不写库）。
- [ ] **E2** `m{a}` 标记当前条目 → 切走后 `'{a}` 跳回；`:bmark kw`/`:bmarks`
      /`:bmgo kw`/`:delbmarks` 族；`:keyword` 兼容入口。
- [ ] **E3** `yn`/`yf`/`yc` 复制名称/全路径/内容（OSC52 需终端支持）。
- [ ] **E4** 选择器内 `d` 删除条目 → `u` 撤销。
- [ ] **E5** `:frecency stats`（CLI 或 TUI 内）输出计数；数据目录 redb 正常。

## F. 主题与预览（M34/M39，有图形环境则验）

- [ ] **F1** `:theme` 无参列出（含内置 snowwhite）；`:theme snowwhite` 浅色 +
      图标 Light 联动；`:theme dracula`；`:theme gruvbox-dark` 回默认。
- [ ] **F2** 图片预览：小图原尺寸居中不放大；大图 fit-width；超高显示顶部。
- [ ] **F3** PDF（无文本语料）首页图像；`[preview] pdf_image_pages` 分流；
      有文本语料走文本提取。
- [ ] **F4** DOCX/XLSX/PPTX 文本提取（零外部工具）；损坏 Office 可读提示。
- [ ] **F5** 二进制/非 UTF-8 预览 → 信息面板（魔数类型/大小/修改时间）。

## G. CLI 与管道（单终端即可全验）

- [ ] **G1** 交互 pick（stdout 为 TTY）：
      `printf 'a.txt\nb.txt' | rvifm pick` → 交互选择器列出这两个候选；
      `--query txt` 预过滤；`--multi` 多选输出多行。
- [ ] **G2** filter pick（stdout 重定向）：
      `printf 'a.txt\nb.txt' | rvifm pick > /tmp/p.out`（stdout 非 TTY）→
      `/tmp/p.out` 含 fzf filter 语义命中行；`--print0` NUL 分隔；
      `--select-1` 唯一命中直出；`--exit-0` 空命中退出 0；`--no-sort` 保序。
- [ ] **G3** `rvifm pick --topic dir` 与 `--topic file` 对称评分；
      `--preview` 接同源预览窗格（≥60 列双栏，窄端降级单栏）。
- [ ] **G4** `rvifm record --topic dir <目录>`（真实 cd 后）；随后
      `rvifm pick` 该目录排名上升——**record 是唯一写入口，pick 全程只读**
      （可 `ls ~/.local/share/rvifm/` 观察无 pending 残留）。
- [ ] **G5** `rvifm init bash`（及 zsh/fish/powershell）输出 hook 脚本，
      只输出不写 rc 文件。
- [ ] **G6** `rvifm maintenance compact` 正常退出；`:frecency export
      /tmp/f.json` 全量导出；`:frecency prune --before 2026-01-01` 裁剪；
      `:frecency purge confirm` 需确认门。
- [ ] **G7** `rv` 短别名与 `rvifm` 行为一致（`:!`、`s` shell、`--askpass`）。

## H. 平台特定（按当前 OS 选测）

### Unix（Linux/macOS）

- [ ] **H1** 提权链：对 root 属主只读目录执行 `dd` → EACCES → AskPass 输入 →
      `sudo -A` 重试成功；Esc 取消后 TUI 状态不变、无半成品。
- [ ] **H2** `:link` 对目录/文件创建符号链接；`u` 可逆；断链可读。
- [ ] **H3** `:chmod` 权限变更后 `ls -l` 核验；递归到子目录。

### Windows

- [ ] **H4** UAC：对受保护目录 `dd` → UAC 弹窗；**取消**后状态不变；
      确认后 cmd.exe 承载执行成功（主进程未提权）。
- [ ] **H5** `:link` 对目录创建 **junction** 回退；`u` 可逆。
- [ ] **H6** 数据目录位于 `%LOCALAPPDATA%\rvifm\`；`remotes.toml`/session
      可被读取。

## I. 特殊环境（选做，有条件则验）

- [ ] **I1** tmux 3.3+（`allow-passthrough on`）：图片 passthrough 可见；
      `:!cmd` 新窗口执行、退出后 TUI 状态一致。
- [ ] **I2** 无图形协议终端：halfblocks 兜底出块；`NO_COLOR=1` 启动降级
      无 panic；Nerd Font 缺失图标降级纯文本。
- [ ] **I3** 损坏图片：预览可读提示不 panic；大图（>400 万像素）不 OOM。
- [ ] **I4** 损坏/加密归档：可读错误（区分"损坏 vs 加密 vs 格式不支持"）。
- [ ] **I5** 窄终端（<60 列）：`rvifm pick` 单栏降级；TUI 布局最小可用。

## J. 远端 SFTP（选做，需可达 sshd）

- [ ] **J1** `:remote <名>` 进入；`~/.ssh/config` Host 与 `remotes.toml`
      覆盖；浏览/排序/搜索复用本地语义。
- [ ] **J2** 指纹门：首连默认 `strict_host_key = ask` → FingerprintConfirm
      覆盖层 y/n；确认后写入 known_hosts；变造指纹 → 拒连含指引；
      `accept`/`no` 语义按配置。
- [ ] **J3** 远端写：`dd`/`p`/`P`/`:mkdir`/`:mk`；断连自动重连一次；
      传输中断 `.part` 现场保留（重连续传可选验）；失败无半成品。
- [ ] **J4** 跨 VFS：本地 `yy` → 远端 `p`（及反向）；ProxyJump/ssh-agent
      （环境有则验）。

## K. 结论（放行证据）

- **操作系统 / 终端**：（如 Arch + foot 1.20）
- **产物来源**：（GitHub 归档 / cargo install）
- **验证人 / 日期**：
- **门禁必做组（A/B/C/D/E/H/G）完成度**：__ / __ 通过，未通过项：
- **选做组完成情况**：
- **结论**：
  - [ ] 通过——放行 crates.io publish（执行 `cargo publish --locked`，发布后
        API 验证版本号）；
  - [ ] 存在阻塞缺陷——不放行，现象记录于 NOTES + FIX 登记，修复后重跑
        失败项。
