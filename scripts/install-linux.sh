#!/bin/sh
# VoxPen Desktop — Linux install script
# Installs the native binary, resources, .desktop entry, and icon.
#
# Usage:
#   ./scripts/install-linux.sh          # install from build output
#   ./scripts/install-linux.sh uninstall # remove everything
#
# Requires: npx tauri build (or cargo build --release) to have been run first.

set -eu

APP_NAME="voxpen-desktop"
DISPLAY_NAME="VoxPen"
COMMENT="VoxPen Desktop — Voice-to-Text"
IDENTIFIER="com.voxpen.desktop"

PREFIX="${HOME}/.local"
BIN_DIR="${PREFIX}/bin"
RESOURCES_DIR="${BIN_DIR}/resources"
ICON_DIR="${PREFIX}/share/icons"
DESKTOP_DIR="${PREFIX}/share/applications"

PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BUILD_BIN="${PROJECT_DIR}/src-tauri/target/release/${APP_NAME}"
BUILD_RESOURCES="${PROJECT_DIR}/src-tauri/target/release/resources"
ICON_SRC="${PROJECT_DIR}/src-tauri/icons/128x128.png"

# --------------------------------------------------------------------------- #

usage() {
    echo "Usage: $0 [uninstall]"
    echo ""
    echo "  (no args)   Build (if needed) and install VoxPen Desktop"
    echo "  uninstall   Remove VoxPen Desktop from ${PREFIX}"
}

do_uninstall() {
    echo "Uninstalling VoxPen Desktop..."
    rm -f  "${BIN_DIR}/${APP_NAME}"
    rm -f  "${BIN_DIR}/voxpen"
    rm -rf "${RESOURCES_DIR}"
    rm -f  "${ICON_DIR}/voxpen.png"
    rm -f  "${DESKTOP_DIR}/voxpen.desktop"
    echo "Done."
}

do_install() {
    # ---- pre-flight checks ------------------------------------------------ #
    if [ ! -f "${BUILD_BIN}" ]; then
        echo "Binary not found at ${BUILD_BIN}"
        echo "Run 'npx tauri build' first, or 'cargo build --release --manifest-path src-tauri/Cargo.toml'."
        exit 1
    fi

    # ---- create directories ------------------------------------------------ #
    mkdir -p "${BIN_DIR}" "${RESOURCES_DIR}" "${ICON_DIR}" "${DESKTOP_DIR}"

    # ---- binary ------------------------------------------------------------ #
    echo "Installing binary..."
    cp -f "${BUILD_BIN}" "${BIN_DIR}/${APP_NAME}"
    chmod +x "${BIN_DIR}/${APP_NAME}"
    # convenience symlink
    ln -sf "${APP_NAME}" "${BIN_DIR}/voxpen"

    # ---- resources (paste script, etc.) ------------------------------------ #
    if [ -d "${BUILD_RESOURCES}" ]; then
        echo "Installing resources..."
        cp -rf "${BUILD_RESOURCES}/." "${RESOURCES_DIR}/"
        chmod +x "${RESOURCES_DIR}"/*.sh 2>/dev/null || true
    else
        echo "Warning: ${BUILD_RESOURCES} not found — resources not installed."
        echo "         Auto-paste may not work. Run 'npx tauri build' for a full build."
    fi

    # ---- icon -------------------------------------------------------------- #
    if [ -f "${ICON_SRC}" ]; then
        echo "Installing icon..."
        cp -f "${ICON_SRC}" "${ICON_DIR}/voxpen.png"
    fi

    # ---- .desktop entry ---------------------------------------------------- #
    echo "Installing desktop entry..."
    cat > "${DESKTOP_DIR}/voxpen.desktop" <<DESKTOP
[Desktop Entry]
Name=${DISPLAY_NAME}
Comment=${COMMENT}
Exec=${BIN_DIR}/${APP_NAME}
Icon=${ICON_DIR}/voxpen.png
Type=Application
Categories=Utility;Audio;
Terminal=false
StartupNotify=false
StartupWMClass=${IDENTIFIER}
DESKTOP

    # ---- summary ----------------------------------------------------------- #
    echo ""
    echo "VoxPen Desktop installed:"
    echo "  Binary:    ${BIN_DIR}/${APP_NAME}"
    echo "  Symlink:   ${BIN_DIR}/voxpen"
    echo "  Resources: ${RESOURCES_DIR}/"
    echo "  Icon:      ${ICON_DIR}/voxpen.png"
    echo "  Desktop:   ${DESKTOP_DIR}/voxpen.desktop"
    echo ""

    # ---- dependency check -------------------------------------------------- #
    missing=""
    for cmd in wl-copy ydotool xdotool; do
        if ! command -v "$cmd" >/dev/null 2>&1; then
            missing="${missing} ${cmd}"
        fi
    done
    if [ -n "${missing}" ]; then
        echo "Warning: missing optional dependencies:${missing}"
        echo "  Install them for auto-paste support on Wayland."
        echo "  Arch:   sudo pacman -S wl-clipboard ydotool xdotool"
        echo "  Debian: sudo apt install wl-clipboard ydotool xdotool"
        echo ""
    fi

    # ---- ydotoold check ---------------------------------------------------- #
    if command -v ydotool >/dev/null 2>&1; then
        if [ ! -S "/tmp/.ydotool_socket" ]; then
            echo "Warning: ydotoold is not running."
            echo "  Auto-paste requires ydotoold. Start it with:"
            echo "  sudo ydotoold --socket-path /tmp/.ydotool_socket --socket-perm 0666 &"
            echo ""
        fi
    fi

    echo "Run 'voxpen' or 'voxpen-desktop' to start."
}

# --------------------------------------------------------------------------- #

case "${1:-}" in
    uninstall)
        do_uninstall
        ;;
    -h|--help|help)
        usage
        ;;
    "")
        do_install
        ;;
    *)
        echo "Unknown argument: $1"
        usage
        exit 1
        ;;
esac
