//! `bax logs` 子命令。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use bax_core::{
    ClockSource, Connection, LogEvent, MatchState, Phase, Team, Timestamp, Tracker, TrackerConfig,
    Update,
};
use bax_engine::history;
use bax_gamelog::tail::{latest_log_file, list_log_files};
use bax_gamelog::{LogWatcher, LogZone, Origin, WatchItem, discovery};
use bax_store::Store;

pub fn find() -> Result<()> {
    match discovery::find_log_dir() {
        Some(dir) => println!("日志目录：{}", dir.display()),
        None => println!("没有找到日志目录"),
    }
    println!("候选位置：");
    for dir in discovery::candidate_log_dirs() {
        let state = if discovery::contains_logs(&dir) {
            "有日志"
        } else if dir.is_dir() {
            "存在，无日志"
        } else {
            "不存在"
        };
        println!("  [{state}] {}", dir.display());
    }
    Ok(())
}

fn resolve_dir(dir: Option<PathBuf>) -> Result<PathBuf> {
    match dir {
        Some(dir) => Ok(dir),
        None => discovery::find_log_dir().context("没有找到日志目录，请用 --dir 指定"),
    }
}

pub fn replay(
    file: Option<PathBuf>,
    dir: Option<PathBuf>,
    all: bool,
    events: bool,
    tsv: bool,
) -> Result<()> {
    if all {
        return replay_all(&resolve_dir(dir)?, tsv);
    }
    let file = match file {
        Some(file) => file,
        None => {
            let dir = resolve_dir(dir)?;
            latest_log_file(&dir)?.with_context(|| format!("{} 里没有日志", dir.display()))?
        }
    };
    println!("回放 {}", file.display());
    let report = replay_file(&file, events)?;
    for m in &report.matches {
        print_match(m);
    }
    println!(
        "{} 行，{} 个事件，{} 场比赛",
        report.lines,
        report.events,
        report.matches.len()
    );
    Ok(())
}

#[derive(Debug, Default)]
struct FileReport {
    lines: u64,
    events: u64,
    matches: Vec<MatchState>,
    superseded: u64,
    disconnects: u64,
}

/// 把一份日志从头到尾过一遍解析器和对局跟踪器，收集每场比赛的最终状态。
fn replay_file(path: &Path, print_events: bool) -> Result<FileReport> {
    let mut report = FileReport::default();
    let mut last: Option<MatchState> = None;
    let stats = history::replay_file(
        path,
        LogZone::System,
        &TrackerConfig::default(),
        |timed, updates, tracker| {
            if print_events {
                println!("  {:?}", timed.event);
                for update in updates {
                    println!("    -> {update:?}");
                }
            }
            for update in updates {
                match update {
                    Update::MatchDiscovered { .. } => {
                        if let Some(done) = last.take() {
                            report.matches.push(done);
                        }
                    }
                    Update::MatchSuperseded { .. } => report.superseded += 1,
                    Update::Disconnected { .. } => report.disconnects += 1,
                    _ => {}
                }
            }
            if !updates.is_empty() {
                last = tracker.current().cloned();
            }
        },
    )
    .with_context(|| format!("读取 {} 失败", path.display()))?;
    if let Some(done) = last.take() {
        report.matches.push(done);
    }
    report.lines = stats.lines;
    report.events = stats.events;
    Ok(report)
}

fn print_match(m: &MatchState) {
    let alpha = m.roster.iter().filter(|p| p.team == Team::Alpha).count();
    let bravo = m.roster.iter().filter(|p| p.team == Team::Bravo).count();
    let spectators = m.roster.len() - alpha - bravo;
    let duration = match (m.clock, m.times.ended_at) {
        (Some(clock), Some(end)) => {
            let secs = end.millis_since(clock.start_at) / 1000;
            format!("{}:{:02}", secs / 60, secs % 60)
        }
        _ => "未知".to_owned(),
    };
    println!(
        "比赛 {}  {:?}/{:?}{}  地图 {}  名单 {}v{}{}  本机 {}  计时 {}  时长 {}",
        m.match_id,
        m.phase,
        m.connection,
        if m.rejoined { " 重连" } else { "" },
        m.map.as_deref().unwrap_or("?"),
        alpha,
        bravo,
        if spectators > 0 {
            format!("+{spectators} 观战")
        } else {
            String::new()
        },
        match (m.self_id, m.my_team()) {
            (Some(id), Some(team)) => format!("{id} {team:?}"),
            _ => "未识别".to_owned(),
        },
        match m.clock.map(|c| c.source) {
            Some(ClockSource::LogEstimate) => "日志推算",
            Some(ClockSource::Manual) => "手动",
            Some(ClockSource::Report) => "战报",
            None => "无",
        },
        duration,
    );
}

