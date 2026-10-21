# WSLENV translates this owned Windows directory; no drive or mount root is assumed.
directory=$OXIDETERM_SHELL_DIR
shell=${SHELL:-/bin/sh}
export OXIDETERM_VIM_INTEGRATION="$directory/oxideterm-free-type.vim"
export OXIDETERM_EMACS_INTEGRATION="$directory/oxideterm-free-type.el"
case "${shell##*/}" in
    bash)
        exec "$shell" --noprofile --rcfile "$directory/bashrc" -i
        ;;
    zsh)
        export OXIDETERM_USER_ZDOTDIR="${ZDOTDIR:-$HOME}"
        export ZDOTDIR="$directory"
        exec "$shell" -i
        ;;
    fish)
        if [ "$1" = 1 ]; then
            exec "$shell" --login --init-command 'source "$OXIDETERM_SHELL_DIR/fish.fish"'
        else
            exec "$shell" --no-config --init-command 'source "$OXIDETERM_SHELL_DIR/fish.fish"'
        fi
        ;;
    nu)
        if [ "$1" = 1 ]; then
            exec "$shell" --execute "$(cat "$directory/nushell.nu")"
        else
            exec "$shell" --no-config-file --execute "$(cat "$directory/nushell.nu")"
        fi
        ;;
    pwsh|powershell)
        if [ "$1" = 1 ]; then
            exec "$shell" -NoLogo -NoExit -Command '. "$env:OXIDETERM_SHELL_DIR/profile.ps1"'
        else
            exec "$shell" -NoLogo -NoExit -NoProfile -Command '. "$env:OXIDETERM_SHELL_DIR/profile.ps1"'
        fi
        ;;
    *)
        # Unsupported shells keep their normal startup and never receive
        # syntax intended for a different shell.
        exec "$shell" -i
        ;;
esac
