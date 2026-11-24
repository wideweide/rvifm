# rvifm 用户手册

> **定位**：本手册面向初次使用者，按主题分节、每节给最小示例。TUI 内的
> 速查请按 `:help`（帮助页随版本内嵌）；配置细节与项目背景见
> [README](../README.md)。rvifm 参考 vifm 的交互范式，非 fork、非行为级兼容。

## 1. 安装与启动

```bash
cargo install rvifm          # 同时安装 rvifm 与短别名 rv
rvifm                        # 启动（rv 为等价短别名）
rvifm --help                 # 命令行帮助
```

- 首次启动零配置可用：默认主题 `gruvbox-dark`（39 套内置 + 捆绑 `snowwhite`
  浅色主题可选，见 §11）。
- 建议终端：支持 Nerd Font 的终端可显示文件图标；不支持的终端自动降级为
  纯文本，功能不受影响。
- 退出会话（tabs/位置）默认自动保存并在下次启动恢复（`[ui] session_restore`）。

## 2. 快速上手（5 分钟）

> **新手推荐**：直接输入 `:tutor` 进入导师模式——**13 个模块 100 项覆盖全部
> 键位与命令**（核心导航/文件操作/搜索/选择批量/marks/标签历史/视图排序/
> 归档/运维/命令行/剪贴板 git/远端）。面板显示期间练习键照常可用（透明放行）：
> **Ctrl+Y**=掌握、**Ctrl+N**=未掌握（错题本下次优先复习）、**Ctrl+Q**=退出。
> 进度保存在本地；`:tutor reset <模块>` 重置。它既是交互教程也是发布前自验
> 清单；跑完 `core_nav` 即完成本节入门。

### 2.1 浏览

```
j / k      光标下移/上移（首尾环绕）
l / Enter  进入目录或归档；普通文件交给系统默认程序打开
h          返回上级目录
gg / G     跳到顶部 / 末项
Ctrl-d / Ctrl-u   半页下翻 / 上翻
Tab        切换左右 pane 焦点（双栏均可导航与执行普通模式操作）
```

状态栏/面包屑中 HOME 下的路径缩写为 `~/…`（导航仍用绝对路径）；标签栏可鼠标
点击切换，分隔线由 `[ui] pane_separator` 控制（默认不显示）。`h` 返回父目录后
再 `l` 重返会还原该目录此前的光标位置。

### 2.2 第一次文件操作与撤销

```
yy         复制选中条目到 yank 缓冲区
p          粘贴（复制）到当前目录        P   粘贴（移动）
dd         删除（进应用内暂存，可恢复）  DD  永久删除（强提示，不可恢复）
u          撤销上一次操作（逆序、跨 tab）
cw         批量重命名（$EDITOR 编辑名单）
```

最小闭环示例：`j` 移到文件 → `yy` → `p` → 出现副本 → `u` → 副本消失。
删除同样 `u` 可逆（暂存恢复）；`DD` 与明确提示的不可逆操作除外。

### 2.3 归档（浏览与创建）

```
l          在 .zip/.jar/.tar*/.7z 上按 l 直接"进入"归档（只读虚拟目录）
h          在归档根返回归档所在物理目录
:archive zip backup    把当前标记集合（无标记 = 光标条目）打包为 backup.zip
:archive                无参打开交互对话框（格式/校验文件可选）
```

创建支持 zip / tar / tar.gz / **7z**（内建）与 rar / cab / lzh / tar.xz / tar.bz2 /
tar.zst（外部 CLI 降级，需 PATH 有对应工具）；完成后生成校验文件
（默认 md5，可 sha256/none）。`:archive 名.zip checksum=sha256` 可省格式名参，
从扩展名推断。`.rar` 不支持虚拟浏览（提示解压）。

`:extract`（无参）解压到**对侧 pane**，也可 `left`/`right`/`当前`/`对侧`/路径指定；
归档虚拟目录内 `yy` 标记成员后到物理目录 `p`/`P` 按成员解压，均可用 `u` 撤销。
归档文件的预览显示成员名列表。

### 2.4 frecency 跳转（高频目录/文件）

