# bash completion for vedi

# Takes compgen's lines as they are: a name may hold spaces, and an
# unquoted $(...) would expand one holding [ or * as a pattern.
_vedi_lines() {
    local line
    while IFS= read -r line; do
        COMPREPLY+=("$line")
    done < <(compgen "$@")
}

_vedi() {
    local IFS=$' \t\n' cur=${COMP_WORDS[COMP_CWORD]} prev=${COMP_WORDS[COMP_CWORD-1]} i
    COMPREPLY=()
    # --flag=value arrives as three words: the flag, =, the value.
    if [[ $cur == = ]]; then
        cur=
    elif [[ $prev == = ]]; then
        prev=${COMP_WORDS[COMP_CWORD-2]}
    fi
    for ((i = 1; i < COMP_CWORD; i++)); do
        if [[ ${COMP_WORDS[i]} == -- ]]; then
            prev=--
        fi
    done
    case $prev in
    --wrap-style)
        COMPREPLY=($(compgen -W 'char word' -- "$cur"))
        return
        ;;
    --view-style)
        COMPREPLY=($(compgen -W 'color plain raw' -- "$cur"))
        return
        ;;
    --completion)
        COMPREPLY=($(compgen -W 'bash zsh fish' -- "$cur"))
        return
        ;;
    --one-page-rows-below | --scrolled-by | --cursor-row | --cursor-col | --tab-width)
        return
        ;;
    --clipboard-cmd | --open-cmd)
        _vedi_lines -c -- "$cur"
        return
        ;;
    esac
    if [[ $cur == -* && $prev != -- ]]; then
        COMPREPLY=($(compgen -W '
            -S --wrap --no-wrap --wrap-style
            -F --quit-if-one-page --no-quit-if-one-page --one-page-rows-below
            --auto-reload --no-auto-reload
            --scrolled-by --cursor-row --cursor-col
            --clipboard-cmd --no-clipboard-cmd
            --open-cmd --no-open-cmd
            --tab-width
            --edge-markers --no-edge-markers
            --file-separators --no-file-separators
            --status-line --no-status-line
            --view-style
            --config --completion
            -h --help -v --version' -- "$cur"))
        return
    fi
    _vedi_lines -f -- "$cur"
}

complete -o filenames -F _vedi vedi
