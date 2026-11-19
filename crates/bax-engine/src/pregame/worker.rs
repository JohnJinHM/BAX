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

pub(crate) struct Message {
    pub ticket: Ticket,
    pub revision: u64,
    pub view: PregameView,
}

struct Process {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}
impl Process {
    async fn start(root: &std::path::Path) -> Result<Self, String> {
        let python = root.join("Scripts/python.exe");
        if !python.is_file() {
            return Err("OCR 未安装，请运行 scripts/setup-ocr.ps1".into());
        }
        let mut command = tokio::process::Command::new(python);
        command
            .args(["-u", "-c", include_str!("../../../../ocr/worker.py")])
            .arg(root.join("models"))
            .env("PYTHONUTF8", "1")
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
            return Err("OCR 模型未就绪，请重新运行安装脚本".into());
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

async fn resolve(
    client: BatraceClient,
    row: Observation,
    local: Vec<Candidate>,
) -> Result<PregamePlayer, ApiError> {
    let mut candidates = local;
    let mut truncated = false;
    for name in std::iter::once(&row.name).chain(row.alternatives.iter()) {
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
    }
    let mut result = rank(row, candidates, truncated);
    if !result.ambiguous
        && let Some(first) = result.candidates.first_mut()
        && first.profile.is_none()
    {
        match client.player_info(first.id).await {
            Ok(info) => first.profile = ProfileSummary::from_response(&info.data),
            Err(err @ ApiError::RateLimited { .. }) => return Err(err),
            Err(_) => {}
        }
    }
    Ok(result)
}

pub(crate) async fn run(
    root: PathBuf,
    store: PathBuf,
    client: BatraceClient,
    mut snapshots: watch::Receiver<Arc<Snapshot>>,
    tx: mpsc::Sender<Message>,
) {
    let mut process = None;
    let mut active = None;
    let mut previous = Vec::new();
    let mut cache: HashMap<(String, i64, Vec<String>), PregamePlayer> = HashMap::new();
    let mut index = Vec::new();
    let mut pause = tokio::time::Instant::now();
    let mut process_retry = tokio::time::Instant::now();
    let mut tick = tokio::time::interval(Duration::from_secs(2));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = tick.tick() => {},
            changed = snapshots.changed() => {
                if changed.is_err() { break; }
                let snapshot = snapshots.borrow().clone();
                if ticket(&snapshot).is_none() {
                    active = None; previous.clear(); cache.clear();
                    if !enabled(&snapshot) { process = None; }
                }
                continue;
            },
        }
        let snapshot = snapshots.borrow().clone();
        let Some(key) = ticket(&snapshot) else {
            active = None;
            previous.clear();
            cache.clear();
            if !enabled(&snapshot) {
                process = None;
            }
            continue;
        };
        if active.as_ref() != Some(&key) {
            active = Some(key.clone());
            previous.clear();
            cache.clear();
            index = tokio::task::spawn_blocking({
                let p = store.clone();
                move || local_index(p)
            })
            .await
            .unwrap_or_default();
        }
        if process.is_none() && tokio::time::Instant::now() < process_retry {
            continue;
        }
        if process.is_none() {
            match Process::start(&root).await {
                Ok(p) => process = Some(p),
                Err(status) => {
                    let _ = tx
                        .send(Message {
                            ticket: key,
                            revision: snapshot.revision,
                            view: PregameView {
                                players: vec![],
                                status,
                            },
                        })
                        .await;
                    process_retry = tokio::time::Instant::now() + Duration::from_secs(30);
                    continue;
                }
            }
        }
        let result = match process.as_mut() {
            Some(p) => p.capture().await,
            None => continue,
        };
        let (rows, status) = match result {
            Ok(value) => value,
            Err(status) => {
                process = None;
                previous.clear();
                let _ = tx
                    .send(Message {
                        ticket: key,
                        revision: snapshot.revision,
                        view: PregameView {
                            players: vec![],
                            status,
                        },
                    })
                    .await;
                process_retry = tokio::time::Instant::now() + Duration::from_secs(30);
                continue;
            }
        };
        if ticket(&snapshots.borrow()).as_ref() != Some(&key) {
            continue;
        }
        if rows.is_empty() || rows != previous {
            previous = rows;
            let _ = tx
                .send(Message {
                    ticket: key,
                    revision: snapshot.revision,
                    view: PregameView {
                        players: vec![],
                        status: if previous.is_empty() {
                            status
                        } else {
                            "正在核对画面…".into()
                        },
                    },
                })
                .await;
            continue;
        }
        let mut players = Vec::new();
        let mut pending = Vec::new();
        for row in &rows {
            let ck = (normalized(&row.name), row.level, row.alternatives.clone());
            if let Some(c) = cache.get(&ck) {
                let mut p = c.clone();
                p.observed = row.clone();
                players.push(p);
                continue;
            }
            let p = rank(row.clone(), index.clone(), false);
            if prefer_local(&p) {
                cache.insert(ck, p.clone());
            } else if !row.name.contains(['□', '\u{fffd}']) {
                pending.push((players.len(), row.clone(), p.candidates.clone()));
            }
            players.push(p);
        }
        reject_duplicate_candidates(&mut players);
        let _ = tx
            .send(Message {
                ticket: key.clone(),
                revision: snapshot.revision,
                view: PregameView {
                    players: players.clone(),
                    status: if pending.is_empty() {
                        "候选待日志确认"
                    } else {
                        "正在查询候选…"
                    }
                    .into(),
                },
            })
            .await;
        let mut failed = false;
        for chunk in pending.chunks(3) {
            if tokio::time::Instant::now() < pause {
                failed = true;
                break;
            }
            if ticket(&snapshots.borrow()).as_ref() != Some(&key) {
                break;
            }
            let mut tasks = tokio::task::JoinSet::new();
            for (position, row, local) in chunk.iter().cloned() {
                let client = client.clone();
                tasks.spawn(async move { (position, resolve(client, row, local).await) });
            }
            while let Some(Ok((pos, result))) = tasks.join_next().await {
                match result {
                    Ok(p) => {
                        cache.insert(
                            (
                                normalized(&p.observed.name),
                                p.observed.level,
                                p.observed.alternatives.clone(),
                            ),
                            p.clone(),
                        );
                        players[pos] = p;
                    }
                    Err(err) => {
                        failed = true;
                        let delay = match err {
                            ApiError::RateLimited { retry_after } => retry_after
                                .unwrap_or(Duration::from_secs(30))
                                .max(Duration::from_secs(30)),
                            _ => Duration::from_secs(30),
                        };
                        pause = tokio::time::Instant::now() + delay;
                    }
                }
            }
            if failed {
                break;
            }
        }
        reject_duplicate_candidates(&mut players);
        if !pending.is_empty() {
            let _ = tx
                .send(Message {
                    ticket: key,
                    revision: snapshot.revision,
                    view: PregameView {
                        players,
                        status: if failed {
                            "查询暂不可用，稍后重试"
                        } else {
                            "候选待日志确认"
                        }
                        .into(),
                    },
                })
                .await;
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
        let p = resolve(client, row(), vec![]).await.expect("resolve");
        assert_eq!(p.candidates[0].id, PlayerId(1));
        assert!(!p.ambiguous);
        assert!(p.candidates[0].profile.is_some());
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
