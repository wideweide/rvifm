// M14（双栏模型与标签栏）harness 文档批次。
const fs = require('fs');
function patch(file, pairs) {
  let s = fs.readFileSync(file, 'utf8');
  for (const [oldStr, newStr] of pairs) {
    if (!s.includes(oldStr)) {
      console.log('MISS in ' + file + ': <<<' + oldStr.slice(0, 90) + '>>>');
      process.exit(1);
    }
    s = s.replace(oldStr, newStr);
  }
  fs.writeFileSync(file, s, 'utf8');
  console.log(file, 'ok');
}

// 1) SPEC：M1 SIMPLIFIED 行修正 + 新增 M14 节
let spec = fs.readFileSync('docs/SPEC.md', 'utf8');
patch('docs/SPEC.md', [
  [
    '- 双栏布局可先做单栏，但数据结构必须为双栏预留。',
    '- 双栏布局（M14 起为默认，对齐 vifm）：左/右两个 pane 各自独立导航；'
      + 'M11–M13 时期的“单栏 + 常驻右预览窗格”模型由 M14 改造取代。',
  ],
  [
    '#### vifm 对齐增强（M13，2026-11-14 增补；依据：vifm 官方文档 + GitHub issues）',
    '#### M14 — 双栏模型与标签栏（2026-11-15 增补；用户实测反馈 + vifm 官方模型）\n\n'
      + '**背景**：v0.4.0 实测（Linux/CachyOS）发现两处与 vifm 模型不符——'
      + '①rvifm 右窗格为常驻预览，vifm 默认左右两个 pane 均为**目录列表**（可各自导航），'
      + 'w/:view 才把非活动 pane 切换为预览；②多标签功能已实现但无标签栏 UI，切换无可见反馈。\n\n'
      + '**MUST**\n'
      + '- 双 pane：左（活动）+ 右（非活动）各自持有独立 `Location`/条目列表/光标/滚动，均可导航'
      + '（j/k/gg/G/l/h/Enter）；`Tab` 键切换焦点，导航与文件操作作用于焦点 pane。\n'
      + '- `w` / `:view`：右 pane 在“目录列表”与“活动 pane 光标文件的预览视图”之间切换'
      + '（预览内容跟随活动 pane 光标移动重置）；再按恢复。\n'
      + '- 标签栏 UI：显示每个 tab 的索引与目录名、当前 tab 高亮；`:tabnew`/`gt`/`gT` 切换有可见反馈。\n'
      + '- 每个标签内含完整双 pane 状态；多标签命令语义不变。\n\n'
      + '**SIMPLIFIED**\n'
      + '- 右 pane 仅导航 + 文件操作作用于焦点 pane；搜索（/）、可视选择（v）、标记（t）仅左 pane'
      + '（右 pane 触发时提示）。树视图/跳转选择器/提权交互仅左 pane。\n'
      + '- 右 pane 无独立撤销语义：撤销恢复后双 pane 均刷新。\n\n'
      + '**OUT_OF_SCOPE**\n'
      + '- 右 pane 独立搜索/可视选择/标签；超过两个 pane（vifm issue #1062）。\n\n'
      + '#### vifm 对齐增强（M13，2026-11-14 增补；依据：vifm 官方文档 + GitHub issues）',
  ],
]);

// 2) NOTES：决策
let notes = fs.readFileSync('docs/NOTES.md', 'utf8');
const nAnchor = '### 2026-11-14 — M13 vifm 对齐增强立项（第一/二档）+ :view 语义澄清';
if (!notes.includes(nAnchor)) { console.log('NOTES ANCHOR MISSING'); process.exit(1); }
notes = notes.replace(nAnchor,
  '### 2026-11-15 — M14 双栏模型与标签栏（用户实测反馈驱动）\n\n'
  + '- **背景**：v0.4.0 在 Linux（CachyOS）实测发现：①“启动进入 TUI（列表 + 右侧预览窗格）”'
  + '与 vifm 经典模型不符——vifm 默认左右两个 pane 都是目录列表，w/:view 才把非活动 pane 切成预览；'
  + '②多标签功能存在（exec/测试覆盖）但无标签栏 UI，`:tabnew` 落点为同目录导致视觉零变化，用户无从验证。\n'
  + '- **根源归属**：①SPEC M1 SIMPLIFIED“可先做单栏”被 M1/M11 沿用且 M13 :view 立项时未把双栏模型'
  + '作为前置一起修正（需求文档缺陷）；②SPEC §4.2 未定义标签栏 UI，ACCEPTANCE §3.C 无可见性验收'
  + '（验收文档缺陷）。§16 手册标题滞留 v0.2.0（文档维护缺陷）。\n'
  + '- **结论（用户确认“按此立项推进”）**：M14-1 右 pane 改为独立目录列表（独立 Location/光标，可导航，'
  + 'Tab 键切焦点，写操作作用于焦点 pane）+ `w`/:view 切换右 pane 预览视图；M14-2 标签栏 UI；'
  + 'M14-3 ACCEPTANCE §16 全面修订（0.5.0 语义）；M14-4 发布 0.5.0。\n'
  + '- **SIMPLIFIED 裁剪（NOTES 记录）**：右 pane 仅导航（搜索/可视选择/标记/树/选择器/提权仅左 pane，'
  + '触发时提示）；右 pane 无独立撤销语义（恢复后双 pane 刷新）；不实现第三 pane（issue #1062 不立项）。\n'
  + '- **影响文件**：docs/SPEC.md M1 SIMPLIFIED/M14 节、docs/TASKS.md M14、docs/ACCEPTANCE.md §16、'
  + 'src/state.rs（Pane 结构）、src/tab.rs、src/app.rs（焦点分发/渲染）。\n\n'
  + nAnchor);
