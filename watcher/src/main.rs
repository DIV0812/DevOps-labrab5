mod config;

use config::Config;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// Одна запись истории (сохраняется в файл как YAML-список).
#[derive(Serialize, Deserialize, Debug)]
pub struct HistoryEntry {
    /// UTC timestamp в секундах.
    pub timestamp: u64,
    /// Снимок метрик.
    pub metrics: MetricsSnapshot,
}

/// Снимок процессов.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ProcessesSnapshot {
    /// Суммарное количество процессов.
    pub total: u32,
    /// Количество zombie-процессов.
    pub zombie: u32,
}

/// Снимок оперативной памяти (все значения в МБ).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemorySnapshot {
    /// Суммарный объём памяти в МБ.
    pub total: u32,
    /// Используемая память в МБ.
    pub used: u32,
    /// Свободная память в МБ.
    pub free: u32,
    /// Закешированная память в МБ (включая буферы).
    pub cached: u32,
    /// Полный объём файла подкачки в МБ.
    pub swap_total: u32,
    /// Используемый объём файла подкачки в МБ.
    pub swap_used: u32,
}

/// Снимок метрик за один момент времени.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MetricsSnapshot {
    /// Снимок процессов.
    pub ps: ProcessesSnapshot,
    /// Карта загруженности процессоров (id → %).
    pub cpus: BTreeMap<u32, f32>,
    /// Снимок памяти.
    pub memory: MemorySnapshot,
    /// Метрики дисков: mount_point → диск.
    pub disk: BTreeMap<String, DiskSnapshot>,
}

impl MetricsSnapshot {
    fn default_snapshot() -> Self {
        Self {
            ps: ProcessesSnapshot { total: 0, zombie: 0 },
            cpus: BTreeMap::new(),
            memory: MemorySnapshot { total: 0, used: 0, free: 0, cached: 0, swap_total: 0, swap_used: 0 },
            disk: BTreeMap::new(),
        }
    }
    fn default_dev() -> Self {
        Self::default_snapshot()
    }
}

/// Снимок метрик одного диска (объёмы в 1Кб блоках).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DiskSnapshot {
    /// Общее количество 1Кб блоков.
    pub total: u64,
    /// Количество занятых 1Кб блоков.
    pub used: u64,
    /// Количество свободных 1Кб блоков.
    pub avail: u64,
    /// Процент использования (0..100).
    #[serde(rename = "use")]
    pub usage: f32,
}

/// История — список записей.
#[derive(Serialize, Deserialize, Debug)]
pub struct HistoryData {
    pub data: Vec<HistoryEntry>,
}

impl Default for HistoryData {
    fn default() -> Self {
        Self {
            data: Vec::new(),
        }
    }
}

/// Считывает количество процессов путём подсчёта папок в /proc.
fn get_process_counts() -> ProcessesSnapshot {
    let mut total: u32 = 0;
    let mut zombie: u32 = 0;

    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            // Проверяем, что имя — число (PID).
            if name_str.parse::<u32>().is_err() {
                continue;
            }
            total += 1;

            // Читаем /proc/<PID>/stat для определения статуса.
            let stat_path = entry.path().join("stat");
            if let Ok(stat_content) = fs::read_to_string(&stat_path) {
                // Формат: pid (comm) state ...
                // state — третье поле (после comm в скобках).
                // Упрощённый парсинг: ищем закрывающую скобку, затем следующее поле.
                if let Some(after_comm) = stat_content.find(')') {
                    let rest = &stat_content[after_comm + 1..].trim();
                    let first_char = rest.chars().next();
                    if first_char == Some('Z') {
                        zombie += 1;
                    }
                }
            }
        }
    }

    ProcessesSnapshot { total, zombie }
}

