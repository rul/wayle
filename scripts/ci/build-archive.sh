#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
	echo "Usage: $0 <target-dir> <version> <arch>" >&2
	exit 1
fi

TARGET_DIR="$1"
VERSION="$2"
ARCH="$3"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$PROJECT_DIR"

"${TARGET_DIR}/wayle" completions bash >completions.bash
"${TARGET_DIR}/wayle" completions zsh >_wayle
"${TARGET_DIR}/wayle" completions fish >wayle.fish

STAGING="wayle-${VERSION}-${ARCH}-linux"
mkdir -p "${STAGING}/icons" "${STAGING}/completions"
cp "${TARGET_DIR}/wayle" "${TARGET_DIR}/wayle-settings" LICENSE "${STAGING}/"
mkdir -p "${STAGING}/licenses"
if [[ -f vendor/wayle-network/LICENSE ]]; then
    cp vendor/wayle-network/LICENSE "${STAGING}/licenses/wayle-network.txt"
fi
if [[ -f "docs/releases/v${VERSION}.md" ]]; then
    cp "docs/releases/v${VERSION}.md" "${STAGING}/INSTALL.md"
fi
{
    echo "Version: ${VERSION}"
    echo "Commit: $(git rev-parse HEAD 2>/dev/null || printf '%s' "${SOURCE_COMMIT:?Set SOURCE_COMMIT when building outside Git}")"
    echo "Architecture: ${ARCH}"
    . /etc/os-release
    echo "Build OS: ${PRETTY_NAME}"
    echo "Rust: $(rustc --version)"
    echo "GTK: $(pkg-config --modversion gtk4)"
    echo "glibc: $(getconf GNU_LIBC_VERSION)"
} > "${STAGING}/BUILD-INFO.txt"
cp -r resources/icons/hicolor "${STAGING}/icons/"
cp completions.bash _wayle wayle.fish "${STAGING}/completions/"
cp resources/wayle.service "${STAGING}/"
cp resources/com.wayle.settings.desktop "${STAGING}/"
cp resources/wayle-settings.svg "${STAGING}/"
tar czf "${STAGING}.tar.gz" "${STAGING}"
