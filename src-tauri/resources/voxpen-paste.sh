#!/bin/sh
# VoxPen auto-paste helper for KDE Wayland.
# Usage: voxpen-paste.sh <text> [original_clipboard]
# Env: VOXPEN_ACTIVE_APP — lowercase WM_CLASS of the focused app (optional)
#      VOXPEN_WINDOW_ID  — X11 window ID for focus restore (optional)
TEXT="$1"
ORIGINAL="$2"

# Detect keyd ctrl/capslock swap
CTRL_CODE=29
if grep -q 'capslock.*leftcontrol' /etc/keyd/*.conf 2>/dev/null; then
    CTRL_CODE=58
fi

# Detect active app for terminal detection.
# VOXPEN_ACTIVE_APP comes from X11 WM_CLASS — empty for native Wayland windows.
# Use kdotool to get the classname for native Wayland windows.
APP="$VOXPEN_ACTIVE_APP"
if [ -z "$APP" ] && command -v kdotool >/dev/null 2>&1; then
    KWID=$(kdotool getactivewindow 2>/dev/null)
    if [ -n "$KWID" ]; then
        APP=$(kdotool getwindowclassname "$KWID" 2>/dev/null)
    fi
fi

IS_TERMINAL=0
case "$APP" in
    ghostty|com.mitchellh.ghostty|kitty|alacritty|wezterm*|foot|terminator|tilix|sakura|contour|rio|st|st-*)
        IS_TERMINAL=1 ;;
    *terminal*|*konsole*|*xterm*|*urxvt*|*gnome-terminal*|*xfce4-terminal*|*mate-terminal*|*lxterminal*)
        IS_TERMINAL=1 ;;
esac

export YDOTOOL_SOCKET="${YDOTOOL_SOCKET:-/tmp/.ydotool_socket}"

# 0. Restore keyboard focus via kdotool (works for both native Wayland and XWayland).
#    kdotool uses KDE's native window management protocol, which properly
#    delivers wl_keyboard.enter — unlike xdotool or KWin scripting.
if command -v kdotool >/dev/null 2>&1; then
    KWID=$(kdotool getactivewindow 2>/dev/null)
    if [ -n "$KWID" ]; then
        kdotool windowactivate "$KWID" 2>/dev/null
    fi
    sleep 0.05
elif [ -n "$VOXPEN_WINDOW_ID" ]; then
    # Fallback: xdotool for XWayland apps
    xdotool windowactivate "$VOXPEN_WINDOW_ID" 2>/dev/null
    sleep 0.05
fi

# 1. Set clipboard
printf '%s' "$TEXT" | wl-copy --type text/plain

# 2. Wait for wl-copy daemon to register with compositor
sleep 0.15

# 3. Simulate paste keystroke
if [ "$IS_TERMINAL" = "1" ]; then
    # Terminal: Ctrl+Shift+V (Shift = evdev 42)
    SHIFT_CODE=42
    ydotool key --key-delay 100 "${CTRL_CODE}:1" "${SHIFT_CODE}:1" 47:1 47:0 "${SHIFT_CODE}:0" "${CTRL_CODE}:0"
else
    # Normal app: Ctrl+V
    ydotool key --key-delay 100 "${CTRL_CODE}:1" 47:1 47:0 "${CTRL_CODE}:0"
fi

# 4. Wait for target app to read clipboard
sleep 0.25

# 5. Restore original clipboard
if [ -n "$ORIGINAL" ]; then
    printf '%s' "$ORIGINAL" | wl-copy --type text/plain
fi
