# ~/.bashrc.d/prompt.sh (from gump): same prompt as the zsh one.
# user@host dir (git branch) $  — bold, branch in orange.
__gump_branch() {
  local b
  b=$(git symbolic-ref --short HEAD 2>/dev/null || git rev-parse --short HEAD 2>/dev/null) || return
  printf ' \001\e[38;5;208m\002(%s)\001\e[39m\002' "$b"
}
PS1='\[\e[1;36m\]\u@\h\[\e[39m\] \[\e[34m\]\w\[\e[39m\]$(__gump_branch) $\[\e[0m\] '
