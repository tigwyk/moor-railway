#!/bin/sh
set -e

# Extract the DNS resolver from the container's /etc/resolv.conf
# so nginx can resolve Railway internal hostnames (*.railway.internal)
# at request time instead of only at startup.
RESOLVER=$(awk '/^nameserver/{print $2; exit}' /etc/resolv.conf)
RESOLVER=${RESOLVER:-8.8.8.8}

# Nginx requires IPv6 addresses in brackets: [fd12::10]
case "$RESOLVER" in
    *:*) RESOLVER="[$RESOLVER]" ;;
esac

echo "Using DNS resolver: $RESOLVER"

# Substitute __RESOLVER__ placeholder in nginx config
sed -i "s/__RESOLVER__/$RESOLVER/g" /etc/nginx/nginx.conf

exec nginx -g 'daemon off;'