/// 回放整个目录并汇总，用来在大量真实日志上检查解析与识别是否稳定。
fn replay_all(dir: &Path, tsv: bool) -> Result<()> {
    let files = list_log_files(dir)?;
    if files.is_empty() {
        bail!("{} 里没有日志", dir.display());
    }
    let started = Instant::now();
    let mut total = Totals::default();
    if tsv {
        println!("match_id\tphase\trejoined\troster\tclock_start_ms\tended_at_ms\tloaded_at_ms");
    }
    for file in &files {
        match replay_file(file, false) {
            Ok(report) if tsv => {
                for m in &report.matches {
                    let ms = |t: Option<Timestamp>| {
                        t.map(|t| t.as_millis().to_string()).unwrap_or_default()
                    };
                    println!(
                        "{}\t{:?}\t{}\t{}\t{}\t{}\t{}",
                        m.match_id,
                        m.phase,
                        m.rejoined,
                        m.roster.len(),
                        ms(m.clock.map(|c| c.start_at)),
                        ms(m.times.ended_at),
                        ms(m.times.scenario_loaded_at),
                    );
                }
            }
            Ok(report) => total.add(&report),
            Err(err) => {
                total.failed_files += 1;
                eprintln!("{err:#}");
            }
        }
    }
    if !tsv {
        total.print(files.len(), started.elapsed());
    }
    Ok(())
}

#[derive(Debug, Default)]
struct Totals {
    failed_files: u64,
    lines: u64,
    events: u64,
    matches: u64,
    superseded: u64,
    disconnects: u64,
    rejoined: u64,
    with_roster: u64,
    self_found: u64,
    with_clock: u64,
    with_map: u64,
    empty_names: u64,
    roster_players: u64,
    phases: BTreeMap<String, u64>,
    roster_sizes: BTreeMap<String, u64>,
    scenarios: BTreeMap<String, u64>,
    /// 新开的对局（非重连）里有名单、已过加载却没有计时基准的，说明没有等到场景加载完成
    fresh_roster_without_clock: u64,
    durations_min: BTreeMap<u64, u64>,
}

impl Totals {
    fn add(&mut self, report: &FileReport) {
        self.lines += report.lines;
        self.events += report.events;
        self.superseded += report.superseded;
        self.disconnects += report.disconnects;
        for m in &report.matches {
            self.matches += 1;
            self.rejoined += u64::from(m.rejoined);
            let phase = format!(
                "{:?}{}",
                m.phase,
                if m.connection == Connection::Connected {
                    "（仍连接）"
                } else {
                    ""
                }
            );
            *self.phases.entry(phase).or_default() += 1;
            if !m.roster.is_empty() {
                self.with_roster += 1;
                self.self_found += u64::from(m.self_id.is_some());
                self.roster_players += m.roster.len() as u64;
                self.empty_names += m.roster.iter().filter(|p| p.name.is_none()).count() as u64;
                let alpha = m.roster.iter().filter(|p| p.team == Team::Alpha).count();
                let bravo = m.roster.iter().filter(|p| p.team == Team::Bravo).count();
                let spectators = m.roster.len() - alpha - bravo;
                let size = if spectators > 0 {
                    format!("{alpha}v{bravo}+观战")
                } else {
                    format!("{alpha}v{bravo}")
                };
                *self.roster_sizes.entry(size).or_default() += 1;
                if !m.rejoined && m.clock.is_none() && m.phase >= Phase::Deployment {
                    self.fresh_roster_without_clock += 1;
                }
            }
            self.with_clock += u64::from(m.clock.is_some());
            self.with_map += u64::from(m.map.is_some());
            let kind = match m.scenario.as_deref() {
                Some(s) if s.ends_with("_network") => "网络对战",
                Some(s) if s.ends_with("_Skirmish") => "遭遇战",
                Some(_) => "其他场景",
                None => "未知",
            };
            *self.scenarios.entry(kind.to_owned()).or_default() += 1;
            if let (Some(clock), Some(end)) = (m.clock, m.times.ended_at) {
                let minutes = (end.millis_since(clock.start_at) / 60_000).max(0) as u64;
                *self.durations_min.entry(minutes / 10 * 10).or_default() += 1;
            }
        }
    }

