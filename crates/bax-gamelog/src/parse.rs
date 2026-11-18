//! 把日志行解析成事件。
//!
//! [`LogParser`] 逐行接收文本。绝大多数行与对局无关，解析器对它们只做几次前缀比较，
//! 不分配内存；一份几十万行的日志可以很快扫完。
//!
//! 解析规则移植自 BA-ELO-Overlay 的 `LogWatcher`（`src/ba_elo_overlay.py`），并做了这些改动：
//! 按记录而不是按行解析，事件因此带有时间；名单里的空名字不再被丢弃；
//! 增加了 `Network lifecycle` 结构化行、加载、场景就绪、比赛结束和重连的识别。

use bax_core::{ExitCause, LogEvent, MatchId, PlayerId, RoomKind, RoomStatus, RosterEntry, Team};

use crate::time::LogTime;

/// 带时间的事件。时间取自所在记录的时间戳行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedEvent {
    pub at: LogTime,
    pub event: LogEvent,
}

#[derive(Debug)]
enum Mode {
    /// 刚读到时间戳行，下一行是级别与消息。
    Header,
    /// 当前记录与对局无关，跳过它的后续行。
    Skip,
    /// 正在收集名单。
    PlayerList {
        players: Vec<RosterEntry>,
        published: usize,
    },
    /// 游戏启动信息，其中一行是版本号。
    StartupInfo,
}

/// 流式日志解析器。
#[derive(Debug)]
pub struct LogParser {
    time: Option<LogTime>,
    mode: Mode,
}

impl Default for LogParser {
    fn default() -> Self {
        LogParser::new()
    }
}

impl LogParser {
    pub fn new() -> Self {
        LogParser {
            time: None,
            mode: Mode::Skip,
        }
    }

    /// 回到初始状态。换文件或文件被截断时调用。
    pub fn reset(&mut self) {
        self.time = None;
        self.mode = Mode::Skip;
    }

    /// 是否有一条多行记录还没收完。
    pub fn has_pending(&self) -> bool {
        matches!(&self.mode, Mode::PlayerList { players, published } if players.len() > *published)
    }

    /// 接收一行（不含换行符），把解析出的事件追加到 `out`。
    pub fn push_line(&mut self, line: &str, out: &mut Vec<TimedEvent>) {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(time) = LogTime::parse_bracketed(line) {
            self.flush(out);
            self.time = Some(time);
            self.mode = Mode::Header;
            return;
        }
        let Some(at) = self.time else {
            return;
        };
        match &mut self.mode {
            Mode::Skip => {}
            Mode::Header => self.mode = self.on_header(at, line, out),
            Mode::PlayerList { players, .. } => match parse_roster_line(line) {
                Some(RosterLine::Player(entry)) => players.push(entry),
                // AI 玩家不进名单，但名单还没有结束
                Some(RosterLine::Ai) => {}
                // 空行或别的内容：名单到此为止
                None => self.flush(out),
            },
            Mode::StartupInfo => {
                if let Some(version) = line.strip_prefix("Version: ") {
                    emit(
                        out,
                        at,
                        LogEvent::GameVersion {
                            version: version.trim().to_owned(),
                        },
                    );
                    self.mode = Mode::Skip;
                } else if line.is_empty() {
                    self.mode = Mode::Skip;
                }
            }
        }
    }

    /// 发布目前收集到的名单，但保留记录，后续行到达时仍可补齐。
    pub fn publish_pending(&mut self, out: &mut Vec<TimedEvent>) {
        if let Mode::PlayerList { players, published } = &mut self.mode
            && players.len() > *published
            && let Some(at) = self.time
        {
            emit(
                out,
                at,
                LogEvent::PlayerList {
                    players: players.clone(),
                },
            );
            *published = players.len();
        }
    }

    /// 结束多行记录。遇到记录边界、换文件或离线回放结束时调用。
    pub fn flush(&mut self, out: &mut Vec<TimedEvent>) {
        if let Mode::PlayerList { players, published } =
            std::mem::replace(&mut self.mode, Mode::Skip)
            && players.len() > published
            && let Some(at) = self.time
        {
            emit(out, at, LogEvent::PlayerList { players });
        }
    }

    fn on_header(&mut self, at: LogTime, line: &str, out: &mut Vec<TimedEvent>) -> Mode {
        // 与对局有关的消息都是 Log 级别
        let Some(message) = line.strip_prefix("Log: ") else {
            return Mode::Skip;
        };
        if message.starts_with("Player list:") {
            return Mode::PlayerList {
                players: Vec::with_capacity(10),
                published: 0,
            };
        }
        if message.starts_with("BrokenArrow startup info:") {
            return Mode::StartupInfo;
        }
        if let Some(event) = parse_message(message) {
            emit(out, at, event);
        }
        Mode::Skip
    }
}

