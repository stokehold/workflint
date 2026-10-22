#compdef workflint
# zsh completion for workflint. `workflint completion zsh` prints it.

_workflint() {
  if (( CURRENT > 2 )) && [[ $words[2] == completion ]]; then
    (( CURRENT == 3 )) && _values shell @SHELLS@
    return
  fi
  local -a commands
  if [[ $words[2] == check ]] && (( CURRENT > 2 )); then
    words=($words[1] $words[3,-1])
    (( CURRENT-- ))
  elif (( CURRENT == 2 )) && [[ $PREFIX != -* ]]; then
    commands=('1:: :(@COMMANDS@)')
  fi
  # -A: like Go's flag package, no flag after the first argument that is not a flag.
  _arguments -S -A '-*' \
    '-help[Show the usage]' \
@SPECS@    $commands \
    '*:file:_files -g "*.(yml|yaml)(-.)"'
}

if [[ $zsh_eval_context[-1] == loadautofunc ]]; then
  _workflint "$@"
else
  compdef _workflint workflint
fi
