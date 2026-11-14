// M13（vifm 对齐增强）：harness 文档批次——SPEC 新节 + NOTES 决策 +
// TASKS 任务 + ACCEPTANCE §16 人工项。
const fs = require('fs');
function patch(file, pairs) {
  let s = fs.readFileSync(file, 'utf8');
  for (const [oldStr, newStr] of pairs) {
    if (!s.includes(oldStr)) {
      console.log('MISS in ' + file + ': <<<' + oldStr.slice(0, 100) + '>>>');
      process.exit(1);
    }
    s = s.replace(oldStr, newStr);
  }
  fs.writeFileSync(file, s, 'utf8');
  console.log(file, 'ok');
}

// 1) SPEC：新节（M13 vifm 对齐增强，第一/二档定稿）
let spec = fs.readFileSync('docs/SPEC.md', 'utf8');
const specAnchor = '#### TUI 跳转选择器（2026-11-14 增补）';
if (!spec.includes(specAnchor)) { console.log('SPEC ANCHOR MISSING'); process.exit(1); }
const specAdd = [
  '#### vifm 对齐增强（M13，2026-11-14 增补；依据：vifm 官方文档 + GitHub issues）',
  '',
  '**第一批（低成本）**',
  '- f/F/;/,：首字母快速搜索（按文件名首字符正向/反向移动，官方定义环绕列表首尾；;/, 为重复导航）。',
  '- :select / :unselect：按 pattern 批量选择/取消（如 :select *.md），形态对齐官方 [range]select {pattern}；t 单文件切换语义不变（官方 t = select or unselect (tag)）。',
  '- DD：永久删除——跳过应用内暂存，不可逆强提示（官方：like dd but always omits trash）。',
  '- j/k 列表环绕（wrap-around）：光标到列表尾/首后继续移动则环绕（官方 issue #874）。',
  '- Ctrl-G：文件详情对话框（名称/大小/权限/时间/路径等，比状态栏更全）。',
  '',
  '**第二批（中成本）**',
  '- cw 批量重命名：多选时生成名单文件并交 $EDITOR 编辑，应用后每文件可 u 逆（官方：cw 多选时 behaves as :rename without arguments；issues #515/#682）。',
  '- :tree 树视图：当前目录为根的树形浏览，与文件系统状态和过滤器联动（官方：automatically kept in sync）。',
  '- 后台作业 + :jobs：重操作（复制/移动）后台执行不阻塞 UI（官方：background command by adding " &"；issues #481 最高呼声/#210/#336）。',
  '- view mode（w / :view）：切换**非活动窗格（右预览窗格）**为当前光标文件的完整内容视图（**非全屏**：保持双栏布局；j/k 在列表移动时视图跟随光标文件重置）；再次 w 或 :view 关闭并恢复摘要预览。用户澄清：vifm 默认 w 映射 :view，显示于对侧窗格、受窗格布局限制、光标保持在列表窗格。',
  '- 暂存区菜单（对齐官方 :lstrash）：列出应用内暂存文件并可逐项恢复（对 u 逆序撤销的补充，纯导航语义）。',
  '- 选择器与上述新命令均不触碰 frecency 时钟（只读红线不变）。',
  '',
  specAnchor,
].join('\n');
spec = spec.replace(specAnchor, specAdd);
fs.writeFileSync('docs/SPEC.md', spec, 'utf8');
console.log('docs/SPEC.md ok');

// 2) NOTES：决策与 :view 语义澄清
let notes = fs.readFileSync('docs/NOTES.md', 'utf8');
const nAnchor = '### 2026-11-14 — TUI 跳转选择器 o / :j / :J（双视角，用户确认形态）';
if (!notes.includes(nAnchor)) { console.log('NOTES ANCHOR MISSING'); process.exit(1); }
const notesAdd = [
  '### 2026-11-14 — M13 vifm 对齐增强立项（第一/二档）+ :view 语义澄清',
  '',
  '- **来源边界（用户指定）**：候选清单仅依据三来源——vifm GitHub 仓库、open issues（按评论/热度）、vifm 官方文档（vifm.info/docs.shtml）；博客/对比文/个人笔记视为可能过时或自定义配置，不采用。',
  '- **三条语义修正（用户核对官方文档确认）**：',
  '  1. vifm 默认（cpoptions 含 t flag）下 Ctrl-I/Tab/Space 均为**切换活动窗格**，非历史跳转——清单撤下 Ctrl-I 条目；',
  '  2. h 即返回父目录（cd ..），与目录历史无关——rvifm ReturnParent 语义已一致；',
  '  3. t = select or unselect (tag) 单文件（官方原文），rvifm t 已对齐；:select/:unselect 补的是**按 pattern 批量**形态。',
  '- **:view 语义澄清（用户）**：w 默认映射 :view；视图打开于**非活动窗格**（对侧预览窗格），非全屏、受窗格布局限制；光标保持在列表窗格，j/k 移动时预览跟随更新；再次 w/:view 关闭。rvifm 落地：预览窗格在摘要预览（现状）与完整文件视图间切换，保持双栏布局。',
  '- **立项范围**：第一批 f/F/;/,、:select/:unselect、DD、j/k wrap、Ctrl-G；第二批 cw 批量重命名、:tree、后台作业+:jobs、view mode、暂存区菜单。发布节奏 0.4.0。',
  '- **备选与放弃**：Ctrl-I 历史跳转（默认被 Tab 占用）；第三档 :compare/VCS/自定义视图与第四档（寄存器/OS 回收站/Lua）暂不立项（第四档需先解禁 SPEC 约束）。',
  '',
  nAnchor,
].join('\n');
notes = notes.replace(nAnchor, notesAdd);
fs.writeFileSync('docs/NOTES.md', notes, 'utf8');
console.log('docs/NOTES.md ok');