```
o          在焦点 pane 打开 frecency 搜索区（目录/文件合并，按使用频率排序）
:j / :J    同一焦点 pane 搜索区；分别只显示目录 / 文件
:recent    近期视角浮层（按最后访问时间倒序）
'{字符}    marks 跳转（先 m{字符} 标记；多字符 keyword 搜索高优先级匹配）
```

使用越多的目录/文件排名越靠前；在焦点 pane 输入筛选词，命中路径会高亮，
对侧 pane 预览当前候选；`l/Enter` 跳转，鼠标单击候选也会跳转。独立
`rvifm pick` 固定左侧搜索、右侧预览。数据持久化在 frecency 存储（redb），跨会话累积。

## 3. 浏览

- 双栏模型：左右是能力一致的独立目录列表，快捷键与目录/文件命令作用于焦点 pane；
  `Tab` 切换焦点，`w` 在右 pane 的列表/预览间切换（`:view` 同义）。
  `w` 开启的预览渲染在**非焦点 pane**：焦点在左预览在右，焦点在右预览在左。
  `w` 开启后焦点留在原 pane，`j/k` 移动光标时预览自动跟随；按 `Tab` 进入预览窗格（`j/k` 滚动、`/` 搜索），再按 `Tab` 返回。
  两栏均显示 `..`（根目录隐藏）；`h`/左方向键返回父目录，`l`/Enter 进入。普通模式快捷键和命令作用于焦点 pane，行为受当前路径类型能力约束。
  `:`（命令作用于焦点 pane）、`gt`/`gT`/`{N}gt`（切标签）、`gg`（跳顶部）与左栏一致；`o`、Visual/tag、`yy`/`p`/`P` 也可在右栏焦点下使用。
- `/` 搜索**作用于焦点 pane**：实时跳转到第一个匹配、匹配子串高亮、
  状态栏计数 `i/N`；`n`/`N` 下一个/上一个。
- `={glob}` 过滤（如 `=*.txt`），Esc 清除。
- `f/F {字符}` 按文件名首字符快速定位（`;` 同向重复、`,` 反向）。
- `..` 行在列表顶部（`l`/`Enter` 返回上级；根目录隐藏）。
- 鼠标：单击切换焦点至点击 pane 并聚焦条目（左右一致）、已聚焦行再击打开、
  双击直接打开、右键上下文菜单（左栏）、滚轮滚动焦点 pane 列表。

## 4. 文件操作

| 键 | 作用 | 可逆 |
|---|---|---|
| `yy` / `Nyy` | 复制到 yank 缓冲区（可视选择/标记优先） | — |
| `p` / `P` | 粘贴复制 / 粘贴移动 | `u` |
| `dd` | 删除（应用内暂存） | `u` 或 `:trash` 恢复 |
| `DD` | 永久删除（强提示） | 否 |
| `cw` | 批量重命名（$EDITOR） | `u`（整批） |
| `:rename s/旧/新/[i]`、`prefix=`/`suffix=`/`seq=` | 结构化批量重命名（结果视图 `y` 应用） | `u`（逐项） |
| `:copy` / `:move <目录>` | 复制/移动 | `u` |
| `:link <dest>` | 创建符号链接（dest 为目录时批量建链） | `u` |
| `:mkdir` / `:touch` / `:mk` | 创建目录/文件；`:mk` 创建并进入 | `u` |
| `y`/`yn`/`yf`/`yc` | 复制条目名/名称/全路径/内容（OSC52 优先） | — |

同名目标默认报错不覆盖；删除默认走可恢复路径（暂存区，`:trash` 菜单可
浏览恢复）。

## 5. 选择与批量

```
v / V      可视选择 / 行可视（互斥；Esc 退出）
t          标记/取消标记当前条目（tag）
:select {glob} / :unselect   按 pattern 批量标记/取消
:tag copy|move|delete <目录>  对标记集合批量操作（复制/移动后台执行，:jobs 可见）
```

## 6. 搜索与过滤

