status is-interactive; or return 0
# Fish 4 emits OSC 133 itself, including the accepted command text.
set -l major (string split . -- $version)[1]
if test "$major" -ge 4
    return 0
end
set -q __oxideterm_command_hooks; and return 0
if functions fish_prompt | string match -qr '\]133;|\]633;'
    return 0
end

function __oxideterm_restore_status
    return $argv[1]
end

functions -c fish_prompt __oxideterm_original_command_prompt
function fish_prompt
    set -l result $status
    printf '\e]133;A\a'
    __oxideterm_restore_status $result
    __oxideterm_original_command_prompt
    printf '\e]133;B\a'
end

function __oxideterm_command_start --on-event fish_preexec
    printf '\e]133;C\a'
end

function __oxideterm_command_end --on-event fish_postexec
    set -l result $status
    printf '\e]133;D;%s\a' $result
    return $result
end
set -g __oxideterm_command_hooks 1
