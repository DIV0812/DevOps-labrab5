# =============================================================================
# Makefile для lab5 — сборка watcher (Rust) + установка systemd-сервиса
# и таймера. Цели build/install/uninstall завершены студентом.
# =============================================================================

# ---------- Настройки --------------------------------------------------------
CARGO            := cargo
CARGO_FLAGS      := --release
PROJECT_DIR      := watcher
BUILD_DIR        := build
BINARY_NAME      := lab5-watcher
LAUNCHER_NAME    := lab5-launch.sh
SERVICE_NAME     := lab5-watcher.service
TIMER_NAME       := lab5-watcher.timer
TARGET           := $(PROJECT_DIR)/target/release/$(BINARY_NAME)
SCRIPTS_DIR      := scripts
LAUNCHER_SRC     := $(SCRIPTS_DIR)/watcher-launch.sh
SYSTEMD_UNIT_DIR := /etc/systemd/system
PREFIX           ?= /usr/local
INSTALL_BINDIR    = $(PREFIX)/bin

# Переменные для установки студентом
# Конфигурация обычно хранится по пути /etc/<prog_name>/
# Рабочие файлы длительного хранения обычно располагаются по пути /var/lib/<prog_name>/
CONFIG_PATH  ?= /etc/lab5-watcher/config.toml
HISTORY_FILE ?= /var/lib/lab5-watcher/history.yaml

# -----------------------------------------------------------------------------
# Цель по умолчанию: сборка проекта.
# -----------------------------------------------------------------------------
all: build

# ---------- Сборка -----------------------------------------------------------
# Цель сборки. Зависит от существования файлов, которые необходимо собрать.
build: $(BUILD_DIR)/$(BINARY_NAME) $(BUILD_DIR)/$(LAUNCHER_NAME) $(BUILD_DIR)/$(SERVICE_NAME) $(BUILD_DIR)/$(TIMER_NAME)

# Цель $(TARGET) реализована в качестве примера, но также запускает сборку исполнимой программы.
$(TARGET):
	@echo "=== Building $(BINARY_NAME) ==="
	cd $(PROJECT_DIR) && $(CARGO) build $(CARGO_FLAGS)
	@echo "=== Build complete: target = $(TARGET) ==="

# Собираем lab5-watcher и перемещаем бинарник в папку build.
$(BUILD_DIR)/$(BINARY_NAME): $(TARGET)
	@mkdir -p $(BUILD_DIR)
	@echo "=== Installing build artifact $(BUILD_DIR)/$(BINARY_NAME) ==="
	cp $(TARGET) $@

# Копируем лаунчер в build и прописываем пути через sed.
$(BUILD_DIR)/$(LAUNCHER_NAME): $(LAUNCHER_SRC)
	@mkdir -p $(BUILD_DIR)
	@echo "=== Preparing launcher $(BUILD_DIR)/$(LAUNCHER_NAME) ==="
	cp $(LAUNCHER_SRC) $@
	sed -i -e 's|@CONFIG_PATH@|$(CONFIG_PATH)|g' -e 's|@HISTORY_FILE@|$(HISTORY_FILE)|g' $@
	chmod +x $@

# Создаём юнит сервиса: одноразовый тип, логи в journald, без секции [Install].
$(BUILD_DIR)/$(SERVICE_NAME): $(BUILD_DIR)/$(LAUNCHER_NAME)
	@mkdir -p $(BUILD_DIR)
	@echo "=== Creating systemd unit $(BUILD_DIR)/$(SERVICE_NAME) ==="
	@printf '%s\n' '[Unit]' 'Description=Lab5 watcher: one-shot metrics collector' 'After=multi-user.target' 'Wants=multi-user.target' '' '[Service]' 'Type=oneshot' 'ExecStart=@LAUNCHER_PATH@' 'StandardOutput=journal' 'StandardError=journal' > $@

