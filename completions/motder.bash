# bash completion for motder

_motder() {
    local cur prev words cword
    if declare -F _init_completion >/dev/null; then
        _init_completion || return
    else
        # Works without the bash-completion package too.
        words=("${COMP_WORDS[@]}")
        cword=$COMP_CWORD
        cur=${COMP_WORDS[COMP_CWORD]}
        prev=${COMP_WORDS[COMP_CWORD-1]}
    fi

    local commands="add list ls dismiss rm clear show install uninstall help"
    local globals="--state-dir --motd-dir --help --version"

    case "$prev" in
        --state-dir|--motd-dir)
            if declare -F _filedir >/dev/null; then
                _filedir -d
            else
                COMPREPLY=( $(compgen -d -- "$cur") )
            fi
            return
            ;;
    esac

    # Find the subcommand, skipping global options and their values.
    local i cmd=""
    for (( i = 1; i < cword; i++ )); do
        case "${words[i]}" in
            --state-dir|--motd-dir) (( i++ )) ;;
            -*) ;;
            *) cmd="${words[i]}"; break ;;
        esac
    done

    case "$cmd" in
        "")
            COMPREPLY=( $(compgen -W "$commands $globals" -- "$cur") )
            ;;
        list|ls)
            COMPREPLY=( $(compgen -W "--json" -- "$cur") )
            ;;
        uninstall)
            COMPREPLY=( $(compgen -W "--purge" -- "$cur") )
            ;;
        dismiss|rm)
            local n
            n=$(motder list --json 2>/dev/null | grep -c '"index"')
            [[ $n -gt 0 ]] && COMPREPLY=( $(compgen -W "$(seq 1 "$n")" -- "$cur") )
            ;;
    esac
}

complete -F _motder motder