fn emit(out: &mut Vec<TimedEvent>, at: LogTime, event: LogEvent) {
    out.push(TimedEvent { at, event });
}

/// 解析单行消息（`Log: ` 之后的部分）。
fn parse_message(message: &str) -> Option<LogEvent> {
    // 带标签的消息，例如 `[NET_ROOM] WaitingRoom entered, fight id: 1`
    if let Some(rest) = message.strip_prefix('[') {
        let (tag, body) = rest.split_once("] ")?;
        return match tag {
            "NET_ROOM" => parse_net_room(body),
            _ => None,
        };
    }
    let first = *message.as_bytes().first()?;
    match first {
        b'G' => {
            if let Some(name) = message.strip_prefix("GetPersonaName") {
                let name = name.trim();
                return (!name.is_empty()).then(|| LogEvent::Persona {
                    name: name.to_owned(),
                });
            }
            // 旧版日志的房间消息不带标签
            parse_room_marker(message)
        }
        b'W' => parse_room_marker(message),
        b'D' => {
            let name = message.strip_prefix("Deck set to: ")?.trim();
            Some(LogEvent::DeckSelected {
                name: (!name.is_empty() && name != "null").then(|| name.to_owned()),
            })
        }
        b'S' => {
            if message == "Scenario controller loaded" {
                return Some(LogEvent::ScenarioLoaded);
            }
            if message == "Start reconnect to battle" {
                return Some(LogEvent::ReconnectStarted);
            }
            parse_loading(message)
        }
        b'R' => parse_room_client(message),
        b'F' => parse_fight_info(message),
        _ => None,
    }
}

fn parse_net_room(body: &str) -> Option<LogEvent> {
    if let Some(fields) = body.strip_prefix("Network lifecycle | ") {
        return parse_lifecycle(fields);
    }
    if let Some(fields) = body.strip_prefix("Network traffic history | ") {
        let mut cause = None;
        let mut fight = None;
        for (key, value) in key_values(fields) {
            match key {
                "Cause" => cause = Some(value),
                "FightId" => fight = value.parse::<u64>().ok(),
                // 两个字段都在行首，后面是很长的流量采样
                _ if cause.is_some() && fight.is_some() => break,
                _ => {}
            }
        }
        return match (cause, fight) {
            (Some("MatchEnded"), Some(id)) => Some(LogEvent::MatchEnded {
                match_id: MatchId(id),
            }),
            _ => None,
        };
    }
    parse_room_marker(body)
}

/// `Stage=Game | Action=Entered | … | RoomType=WaitingRoom | FightId=1 | Status=WaitClients`
fn parse_lifecycle(fields: &str) -> Option<LogEvent> {
    let mut stage = None;
    let mut action = None;
    let mut cause = None;
    let mut room = None;
    let mut fight = None;
    let mut status = None;
    for (key, value) in key_values(fields) {
        match key {
            "Stage" => stage = Some(value),
            "Action" => action = Some(value),
            "Cause" => cause = Some(ExitCause::from_log_name(value)),
            "RoomType" => room = room_kind(value),
            "FightId" => fight = value.parse::<u64>().ok().map(MatchId),
            "Status" => status = Some(RoomStatus::from_log_name(value)),
            _ => {}
        }
    }
    if stage != Some("Game") {
        return None;
    }
    match action? {
        "Entered" => Some(LogEvent::RoomEntered {
            room: room?,
            match_id: fight?,
            status,
        }),
        "Exited" => Some(LogEvent::RoomExited {
            room: room?,
            match_id: fight,
            cause,
            status,
        }),
        _ => None,
    }
}

fn key_values(fields: &str) -> impl Iterator<Item = (&str, &str)> {
    fields
        .split(" | ")
        .filter_map(|part| part.split_once('='))
        .map(|(k, v)| (k.trim(), v.trim()))
}

fn room_kind(name: &str) -> Option<RoomKind> {
    match name {
        "WaitingRoom" => Some(RoomKind::WaitingRoom),
        "GameRoom" => Some(RoomKind::GameRoom),
        _ => None,
    }
}

