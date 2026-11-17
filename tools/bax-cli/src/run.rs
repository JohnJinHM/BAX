//! `bax run`：无界面运行引擎。

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use bax_core::{Phase, Team};
use bax_engine::{
    Clock, Engine, EngineConfig, FetchState, MatchView, PlayerView, Snapshot, SystemClock,
};

pub fn run(data_dir: PathBuf, dir: Option<PathBuf>, json: bool) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let mut config = EngineConfig::new(&data_dir);
        config.log_dir = dir;
        let engine = Engine::start(config, Arc::new(SystemClock))?;
        if !json {
            println!("数据目录：{}（Ctrl+C 结束）", data_dir.display());
        }
        let mut updates = engine.subscribe();
        loop {
            tokio::select! {
                changed = updates.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    let snapshot = updates.borrow_and_update().clone();
                    if json {
                        println!("{}", serde_json::to_string(&*snapshot)?);
                    } else {
                        print_snapshot(&snapshot);
                    }
                }
                _ = tokio::signal::ctrl_c() => break,
            }
        }
        engine.shutdown().await;
        Ok(())
    })
}

fn print_snapshot(s: &Snapshot) {
    let log = if s.log.searching {
        "正在寻找日志目录".to_owned()
    } else {
        match (&s.log.dir, &s.log.error) {
            (_, Some(err)) => format!("日志读取失败：{err}"),
            (Some(dir), None) if s.log.ready => format!("日志 {dir}"),
            (Some(dir), None) => format!("日志 {dir}（补读中）"),
            (None, None) => "日志未就绪".to_owned(),
        }
    };
    let api = if s.api.in_flight + s.api.queued > 0 {
        format!("  接口：进行中 {}，排队 {}", s.api.in_flight, s.api.queued)
    } else {
        String::new()
    };
    println!("── 修订 {} ── {log}{api}", s.revision);
    if let Some(err) = &s.api.last_error {
        println!("   最近一次请求失败：{}", err.message);
    }
    match &s.current {
        Some(m) => print_match(m),
        None => println!("   还没有见到比赛"),
    }
}

fn print_match(m: &MatchView) {
    let timer = m.clock.map(|clock| {
        let ms = SystemClock.now().millis_since(clock.start_at);
        if m.phase == Phase::Ended {
            String::new()
        } else if ms < 0 {
            format!("  开局倒计时 {}", mmss(-ms))
        } else {
            format!("  已开局 {}", mmss(ms))
        }
    });
    println!(
        "比赛 {}  {}{}  地图 {}{}",
        m.match_id,
        phase_name(m.phase),
        if m.rejoined { "（重连）" } else { "" },
        m.map.as_deref().unwrap_or("?"),
        timer.unwrap_or_default()
    );
    if !m.roster_known {
        println!("  等待室（队伍未定）");
        for p in &m.players {
            print_player(p);
        }
        return;
    }
    for summary in &m.teams {
        let average = summary
            .average_rating
            .map(|r| format!("{r:.0}"))
            .unwrap_or_else(|| "-".to_owned());
        let mine = if m.self_team == Some(summary.team) {
            "（本方）"
        } else {
            ""
        };
        println!(
            "  {:?}{mine}  平均 ELO {average}（{}/{} 人有分）",
            summary.team, summary.rated, summary.players
        );
        for p in m.players.iter().filter(|p| p.team == Some(summary.team)) {
            print_player(p);
        }
    }
    let watchers: Vec<&PlayerView> = m
        .players
        .iter()
        .filter(|p| p.team == Some(Team::Spectators))
        .collect();
    if !watchers.is_empty() {
        println!("  观战");
        for p in watchers {
            print_player(p);
        }
    }
}

fn print_player(p: &PlayerView) {
    let profile = p.profile.as_ref();
    let rating = profile
        .and_then(|x| x.rating)
        .map(|r| format!("{r:>5.0}"))
        .unwrap_or_else(|| "    -".to_owned());
    let level = profile
        .and_then(|x| x.level)
        .map(|l| format!("{l:>3}"))
        .unwrap_or_else(|| "  -".to_owned());
    let win_rate = profile
        .and_then(|x| x.ranked.as_ref())
        .and_then(|r| r.win_rate)
        .map(|w| format!("{:>3.0}%", w * 100.0))
        .unwrap_or_else(|| "   -".to_owned());
    let stale = if profile.is_some() && !p.profile_current {
        "旧"
    } else {
        "  "
    };
    let fetch = match p.fetch {
        FetchState::Idle => String::new(),
        FetchState::Queued => " [排队]".to_owned(),
        FetchState::Fetching => " [查询中]".to_owned(),
        FetchState::Failed { kind, .. } => format!(" [失败 {kind:?}]"),
    };
    let met = if p.encounters.total > 0 {
        format!(
            " 同场{}次(友{}/敌{})",
            p.encounters.total, p.encounters.as_teammate, p.encounters.as_opponent
        )
    } else {
        String::new()
    };
    println!(
        "    {}ELO {rating}{stale} Lv{level} 胜率{win_rate}{met}{fetch}  {}",
        if p.is_self { "*" } else { " " },
        p.name.as_deref().unwrap_or("（无名）"),
    );
}

fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::WaitingRoom => "等待室",
        Phase::Loading => "加载中",
        Phase::Deployment => "准备阶段",
        Phase::InBattle => "对局中",
        Phase::Ended => "已结束",
    }
}

fn mmss(ms: i64) -> String {
    let secs = ms / 1000;
    format!("{}:{:02}", secs / 60, secs % 60)
}