    fn print(&self, files: usize, elapsed: Duration) {
        println!(
            "{files} 份日志，{} 行，{} 个事件，用时 {:.1} 秒（{} 份读取失败）",
            self.lines,
            self.events,
            elapsed.as_secs_f64(),
            self.failed_files
        );
        println!(
            "比赛 {} 场：有名单 {}，识别出本机 {}，有计时基准 {}，有地图 {}，重连进入 {}",
            self.matches,
            self.with_roster,
            self.self_found,
            self.with_clock,
            self.with_map,
            self.rejoined
        );
        println!(
            "被新比赛取代 {} 次，断线 {} 次；名单共 {} 人次，其中空名字 {}",
            self.superseded, self.disconnects, self.roster_players, self.empty_names
        );
        println!(
            "新开且有名单、已过加载却没有计时基准：{}",
            self.fresh_roster_without_clock
        );
        print_table("结束时的阶段", &self.phases);
        print_table("名单规模", &self.roster_sizes);
        print_table("场景类型", &self.scenarios);
        let durations: BTreeMap<String, u64> = self
            .durations_min
            .iter()
            .map(|(k, v)| (format!("{k:>3}–{} 分钟", k + 9), *v))
            .collect();
        print_table("对局时长（计时起点到结束）", &durations);
    }
}

fn print_table(title: &str, rows: &BTreeMap<String, u64>) {
    println!("{title}：");
    for (key, count) in rows {
        println!("  {count:>6}  {key}");
    }
}

pub fn watch(dir: Option<PathBuf>) -> Result<()> {
    let dir = resolve_dir(dir)?;
    println!("监视 {}（Ctrl+C 结束）", dir.display());
    let mut watcher = LogWatcher::new(&dir);
    let mut tracker = Tracker::default();
    loop {
        let mut items = Vec::new();
        if let Err(err) = watcher.poll(&mut |item| items.push(item)) {
            eprintln!("读取失败：{err}");
        }
        for item in items {
            match item {
                WatchItem::FileOpened { path, origin } => {
                    println!("打开 {}（{}）", path.display(), origin_name(origin))
                }
                WatchItem::Truncated { path } => println!("文件被截断：{}", path.display()),
                WatchItem::BackfillDone => {
                    println!("补读完成");
                    if let Some(m) = tracker.current() {
                        print_match(m);
                    }
                }
                WatchItem::Event { origin, event } => {
                    let at = event.at.to_utc(LogZone::System);
                    let updates = tracker.apply(at, &event.event);
                    if origin == Origin::Live {
                        print!("[{}] ", event.at);
                        print_live(&event.event, &updates, &tracker);
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(300));
    }
}

fn origin_name(origin: Origin) -> &'static str {
    match origin {
        Origin::Backfill => "补读",
        Origin::Live => "实时",
    }
}

fn print_live(event: &LogEvent, updates: &[Update], tracker: &Tracker) {
    println!("事件 {event:?}");
    for update in updates {
        println!("  -> {update:?}");
    }
    if updates
        .iter()
        .any(|u| matches!(u, Update::RosterReady { .. } | Update::MatchEnded { .. }))
        && let Some(m) = tracker.current()
    {
        print_match(m);
    }
}

pub fn import(data_dir: &Path, dir: Option<PathBuf>, recent: Option<usize>) -> Result<()> {
    let dir = resolve_dir(dir)?;
    let mut files = list_log_files(&dir)?;
    if let Some(recent) = recent {
        let skip = files.len().saturating_sub(recent);
        files.drain(..skip);
    }
    if files.is_empty() {
        bail!("{} 里没有日志", dir.display());
    }
    let store_path = bax_engine::store_path(data_dir);
    let mut store =
        Store::open(&store_path).with_context(|| format!("打不开 {}", store_path.display()))?;
    println!("导入 {} 份日志到 {}", files.len(), store_path.display());

    let started = Instant::now();
    let count = files.len();
    let report = history::import_logs(&mut store, &files, LogZone::System, |index, _| {
        if index % 100 == 0 {
            eprintln!("  {}/{count}", index + 1);
        }
    });
    println!(
        "完成：读了 {} 份，记录 {} 场比赛，用时 {:.1} 秒",
        report.files,
        report.matches,
        started.elapsed().as_secs_f64()
    );
    for (path, reason) in &report.failed {
        eprintln!("  失败 {}：{reason}", path.display());
    }
    if let Some((id, count, total)) = store.most_common_player()? {
        println!("出现最多的玩家 {id}：{count}/{total} 场有名单的比赛");
    }
    Ok(())
}
