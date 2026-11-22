#!/usr/bin/env bash
# =============================================================================
# watcher-launch.sh — скрипт запуска утилиты lab5-watcher.
# Вызывается systemd timer-based service (oneshot).
# Значения @CONFIG_PATH@, @HISTORY_FILE@, @PROGRAM_PATH@ подставляются make.
# =============================================================================

echo "=== Watcher launcher ==="

# ---------- Блок переменных --------------------------------------------------
CONFIG_PATH="@CONFIG_PATH@"
HISTORY_FILE="@HISTORY_FILE@"
PROGRAM_PATH="@PROGRAM_PATH@"

# ---------- Блок проверки и создания конфигурации ----------------------------
if [ ! -f "$CONFIG_PATH" ]; then
    echo "Config '$CONFIG_PATH' is missing, creating default config"
    mkdir -p "$(dirname "$CONFIG_PATH")"
    cat > "$CONFIG_PATH" <<'EOF'
[thresholds]
zombie_max = 10
cpu_max = 90.0
cpu_min = 5.0
mem_free_min = 512
swap_max = 8192
disk_used_percent = 60
disk_fill_speed = 1048576

[settings]
retention_count = 5
print_stats = true
EOF
    echo "Config written to '$CONFIG_PATH'"
fi

# ---------- Блок проверки и создания папки хранения истории ------------------
HISTORY_DIR="$(dirname "$HISTORY_FILE")"
if [ ! -d "$HISTORY_DIR" ]; then
    echo "History directory '$HISTORY_DIR' is missing, creating it"
    mkdir -p "$HISTORY_DIR"
fi

# ---------- Блок проверки программы lab5-watcher -----------------------------
if [ ! -x "$PROGRAM_PATH" ]; then
    echo "ERROR: program '$PROGRAM_PATH' is missing or not executable" >&2
    exit 1
fi

# ---------- Блок запуска программы с аргументами -----------------------------
echo "Starting: $PROGRAM_PATH $CONFIG_PATH $HISTORY_FILE"
exec "$PROGRAM_PATH" "$CONFIG_PATH" "$HISTORY_FILE"