fs.writeFileSync('docs/NOTES.md', notes, 'utf8');
console.log('docs/NOTES.md ok');

// 3) TASKS：M14 立项
let tasks = fs.readFileSync('docs/TASKS.md', 'utf8');
const tAnchor = '- [x] M13-11 发布 0.4.0：crates.io（API 验证 0.4.0）+ tag `v0.4.0` → GitHub Release（四 job 全绿，双归档已上传）';
if (!tasks.includes(tAnchor)) { console.log('TASKS ANCHOR MISSING'); process.exit(1); }
tasks = tasks.replace(tAnchor, tAnchor + '\n'
  + '\n## M14 — 双栏模型与标签栏（2026-11-15 增补；用户实测反馈，见 SPEC M14 节）\n\n'
  + '- [ ] M14-1 双 pane 模型：右 pane 独立目录列表（独立 Location/光标/滚动，可导航）+ `Tab` 键焦点切换'
  + ' + 焦点 pane 承接导航与文件操作 + `w`/`:view` 切换右 pane 预览视图（替代常驻预览窗格）\n'
  + '- [ ] M14-2 标签栏 UI（tab 索引 + 目录名 + 当前高亮；`:tabnew`/`gt` 可见反馈）\n'
  + '- [ ] M14-3 ACCEPTANCE §16 修订（0.5.0 语义：双栏描述/标签栏可见性验收）\n'
  + '- [ ] M14-4 发布 0.5.0：crates.io + tag `v0.5.0` → GitHub Release');
fs.writeFileSync('docs/TASKS.md', tasks, 'utf8');
console.log('docs/TASKS.md ok');

// 4) ACCEPTANCE §16 修订（标题 + A 节 + C 节）
let acc = fs.readFileSync('docs/ACCEPTANCE.md', 'utf8');
patch('docs/ACCEPTANCE.md', [
  [
    '## 16. 人工验证清单（v0.2.0 手册，测试人员逐项勾选）',
    '## 16. 人工验证清单（v0.5.0 手册，测试人员逐项勾选）',
  ],
  [
    '- [ ] `rvifm` 启动进入 TUI（列表 + 右侧预览窗格 + 底部状态栏），\n'
      + '      `q` 退出干净，无乱码残留、无 panic。',
    '- [ ] `rvifm` 启动进入 TUI（**双栏：左右两个目录列表** + 底部状态栏），\n'
      + '      `q` 退出干净，无乱码残留、无 panic。',
  ],
  [
    '### C. 多标签\n\n'
      + '- [ ] `:tabnew` 新建标签；`gt`/`gT` 切换；`2 gt` 跳第二个；`:tabclose` 关闭。\n'
      + '- [ ] 两个 tab 各自浏览不同目录，切回时目录/光标/搜索词保持独立。  -- 这条无从测试',
    '### C. 多标签\n\n'
      + '- [ ] **标签栏可见**：顶部显示每个 tab 的索引与目录名，当前 tab 高亮。\n'
      + '- [ ] `:tabnew` 新建标签（标签栏计数 +1）；`gt`/`gT` 切换（高亮移动）；`2 gt` 跳第二个；`:tabclose` 关闭。\n'
      + '- [ ] 两个 tab 各自浏览不同目录，切回时目录/光标/搜索词保持独立。\n'
      + '- [ ] 双 pane：`Tab` 键切换焦点（左/右 pane），焦点 pane 承接导航与文件操作；\n'
      + '      `w`/`:view` 把右 pane 切换为活动 pane 光标文件的预览视图，再按恢复目录列表。',
  ],
]);