/// Считывает загрузку CPU из /proc/stat. Делает два снимка с интервалом 1 сек.
fn get_cpu_usages() -> BTreeMap<u32, f32> {
    let parse_cpu_line = |line: &str| -> Option<(u64, u64)> {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 10 { return None; }
        // Поля: user nice system idle iowait irq softirq steal
        let user: u64 = fields[1].parse().ok()?;
        let nice: u64 = fields[2].parse().ok()?;
        let system: u64 = fields[3].parse().ok()?;
        let idle: u64 = fields[4].parse().ok()?;
        let iowait: u64 = fields[5].parse().ok()?;
        let irq: u64 = fields[6].parse().ok()?;
        let softirq: u64 = fields[7].parse().ok()?;
        let steal: u64 = fields[8].parse().ok()?;
        let total = user + nice + system + idle + iowait + irq + softirq + steal;
        let busy = total - idle - iowait;
        Some((total, busy))
    };

    let content = match fs::read_to_string("/proc/stat") {
        Ok(c) => c,
        Err(_) => return BTreeMap::new(),
    };

    // Сохраняем все строки первого снимка в карту: CPU id → (total, busy).
    let mut first_snap: BTreeMap<u32, (u64, u64)> = BTreeMap::new();

    for line in content.lines() {
        if !line.starts_with("cpu") || line.trim_start().starts_with("cpu ") {
            continue; // пропускаем общую строку "cpu ".
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 10 { continue; }
        if let Ok(id) = fields[0].trim_start_matches("cpu").parse::<u32>() {
            if let Some(tb) = parse_cpu_line(line) {
                first_snap.insert(id, tb);
            }
        }
    }

    // Общий CPU (первая строка "cpu ").
    let cpu_line = content.lines().next();
    let first_total = cpu_line.and_then(parse_cpu_line);

    std::thread::sleep(Duration::from_secs(1));

    let content2 = match fs::read_to_string("/proc/stat") {
        Ok(c) => c,
        Err(_) => return BTreeMap::new(),
    };

    // Второй снимок: общий CPU.
    let second_line = content2.lines().next();
    let second_total = second_line.and_then(parse_cpu_line);

    let mut result: BTreeMap<u32, f32> = BTreeMap::new();

    // Общий CPU (id 0).
    if let (Some((t1, b1)), Some((t2, b2))) = (first_total, second_total) {
        let dt = ((t2 as i64) - (t1 as i64)).max(1) as f64;
        let db = ((b2 as i64) - (b1 as i64)).max(0) as f64;
        let pct = (db / dt) * 100.0;
        result.insert(0, pct.clamp(0.0, 100.0) as f32);
    }

    // Отдельные CPU (cpu0, cpu1, ...) — вычисляем дельту для каждого.
    for line in content2.lines() {
        if !line.starts_with("cpu") || line.trim_start().starts_with("cpu ") {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 10 { continue; }
        if let Ok(id) = fields[0].trim_start_matches("cpu").parse::<u32>() {
            if let Some((t1, b1)) = first_snap.get(&id) {
                if let Some((t2, b2)) = parse_cpu_line(line) {
                    let dt = ((t2 as i64) - (*t1 as i64)).max(1) as f64;
                    let db = ((b2 as i64) - (*b1 as i64)).max(0) as f64;
                    let pct = (db / dt) * 100.0;
                    result.insert(id, pct.clamp(0.0, 100.0) as f32);
                }
            }
        }
    }

    result
}

/// Считывает /proc/meminfo и возвращает метрики памяти в МБ.
fn get_memory_info() -> MemorySnapshot {
    let content = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mut info: BTreeMap<&str, u64> = BTreeMap::new();

    for line in content.lines() {
        if let Some((key, value)) = line.split_once(':') {
            if let Some(kb_str) = value.trim().split_whitespace().next() {
                if let Ok(kb) = kb_str.parse::<u64>() {
                    info.insert(key.trim(), kb);
                }
            }
        }
    }

    let mem_total_kb = *info.get("MemTotal").unwrap_or(&0);
    let mem_free = *info.get("MemFree").unwrap_or(&0);
    let mem_available = *info.get("MemAvailable").unwrap_or(&0);
    let buffers = *info.get("Buffers").unwrap_or(&0);
    let cached = *info.get("Cached").unwrap_or(&0);
    let swap_total_kb = *info.get("SwapTotal").unwrap_or(&0);
    let swap_free = info.get("SwapFree").copied().unwrap_or(swap_total_kb);

    // Переводим в МБ (1 МБ = 1024 КБ).
    // cached включает buffers.
    let effective_cached = cached + buffers;

    let mem_used = if mem_available > 0 {
        mem_total_kb - mem_available
    } else {
        mem_total_kb - mem_free
    };
    let swap_used = swap_total_kb.saturating_sub(swap_free);

    MemorySnapshot {
        total: (mem_total_kb / 1024) as u32,
        used: (mem_used / 1024) as u32,
        free: (mem_free / 1024) as u32,
        cached: (effective_cached / 1024) as u32,
        swap_total: (swap_total_kb / 1024) as u32,
        swap_used: (swap_used / 1024) as u32,
    }
}

/// Считает список точек монтирования локальных ФС, у которых device начинается с /dev.
fn get_filesystem_names() -> Vec<String> {
    let output = match Command::new("df")
        .args(&["--output=source,target", "--local"])
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return Vec::new(),
    };

    output
        .lines()
        .skip(1) // пропускаем заголовок.
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() >= 2 && fields[0].starts_with("/dev") {
                Some(fields[1].trim().to_string())
            } else {
                None
            }
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// Считывает метрики одной ФС из df (возвращает в 1Кб блоках).
fn get_disk_snapshot(mount_point: &str) -> DiskSnapshot {
    let output = Command::new("df")
        .args(&["--output=size,used,avail,pcent", "--block-size=1024"])
        .arg(mount_point)
        .output();

    let output = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return DiskSnapshot { total: 0, used: 0, avail: 0, usage: 0.0 },
    };

    let Some(line) = output.lines().nth(1) else {
        return DiskSnapshot { total: 0, used: 0, avail: 0, usage: 0.0 };
    }; // пропускаем заголовок df.

    let fields: Vec<&str> = line.trim().split_whitespace().collect();
    if fields.len() >= 4 {
        let total: u64 = fields[0].parse().unwrap_or(0);
        let used: u64 = fields[1].parse().unwrap_or(0);
        let avail: u64 = fields[2].parse().unwrap_or(0);
        let usage_f32 = fields[3].trim_end_matches('%').parse::<f32>().unwrap_or(0.0);

        return DiskSnapshot { total, used, avail, usage: usage_f32 };
    }

    DiskSnapshot { total: 0, used: 0, avail: 0, usage: 0.0 }
}

/// Собирает все системные метрики за один замер.
pub fn collect_metrics() -> Result<MetricsSnapshot, String> {
    let ps = get_process_counts();
    let cpus = get_cpu_usages();
    let memory = get_memory_info();

    let disks = get_filesystem_names();
    let mut disk_metrics = BTreeMap::new();
    for fs in &disks {
        let dm = get_disk_snapshot(&fs);
        disk_metrics.insert(fs.clone(), dm);
    }

    Ok(MetricsSnapshot {
        ps,
        cpus,
        memory,
        disk: disk_metrics,
    })
}

/// Вычисляет средние значения и стандартные отклонения по истории.
pub fn calculate_stats(history: &HistoryData) -> (MetricsSnapshot, MetricsSnapshot) {
    let len = history.data.len();
    if len == 0 {
        return (MetricsSnapshot::default_snapshot(), MetricsSnapshot::default_dev());
    }

    // Используем f64 для накопления, чтобы избежать переполнения.
    let mut sum_ps_total: f64 = 0.0;
    let mut sum_ps_zombie: f64 = 0.0;
    let mut sum_mem: (f64, f64, f64, f64, f64, f64) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let mut sum_cpus: BTreeMap<u32, f64> = BTreeMap::new();
    let mut sum_disk: BTreeMap<String, (f64, f64, f64, f64)> = BTreeMap::new();

    for entry in &history.data {
        let m = &entry.metrics;
        sum_ps_total += m.ps.total as f64;
        sum_ps_zombie += m.ps.zombie as f64;

        sum_mem.0 += m.memory.total as f64;
        sum_mem.1 += m.memory.used as f64;
        sum_mem.2 += m.memory.free as f64;
        sum_mem.3 += m.memory.cached as f64;
        sum_mem.4 += m.memory.swap_total as f64;
        sum_mem.5 += m.memory.swap_used as f64;

        for (&id, &pct) in &m.cpus {
            *sum_cpus.entry(id).or_insert(0.0) += pct as f64;
        }
        for (name, dm) in &m.disk {
            let entry_disk = sum_disk.entry(name.clone()).or_insert((0.0, 0.0, 0.0, 0.0));
            entry_disk.0 += dm.total as f64;
            entry_disk.1 += dm.used as f64;
            entry_disk.2 += dm.avail as f64;
            entry_disk.3 += dm.usage as f64;
        }
    }

    // Средние значения.
    let avg = MetricsSnapshot {
        ps: ProcessesSnapshot {
            total: (sum_ps_total / len as f64) as u32,
            zombie: (sum_ps_zombie / len as f64) as u32,
        },
        cpus: sum_cpus.into_iter().map(|(id, v)| (id, (v / len as f64) as f32)).collect(),
        memory: MemorySnapshot {
            total: (sum_mem.0 / len as f64) as u32,
            used: (sum_mem.1 / len as f64) as u32,
            free: (sum_mem.2 / len as f64) as u32,
            cached: (sum_mem.3 / len as f64) as u32,
            swap_total: (sum_mem.4 / len as f64) as u32,
            swap_used: (sum_mem.5 / len as f64) as u32,
        },
        disk: sum_disk.into_iter().map(|(name, v)| {
            (name, DiskSnapshot {
                total: (v.0 / len as f64) as u64,
                used: (v.1 / len as f64) as u64,
                avail: (v.2 / len as f64) as u64,
                usage: (v.3 / len as f64) as f32,
            })
        }).collect(),
    };

    // Стандартные отклонения (по выборке, n-1).
    let mut dev_ps_total: f64 = 0.0;
    let mut dev_ps_zombie: f64 = 0.0;
    let mut dev_mem: (f64, f64, f64, f64, f64, f64) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let mut dev_cpus: BTreeMap<u32, f64> = BTreeMap::new();
    let mut dev_disk: BTreeMap<String, (f64, f64, f64, f64)> = BTreeMap::new();

    for entry in &history.data {
        let m = &entry.metrics;
        dev_ps_total += (m.ps.total as f64 - avg.ps.total as f64).powi(2);
        dev_ps_zombie += (m.ps.zombie as f64 - avg.ps.zombie as f64).powi(2);

        dev_mem.0 += (m.memory.total as f64 - avg.memory.total as f64).powi(2);
        dev_mem.1 += (m.memory.used as f64 - avg.memory.used as f64).powi(2);
        dev_mem.2 += (m.memory.free as f64 - avg.memory.free as f64).powi(2);
        dev_mem.3 += (m.memory.cached as f64 - avg.memory.cached as f64).powi(2);
        dev_mem.4 += (m.memory.swap_total as f64 - avg.memory.swap_total as f64).powi(2);
        dev_mem.5 += (m.memory.swap_used as f64 - avg.memory.swap_used as f64).powi(2);

        for (&id, &pct) in &m.cpus {
            let avg_pct = avg.cpus.get(&id).copied().unwrap_or(0.0);
            let entry = dev_cpus.entry(id).or_insert(0.0);
            *entry += (pct - avg_pct).powi(2) as f64;
        }
        for (name, dm) in &m.disk {
            if let Some(avg_dm) = avg.disk.get(name) {
                let entry_dev = dev_disk.entry(name.clone()).or_insert((0.0, 0.0, 0.0, 0.0));
                entry_dev.0 += (dm.total as f64 - avg_dm.total as f64).powi(2);
                entry_dev.1 += (dm.used as f64 - avg_dm.used as f64).powi(2);
                entry_dev.2 += (dm.avail as f64 - avg_dm.avail as f64).powi(2);
                entry_dev.3 += (dm.usage - avg_dm.usage).powi(2) as f64;
            }
        }
    }

    let scale = if len > 1 { 1.0 / ((len - 1) as f64) } else { 1.0 };
    let dev_result = MetricsSnapshot {
        ps: ProcessesSnapshot {
            total: (dev_ps_total * scale).sqrt() as u32,
            zombie: (dev_ps_zombie * scale).sqrt() as u32,
        },
        cpus: dev_cpus.into_iter().map(|(id, v)| (id, (v * scale).sqrt() as f32)).collect(),
        memory: MemorySnapshot {
            total: (dev_mem.0 * scale).sqrt() as u32,
            used: (dev_mem.1 * scale).sqrt() as u32,
            free: (dev_mem.2 * scale).sqrt() as u32,
            cached: (dev_mem.3 * scale).sqrt() as u32,
            swap_total: (dev_mem.4 * scale).sqrt() as u32,
            swap_used: (dev_mem.5 * scale).sqrt() as u32,
        },
        disk: dev_disk.into_iter().map(|(name, v)| {
            (name, DiskSnapshot {
                total: (v.0 * scale).sqrt() as u64,
                used: (v.1 * scale).sqrt() as u64,
                avail: (v.2 * scale).sqrt() as u64,
                usage: (v.3 * scale).sqrt() as f32,
            })
        }).collect(),
    };

    (avg, dev_result)
}

/// Разбивает текст на строки макс. 79 символов и отправляет через wall, по 20 строк за вызов.
pub fn send_wall(msg: &str) {
    let max_line_len = 79;
    let max_lines_per_call = 20;

    // Разбиваем текст на строки по max_line_len символов.
    let mut lines: Vec<String> = Vec::new();
    for line in msg.lines() {
        if line.len() <= max_line_len {
            lines.push(line.to_string());
        } else {
            // Разбиваем длинную строку на части.
            for chunk in line.as_bytes().chunks(max_line_len) {
                if let Ok(s) = String::from_utf8(chunk.to_vec()) {
                    lines.push(s);
                }
            }
        }
    }

    // Отправляем порциями по max_lines_per_call строк.
    for chunk in lines.chunks(max_lines_per_call) {
        let text: String = chunk.join("\n");
        if text.is_empty() { continue; }

        let child = Command::new("wall")
            .stdin(std::process::Stdio::piped())
            .spawn();

        if let Ok(mut ch) = child {
            if let Some(mut stdin) = ch.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
                let _ = stdin.flush();
            }
            let _ = ch.wait();
        }
    }
}

