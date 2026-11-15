#!/usr/bin/env bash
# =============================================================================
# env-setup.sh — подготовка окружения для lab5 (основы DevOps)
# Устанавливает зависимости: make, gcc (для Rust), curl (для rustup).
# Пример скрипта подготовки окружения проекта.
#
# Студентам: запустить до первой сборки.
#   bash env-setup.sh
# =============================================================================

set -euo pipefail

echo "=== Environment setup for lab5 ==="

# Определяем дистрибутив для пакетного менеджера.
if command -v apt-get &>/dev/null; then
    PKG_MANAGER="apt"
elif command -v dnf &>/dev/null; then
    PKG_MANAGER="dnf"
elif command -v yum &>/dev/null; then
    PKG_MANAGER="yum"
elif command -v pacman &>/dev/null; then
    PKG_MANAGER="pacman"
else
    echo "ERROR: Unsupported package manager." >&2
    exit 1
fi

echo "Detected package manager: $PKG_MANAGER"

# Обновляем списки пакетов.
case "$PKG_MANAGER" in
    apt)
        sudo apt-get update
        ;;
    dnf|yum)
        sudo "$PKG_MANAGER" check-update || true
        ;;
    pacman)
        sudo pacman -Sy --noconfirm
        ;;
esac

# Устанавливаем необходимые пакеты.
case "$PKG_MANAGER" in
    apt)
        sudo apt-get install -y make gcc curl git
        ;;
    dnf|yum)
        sudo "$PKG_MANAGER" install -y make gcc curl git
        ;;
    pacman)
        sudo pacman -S --noconfirm make gcc curl git
        ;;
esac

# Проверяем, что make установлен.
if command -v make &>/dev/null; then
    echo "make is installed: $(make --version | head -1)"
else
    echo "ERROR: make installation failed." >&2
    exit 1
fi

# Проверяем, что rustc установлен (устанавливаем через rustup если нет).
if command -v rustc &>/dev/null; then
    echo "Rust is installed: $(rustc --version)"
else
    # Устанавливаем rustup + Rust.
    if ! command -v curl &>/dev/null; then
        case "$PKG_MANAGER" in
            apt) sudo apt-get install -y curl ;;
            dnf|yum) sudo "$PKG_MANAGER" install -y curl ;;
            pacman) sudo pacman -S --noconfirm curl ;;
        esac
    fi

    echo "Installing rustup (Rust toolchain)..."
    # --no-submodules — установка без подмодулей (для чистого Rust-проекта).
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

    # Источаем переменные окружения (добавляем в PATH для текущего сеанса).
    source "$HOME/.cargo/env" 2>/dev/null || true
fi

# Проверяем, что cargo установлен.
if command -v cargo &>/dev/null; then
    echo "cargo is installed: $(cargo --version)"
else
    echo "ERROR: cargo installation failed." >&2
    exit 1
fi

echo "=== Environment setup complete ==="
echo ""
echo "You can now create all missing files and then install project"
echo ""
