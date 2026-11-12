# rvifm shell hooks —— 在 `cd` 真正成功后调用 `record`

> 原则（SPEC §4.6）：**record 是唯一写入入口**，且只在 shell 真正 `cd`
> 成功（或文件管理器真正跳转/打开成功）后调用。失败路径不调用——
> 避免"意图访问"污染评分。

## Bash

```bash
# ~/.bashrc
rvifm_cd() {
    # 先确认目标存在且是目录，失败不记录
    command cd "$1" || return 1
    command rvifm record --topic dir "$PWD" >/dev/null 2>&1 &
}
alias cd='rvifm_cd'
```

说明：
- `command cd` 绕过 alias 自身，避免无限递归。
- `record` 放后台（`&`）且丢弃输出，不阻塞提示符。
- 目标不存在时 `cd` 失败直接返回，不会调用 record。

## Fish

```fish
# ~/.config/fish/functions/rvifm_cd.fish
function rvifm_cd --wraps=cd
    if command cd $argv
        command rvifm record --topic dir $PWD >/dev/null 2>&1 &
    end
end
alias cd=rvifm_cd
```

## 验证要点（对应 ACCEPTANCE §10）

- `cd` 到不存在的目录：`record` 不被调用（`events` 条数不变）。
- `cd` 成功：`events` 条数 +1、`global_tick` +1。
- `record` 后台执行不阻塞提示符（连续 `cd` 数十次无卡顿）。
- 杀掉后台 `record` 进程不影响 shell；残留的 `events.pending`
  在下次 `pick`/TUI 启动时自动合并。