/// Сохраняет историю в YAML-файл.
fn save_history(path: &Path, data: &HistoryData) {
    match yaml_serde::to_string(data) {
        Ok(content) => {
            if let Err(e) = fs::write(path, content) {
                eprintln!("Failed to save history to '{}': {}", path.display(), e);
            }
        }
        Err(e) => {
            eprintln!("Failed to serialize history: {}", e);
        }
    }
}

/// Загружает историю из YAML-файла.
fn load_history(path: &Path) -> HistoryData {
    match fs::read_to_string(path) {
        Ok(content) => {
            if let Ok(data) = yaml_serde::from_str::<HistoryData>(&content) {
                return data;
            }
            // Если не удалось десериализовать — создаём пустой файл.
            let _ = fs::write(path, "[]\n");
            HistoryData::default()
        }
        Err(_) => HistoryData::default(),
    }
}

/// Формирует строку статистики для wall-отправки.
fn format_stats(metrics: &MetricsSnapshot) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(format!("procs total={} zombie={}", metrics.ps.total, metrics.ps.zombie));

    // Процессоры.
    if !metrics.cpus.is_empty() {
        let cpu_strs: Vec<String> = metrics.cpus.iter()
            .map(|(id, pct)| format!("cpu{}={:.2}%", id, *pct as f64))
            .collect();
        parts.push(format!("cpu {}", cpu_strs.join(" ")));
    }

    // Память (в МБ).
    parts.push(format!(
        "mem total={}MB used={}MB free={}MB cached={}MB swap_total={}MB swap_used={}MB",
        metrics.memory.total, metrics.memory.used, metrics.memory.free,
        metrics.memory.cached, metrics.memory.swap_total, metrics.memory.swap_used
    ));

    // Диски.
    if !metrics.disk.is_empty() {
        let disk_strs: Vec<String> = metrics.disk.iter()
            .map(|(name, dm)| format!("{} {:.2}%", name, dm.usage))
            .collect();
        parts.push(format!("disk {}", disk_strs.join(" ")));
    }

    parts.join("; ")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <config_path> <history_file>", args[0]);
        eprintln!("  config_path   — путь к конфигурационному файлу (TOML или YAML)");
        eprintln!("  history_file  — путь к файлу истории метрик (YAML)");
        std::process::exit(1);
    }

    let config_path = &args[1];
    let history_path = Path::new(&args[2]);

    let config = match Config::load(config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Error loading config from '{}': {}", config_path, e);
            std::process::exit(1);
        }
    };

    // Время запуска.
    let now = chrono::Utc::now();
    eprintln!("[{}] Watcher started", now.format("%Y-%m-%dT%H:%M:%SZ"));

    // Загружаем историю.
    let mut history = load_history(history_path);

    // Собираем метрики (один раз).
    match collect_metrics() {
        Ok(metrics) => {
            let timestamp = now.timestamp() as u64;

            let entry = HistoryEntry {
                timestamp,
                metrics: metrics.clone(),
            };

            history.data.push(entry);

            // Ограничиваем размер истории.
            if history.data.len() > config.settings.retention_count as usize {
                let keep = config.settings.retention_count as usize;
                history.data.drain(..history.data.len() - keep);
            }

            save_history(history_path, &history);

            // Вычисляем средние и отклонения.
            let (avg, _dev) = calculate_stats(&history);

            // Проверяем пороги и отправляем алерты.
                    check_and_alert(
                &metrics,
                &avg,
                &config.thresholds,
                if history.data.len() >= 2 {
                    Some(&history.data[history.data.len() - 2].metrics.disk)
                } else {
                    None
                },
            );

            // Отправляем статистику через wall, если включено.
            if config.settings.print_stats {
                let stats_text = format_stats(&metrics);
                send_wall(&stats_text);
            }
        }
        Err(reason) => {
            eprintln!("[ALERT] Metrics collection failed: {}", reason);
            std::process::exit(2);
        }
    }
}

