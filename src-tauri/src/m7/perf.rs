//! M9 性能记录（Q210、Q217）。
//!
//! 只有数值、固定词表和匿名的进程信息；不记录文件正文、diff、路径原文、命令输出或密钥。
//! 记录器空闲时不跑循环：帧与交互由前端在用户交互后采样，这里只保存前端送来的聚合结果；
//! 进程 CPU 与内存每 5 秒采样一次，只保留最近 30 分钟（Q210）。

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

/// 进程采样间隔与保留时长（Q210）。
pub const SYSTEM_SAMPLE_INTERVAL: Duration = Duration::from_secs(5);
pub const SYSTEM_RETENTION_MS: u64 = 30 * 60 * 1000;
/// 打开 / 切换仓库的分段耗时只保留最近这些次。
const OPEN_RECORD_CAPACITY: usize = 20;
/// 监视刷新按分钟分桶，保留最近 30 分钟。
const WATCH_BUCKET_CAPACITY: usize = 30;

/// 一帧耗时统计（由前端在交互结束后聚合）。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FrameStats {
    /// `diff-scroll` / `log-scroll` / `window-resize` / `other`。
    pub scenario: String,
    pub count: u64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
    pub over_16_ms: u64,
    pub over_33_ms: u64,
}

/// 交互延迟统计（Chromium Event Timing）。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InteractionStats {
    /// `tab` / `file` / `popover` / `other`。
    pub category: String,
    pub count: u64,
    pub p95_ms: f64,
    pub max_ms: f64,
}

/// 长任务（Chromium `longtask`；WebKit 用超过 100ms 的帧间隔代替）。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LongTaskStats {
    pub count: u64,
    pub total_ms: f64,
    pub longest_ms: Vec<f64>,
    /// 是否由帧间隔推算（WebKit 没有 longtask）。
    pub from_frame_gaps: bool,
}

/// 屏幕与外观（Windows 电脑配置的一部分；分辨率与缩放由前端提供）。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    pub screen_width: u32,
    pub screen_height: u32,
    pub device_pixel_ratio: f64,
    /// `mica` / `css` / `solid` / `native`。
    pub window_material: String,
    /// 前端报告的 UA 平台（`navigator.platform`）；只用于区分 WebView。
    pub webview_platform: String,
}

/// 打开或切换仓库的分段耗时（Q211）。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OpenRecord {
    /// 绑定的 worktree ID（S10：防止多 worktree 切换时迟到数据串台）。
    pub worktree_id: Option<String>,
    /// 本次运行第一次打开这个 worktree，还是切换到已打开过的。
    pub first_open: bool,
    /// 改动文件数；文件列表到达前为 None。
    pub changed_files: Option<u32>,
    /// 前端发起打开请求到后端开始处理（IPC 排队）。
    pub trigger_ms: Option<f64>,
    /// 会话就绪（含环境捕获与读取 Git 版本）。
    pub session_ready_ms: Option<f64>,
    /// 其中的环境捕获耗时。
    pub env_capture_ms: Option<f64>,
    /// 其中的读取 Git 版本耗时。
    pub git_version_ms: Option<f64>,
    /// 文件列表数据到达。
    pub list_data_ms: Option<f64>,
    /// 文件列表首帧绘制。
    pub list_paint_ms: Option<f64>,
    /// 日志首屏绘制；日志还没打开时为 None。
    pub log_paint_ms: Option<f64>,
    pub started_at_ms: u64,
}

/// 单个进程的 CPU 与内存采样（S10：规格要求的“各进程各自数值”）。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProcessSampleItem {
    pub pid: u32,
    pub cpu_percent: f64,
    pub rss_bytes: u64,
}

/// 进程 CPU 与内存的一次采样。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SystemSample {
    pub at_ms: u64,
    /// 主进程 CPU 百分比（0–100 的百分比，单核为 100）。
    pub main_cpu_percent: f64,
    pub main_rss_bytes: u64,
    /// 全部子进程的合计。Windows 上是 WebView2；macOS 的 WebKit 进程由 launchd 拉起，不是 Revia 的子进程，这里通常为空（规格只要求 macOS 统计主进程）。
    pub children_cpu_percent: f64,
    pub children_rss_bytes: u64,
    pub child_count: u32,
    /// 各进程各自数值（主进程与各子进程分别记录）。
    pub processes: Vec<ProcessSampleItem>,
}

