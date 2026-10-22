# fish completion for workflint. `workflint completion fish` prints it.

# Whether the token under the cursor may be a flag. Go's flag package ends the flags at `--`
# and at the first argument that is not a flag.
function __workflint_flags_allowed
    set -l args (commandline -opc)
    set -e args[1]
    test "$args[1]" = completion; and return 1
    test "$args[1]" = check; and set -e args[1]
    while set -q args[1]
        switch $args[1]
            case -- -
                return 1
            case '-*=*'
            case '-*'
                if contains -- (string replace -r -- '^--?' '' $args[1]) @VALUE_FLAGS@
                    set -e args[1]
                end
            case '*'
                return 1
        end
        set -e args[1]
    end
    return 0
end

# Whether the token under the cursor is the first argument.
function __workflint_first_argument
    test (count (commandline -opc)) -eq 1
end

# Whether the token under the cursor names the shell of `workflint completion`.
function __workflint_completing_shell
    set -l args (commandline -opc)
    test (count $args) -eq 2; and test "$args[2]" = completion
end

complete -c workflint -n __workflint_completing_shell -f -a '@SHELLS@'
complete -c workflint -n __workflint_first_argument -a check -d 'Lint the files (the default command)'
complete -c workflint -n __workflint_first_argument -a completion -d 'Print a shell completion script'
complete -c workflint -n __workflint_flags_allowed -o help -d 'Show the usage'
@LINES@