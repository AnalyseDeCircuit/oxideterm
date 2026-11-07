# Read each accessible domain independently. Never resolve a label by trying
# an action in another domain after a permission failure.
uid=$(id -u)
agent_domain="user/$uid"
for domain in system "user/$uid" "gui/$uid"; do
    inventory=$(launchctl print "$domain" 2>/dev/null) || continue
    [ "$domain" != "gui/$uid" ] || agent_domain=$domain
    printf '%s\n' "$inventory" | awk -v domain="$domain" '
        /^[[:space:]]*services = \{/ { section=1; found=1; next }
        section && /^[[:space:]]*\}/ { section=0 }
        section && NF == 3 && $1 ~ /^[0-9]+$/ && ($2 == "-" || $2 ~ /^-?[0-9]+$/) {
            printf "LAUNCHD_LIVE\t%s\t%s\t%s\t%s\n", domain, $1, $2, $3
        }
        END { if (!found) print "LAUNCHD_ERROR\tUnrecognized launchd domain inventory." }
    '
    disabled=$(launchctl print-disabled "$domain" 2>/dev/null)
    if [ $? -eq 0 ]; then
        printf 'LAUNCHD_OVERRIDES\t%s\n' "$domain"
        printf '%s\n' "$disabled" | awk -v domain="$domain" '/=>/ { printf "LAUNCHD_DISABLED\t%s\t%s\n", domain, $0 }'
    fi
done
