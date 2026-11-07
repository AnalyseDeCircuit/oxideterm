# Enrich the domain inventory with administrator and user-owned definitions,
# without exporting environment variables or other secrets.
for directory in "$HOME/Library/LaunchAgents" /Library/LaunchAgents /Library/LaunchDaemons; do
    [ -d "$directory" ] || continue
    case "$directory" in */LaunchDaemons) domain=system ;; *) domain=$agent_domain ;; esac
    for plist in "$directory"/*.plist; do
        [ -f "$plist" ] && [ -r "$plist" ] || continue
        label=$(plutil -extract Label raw -expect string -n -o - "$plist" 2>/dev/null) || continue
        case "$label" in ''|*/*|*[!a-zA-Z0-9._:@-]*) continue ;; esac
        if launchctl print "$domain/$label" >/dev/null 2>&1; then loaded=true; else loaded=false; fi
        path=$(printf '%s' "$plist" | base64 | tr -d '\n')
        printf 'LAUNCHD_CONFIG\t%s\t%s\t%s\t{"Label":"%s"' "$domain" "$loaded" "$path" "$label"
        for key in Program UserName; do
            value=$(plutil -extract "$key" raw -expect string -n -o - "$plist" 2>/dev/null) || continue
            encoded=$(printf '%s' "$value" | base64 | tr -d '\n')
            printf ',"%sBase64":"%s"' "$key" "$encoded"
        done
        for key in Disabled RunAtLoad StartOnMount StartInterval; do
            case "$key" in StartInterval) type=integer ;; *) type=bool ;; esac
            value=$(plutil -extract "$key" raw -expect "$type" -o - "$plist" 2>/dev/null) || continue
            printf ',"%s":%s' "$key" "$value"
        done
        for key in ProgramArguments StartCalendarInterval KeepAlive WatchPaths QueueDirectories; do
            # plutil cannot emit scalar JSON fragments; KeepAlive also accepts a boolean.
            if [ "$key" = KeepAlive ]; then
                value=$(plutil -extract "$key" raw -expect bool -o - "$plist" 2>/dev/null)
                if [ $? -eq 0 ]; then printf ',"%s":%s' "$key" "$value"; continue; fi
            fi
            value=$(plutil -extract "$key" json -o - "$plist" 2>/dev/null) || continue
            printf ',"%s":%s' "$key" "$value"
        done
        printf '}\n'
    done
done
