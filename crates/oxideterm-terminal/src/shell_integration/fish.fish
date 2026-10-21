status is-interactive; or return 0
# Fish 4 emits OSC 133 itself, including the accepted command text.
set -l major (string split . -- $version)[1]
if test "$major" -ge 4
    return 0
end
function __oxideterm_restore_status
    return $argv[1]
end

function __oxideterm_install_command_prompt --on-event fish_prompt
    set -l result $status
    # The prompt event survives themes redefining fish_prompt after startup.
    set -l definition (functions fish_prompt)
    if string match -q '*__oxideterm_original_command_prompt*' -- $definition
        set -g __oxideterm_command_enabled 1
    else if string match -qr '\]133;|\]633;' -- $definition
        set -g __oxideterm_command_enabled 0
    else
        functions -e __oxideterm_original_command_prompt
        functions -c fish_prompt __oxideterm_original_command_prompt
        function fish_prompt
            set -l result $status
            printf '\e]133;A;aid=oxideterm\a'
            __oxideterm_restore_status $result
            __oxideterm_original_command_prompt
            printf '\e]133;B;aid=oxideterm\a'
        end
        set -g __oxideterm_command_enabled 1
    end
    return $result
end

function __oxideterm_command_start --on-event fish_preexec
    if test "$__oxideterm_command_enabled" = 1
        set -g __oxideterm_command_running 1
        printf '\e]133;C;aid=oxideterm\a'
    end
end

function __oxideterm_command_end --on-event fish_postexec
    set -l result $status
    if test "$__oxideterm_command_running" = 1
        printf '\e]133;D;%s;aid=oxideterm\a' $result
        set -g __oxideterm_command_running 0
    end
    return $result
end
__oxideterm_install_command_prompt
