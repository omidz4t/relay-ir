#!/bin/bash

# Safeguards: IPs that must NEVER be banned
SAFE_IPS=("127.0.0.1" "::1")

if [ -z "$1" ]; then
    echo "Usage: $0 <email_address>"
    exit 1
fi

SPAMMER_EMAIL="$1"
echo "Searching for IPs associated with: $SPAMMER_EMAIL"

# Extract IPs specifically from rip= (Dovecot) and client=...[IP] (Postfix) lines
# This prevents picking up timestamps or other random strings with colons.
IPS=$(journalctl --since "24 hours ago" | grep "$SPAMMER_EMAIL" | grep -oE "rip=[0-9a-fA-F.:]+|client=[^ ]+\[[0-9a-fA-F.:]+\]" | sed -E 's/.*rip=//; s/.*\[//; s/\].*//' | sort -u)

if [ -z "$IPS" ]; then
    echo "No IP addresses found for $SPAMMER_EMAIL in the last 24 hours."
    exit 0
fi

# Ensure the nftables table and chain exist
/usr/sbin/nft add table inet filter 2>/dev/null
/usr/sbin/nft add chain inet filter input { type filter hook input priority 0 \; policy accept \; } 2>/dev/null

for IP in $IPS; do
    # Check against safe list
    IS_SAFE=0
    for SAFE in "${SAFE_IPS[@]}"; do
        if [ "$IP" == "$SAFE" ]; then
            IS_SAFE=1
            break
        fi
    done

    if [ $IS_SAFE -eq 1 ]; then
        echo "Skipping safe IP: $IP"
        continue
    fi

    # Interactive confirmation
    read -p "Found IP $IP for $SPAMMER_EMAIL. Ban it? (yes/no): " choice
    if [[ "$choice" != "yes" ]]; then
        echo "Skipping $IP"
        continue
    fi

    echo "Banning IP: $IP"
    # Detect if IPv6 (contains colon) or IPv4
    if [[ "$IP" == *":"* ]]; then
        /usr/sbin/nft add rule inet filter input ip6 saddr "$IP" drop
    else
        /usr/sbin/nft add rule inet filter input ip saddr "$IP" drop
    fi
done

echo ""
echo "Current ban rules:"
/usr/sbin/nft list table inet filter
