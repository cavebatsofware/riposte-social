# X-Forwarded-For Injection Test Script
# Tests whether the middleware can be tricked into logging a wrong IP

TARGET="${1:-http://localhost:3000}"
ENDPOINT="${2:-/}"

echo "=== X-Forwarded-For Injection Tests ==="
echo "Target: $TARGET$ENDPOINT"
echo ""

# 1. Basic spoofed IP
echo "1. Basic spoofed IP (single value)"
curl -s -D - -o /dev/null -H "X-Forwarded-For: 1.2.3.4" "$TARGET$ENDPOINT"
echo ""

# 2. Multiple IPs - attacker prepends fake IP
echo "2. Prepended fake IP (attacker,real) - should use attacker IP if vulnerable"
curl -s -D - -o /dev/null -H "X-Forwarded-For: 6.6.6.6, 203.0.113.50" "$TARGET$ENDPOINT"
echo ""

# 3. Multiple IPs - attacker appends fake IP
echo "3. Appended fake IP (real,attacker) - tests if middleware uses last instead of first"
curl -s -D - -o /dev/null -H "X-Forwarded-For: 203.0.113.50, 6.6.6.6" "$TARGET$ENDPOINT"
echo ""

# 4. Multiple X-Forwarded-For headers
echo "4. Multiple X-Forwarded-For headers (header injection)"
curl -s -D - -o /dev/null -H "X-Forwarded-For: 1.1.1.1" -H "X-Forwarded-For: 2.2.2.2" "$TARGET$ENDPOINT"
echo ""

# 5. IPv6 spoofing
echo "5. IPv6 address"
curl -s -D - -o /dev/null -H "X-Forwarded-For: 2001:db8::1" "$TARGET$ENDPOINT"
echo ""

# 6. IPv6 loopback
echo "6. IPv6 loopback (::1)"
curl -s -D - -o /dev/null -H "X-Forwarded-For: ::1" "$TARGET$ENDPOINT"
echo ""

# 7. IPv4 loopback
echo "7. IPv4 loopback (127.0.0.1)"
curl -s -D - -o /dev/null -H "X-Forwarded-For: 127.0.0.1" "$TARGET$ENDPOINT"
echo ""

# 8. Private IP ranges
echo "8. Private IP (10.x.x.x)"
curl -s -D - -o /dev/null -H "X-Forwarded-For: 10.0.0.1" "$TARGET$ENDPOINT"
echo ""

echo "9. Private IP (192.168.x.x)"
curl -s -D - -o /dev/null -H "X-Forwarded-For: 192.168.1.1" "$TARGET$ENDPOINT"
echo ""

# 10. Malformed values
echo "10. Malformed: garbage string"
curl -s -D - -o /dev/null -H "X-Forwarded-For: not-an-ip" "$TARGET$ENDPOINT"
echo ""

echo "11. Malformed: empty value"
curl -s -D - -o /dev/null -H "X-Forwarded-For: " "$TARGET$ENDPOINT"
echo ""

echo "12. Malformed: spaces only"
curl -s -D - -o /dev/null -H "X-Forwarded-For:    " "$TARGET$ENDPOINT"
echo ""

# 13. Port injection
echo "13. IP with port (1.2.3.4:8080)"
curl -s -D - -o /dev/null -H "X-Forwarded-For: 1.2.3.4:8080" "$TARGET$ENDPOINT"
echo ""

# 14. Null byte injection
echo "14. Null byte injection"
curl -s -D - -o /dev/null -H $'X-Forwarded-For: 1.2.3.4\x00 5.6.7.8' "$TARGET$ENDPOINT"
echo ""

# 15. Newline injection (header splitting)
echo "15. Newline injection (CRLF)"
curl -s -D - -o /dev/null -H $'X-Forwarded-For: 1.2.3.4\r\nX-Injected: evil' "$TARGET$ENDPOINT"
echo ""

# 16. Unicode/homoglyph
echo "16. Unicode lookalike digits"
curl -s -D - -o /dev/null -H "X-Forwarded-For: １.２.３.４" "$TARGET$ENDPOINT"
echo ""

# 17. Case variations of header name
echo "17. Lowercase header: x-forwarded-for"
curl -s -D - -o /dev/null -H "x-forwarded-for: 7.7.7.7" "$TARGET$ENDPOINT"
echo ""

echo "18. Mixed case: X-FORWARDED-FOR"
curl -s -D - -o /dev/null -H "X-FORWARDED-FOR: 8.8.8.8" "$TARGET$ENDPOINT"
echo ""

# 19. Alternative proxy headers
echo "19. X-Real-IP header (alternative)"
curl -s -D - -o /dev/null -H "X-Real-IP: 9.9.9.9" "$TARGET$ENDPOINT"
echo ""

echo "20. Forwarded header (RFC 7239)"
curl -s -D - -o /dev/null -H "Forwarded: for=4.4.4.4" "$TARGET$ENDPOINT"
echo ""

# 21. Both X-Forwarded-For and X-Real-IP
echo "21. Both X-Forwarded-For and X-Real-IP"
curl -s -D - -o /dev/null -H "X-Forwarded-For: 1.1.1.1" -H "X-Real-IP: 2.2.2.2" "$TARGET$ENDPOINT"
echo ""

# 22. Extremely long header
echo "22. Very long X-Forwarded-For (overflow test)"
LONG_IPS=$(printf '1.1.1.%d, ' {1..100} | sed 's/, $//')
curl -s -D - -o /dev/null -H "X-Forwarded-For: $LONG_IPS" "$TARGET$ENDPOINT"
echo ""

echo "=== Tests Complete ==="
echo ""
echo "Check your server logs to see which IPs were logged for each request."
