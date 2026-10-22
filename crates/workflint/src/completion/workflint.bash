# bash completion for workflint. `workflint completion bash` prints it.

_workflint() {
    local IFS=$' \t\n'
    local all_flags='@ALL_FLAGS@ -help'
    local bool_flags='@BOOL_FLAGS@'
    local value_flags='@VALUE_FLAGS@'
    local file_flags='@FILE_FLAGS@'
    local command_flags='@COMMAND_FLAGS@'
    local format_flags='@FORMAT_FLAGS@'

    # Bash splits a word at `=` and `:`. Join the parts that touch, so that `-format=js` is one
    # word. Readline replaces only the text after the last `=` or `:`, which is `$part`.
    local -a words
    local rest=$COMP_LINE trimmed word part=${COMP_WORDS[COMP_CWORD]} i n=-1
    for (( i = 0; i <= COMP_CWORD; i++ )); do
        word=${COMP_WORDS[i]}
        trimmed=${rest#"${rest%%[![:space:]]*}"}
        if (( n >= 0 )) && [[ $trimmed == "$rest" ]]; then
            words[n]+=$word
        else
            words[++n]=$word
        fi
        rest=${trimmed#"$word"}
    done
    case $part in
        = | :) part= ;;
    esac
    local cur=${words[n]}
    local prefix=${cur%"$part"}

    COMPREPLY=()
    if (( n >= 2 )) && [[ ${words[1]} == completion ]]; then
        (( n == 2 )) && _workflint_reply "$prefix" "$(compgen -W '@SHELLS@' -- "$cur")"
        return 0
    fi

    # Go's flag package ends the flags at `--` and at the first argument that is not a flag.
    local flags_end= name
    i=1
    (( n > 1 )) && [[ ${words[1]} == check ]] && i=2
    while (( i < n )); do
        word=${words[i]}
        case $word in
            -- | - | [!-]*)
                flags_end=1
                break
                ;;
            -*=*) ;;
            *)
                name=${word#-}
                name=${name#-}
                if _workflint_has "$value_flags" "$name"; then
                    if (( i + 1 == n )); then
                        _workflint_value "$name" "" "$cur"
                        return 0
                    fi
                    (( i++ ))
                fi
                ;;
        esac
        (( i++ ))
    done

    if [[ -z $flags_end ]]; then
        case $cur in
            -*=*)
                name=${cur%%=*}
                name=${name#-}
                name=${name#-}
                _workflint_value "$name" "${prefix#"${cur%%=*}="}" "${cur#*=}"
                return 0
                ;;
            --*)
                _workflint_reply "$prefix" "$(compgen -W "$all_flags" -- "${cur#-}" | sed 's/^/-/')"
                return 0
                ;;
            -*)
                _workflint_reply "$prefix" "$(compgen -W "$all_flags" -- "$cur")"
                return 0
                ;;
        esac
        if (( n == 1 )); then
            _workflint_reply "$prefix" "$(compgen -W '@COMMANDS@' -- "$cur")"
        fi
    fi
    _workflint_files "$prefix" "$cur"
    return 0
}

# Completes the value `cur` of the flag `name`, without `prefix`.
_workflint_value() {
    local name=$1 prefix=$2 cur=$3 IFS=$' \t\n'
    if _workflint_has "$file_flags" "$name"; then
        _workflint_reply "$prefix" "$(compgen -f -- "$cur")"
    elif _workflint_has "$command_flags" "$name"; then
        _workflint_reply "$prefix" "$(compgen -c -- "$cur"; compgen -f -- "$cur")"
    elif _workflint_has "$format_flags" "$name"; then
        _workflint_reply "$prefix" "$(compgen -W '@FORMATS@' -- "$cur")"
    elif _workflint_has "$bool_flags" "$name"; then
        _workflint_reply "$prefix" "$(compgen -W 'true false' -- "$cur")"
    fi
}

# Completes the file argument `cur`, without `prefix`: directories, and files named *.yml or
# *.yaml.
_workflint_files() {
    _workflint_reply "$1" "$(compgen -d -- "$2"; compgen -f -X '!*.yml' -- "$2"; compgen -f -X '!*.yaml' -- "$2")"
}

# Adds each line of $2 to the completions, without the prefix $1.
_workflint_reply() {
    local line IFS=$'\n'
    for line in $2; do
        COMPREPLY[${#COMPREPLY[@]}]=${line#"$1"}
    done
}

# Whether the words $1 include $2.
_workflint_has() {
    case " $1 " in
        *" $2 "*) return 0 ;;
    esac
    return 1
}

complete -o filenames -F _workflint workflint