```
/{词}      焦点 pane 内搜索（incsearch + 高亮 + i/N）
n / N      下一个 / 上一个匹配
={glob}    过滤（Esc 清除）
f / F {字符}  首字符快速定位
:find {模式}  递归文件名查找（结果 = 虚拟目录；上限 1 万）
              过滤器：--size [+-=]N[bkmg]、--mtime [+-]N（天）、
              --type f|d、--depth N；命名搜索：:find save <名> /
              :find load <名>（仅本会话）
:grep {模式}  内容搜索（regex；条目 = 路径:行号: 内容；上限 1 万）
```

`:find`/`:grep` 的结果是虚拟目录：`j/k` 浏览、`l/Enter` 定位、
`Esc`/`h` 退出、预览与普通目录一致。远端（SFTP）视图内同样可用
（远端 `:find` 不支持过滤器与命名搜索）。

## 7. 归档

- **浏览**：`.zip` `.jar` `.tar` `.tar.gz` `.tgz` `.tar.bz2` `.tar.zst`
  `.tar.xz` `.7z` 直接当目录浏览（只读；归档内不进撤销栈）。
- **创建**：`:archive [zip|tar|tar.gz|7z] <名> [checksum=md5|sha256|none]`（内建四种）；
  rar/cab/lzh/tar.xz/tar.bz2/tar.zst 走外部 CLI（PATH 探测，缺失时优雅终止）。
- 大目录打包后台执行（`:jobs` 可见/可取消）；完成后生成校验文件。
- 归档成员读取有 64MB 上限、xz 有解压上限——超限给可读错误，不会 OOM。

## 8. 跳转（frecency / marks / 书签）

```
o / :j / :J             焦点 pane 内嵌 frecency 搜索（见 §2.4）
:recent                  近期条目选择浮层
m{字符}    标记当前条目（".." 行 = 当前目录）
'{字符}    跳到该 keyword 的最高排名条目
:bmark <tag...> / :bmarks / :bmgo <tag...> / :delbmarks
           书签命令族（与 marks 共用 frecency 数据；清除保留累积分数）
```

隐私：命令文本与访问记录会写入 frecency 存储；`[frecency] ignore_patterns`
可排除敏感路径与命令（见 README 配置说明）。

## 9. 视图与标签

```
:tabnew / :tabclose / gt / gT / {N}gt / :tabmove   多标签
:tabname <名>    命名当前标签（无参清除；仅本会话）；标签溢出自动滚动（‹ ›）
S                排序对话框；:sort 字段,... 多字段排序
:tree [depth]    树视图（只读快照）
:compare [byname|bycontents]   左右 pane 目录对比；dp/do 同步（只读边界内可逆）
:jobs            后台作业（x 取消、p 暂停/恢复、Enter 详情）
:theme [名]      切换主题（运行时即时生效）
:trash           暂存区菜单
:histprev / :histnext / :history   目录历史
:goto <偏移>     hex 查看器偏移跳转（0x 十六进制或十进制）
```

`w` 打开二进制文件（前 8KB 含 NUL 判定）时进入 **hex 完整视图**：
16 字节/行（偏移 + 十六进制 + ASCII 侧栏），内容分块读取（视口窗口 ≤1MB，
大文件不整读）；`:goto 0x100` 或 `:goto 256` 按 16 字节行对齐跳转。无编辑。

PDF 查看器：`[`/`]` 逐页翻页（任意页按需渲染；`[preview] pdf_image_pages>0`
或装有 pdftoppm/mutool 时生效，文本层足够且未开 `pdf_prefer_image` 时回退
文本预览）。CSV/TSV 文件预览为表格对齐形态；图片信息面板显示容器尺寸/格式
（PNG/JPEG/GIF/BMP 头解析，无 EXIF 全字段）。

布局选项：`[ui] layout = dual|single|preview-right|preview-bottom`（缺省 dual）。
`single` 单栏全宽（view 模式查看器占全宽）；`preview-right` 查看器恒在右半；
`preview-bottom` 查看器占下半全宽（列表上半双栏）。

## 10. 远端（SFTP）

```
:remote <名>     进入远端（~/.ssh/config 别名 + remotes.toml 覆盖）
:remote          列出全部远端
```