# Создаём таймер: старт после загрузки, повтор через интервал после прошлого запуска.
$(BUILD_DIR)/$(TIMER_NAME): $(BUILD_DIR)/$(SERVICE_NAME)
	@mkdir -p $(BUILD_DIR)
	@echo "=== Creating systemd timer $(BUILD_DIR)/$(TIMER_NAME) ==="
	@printf '%s\n' '[Unit]' 'Description=Lab5 watcher timer' '' '[Timer]' 'OnBootSec=1min' 'OnUnitActiveSec=2min' 'AccuracySec=1s' 'Unit=$(SERVICE_NAME)' '' '[Install]' 'WantedBy=timers.target' > $@

# ---------- Установка ---------------------------------------------------------
install: build
	@echo "=== Installing $(BINARY_NAME) ==="
	@if [ "$$(id -u)" -ne 0 ]; then echo "ERROR: install needs root, run: sudo make install" >&2; exit 1; fi
	@mkdir -p $(INSTALL_BINDIR)
	cp $(BUILD_DIR)/$(BINARY_NAME) $(INSTALL_BINDIR)/$(BINARY_NAME)
	chmod +x $(INSTALL_BINDIR)/$(BINARY_NAME)
	cp $(BUILD_DIR)/$(LAUNCHER_NAME) $(INSTALL_BINDIR)/$(LAUNCHER_NAME)
	chmod +x $(INSTALL_BINDIR)/$(LAUNCHER_NAME)
	sed -i -e 's|@PROGRAM_PATH@|$(INSTALL_BINDIR)/$(BINARY_NAME)|g' $(INSTALL_BINDIR)/$(LAUNCHER_NAME)
	@echo "=== Installing $(SERVICE_NAME) ==="
	@mkdir -p $(SYSTEMD_UNIT_DIR)
	cp $(BUILD_DIR)/$(SERVICE_NAME) $(SYSTEMD_UNIT_DIR)/$(SERVICE_NAME)
	cp $(BUILD_DIR)/$(TIMER_NAME) $(SYSTEMD_UNIT_DIR)/$(TIMER_NAME)
	sed -i -e 's|@LAUNCHER_PATH@|$(INSTALL_BINDIR)/$(LAUNCHER_NAME)|g' $(SYSTEMD_UNIT_DIR)/$(SERVICE_NAME)
	systemctl daemon-reload
	systemctl enable --now $(TIMER_NAME)
	@echo "=== Install complete ==="
	@echo "  binary    = $(INSTALL_BINDIR)/$(BINARY_NAME)"
	@echo "  launcher  = $(INSTALL_BINDIR)/$(LAUNCHER_NAME)"
	@echo "  service   = $(SYSTEMD_UNIT_DIR)/$(SERVICE_NAME)"
	@echo "  timer     = $(SYSTEMD_UNIT_DIR)/$(TIMER_NAME)"
	@echo "  config    = $(CONFIG_PATH)"
	@echo "  history   = $(HISTORY_FILE)"

# ---------- Удаление ----------------------------------------------------------
uninstall:
	@echo "=== Uninstalling $(SERVICE_NAME) ==="
	@if [ "$$(id -u)" -ne 0 ]; then echo "ERROR: uninstall needs root, run: sudo make uninstall" >&2; exit 1; fi
	-systemctl disable --now $(TIMER_NAME)
	-systemctl stop $(SERVICE_NAME)
	systemctl daemon-reload
	rm -f $(SYSTEMD_UNIT_DIR)/$(SERVICE_NAME) $(SYSTEMD_UNIT_DIR)/$(TIMER_NAME)
	@echo "=== Uninstalling $(BINARY_NAME) ==="
	rm -f $(INSTALL_BINDIR)/$(BINARY_NAME) $(INSTALL_BINDIR)/$(LAUNCHER_NAME)
	rm -rf $(dir $(CONFIG_PATH)) $(dir $(HISTORY_FILE))
	@echo "=== Uninstall complete ==="

# ---------- Очистка репозитория -----------------------------------------------
clean:
	@echo "=== Cleaning repository ==="
	cd $(PROJECT_DIR) && $(CARGO) clean
	cd $(BUILD_DIR) && find . ! -name '.gitkeep' -delete
	@echo "=== Clean complete ==="

.PHONY: all build install uninstall clean
