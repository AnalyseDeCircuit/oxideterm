[[ -o interactive ]] || return 0
(( ${+__oxideterm_command_hooks} )) && return 0
case "$PS1" in
    *']133;'*|*']633;'*) return 0 ;;
esac

__oxideterm_command_end() {
    local result=$?
    if (( ${__oxideterm_command_running:-0} )); then
        builtin printf '\033]133;D;%s\007' "$result"
    fi
    __oxideterm_command_running=0
    return "$result"
}

__oxideterm_command_prompt() {
    local result=$?
    if [[ "$PS1" != "${__oxideterm_wrapped_prompt-}" ]]; then
        case "$PS1" in
            *']133;'*|*']633;'*) __oxideterm_command_enabled=0; return "$result" ;;
        esac
        __oxideterm_wrapped_prompt=$'%{\e]133;A\a%}'"$PS1"$'%{\e]133;B\a%}'
        PS1=$__oxideterm_wrapped_prompt
    fi
    __oxideterm_command_enabled=1
    return "$result"
}

__oxideterm_command_start() {
    (( ${__oxideterm_command_enabled:-0} )) || return 0
    __oxideterm_command_running=1
    builtin printf '\033]133;C\007'
}

# Capture the exit status before other precmd hooks and wrap the prompt after
# theme hooks. All existing hook functions keep their original relative order.
typeset -ga precmd_functions preexec_functions
precmd_functions=(__oxideterm_command_end "${precmd_functions[@]}" __oxideterm_command_prompt)
preexec_functions+=(__oxideterm_command_start)
typeset -g __oxideterm_command_hooks=1
