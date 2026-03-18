#!/bin/sh
# VoxPen auto-paste helper for KDE Wayland.
# Usage: voxpen-paste.sh <text> [original_clipboard]
TEXT="$1"
ORIGINAL="$2"

# Detect keyd ctrl/capslock swap
CTRL_CODE=29
if grep -q 'capslock.*leftcontrol' /etc/keyd/*.conf 2>/dev/null; then
    CTRL_CODE=58
fi

export YDOTOOL_SOCKET="${YDOTOOL_SOCKET:-/tmp/.ydotool_socket}"

# 1. Set clipboard
printf '%s' "$TEXT" | wl-copy --type text/plain

# 2. Wait for wl-copy daemon to register with compositor
sleep 0.15

# 3. Simulate Ctrl+V
ydotool key --key-delay 100 "${CTRL_CODE}:1" 47:1 47:0 "${CTRL_CODE}:0"

# 4. Wait for target app to read clipboard
sleep 0.25

# 5. Restore original clipboard
if [ -n "$ORIGINAL" ]; then
    printf '%s' "$ORIGINAL" | wl-copy --type text/plain
fi