/// 监视触发的刷新次数（按最近 30 分钟分桶）。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WatchStats {
    pub total: u64,
    /// 被忽略路径的事件（已跳过，不刷新）。
    pub ignored: u64,
    /// `.git` 内部（index、HEAD、refs）。
    pub git_internal: u64,
    /// 普通文件。
    pub normal: u64,
    /// 最近 30 分钟每分钟的刷新次数，旧到新。
    pub per_minute: Vec<u64>,
}

/// 监视事件的分类（`watch.rs` 判定后上报）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchEventKind {
    Ignored,
    GitInternal,
    Normal,
}

#[derive(Debug, Default)]
struct State {
    frames: Vec<FrameStats>,
    interactions: Vec<InteractionStats>,
    long_tasks: LongTaskStats,
    display: DisplayInfo,
    opens: VecDeque<OpenRecord>,
    samples: VecDeque<SystemSample>,
    watch: WatchStats,
    watch_bucket_started_ms: u64,
    cpu: Option<CpuBaseline>,
}

/// CPU 百分比需要两次采样的差值；保存上一次的累计时间。
#[derive(Debug, Clone, Default)]
struct CpuBaseline {
    at_ms: u64,
    main_cpu_seconds: f64,
    children_cpu_seconds: f64,
    pid_seconds: std::collections::HashMap<u32, f64>,
}

#[derive(Debug, Default)]
pub struct PerfStore {
    state: Mutex<State>,
}

fn now_ms() -> u64 {
    super::now_ms()
}