远端内浏览/搜索/文件操作与本地同键；连接失败自动重连一次、失败现场保留
`.part`。首连指纹确认：`strict_host_key = ask|accept|no`（缺省 **ask**——
覆盖层展示指纹，`y` 接受写入 known_hosts、`n`/Esc 拒绝中止且状态不变；
`accept` = 旧 TOFU 自动写入；`no` 等效 `known_hosts = false`）；指纹变更
仍默认拒连。连接增强：ProxyJump（`~/.ssh/config` 透传）、ssh-agent 认证
（`SSH_AUTH_SOCK`，unix）、keepalive 30s。

## 11. 配置

配置目录：Linux/macOS `~/.config/rvifm/`，Windows `%APPDATA%\rvifm\`。

| 文件 | 作用 |
|---|---|
| `config.toml` | 键位分层（`[normal]`/`[visual]`/`[view]` 的 prepend/append/replace）、opener 规则、`[ui]`（theme/pane_borders/attrs_column…）、`[preview]`、`[frecency]`、`[keymap]` |
| `themes/<名>.toml` | 用户主题（opaline TOML） |
| `remotes.toml` | SFTP 远端覆盖（在**数据目录**：Linux/macOS `~/.local/share/rvifm/`、Windows `%LOCALAPPDATA%\rvifm\`） |
| `session.toml` | 会话自动保存（可关） |

最小示例（键位 + 主题）：

```toml
[ui]
theme = "gruvbox-dark"      # 或 dracula / snowwhite / 用户主题名

[normal]
prepend_keymap = [
  { on = ["C-o"], run = "histprev" },
  { on = ["C-i"], run = "histnext" },
]
```

完整字段与示例见 README「配置」章节；主题可用名可用 `:theme` 无参列出。

## 12. 键位速查表

| 分类 | 键位 |
|---|---|
| 导航 | `j/k` `↑↓←→`（同 k/j/h/l）`l/Enter` `h` `gg/G` `Ctrl-d/u` `..` |
| 焦点/视图 | `Tab` `w`/`:view` `S` `:sort` `:tree` `:theme` `Ctrl-G` |
| 搜索 | `/` `n/N` `={glob}` `f/F{字符}` `:find` `:grep` |
| 选择/批量 | `v` `V` `t` `:select` `:unselect` `:tag` |
| 文件操作 | `yy` `p/P` `dd` `DD` `u` `cw` `:rename` `:copy` `:move` `:link` `:mkdir` `:touch` `:mk` `y/yn/yf/yc` |
| 跳转 | `o` `:j/:J` `:recent` `'{字符}` `m{字符}` `:bmark` 族 `:cmdmark` |
| 标签/历史 | `:tabnew` `gt/gT` `:tabmove` `:histprev` `:histnext` `:history` |
| 归档/对比/作业 | `:archive` `:compare` `dp/do` `:jobs` `:trash` |
| 远端 | `:remote` |
| 其他 | `s`（shell）`:!<cmd>` `gi` `:q` `:help` `Esc` |

## 13. 命令速查表

| 命令 | 作用 |
|---|---|
| `:cd` `:mkdir` `:touch` `:mk` | 目录/文件创建与切换 |
| `:rename` `:copy` `:move` `:link` | 重命名 / 复制 / 移动 / 建链 |
| `:archive` `:extract` `:compare` | 归档创建（支持 `名 checksum=…` 推断格式）/ 解压到对侧或指定侧 / 目录对比 |
| `:find` `:grep` | 递归搜索 / 内容搜索 |
| `:select` `:unselect` `:tag` | 批量选择与操作 |
| `:bmark` `:bmarks` `:bmgo` `:delbmarks` `:keyword` | 书签族 |
| `:tabnew` `:tabclose` `:tabmove` | 标签管理 |
| `:sort` `:tree` `:view` `:theme` | 视图控制 |
| `:remote` | SFTP |
| `:jobs` `:trash` `:file` `:history` | 任务 / 暂存区 / 详情 / 历史 |
| `:!` `s` `gi` | 外部命令 / shell / git TUI |
| `:q` `:help` | 退出 / 帮助 |

---

问题排查：预览不可用（图像）时确认终端图形协议与字体（§1）；PDF 图像预览
按"文本提取 → 栅格化（`pdf-visual` feature）/外部工具 → 提示"顺序回退；
提权仅作用于单次写操作（`sudo -A`，取消后状态不变）。