/// `WaitingRoom entered, fight id: 1`、`GameRoom exited` 这类不带状态的标记。
fn parse_room_marker(body: &str) -> Option<LogEvent> {
    let (room_name, rest) = body.split_once(' ')?;
    let room = room_kind(room_name)?;
    if let Some(id) = rest.strip_prefix("entered, fight id:") {
        return Some(LogEvent::RoomEntered {
            room,
            match_id: MatchId(id.trim().parse().ok()?),
            status: None,
        });
    }
    (rest == "exited").then_some(LogEvent::RoomExited {
        room,
        match_id: None,
        cause: None,
        status: None,
    })
}

/// `Start loading battle... map: Airport, scenario: - Airport_network`
fn parse_loading(message: &str) -> Option<LogEvent> {
    let rest = message.strip_prefix("Start loading battle... map: ")?;
    let (map, scenario) = match rest.split_once(", scenario: ") {
        Some((map, scenario)) => {
            let scenario = scenario.strip_prefix("- ").unwrap_or(scenario).trim();
            (map, (!scenario.is_empty()).then(|| scenario.to_owned()))
        }
        None => (rest, None),
    };
    Some(LogEvent::LoadingStarted {
        map: map.trim().to_owned(),
        scenario,
    })
}

/// `Room: (WaitingRoom|1), Client: (名字|101)`。名字里可以有竖线和括号，所以从右往左找。
fn parse_room_client(message: &str) -> Option<LogEvent> {
    let rest = message.strip_prefix("Room: (")?;
    let (room_name, rest) = rest.split_once('|')?;
    let room = room_kind(room_name)?;
    let (_, client) = rest.split_once("), Client: (")?;
    let client = client.strip_suffix(')')?;
    let (name, id) = client.rsplit_once('|')?;
    let name = name.trim();
    Some(LogEvent::RoomClient {
        room,
        player: PlayerId(id.trim().parse().ok()?),
        name: (!name.is_empty()).then(|| name.to_owned()),
    })
}

/// `FID:9000001 v1.2.2.1 - 名字 (2026-11-17 02:52)`
fn parse_fight_info(message: &str) -> Option<LogEvent> {
    let rest = message.strip_prefix("FID:")?;
    let (id, rest) = rest.split_once(' ')?;
    let match_id = MatchId(id.parse().ok()?);
    let name = rest.split_once(" - ").map(|(_version, tail)| {
        // 去掉末尾的 UTC 时间
        match tail.rfind(" (") {
            Some(i) if tail.ends_with(')') => &tail[..i],
            _ => tail,
        }
        .trim()
    });
    Some(LogEvent::FightInfo {
        match_id,
        player_name: name.filter(|n| !n.is_empty()).map(str::to_owned),
    })
}

/// 名单里的一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RosterLine {
    Player(RosterEntry),
    /// AI 玩家。它们的 ID 是负数，每场比赛重新编号，不代表固定的身份，所以不进名单。
    Ai,
}

/// 解析名单里的一行。
///
/// 当前格式是 `ID: 1, Name: 名字, Team: Alpha`；旧版游戏写作 `ID: 1 Name: 名字Team: Alpha`，
/// 名字与 `Team:` 之间没有分隔。名字可以为空，也可以包含逗号。
/// 遭遇战和合作模式里的 AI 玩家 ID 为负数，例如 `ID: -12, Name: Bot 4, Team: Alpha`。
pub(crate) fn parse_roster_line(line: &str) -> Option<RosterLine> {
    let rest = line.strip_prefix("ID:")?.trim_start();
    let (is_ai, rest) = match rest.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, rest),
    };
    let digits = rest
        .bytes()
        .position(|c| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    if digits == 0 {
        return None;
    }
    let id = PlayerId(rest[..digits].parse().ok()?);
    let rest = &rest[digits..];
    let comma_separated = rest.starts_with(',');
    let rest = rest.trim_start_matches(',').trim_start();
    let rest = rest.strip_prefix("Name:")?;
    let team_at = rest.rfind("Team:")?;
    let team = Team::from_log_name(rest[team_at + "Team:".len()..].trim())?;
    let mut name = &rest[..team_at];
    if comma_separated {
        // 只去掉分隔用的那一个逗号，名字本身以逗号结尾时要保留
        name = name.trim_end_matches(' ');
        name = name.strip_suffix(',').unwrap_or(name);
    }
    if is_ai {
        return Some(RosterLine::Ai);
    }
    let name = name.trim();
    Some(RosterLine::Player(RosterEntry {
        id,
        name: (!name.is_empty()).then(|| name.to_owned()),
        team,
    }))
}

#[cfg(test)]
mod tests;
