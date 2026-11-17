//! `bax api` 子命令。

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use bax_batrace::model::{
    MatchReportResponse, PlayerInfoResponse, PlayerMatchesResponse, UnitsResponse,
};
use bax_batrace::{BatraceClient, ClientConfig};
use bax_core::{MatchId, PlayerId};
use bax_engine::ProfileSummary;
use rusqlite::{Connection, OpenFlags};

use crate::ApiCommand;

pub fn api(command: ApiCommand) -> Result<()> {
    if let ApiCommand::CheckCache { path } = &command {
        return check_cache(path);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let client = BatraceClient::new(ClientConfig::default())?;
        match command {
            ApiCommand::Player { id, raw } => {
                let fetched = client.player_info(PlayerId(id)).await?;
                if raw {
                    println!("{}", fetched.raw);
                    return Ok(());
                }
                let summary = ProfileSummary::from_response(&fetched.data);
                println!("{summary:#?}");
                println!("近期比赛 {} 场", fetched.data.recent_match_ids().count());
            }
            ApiCommand::Match { id, raw } => {
                let fetched = client.match_report(MatchId(id)).await?;
                if raw {
                    println!("{}", fetched.raw);
                    return Ok(());
                }
                let info = &fetched.data;
                println!(
                    "地图 {:?}  开始 {:?}  结束 {:?}  时长 {:?} 秒  胜方 {:?}",
                    info.map_id,
                    info.start_time,
                    info.end_time,
                    info.total_play_time_in_sec,
                    info.winner()
                );
                for (player, record) in info.players() {
                    println!(
                        "  {player} {:?}  ELO {:?} → {:?}  单位实例 {}  转交 {}  {}",
                        record.team(),
                        record.old_rating,
                        record.new_rating,
                        record.unit_data.len(),
                        record.transferred_units.len(),
                        record.name.as_deref().unwrap_or("")
                    );
                }
            }
            ApiCommand::Units { raw } => {
                let fetched = client.units().await?;
                if raw {
                    println!("{}", fetched.raw);
                    return Ok(());
                }
                let mut by_category: BTreeMap<Option<i64>, usize> = BTreeMap::new();
                for unit in &fetched.data.units {
                    *by_category.entry(unit.category_type).or_default() += 1;
                }
                println!(
                    "单位 {} 个（接口报告 {:?}），按分类：{by_category:?}",
                    fetched.data.units.len(),
                    fetched.data.count
                );
            }
            ApiCommand::CheckCache { .. } => unreachable!("前面已经处理"),
        }
        Ok(())
    })
}

/// 把错误信息里的具体数值换成 `#`，只留下类型信息，相同的问题就能归并到一起。
fn redact_numbers(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut in_number = false;
    for c in message.chars() {
        if c.is_ascii_digit() || (in_number && c == '.') {
            if !in_number {
                out.push('#');
                in_number = true;
            }
        } else {
            in_number = false;
            out.push(c);
        }
    }
    out
}

/// 原型把接口响应原样存在一张键值表里：键形如 `info:玩家ID`、`match:比赛ID`、
/// `matches:…`、`units`。逐条用 BAX 的数据模型解析，只报告结构问题，不输出内容。
fn check_cache(path: &Path) -> Result<()> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("打不开 {}", path.display()))?;
    let mut stmt = conn.prepare("SELECT k, v FROM c")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    #[derive(Default)]
    struct Tally {
        ok: usize,
        failed: usize,
        notes: BTreeMap<String, usize>,
    }
    let mut tallies: BTreeMap<String, Tally> = BTreeMap::new();
    for row in rows {
        let (key, body) = row?;
        let kind = key.split(':').next().unwrap_or_default().to_owned();
        let tally = tallies.entry(kind.clone()).or_default();
        let mut note = |text: &str| *tally.notes.entry(text.to_owned()).or_default() += 1;
        let result: Result<(), serde_json::Error> = match kind.as_str() {
            "info" => serde_json::from_str::<PlayerInfoResponse>(&body).map(|r| {
                if r.info.is_none() {
                    note("没有基本资料");
                }
                if r.stat_info.is_none() {
                    note("没有累计统计");
                }
                if ProfileSummary::from_response(&r).is_some_and(|s| s.rating.is_none()) {
                    note("没有 ELO");
                }
            }),
            "match" => serde_json::from_str::<MatchReportResponse>(&body).map(|r| {
                let Some(info) = r.match_info else {
                    note("没有战报");
                    return;
                };
                if info.is_empty() {
                    note("战报为空");
                }
                if info.winner().is_none() {
                    note("胜方编号无法识别");
                }
                if info.players().any(|(_, p)| p.team().is_none()) {
                    note("有玩家的队伍编号无法识别");
                }
                if info.players().all(|(_, p)| p.unit_data.is_empty()) {
                    note("没有单位数据");
                }
                if info.players().any(|(_, p)| !p.transferred_units.is_empty()) {
                    note("有转交单位");
                }
            }),
            "matches" => serde_json::from_str::<PlayerMatchesResponse>(&body).map(|r| {
                if r.matches.iter().any(|m| m.id().is_none()) {
                    note("有比赛 ID 无法解析");
                }
            }),
            "units" => serde_json::from_str::<UnitsResponse>(&body).map(|r| {
                if r.units.iter().any(|u| u.id.is_none()) {
                    note("有单位缺少 ID");
                }
            }),
            _ => {
                note("未知的键");
                Ok(())
            }
        };
        match result {
            Ok(()) => tally.ok += 1,
            Err(err) => {
                tally.failed += 1;
                // 只报告错误的类别和位置，不输出响应内容
                *tally
                    .notes
                    .entry(format!("解析失败：{}", redact_numbers(&err.to_string())))
                    .or_default() += 1;
            }
        }
    }
    for (kind, tally) in &tallies {
        println!("{kind}: 解析成功 {}，失败 {}", tally.ok, tally.failed);
        for (note, count) in &tally.notes {
            println!("    {count:>5}  {note}");
        }
    }
    Ok(())
}
