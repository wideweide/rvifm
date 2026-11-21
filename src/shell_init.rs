//! M38-3：`rvifm init <shell>`——shell 集成初始化脚本（一行接入）。
//! `cd` 后 record（进入目录记录）、快捷键调 pick（Ctrl-T bash/zsh；fish
//! 绑定）。init 只输出脚本，不写用户 rc 文件（SPEC OUT_OF_SCOPE）。

/// 生成 shell 集成脚本（`shell` 由 main 分派保证合法）。
pub fn script(shell: &str) -> &'static str {
    match shell {
        "bash" => {
            r#"# rvifm shell 集成（bash）：cd 后记录 + Ctrl-T 调 pick
__rvifm_record() { command rvifm record --topic dir "$PWD" >/dev/null 2>&1; }
[[ $- == *i* ]] && PROMPT_COMMAND="__rvifm_record${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
__rvifm_pick() {
    local sel
    sel=$(command rvifm pick --topic dir "$READLINE_LINE" </dev/tty) && READLINE_LINE="$sel" && READLINE_POINT=${#READLINE_LINE}
}
bind -x '"\C-t": __rvifm_pick'
"#
        }
        "zsh" => {
            r#"# rvifm shell 集成（zsh）：cd 后记录 + Ctrl-T 调 pick
__rvifm_record() { command rvifm record --topic dir "$PWD" >/dev/null 2>&1 }
autoload -Uz add-zsh-hook && add-zsh-hook chpwd __rvifm_record
__rvifm_pick() {
    local sel
    sel=$(command rvifm pick --topic dir "$BUFFER" </dev/tty) && BUFFER="$sel" && CURSOR=${#BUFFER}
}
zle -N __rvifm_pick
bindkey '^T' __rvifm_pick
"#
        }
        "fish" => {
            r#"# rvifm shell 集成（fish）：cd 后记录 + Ctrl-T 调 pick
function __rvifm_record --on-variable PWD
    command rvifm record --topic dir "$PWD" >/dev/null 2>&1
end
function __rvifm_pick
    set -l sel (command rvifm pick --topic dir (commandline -b) </dev/tty)
    and commandline -r "$sel"
end
bind \ct __rvifm_pick
"#
        }
        "powershell" => {
            r#"# rvifm shell 集成（PowerShell）：cd 后记录 + Ctrl-T 调 pick
function prompt {
    command rvifm record --topic dir "$PWD" 2>$null | Out-Null
    "PS $PWD> "
}
if (Get-Module -ListAvailable PSReadLine) {
    Set-PSReadLineKeyHandler -Chord Ctrl+t -ScriptBlock {
        $sel = command rvifm pick --topic dir 2>$null | Select-Object -First 1
        if ($sel) { [Microsoft.PowerShell.PSConsoleReadLine]::Insert("$sel") }
    }
}
"#
        }
        _ => "",
    }
}
