//! Модуль конфигурации watcher-а.
//! Поддерживает форматы TOML и YAML. Выбор парсера — по расширению файла.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Пороговые значения для метрик.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Thresholds {
    /// Количество zombie-процессов, при котором срабатывает порог (0 = отключено).
    pub zombie_max: u32,
    /// Максимальная загрузка CPU в процентах (0.0 = отключено).
    pub cpu_max: f32,
    /// Минимальная загрузка CPU в процентах (0.0 = отключено).
    pub cpu_min: f32,
    /// Минимум свободной памяти в МБ (0 = отключено).
    pub mem_free_min: u32,
    /// Максимум занятого swap в МБ (0 = отключено).
    pub swap_max: u32,
    /// Порог использования диска (%), общий для всех ФС (100.0 = отключено).
    pub disk_used_percent: f32,
    /// Порог скорости заполнения дисков (Кб блоки/замер), общий для всех (0 = отключено).
    pub disk_fill_speed: u64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            zombie_max: 0,
            cpu_max: 0.0,
            cpu_min: 0.0,
            mem_free_min: 0,
            swap_max: 0,
            disk_used_percent: 100.0,
            disk_fill_speed: 0,
        }
    }
}

/// Настройки поведения watcher-а.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Settings {
    /// Сохранять последние N замеров в файле истории.
    pub retention_count: u32,
    /// Отправить собранную статистику через wall (false = только алерты).
    pub print_stats: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            retention_count: 5,
            print_stats: false,
        }
    }
}

/// Структура конфигурации watcher-а.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub thresholds: Thresholds,
    #[serde(default)]
    pub settings: Settings,
}

impl Config {
    /// Определяет формат конфигурации по расширению файла.
    fn detect_format(path: &Path) -> FormatType {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
            .as_deref()
        {
            Some("yaml") | Some("yml") => FormatType::Yaml,
            Some("toml") | Some("conf") => FormatType::Toml,
            // Если расширения нет — пробуем TOML, затем YAML.
            None => FormatType::TryTomlThenYaml,
            _ => FormatType::Toml, // fallback по умолчанию.
        }
    }

    /// Загружает конфигурацию из файла.
    pub fn load(path: &str) -> Result<Self, String> {
        let p = Path::new(path);
        if !p.exists() {
            return Err(format!("config file not found: {}", path));
        }

        let content = fs::read_to_string(p).map_err(|e| format!("cannot read '{}': {}", path, e))?;
        let fmt = Self::detect_format(p);

        // Пытаемся распарсить в зависимости от формата.
        match fmt {
            FormatType::Yaml => {
                yaml_serde::from_str(&content).map_err(|e| format!("yaml parse error: {}", e))
            }
            FormatType::Toml => Self::parse_toml(&content),
            FormatType::TryTomlThenYaml => {
                // Пробуем TOML, если не вышло — YAML.
                if let Ok(cfg) = Self::parse_toml(&content) {
                    return Ok(cfg);
                }
                yaml_serde::from_str(&content).map_err(|e| format!("yaml parse error: {}", e))
            }
        }
    }

    /// Парсит TOML-формат конфигурации.
    fn parse_toml(content: &str) -> Result<Self, String> {
        let value: toml::Value = toml::from_str(content).map_err(|e| format!("toml parse error: {}", e))?;
        let table = value.as_table().ok_or("expected a TOML table at the root")?;

        // Вспомогательная функция для извлечения значения из секции.
        let get_from = |section: &toml::Table, key: &str| -> Option<toml::Value> {
            section.get(key).cloned()
        };

        let thresholds_section = table.get("thresholds")
            .and_then(|v| v.as_table())
            .cloned()
            .unwrap_or_default();

        let settings_section = table.get("settings")
            .and_then(|v| v.as_table())
            .cloned()
            .unwrap_or_default();

        let thresholds = Thresholds {
            zombie_max: get_from(&thresholds_section, "zombie_max")
                .and_then(|v| v.as_integer()).map(|i| i as u32).unwrap_or(0),
            cpu_max: Self::get_f64_val(&thresholds_section, "cpu_max").unwrap_or(0.0) as f32,
            cpu_min: Self::get_f64_val(&thresholds_section, "cpu_min").unwrap_or(0.0) as f32,
            mem_free_min: get_from(&thresholds_section, "mem_free_min")
                .and_then(|v| v.as_integer()).map(|i| i as u32).unwrap_or(0),
            swap_max: get_from(&thresholds_section, "swap_max")
                .and_then(|v| v.as_integer()).map(|i| i as u32).unwrap_or(0),
            disk_used_percent: Self::get_f64_val(&thresholds_section, "disk_used_percent").unwrap_or(100.0) as f32,
            disk_fill_speed: get_from(&thresholds_section, "disk_fill_speed")
                .and_then(|v| v.as_integer()).map(|i| i as u64).unwrap_or(0),
        };

        let settings = Settings {
            retention_count: get_from(&settings_section, "retention_count")
                .and_then(|v| v.as_integer()).map(|i| i as u32).unwrap_or(5),
            print_stats: get_from(&settings_section, "print_stats")
                .and_then(|v| v.as_bool()).unwrap_or(false),
        };

        Ok(Config { thresholds, settings })
    }

    /// Извлекает f64 из TOML-секции по ключу.
    fn get_f64_val(section: &toml::Table, key: &str) -> Option<f64> {
        section.get(key).and_then(|v| v.as_integer().map(|i| i as f64))
            .or_else(|| section.get(key).and_then(|v| v.as_float()))
    }

    /// Сохраняет конфигурацию в YAML-формате (для удобства редактирования студентами).
    pub fn save_yaml(&self, path: &str) -> Result<(), String> {
        let content = yaml_serde::to_string(self).map_err(|e| format!("yaml serialization error: {}", e))?;
        fs::write(path, content).map_err(|e| format!("cannot write '{}': {}", path, e))
    }

    /// Сохраняет конфигурацию в TOML-формате.
    pub fn save_toml(&self, path: &str) -> Result<(), String> {
        let value = toml::to_string(self).map_err(|e| format!("toml serialization error: {}", e))?;
        fs::write(path, value).map_err(|e| format!("cannot write '{}': {}", path, e))
    }
}

/// Тип определения формата конфигурации.
enum FormatType {
    Yaml,
    Toml,
    TryTomlThenYaml,
}