/// Проверяет текущие метрики против порогов и отправляет алерты.
fn check_and_alert(
    metrics: &MetricsSnapshot,
    avg: &MetricsSnapshot,
    thresholds: &config::Thresholds,
    prev_disk: Option<&BTreeMap<String, DiskSnapshot>>,
) {
    let mut alert_names: Vec<String> = Vec::new();

    // zombie_max — по последнему снимку.
    if thresholds.zombie_max > 0 && metrics.ps.zombie >= thresholds.zombie_max {
        alert_names.push(format!("zombie_max: {} >= {}", metrics.ps.zombie, thresholds.zombie_max));
    }

    // cpu_max/cpu_min — и по среднему, и по последнему снимку (ORA).
    let current_cpu = *metrics.cpus.get(&0).unwrap_or(&0.0);
    let avg_cpu = *avg.cpus.get(&0).unwrap_or(&0.0);

    if thresholds.cpu_max > 0.0 {
        if current_cpu >= thresholds.cpu_max || avg_cpu >= thresholds.cpu_max {
            alert_names.push(format!("cpu_max: curr={:.2}% avg={:.2}%", current_cpu, avg_cpu));
        }
    }

    if thresholds.cpu_min > 0.0 {
        if current_cpu <= thresholds.cpu_min || avg_cpu <= thresholds.cpu_min {
            alert_names.push(format!("cpu_min: curr={:.2}% avg={:.2}%", current_cpu, avg_cpu));
        }
    }

    // mem_free_min — и по среднему, и по последнему снимку (ORA).
    let mem_free_avg = avg.memory.free;
    if thresholds.mem_free_min > 0 {
        if metrics.memory.free <= thresholds.mem_free_min || mem_free_avg <= thresholds.mem_free_min {
            alert_names.push(format!("mem_free_min: current={}MB avg={}MB", metrics.memory.free, mem_free_avg));
        }
    }

    // swap_max — и по среднему, и по последнему снимку (ORA).
    let swap_used_avg = avg.memory.swap_used;
    if thresholds.swap_max > 0 {
        if metrics.memory.swap_used >= thresholds.swap_max || swap_used_avg >= thresholds.swap_max {
            alert_names.push(format!("swap_max: current={}MB avg={}MB", metrics.memory.swap_used, swap_used_avg));
        }
    }

    // disk_used_percent — по последнему снимку, общий порог для всех.
    if thresholds.disk_used_percent < 100.0 {
        for (name, dm) in &metrics.disk {
            if dm.usage >= thresholds.disk_used_percent {
                alert_names.push(format!("disk_used_percent: {} {:.2}% >= {:.2}%", name, dm.usage, thresholds.disk_used_percent));
            }
        }
    }

    // disk_fill_speed — разница между текущим и предыдущим снимком для каждого диска.
    if thresholds.disk_fill_speed > 0 {
        if let Some(prev) = prev_disk {
            for (name, dm) in &metrics.disk {
                let prev_used = prev.get(name).map(|p| p.used).unwrap_or(dm.used);
                let diff = if dm.used > prev_used { dm.used - prev_used } else { 0 };
                if diff >= thresholds.disk_fill_speed {
                    alert_names.push(format!("disk_fill_speed: {} +{} blocks", name, diff));
                }
            }
        }
    }

    // Отправляем алерты в journald.
    if !alert_names.is_empty() {
        let now = chrono::Utc::now();
        for name in &alert_names {
            eprintln!("[ALERT] {}: {}", now.format("%Y-%m-%dT%H:%M:%SZ"), name);
        }
    }
}