impl PerfStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// 前端送来的帧、交互和长任务聚合。只保留最近一次（聚合值本身是累计的）。
    pub fn apply_client_report(&self, report: ClientReport) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        match report {
            ClientReport::Frames { scenarios } => {
                state.frames = scenarios
                    .into_iter()
                    .filter(|stats| !stats.scenario.is_empty() && stats.scenario.len() <= 32)
                    .take(8)
                    .collect();
            }
            ClientReport::Interactions { categories } => {
                state.interactions = categories
                    .into_iter()
                    .filter(|stats| !stats.category.is_empty() && stats.category.len() <= 32)
                    .take(8)
                    .collect();
            }
            ClientReport::LongTasks {
                count,
                total_ms,
                longest_ms,
                from_frame_gaps,
            } => {
                state.long_tasks = LongTaskStats {
                    count,
                    total_ms,
                    longest_ms: longest_ms.into_iter().take(10).collect(),
                    from_frame_gaps,
                };
            }
            ClientReport::Display(mut display) => {
                display.window_material = known_material(&display.window_material);
                display.webview_platform = display.webview_platform.chars().take(32).collect();
                state.display = display;
            }
            ClientReport::RepoPaints {
                worktree_id,
                list_data_ms,
                list_paint_ms,
                log_paint_ms,
                changed_files,
                trigger_ms,
            } => {
                // S10：迟到结果严格匹配同一个 worktree 最近一条未完成记录。
                if let Some(record) = state
                    .opens
                    .iter_mut()
                    .rev()
                    .find(|record| !record_settled(record) && (record.worktree_id.is_none() || record.worktree_id.as_deref() == Some(&worktree_id)))
                {
                    record.worktree_id = Some(worktree_id);
                    record.list_data_ms = Some(list_data_ms.max(0.0));
                    record.list_paint_ms = Some(list_paint_ms.max(0.0));
                    record.log_paint_ms = log_paint_ms.map(|value| value.max(0.0));
                    record.changed_files = Some(changed_files);
                    if let Some(trigger) = trigger_ms {
                        record.trigger_ms = Some(trigger.max(0.0));
                    }
                }
            }
        }
    }

    fn begin_open(&self, worktree_id: Option<String>, first_open: bool) -> usize {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.opens.push_back(OpenRecord {
            worktree_id,
            first_open,
            started_at_ms: now_ms(),
            ..OpenRecord::default()
        });
        while state.opens.len() > OPEN_RECORD_CAPACITY {
            state.opens.pop_front();
        }
        state.opens.len() - 1
    }

    /// 记录会话就绪（含环境捕获和读取 Git 版本）。
    pub fn note_session_ready(
        &self,
        worktree_id: Option<String>,
        first_open: bool,
        session_ready_ms: f64,
        env_capture_ms: f64,
        git_version_ms: f64,
    ) {
        let index = self.begin_open(worktree_id, first_open);
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(record) = state.opens.get_mut(index) {
            record.session_ready_ms = Some(session_ready_ms.max(0.0));
            record.env_capture_ms = Some(env_capture_ms.max(0.0));
            record.git_version_ms = Some(git_version_ms.max(0.0));
        }
    }

    /// 前端报告的阶段耗时（触发排队、文件列表数据、绘制）。
    pub fn note_trigger(&self, trigger_ms: f64) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(record) = state.opens.back_mut() {
            record.trigger_ms = Some(trigger_ms.max(0.0));
        }
    }

    pub fn note_system_sample(&self, raw: RawSystemSample) {
        let at_ms = now_ms();
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let (main_cpu, children_cpu, process_items) = match &state.cpu {
            Some(baseline) => {
                let elapsed = (at_ms.saturating_sub(baseline.at_ms)) as f64 / 1000.0;
                if elapsed <= 0.0 {
                    (0.0, 0.0, Vec::new())
                } else {
                    let mut items = Vec::new();
                    for p in &raw.processes {
                        let prev = baseline.pid_seconds.get(&p.pid).copied().unwrap_or(p.cpu_seconds);
                        let percent = (((p.cpu_seconds - prev) / elapsed) * 100.0).clamp(0.0, 100.0 * 64.0);
                        items.push(ProcessSampleItem {
                            pid: p.pid,
                            cpu_percent: round2(percent),
                            rss_bytes: p.rss_bytes,
                        });
                    }
                    (
                        (((raw.main_cpu_seconds - baseline.main_cpu_seconds) / elapsed) * 100.0)
                            .clamp(0.0, 100.0 * 64.0),
                        (((raw.children_cpu_seconds - baseline.children_cpu_seconds) / elapsed)
                            * 100.0)
                            .clamp(0.0, 100.0 * 256.0),
                        items,
                    )
                }
            }
            None => (0.0, 0.0, Vec::new()),
        };
        let mut next_pid_seconds = std::collections::HashMap::new();
        for p in &raw.processes {
            next_pid_seconds.insert(p.pid, p.cpu_seconds);
        }
        state.cpu = Some(CpuBaseline {
            at_ms,
            main_cpu_seconds: raw.main_cpu_seconds,
            children_cpu_seconds: raw.children_cpu_seconds,
            pid_seconds: next_pid_seconds,
        });
        state.samples.push_back(SystemSample {
            at_ms,
            main_cpu_percent: round2(main_cpu),
            main_rss_bytes: raw.main_rss_bytes,
            children_cpu_percent: round2(children_cpu),
            children_rss_bytes: raw.children_rss_bytes,
            child_count: raw.child_count,
            processes: process_items,
        });
        while state
            .samples
            .front()
            .is_some_and(|sample| at_ms.saturating_sub(sample.at_ms) > SYSTEM_RETENTION_MS)
        {
            state.samples.pop_front();
        }
    }

    /// 监视事件：更新计数与每分钟分桶。
    pub fn note_watch_event(&self, kind: WatchEventKind) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let at_ms = now_ms();
        let bucket_ms = 60_000;
        if state.watch_bucket_started_ms == 0 {
            state.watch_bucket_started_ms = at_ms;
            state.watch.per_minute = vec![0];
        }
        let elapsed_buckets = at_ms.saturating_sub(state.watch_bucket_started_ms) / bucket_ms;
        for _ in 0..elapsed_buckets.min(WATCH_BUCKET_CAPACITY as u64) {
            state.watch.per_minute.push(0);
            state.watch_bucket_started_ms += bucket_ms;
        }
        while state.watch.per_minute.len() > WATCH_BUCKET_CAPACITY {
            state.watch.per_minute.remove(0);
        }
        if let Some(last) = state.watch.per_minute.last_mut() {
            *last += 1;
        }
        state.watch.total += 1;
        match kind {
            WatchEventKind::Ignored => state.watch.ignored += 1,
            WatchEventKind::GitInternal => state.watch.git_internal += 1,
            WatchEventKind::Normal => state.watch.normal += 1,
        }
    }

    /// 诊断信息里的 `performance` 一节。只有数值和固定词表。
    pub fn diagnostics_json(&self) -> serde_json::Value {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let samples_kept = &state.samples;
        let cpu_values: Vec<f64> = samples_kept
            .iter()
            .map(|sample| sample.main_cpu_percent + sample.children_cpu_percent)
            .collect();
        let peak_cpu = cpu_values.iter().cloned().fold(0.0, f64::max);
        let average_cpu = if cpu_values.is_empty() {
            0.0
        } else {
            cpu_values.iter().sum::<f64>() / cpu_values.len() as f64
        };
        let peak_rss = samples_kept
            .iter()
            .map(|sample| sample.main_rss_bytes + sample.children_rss_bytes)
            .max()
            .unwrap_or(0);
        serde_json::json!({
            "frames": state.frames,
            "interactions": state.interactions,
            "longTasks": state.long_tasks,
            "display": state.display,
            "opens": state.opens.iter().map(open_entry).collect::<Vec<_>>(),
            "processes": {
                "samples": samples_kept.len(),
                "windowMinutes": SYSTEM_RETENTION_MS / 60_000,
                "averageCpuPercent": round2(average_cpu),
                "peakCpuPercent": round2(peak_cpu),
                "peakRssBytes": peak_rss,
                "last": samples_kept.back(),
            },
            "watch": {
                "total": state.watch.total,
                "ignored": state.watch.ignored,
                "gitInternal": state.watch.git_internal,
                "normal": state.watch.normal,
                "perMinute": state.watch.per_minute,
            },
        })
    }
}

