# =============================================================================
# Makefile для lab5 — сборка watcher (Rust) + установка systemd-сервиса
# Студентам: завершить цели build, install, uninstall.
# Цель clear уже реализована внизу.
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
SYSTEMD_UNIT_DIR := /etc/systemd/system
PREFIX           ?= /usr/local
INSTALL_BINDIR    = $(PREFIX)/bin
TIMER_TEMPLATE   := systemd/lab5-watcher.timer

# Переменные для установки студентом
# Конфигурация обычно хранится по пути /etc/<prog_name>/
# Рабочие файлы длительного хранения обычно располагаются по пути /var/lib/<prog_name>/
CONFIG_PATH  ?= # <Укажите путь>
HISTORY_FILE ?= # <Укажите путь>


# -----------------------------------------------------------------------------
# Цель по умолчанию: сборка проекта.
# -----------------------------------------------------------------------------
all: build

# ---------- Сборка -----------------------------------------------------------
# Цель сборки. Зависит от существования файлов, которые необходимо собрать.
# Для их сборки создайте цели с такими же именами.

build: $(BUILD_DIR)/$(BINARY_NAME) $(BUILD_DIR)/$(LAUNCHER_NAME) $(BUILD_DIR)/$(SERVICE_NAME) $(BUILD_DIR)/$(TIMER_NAME)

# Цель $(TARGET) реализована в качестве примера, но также запускает сборку исполнимой программы.
$(TARGET):
	@echo "=== Building $(BINARY_NAME) ==="
	cd $(PROJECT_DIR) && $(CARGO) build $(CARGO_FLAGS)
	@echo "=== Build complete: target = $(TARGET) ==="

# Создайте цель $(BUILD_DIR)/$(BINARY_NAME).
# Данная цель должна вызвать сборку lab5-watcher,
# а затем переместить его в папку build

# Создайте цель $(BUILD_DIR)/$(LAUNCHER_NAME).
# Данная цель должна скопировать подготовленный вами шаблон watcher-launch.sh в папку build как lab5-launch.sh
# а затем, используя `sed` прописать в него необходимые пути.
# Не забудьте указать путь к watcher-launch.sh как зависимую цель (так как этот файл является зависимостью)

# Создайте цель $(BUILD_DIR)/$(SERVICE_NAME).
# Данная цель должна создать файл юнита systemd и поместить его в папку build.
# Вот задачи, которые должен выполнять этот юнит сервиса:
# Тип: одноразовый. Должен запустить lab5-launcer.sh скрипт.
# Логи должны быть записаны в journald
# Сервис не должен иметь секции установки.
# В описании сервиса можно установить зависимость от достижения цели multi-user.target
# или подобной (достигается после полной загрузки ОС)

# Создайте цель $(BUILD_DIR)/$(TIMER_NAME).
# Данная цель должна создать файл таймера systemd и поместить его в папку build.
# Вот задачи, которые должен выполнять этот таймер:
# Установите запуск с задержкой после Boot.
# Установите перезапуск с пустя некоторое время после последнего запуска.
# Таймер должен запускать сервис.

# ---------- Установка ---------------------------------------------------------
install: build
	@echo "=== Installing $(BINARY_NAME) ==="
	@# Выполните проверку root прав: установка в системные локации, а также работа с systemd требуют root.
	@# Скопируйте программу в INSTALL_BINDIR и убедитесь, что у неё есть права на исполнение.
	@# Скопируйте лаунчер в INSTALL_BINDIR и убедитесь, что у неё есть права на исполнение.
	@# Используя sed, пропишите в лаунчер актуальный путь до программы.
	@echo "=== Installing $(SERVICE_NAME) ==="
	@# Скопируйте systemd unit и systemd timer в SYSTEMD_UNIT_DIR
	@# Используя sed, пропишите в systemd unit актуальный путь до лаунчера.
	@# Перезагрузите daemon systemd (для обновления кеша).
	@# Включите таймер
	@echo "=== Install complete ==="
	@echo "  binary    = $(INSTALL_BINDIR)/$(BINARY_NAME)"
	@echo "  launcher  = $(INSTALL_BINDIR)/$(LAUNCHER_NAME)"
	@echo "  service   = $(SYSTEMD_UNIT_DIR)/$(SERVICE_NAME)"

# ---------- Удаление ----------------------------------------------------------
uninstall:
	@echo "=== Uninstalling $(SERVICE_NAME) ==="
	@# Выполните проверку root прав: установка в системные локации, а также работа с systemd требуют root.
	@# Остановите и отключите таймер, а затем сервис (даже если он не включался до этого).
	@# Не забудьте подавить ошибки, если сервис не запущен/ не найден.
	@# Перезагрузите daemon systemd (для обновления кеша)
	@# Удалите unit и timer из SYSTEMD_UNIT_DIR
	@echo "=== Uninstalling $(BINARY_NAME) ==="
	@# Удалите установленные файлы из INSTALL_BINDIR
	@# Удалите папки программы для конфигурации и истории.
	@echo "=== Uninstall complete ==="

# ---------- Очистка репозитория -----------------------------------------------
clean:
	@echo "=== Cleaning repository ==="
	cd $(PROJECT_DIR) && $(CARGO) clean
	cd $(BUILD_DIR) && find . ! -name '.gitkeep' -delete
	@echo "=== Clean complete ==="

.PHONY: all build install uninstall clean
