//! OCR 子进程与候选查询调度。只发布瞬态视图，不写数据库。
use super::*;
use bax_batrace::{ApiError, BatraceClient};
use bax_store::Store;
use std::{collections::HashMap, path::PathBuf, process::Stdio, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout},
    sync::{mpsc, watch},
};

struct Process {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}
impl Process {
    async fn start(root: &std::path::Path) -> Result<Self, String> {
        let executable = root.join("bax-ocr.exe");
        if !executable.is_file() {
            return Err("OCR 运行组件缺失，请重新安装 OCR 版本 BAX".into());
        }
        let mut command = tokio::process::Command::new(executable);
        command
            .arg(root.join("models"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        let mut child = command
            .spawn()
            .map_err(|_| "OCR 工作进程启动失败".to_owned())?;
        let input = child.stdin.take().ok_or("OCR 输入不可用")?;
        let output = BufReader::new(child.stdout.take().ok_or("OCR 输出不可用")?);
        let mut process = Self {
            child,
            input,
            output,
        };
        let hello = process.read().await?;
        if hello.get("ready").and_then(serde_json::Value::as_bool) != Some(true) {
            return Err("OCR 模型未就绪，请重新安装 BAX".into());
        }
        Ok(process)
    }
    async fn read(&mut self) -> Result<serde_json::Value, String> {
        let mut line = String::new();
        tokio::time::timeout(Duration::from_secs(20), self.output.read_line(&mut line))
            .await
            .map_err(|_| "OCR 响应超时".to_owned())?
            .map_err(|_| "OCR 响应读取失败".to_owned())?;
        if line.len() > 65536 {
            return Err("OCR 响应过大".into());
        }
        serde_json::from_str(&line).map_err(|_| "OCR 工作进程未返回有效结果".into())
    }
    async fn capture(&mut self) -> Result<(Vec<Observation>, String), String> {
        if self
            .child
            .try_wait()
            .map_err(|_| "OCR 状态读取失败")?
            .is_some()
        {
            return Err("OCR 工作进程已退出".into());
        }
        self.input
            .write_all(b"{}\n")
            .await
            .map_err(|_| "OCR 输入失败")?;
        let value = self.read().await?;
        let status = value
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("画面未识别")
            .to_owned();
        let rows: Vec<Observation> = serde_json::from_value(
            value
                .get("players")
                .cloned()
                .unwrap_or(serde_json::json!([])),
        )
        .map_err(|_| "OCR 名单格式错误")?;
        if rows.len() > 10
            || rows.iter().any(|r| {
                !["left", "right"].contains(&r.side.as_str())
                    || r.row > 4
                    || r.name.trim().is_empty()
                    || r.name.chars().count() > 80
                    || r.alternatives.len() > 1
                    || r.alternatives.iter().any(|n| n.chars().count() > 80)
                    || !(0..=100).contains(&r.level)
            })
        {
            return Err("OCR 名单超出范围".into());
        }
        Ok((rows, status))
    }
}

fn local_index(path: PathBuf) -> Vec<Candidate> {
    let Ok(store) = Store::open(&path) else {
        return vec![];
    };
    let Ok(aliases) = store.player_aliases() else {
        return vec![];
    };
    let mut profiles = HashMap::new();
    aliases
        .into_iter()
        .map(|(id, name, level)| {
            let profile = profiles
                .entry(id)
                .or_insert_with(|| {
                    store
                        .player(id)
                        .ok()
                        .flatten()
                        .and_then(|p| p.profile_json)
                        .as_deref()
                        .and_then(ProfileSummary::from_json)
                })
                .clone();
            Candidate {
                id,
                name,
                level,
                local: true,
                profile,
            }
        })
        .collect()
}

fn prefer_local(p: &PregamePlayer) -> bool {
    !p.ambiguous
        && p.candidates.first().is_some_and(|c| {
            (normalized(&c.name) == normalized(&p.observed.name)
                || p.observed
                    .alternatives
                    .iter()
                    .any(|n| normalized(n) == normalized(&c.name)))
                && c.level.is_some_and(|n| n.abs_diff(p.observed.level) <= 10)
        })
}

/// OCR 网络额度独立于正式日志资料请求；失败后本场不自动重试。
struct RequestBudget {
    remaining: usize,
    next: tokio::time::Instant,
    spacing: Duration,
}
impl RequestBudget {
    fn new(remaining: usize, spacing: Duration) -> Self {
        Self {
            remaining,
            next: tokio::time::Instant::now(),
            spacing,
        }
    }
    async fn take(&mut self) -> Result<(), ApiError> {
        if self.remaining == 0 {
            return Err(ApiError::RateLimited { retry_after: None });
        }
        self.remaining -= 1;
        tokio::time::sleep_until(self.next).await;
        self.next = tokio::time::Instant::now() + self.spacing;
        Ok(())
    }
}

async fn resolve(
    client: BatraceClient,
    row: Observation,
    local: Vec<Candidate>,
    budget: &mut RequestBudget,
) -> Result<PregamePlayer, ApiError> {
    // 未知敌我也不查询；不能把画面左/右侧写死为友/敌。
    if row.friendly != Some(false) {
        return Ok(rank(row, local, false));
    }
    let mut candidates = local;
    let mut truncated = false;
    for name in std::iter::once(&row.name).chain(row.alternatives.iter()) {
        budget.take().await?;
        let response = client.search_players(name).await?.data;
        truncated |= response.count as usize > response.players.len();
        for p in response.players {
            if let Some(old) = candidates.iter_mut().find(|c| c.id.get() == p.id) {
                old.name = p.name;
                old.level = p.level;
            } else {
                candidates.push(Candidate {
                    id: PlayerId(p.id),
                    name: p.name,
                    level: p.level,
                    local: false,
                    profile: None,
                });
            }
        }
        let ranked = rank(row.clone(), candidates.clone(), truncated);
        if !ranked.ambiguous && !ranked.candidates.is_empty() {
            break;
        }
    }
    let mut result = rank(row, candidates, truncated);
    if !result.ambiguous
        && let Some(first) = result.candidates.first_mut()
        && first.profile.is_none()
    {
        budget.take().await?;
        let info = client.prefetch_player_info(first.id).await?;
        first.profile = ProfileSummary::from_response(&info.data);
    }
    Ok(result)
}

/// Match identity survives phase, log-file and foreground-screen changes.
#[derive(Default)]
struct CaptureLatch {
    identity: Option<(MatchId, Timestamp)>,
    previous: Vec<Observation>,
    frozen: Option<Vec<Observation>>,
}
impl CaptureLatch {
    fn select(&mut self, identity: (MatchId, Timestamp)) -> bool {
        if self.identity == Some(identity) {
            return false;
        }
        *self = Self {
            identity: Some(identity),
            ..Default::default()
        };
        true
    }
    fn observe(&mut self, rows: Vec<Observation>) -> bool {
        if self.frozen.is_some() {
            return true;
        }
        if !rows.is_empty() && rows == self.previous {
            self.frozen = Some(rows);
            return true;
        }
        self.previous = rows;
        false
    }
}

pub(crate) async fn run(
    root: PathBuf,
    store: PathBuf,
    client: BatraceClient,
    mut snapshots: watch::Receiver<Arc<Snapshot>>,
    tx: mpsc::Sender<Message>,
) {
    let mut process = None;
    let mut latch = CaptureLatch::default();
    let mut view = PregameView::default();
    let mut attempted = std::collections::HashSet::new();
    let mut index = Vec::new();
    let mut stopped = false;
    let mut budget = RequestBudget::new(12, Duration::from_secs(1));
    let mut process_retry = tokio::time::Instant::now();
    let mut tick = tokio::time::interval(Duration::from_secs(2));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = tick.tick() => {},
            changed = snapshots.changed() => {
                if changed.is_err() { break; }
                continue;
            },
        }
        let snapshot = snapshots.borrow().clone();
        let Some(key) = ticket(&snapshot) else {
            process = None;
            continue;
        };
        if latch.select((key.id, key.discovered)) {
            view = PregameView::default();
            attempted.clear();
            stopped = false;
            budget = RequestBudget::new(12, Duration::from_secs(1));
            index = tokio::task::spawn_blocking({
                let p = store.clone();
                move || local_index(p)
            })
            .await
            .unwrap_or_default();
        }
        if latch.frozen.is_none() {
            if tokio::time::Instant::now() < process_retry {
                continue;
            }
            if process.is_none() {
                match Process::start(&root).await {
                    Ok(p) => process = Some(p),
                    Err(status) => {
                        process_retry = tokio::time::Instant::now() + Duration::from_secs(30);
                        view.status = status;
                    }
                }
            }
            if let Some(p) = process.as_mut() {
                match p.capture().await {
                    Ok((rows, status)) => {
                        if latch.observe(rows) {
                            view.players = latch
                                .frozen
                                .as_ref()
                                .expect("frozen")
                                .iter()
                                .map(|row| rank(row.clone(), index.clone(), false))
                                .collect();
                            reject_duplicate_candidates(&mut view.players);
                            tracing::info!(
                                match_id = key.id.get(),
                                players = view.players.len(),
                                enemies = view
                                    .players
                                    .iter()
                                    .filter(|p| p.observed.friendly == Some(false))
                                    .count(),
                                "OCR 名单已锁定，本场不再重新采集"
                            );
                            process = None;
                        } else {
                            view.status = status;
                        }
                    }
                    Err(status) => {
                        view.status = status;
                        process = None;
                        process_retry = tokio::time::Instant::now() + Duration::from_secs(30);
                    }
                }
            }
        }
        if ticket(&snapshots.borrow()).as_ref() != Some(&key) {
            continue;
        }
        if latch.frozen.is_some() && !stopped {
            view.status = "赛前识别，等待日志确认".into();
        }
        let _ = tx
            .send(Message {
                ticket: key.clone(),
                revision: snapshot.revision,
                view: view.clone(),
            })
            .await;
        if latch.frozen.is_none() || stopped {
            continue;
        }
        for pos in 0..view.players.len() {
            let p = &view.players[pos];
            if p.observed.friendly != Some(false)
                || prefer_local(p)
                || p.observed.name.contains(['□', '\u{fffd}'])
                || !attempted.insert((p.observed.side.clone(), p.observed.row))
            {
                continue;
            }
            if ticket(&snapshots.borrow()).as_ref() != Some(&key) {
                break;
            }
            let before = budget.remaining;
            let request = resolve(
                client.clone(),
                p.observed.clone(),
                p.candidates.clone(),
                &mut budget,
            );
            tokio::pin!(request);
            let result = loop {
                tokio::select! {
                    result = &mut request => break Some(result),
                    changed = snapshots.changed() => {
                        if changed.is_err() || ticket(&snapshots.borrow()).as_ref() != Some(&key) {
                            break None;
                        }
                    }
                }
            };
            let Some(result) = result else {
                break;
            };
            match result {
                Ok(p) => view.players[pos] = p,
                Err(_) => {
                    stopped = true;
                    view.status = "查询暂不可用，保留识别结果并等待日志".into();
                }
            }
            // Identifiers only in approved diagnostic fields; no names/search strings.
            tracing::debug!(
                match_id = key.id.get(),
                remaining = before,
                stopped,
                "OCR 候选查询完成"
            );
            reject_duplicate_candidates(&mut view.players);
            let _ = tx
                .send(Message {
                    ticket: key.clone(),
                    revision: snapshot.revision,
                    view: view.clone(),
                })
                .await;
            if stopped {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    fn row() -> Observation {
        Observation {
            side: "left".into(),
            row: 0,
            friendly: Some(false),
            name: "Raven".into(),
            level: 57,
            alternatives: vec![],
        }
    }
    #[test]
    fn local_preference_requires_name_and_plausible_level() {
        let c = Candidate {
            id: PlayerId(1),
            name: "raven".into(),
            level: Some(57),
            local: true,
            profile: None,
        };
        assert!(prefer_local(&rank(row(), vec![c.clone()], false)));
        let mut old = c.clone();
        old.level = Some(10);
        assert!(!prefer_local(&rank(row(), vec![old], false)));
        let mut other = c.clone();
        other.id = PlayerId(2);
        assert!(!prefer_local(&rank(row(), vec![c, other], false)));
    }
    #[tokio::test]
    async fn remote_search_selects_by_level_not_order_and_fetches_only_the_winner() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/players/search"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                serde_json::json!({"count":2,"players":[
                {"id":2,"name":"Raven","level":70,"rating":9999},
                {"id":1,"name":"raven","level":57,"rating":0}]}),
            ))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/players/info"))
            .and(wiremock::matchers::query_param("stbid", "1"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"info":{"id":1,"name":"raven","level":57}})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let client = BatraceClient::new(bax_batrace::ClientConfig {
            base_url: server.uri(),
            ..Default::default()
        })
        .expect("client");
        let p = resolve(
            client,
            row(),
            vec![],
            &mut RequestBudget::new(12, Duration::ZERO),
        )
        .await
        .expect("resolve");
        assert_eq!(p.candidates[0].id, PlayerId(1));
        assert!(!p.ambiguous);
        assert!(p.candidates[0].profile.is_some());
    }
    #[test]
    fn frozen_roster_survives_empty_or_changed_screens_until_a_new_match() {
        let mut latch = CaptureLatch::default();
        let identity = (MatchId(7), Timestamp::from_millis(10));
        assert!(latch.select(identity));
        assert!(!latch.observe(vec![row()]));
        assert!(latch.observe(vec![row()]));
        let frozen = latch.frozen.clone();
        assert!(latch.observe(vec![]));
        let mut changed = row();
        changed.level = 19;
        assert!(latch.observe(vec![changed]));
        assert!(!latch.select(identity));
        assert_eq!(latch.frozen, frozen);
        assert!(latch.select((MatchId(8), identity.1)));
        assert!(latch.frozen.is_none());
    }
    #[tokio::test]
    async fn friendly_and_unknown_sides_never_query_the_api() {
        let server = MockServer::start().await;
        let client = BatraceClient::new(bax_batrace::ClientConfig {
            base_url: server.uri(),
            ..Default::default()
        })
        .expect("test result");
        for friendly in [Some(true), None] {
            let mut observed = row();
            observed.friendly = friendly;
            let mut budget = RequestBudget::new(12, Duration::ZERO);
            let result = resolve(client.clone(), observed, vec![], &mut budget)
                .await
                .expect("test result");
            assert!(result.candidates.is_empty());
            assert_eq!(budget.remaining, 12);
        }
        assert!(
            server
                .received_requests()
                .await
                .expect("test result")
                .is_empty()
        );
    }
    #[tokio::test(start_paused = true)]
    async fn request_budget_spaces_calls_and_stops_at_the_cap() {
        let mut budget = RequestBudget::new(2, Duration::from_secs(1));
        let start = tokio::time::Instant::now();
        budget.take().await.expect("test result");
        budget.take().await.expect("test result");
        assert!(start.elapsed() >= Duration::from_secs(1));
        assert!(budget.take().await.is_err());
    }
    #[test]
    fn local_index_retains_prior_names() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("players.db");
        let mut store = Store::open(&path).expect("store");
        let mut state = bax_core::Tracker::default();
        state.apply(
            Timestamp::from_millis(1),
            &bax_core::LogEvent::RoomEntered {
                room: bax_core::RoomKind::GameRoom,
                match_id: MatchId(7),
                status: Some(bax_core::RoomStatus::WaitClients),
            },
        );
        state.apply(
            Timestamp::from_millis(2),
            &bax_core::LogEvent::PlayerList {
                players: vec![bax_core::RosterEntry {
                    id: PlayerId(1),
                    name: Some("OldRaven".into()),
                    team: bax_core::Team::Alpha,
                }],
            },
        );
        store
            .record_match(state.current().expect("match"))
            .expect("save");
        let index = local_index(path);
        assert!(
            index
                .iter()
                .any(|c| c.id == PlayerId(1) && c.name == "OldRaven")
        );
    }
}