fn record_settled(record: &OpenRecord) -> bool {
    record.list_paint_ms.is_some()
}

fn open_entry(record: &OpenRecord) -> serde_json::Value {
    serde_json::json!({
        "firstOpen": record.first_open,
        "changedFiles": record.changed_files,
        "triggerMs": record.trigger_ms.map(round2),
        "sessionReadyMs": record.session_ready_ms.map(round2),
        "envCaptureMs": record.env_capture_ms.map(round2),
        "gitVersionMs": record.git_version_ms.map(round2),
        "listDataMs": record.list_data_ms.map(round2),
        "listPaintMs": record.list_paint_ms.map(round2),
        "logPaintMs": record.log_paint_ms.map(round2),
    })
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn known_material(value: &str) -> String {
    match value {
        "mica" | "css" | "solid" | "native" => value.to_string(),
        _ => "unknown".into(),
    }
}

/// 前端送来的性能报告（固定字段，前端不能提交任意结构）。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ClientReport {
    Frames {
        scenarios: Vec<FrameStats>,
    },
    Interactions {
        categories: Vec<InteractionStats>,
    },
    #[serde(rename_all = "camelCase")]
    LongTasks {
        count: u64,
        total_ms: f64,
        longest_ms: Vec<f64>,
        from_frame_gaps: bool,
    },
    Display(DisplayInfo),
    #[serde(rename_all = "camelCase")]
    RepoPaints {
        worktree_id: String,
        list_data_ms: f64,
        list_paint_ms: f64,
        log_paint_ms: Option<f64>,
        changed_files: u32,
        #[serde(default)]
        trigger_ms: Option<f64>,
    },
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessRawItem {
    pub pid: u32,
    pub cpu_seconds: f64,
    pub rss_bytes: u64,
}

/// 一次进程采样的原始值（CPU 为累计秒数，由两个样本算差值）。
#[derive(Debug, Clone, Default)]
pub struct RawSystemSample {
    pub main_pid: u32,
    pub main_cpu_seconds: f64,
    pub main_rss_bytes: u64,
    pub children_cpu_seconds: f64,
    pub children_rss_bytes: u64,
    pub child_count: u32,
    pub processes: Vec<ProcessRawItem>,
}

/// 采样本进程与子进程的 CPU / 内存（集中在 platform::system，不启动任何子进程，S3/T1）。
pub fn sample_system() -> Option<RawSystemSample> {
    let snapshot = crate::platform::system::sample_process_tree()?;
    let mut processes = Vec::new();
    processes.push(ProcessRawItem {
        pid: snapshot.main.pid,
        cpu_seconds: snapshot.main.cpu_seconds,
        rss_bytes: snapshot.main.rss_bytes,
    });
    let mut children_cpu_seconds = 0.0;
    let mut children_rss_bytes = 0;
    for child in &snapshot.children {
        children_cpu_seconds += child.cpu_seconds;
        children_rss_bytes += child.rss_bytes;
        processes.push(ProcessRawItem {
            pid: child.pid,
            cpu_seconds: child.cpu_seconds,
            rss_bytes: child.rss_bytes,
        });
    }
    Some(RawSystemSample {
        main_pid: snapshot.main.pid,
        main_cpu_seconds: snapshot.main.cpu_seconds,
        main_rss_bytes: snapshot.main.rss_bytes,
        children_cpu_seconds,
        children_rss_bytes,
        child_count: snapshot.children.len() as u32,
        processes,
    })
}

/// Windows 电脑配置（Q217）。其他平台返回基础信息。
pub fn machine_info() -> serde_json::Value {
    let mut info = serde_json::json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "cpuModel": crate::platform::system::platform_cpu_model(),
        "gpu": crate::platform::system::platform_gpu(),
        "osVersion": crate::platform::system::platform_os_version(),
        "webviewVersion": crate::platform::system::platform_webview_version(),
    });
    info["installShape"] = serde_json::Value::String(
        if crate::distribution::Distribution::current().is_portable() {
            "portable"
        } else {
            "installed"
        }
        .into(),
    );
    info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_frame_report_replaces_previous_values() {
        let store = PerfStore::new();
        store.apply_client_report(ClientReport::Frames {
            scenarios: vec![FrameStats {
                scenario: "diff-scroll".into(),
                count: 10,
                p50_ms: 8.0,
                p95_ms: 16.0,
                max_ms: 20.0,
                over_16_ms: 1,
                over_33_ms: 0,
            }],
        });
        store.apply_client_report(ClientReport::Frames {
            scenarios: vec![FrameStats {
                scenario: "log-scroll".into(),
                count: 2,
                ..Default::default()
            }],
        });
        let json = store.diagnostics_json();
        let frames = json["frames"].as_array().unwrap();
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0]["scenario"], "log-scroll");
    }

    #[test]
    fn unknown_scenario_names_are_dropped() {
        let store = PerfStore::new();
        let long = "x".repeat(64);
        store.apply_client_report(ClientReport::Frames {
            scenarios: vec![
                FrameStats {
                    scenario: "".into(),
                    ..Default::default()
                },
                FrameStats {
                    scenario: long,
                    ..Default::default()
                },
            ],
        });
        assert!(store.diagnostics_json()["frames"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn window_material_is_limited_to_known_values() {
        let store = PerfStore::new();
        store.apply_client_report(ClientReport::Display(DisplayInfo {
            window_material: "rm -rf /".into(),
            screen_width: 100,
            ..Default::default()
        }));
        assert_eq!(store.diagnostics_json()["display"]["windowMaterial"], "unknown");
    }

    #[test]
    fn session_ready_records_phases_for_last_open() {
        let store = PerfStore::new();
        store.note_session_ready(Some("wt".into()), true, 12.0, 9.0, 3.0);
        store.note_trigger(1.5);
        store.apply_client_report(ClientReport::RepoPaints {
            worktree_id: "wt".into(),
            list_data_ms: 40.0,
            list_paint_ms: 55.0,
            log_paint_ms: None,
            changed_files: 7,
            trigger_ms: None,
        });
        let json = store.diagnostics_json();
        let open = &json["opens"][0];
        assert_eq!(open["firstOpen"], true);
        assert_eq!(open["envCaptureMs"], 9.0);
        assert_eq!(open["listPaintMs"], 55.0);
        assert_eq!(open["changedFiles"], 7);
        assert!(open["logPaintMs"].is_null());
    }

    #[test]
    fn watch_events_are_counted_by_kind() {
        let store = PerfStore::new();
        store.note_watch_event(WatchEventKind::Normal);
        store.note_watch_event(WatchEventKind::Ignored);
        store.note_watch_event(WatchEventKind::GitInternal);
        let json = store.diagnostics_json();
        assert_eq!(json["watch"]["total"], 3);
        assert_eq!(json["watch"]["ignored"], 1);
        assert_eq!(json["watch"]["gitInternal"], 1);
        assert_eq!(json["watch"]["normal"], 1);
    }

    #[test]
    fn sample_system_records_processes_and_diffs() {
        let store = PerfStore::new();
        let sample = sample_system();
        assert!(sample.is_some());
        let s = sample.unwrap();
        assert!(s.main_pid > 0);
        store.note_system_sample(s);
        let json = store.diagnostics_json();
        assert_eq!(json["processes"]["samples"], 1);
        let last = &json["processes"]["last"];
        assert!(last["processes"].is_array());
    }
}