// 3) TASKS：M13 任务清单
let tasks = fs.readFileSync('docs/TASKS.md', 'utf8');
const tAnchor = '- [x] M8-13 发布 0.3.0（M8-12 后）：crates.io（API 验证 0.3.0）+ tag `v0.3.0` → GitHub Release（四 job 全绿，双归档已上传）';
if (!tasks.includes(tAnchor)) { console.log('TASKS ANCHOR MISSING'); process.exit(1); }
const tasksAdd = [
  tAnchor,
  '',
  '## M13 — vifm 对齐增强（2026-11-14 增补；第一/二档，见 SPEC M13 节）',
  '',
  '- [ ] M13-1 f/F/;/, 首字母快速搜索（环绕列表首尾，重复导航）',
  '- [ ] M13-2 :select / :unselect 按 pattern 批量选择/取消（t 单文件语义不变）',
  '- [ ] M13-3 DD 永久删除（跳过应用内暂存，不可逆强提示）',
  '- [ ] M13-4 j/k 列表环绕（wrap-around）',
  '- [ ] M13-5 Ctrl-G 文件详情对话框',
  '- [ ] M13-6 cw 批量重命名（多选 → $EDITOR 名单 → 应用，逐项可 u 逆）',
  '- [ ] M13-7 :tree 树视图（FS 状态与过滤器联动）',
  '- [ ] M13-8 后台作业 + :jobs（复制/移动后台执行不阻塞 UI）',
  '- [ ] M13-9 view mode（w/:view：预览窗格切换完整文件视图，跟随列表光标，非全屏）',
  '- [ ] M13-10 暂存区菜单（列出应用内暂存文件，逐项恢复）',
  '- [ ] M13-11 发布 0.4.0：crates.io + tag v0.4.0 → GitHub Release',
].join('\n');
tasks = tasks.replace(tAnchor, tasksAdd);
fs.writeFileSync('docs/TASKS.md', tasks, 'utf8');
console.log('docs/TASKS.md ok');

// 4) ACCEPTANCE §16：H3 节（M13 人工验证）
let acc = fs.readFileSync('docs/ACCEPTANCE.md', 'utf8');
const aAnchor = '### I. 文件打开';
if (!acc.includes(aAnchor)) { console.log('ACC ANCHOR MISSING'); process.exit(1); }
const accAdd = [
  '### H3. vifm 对齐增强（M13）',
  '',
  '- [ ] f 后按首字符（如 r）：光标跳到下一个 r 开头的文件；F 反向；;/, 重复上次搜索；到列表尾后环绕。',
  '- [ ] :select *.md 批量选中匹配文件（加粗+计数）；:unselect 清除；t 单文件切换不受影响。',
  '- [ ] DD 强提示确认后文件不进暂存、u 无法恢复；dd 行为不变（暂存可逆）。',
  '- [ ] 列表尾按 j 环绕到首行；首行按 k 环绕到尾行。',
  '- [ ] Ctrl-G 弹出当前文件详情对话框，Esc 关闭。',
  '- [ ] 多选后 cw：打开 $EDITOR 编辑名单，保存退出后文件名全部更新，u 逐项可逆。',
  '- [ ] :tree 切换树视图（含子目录展开），过滤器联动；再执行返回平铺。',
  '- [ ] 大目录 :copy 触发后台作业：UI 不阻塞，:jobs 查看进度。',
  '- [ ] w/:view：右窗格切换为当前文件完整内容视图（非全屏，双栏保持）；j/k 移动列表时视图跟随；再次 w 恢复摘要预览。',
  '- [ ] 暂存区菜单列出已删除文件，选定后恢复到原路径。',
  '',
  aAnchor,
].join('\n');
acc = acc.replace(aAnchor, accAdd);
fs.writeFileSync('docs/ACCEPTANCE.md', acc, 'utf8');
console.log('docs/ACCEPTANCE.md ok');
