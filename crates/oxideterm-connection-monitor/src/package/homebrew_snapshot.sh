echo '===PACKAGES==='
if ! command -v brew >/dev/null 2>&1; then
    echo '__OXIDE_PACKAGE_UNAVAILABLE__'
else
    printf '__OXIDE_PACKAGE_CAPABILITY__\tpartial\tmacos_brew\nMANAGER\tbrew\ttrue\tbrew\n'
    export HOMEBREW_NO_AUTO_UPDATE=1 HOMEBREW_NO_ANALYTICS=1
    for kind in formula cask; do
        inventory=$(brew list --"$kind" --versions 2>&1)
        if [ $? -ne 0 ]; then
            reason=$(printf '%s\n' "$inventory" | head -n 1 | tr '\t\r' '  ' | cut -c1-240)
            printf '__OXIDE_PACKAGE_WARNING__\t%s: %s\n' "$kind" "$reason"
            continue
        fi
        printf '%s\n' "$inventory" | awk -v kind="$kind" 'NF >= 2 && $1 ~ /^[a-zA-Z0-9._+@\/-]+$/ { printf "BREW_INSTALLED\t%s\t%s\t", kind, $1; for(i=2;i<=NF;i++) printf "%s%s", (i==2?"":" "), $i; printf "\n" }'
    done
    outdated=$(brew outdated --json=v2 2>/dev/null)
    if [ $? -eq 0 ]; then
        echo 'BREW_OUTDATED_BEGIN'
        printf '%s\n' "$outdated"
        echo 'BREW_OUTDATED_END'
    else
        printf '__OXIDE_PACKAGE_WARNING__\tHomebrew outdated-package query failed.\n'
    fi
fi
echo '===PACKAGES_END==='
