//! 对局生命周期。
//!
//! [`Tracker`] 按顺序吃进日志事件，维护“当前这场比赛”的状态，并告诉调用方发生了什么变化。
//! 它遵守三条规则：
//!
//! 1. 状态以比赛 ID 为键，同一事件重复出现时结果不变。游戏对同一件事会写新旧两种日志，
//!    重读日志时事件也会再来一遍。
//! 2. 它不关心事件是实时的还是回放的。回放历史日志得到的状态与当时实时运行得到的状态一致，
//!    要不要因此发起请求或提醒由调用方决定。
//! 3. 玩家以 ID 标识。名单里的名字可以为空。
//!
//! 流程是按真实日志归纳出来的。正常开局的事件顺序是：
//! 进入等待室 → 开始加载 → 进入对局房间 → 名单 → 场景加载完成 → 比赛结束 → 离开对局房间。
//! 重连时游戏先进等待室、状态为 `InProcess`，场景加载完成早于进入对局房间，
//! 此时这个事件不能当作计时起点。

use serde::{Deserialize, Serialize};

use crate::event::{ExitCause, LogEvent, RoomKind, RoomStatus};
use crate::ids::{MatchId, PlayerId};
use crate::roster::{RosterEntry, Team};
use crate::time::Timestamp;

/// 比赛所处的阶段。顺序即先后。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    /// 匹配成功，在等待室里等其他玩家。
    WaitingRoom,
    /// 加载战场，直到所有客户端就绪。
    Loading,
    /// 准备阶段：场景加载完成之后、正式开局之前。
    Deployment,
    /// 正式开局之后。
    InBattle,
    /// 比赛已结束。
    Ended,
}

/// 本机与比赛服务器的连接情况。它与 [`Phase`] 无关：断线之后比赛照常进行。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Connection {
    Connected,
    /// 已离开房间，比赛可能仍在进行。
    Disconnected,
    /// 正在回到一场进行中的比赛，尚未进入对局房间。
    Reconnecting,
}

/// 计时基准的来源。排在后面的更可信，可以覆盖前面的。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClockSource {
    /// 由日志推算：场景加载完成的时刻加上准备阶段的长度。
    LogEstimate,
    /// 用户对照游戏画面手动校准。
    Manual,
    /// BATrace 战报里的开始时间。
    Report,
}

/// 一场比赛的计时基准：正式开局的时刻。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BattleClock {
    pub start_at: Timestamp,
    pub source: ClockSource,
}

/// 等待室里先于名单出现的玩家，队伍未知。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EarlyPlayer {
    pub id: PlayerId,
    pub name: Option<String>,
}

/// 各关键时刻。没发生或没观察到的为 `None`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Timeline {
    /// 第一次见到这场比赛。
    pub discovered_at: Timestamp,
    pub loading_at: Option<Timestamp>,
    pub game_room_at: Option<Timestamp>,
    pub scenario_loaded_at: Option<Timestamp>,
    /// 观察到比赛结束的时刻。重连进入一场已结束的比赛时不知道它何时结束，为 `None`。
    pub ended_at: Option<Timestamp>,
    /// 最近一次离开房间。
    pub left_at: Option<Timestamp>,
}

/// 一场比赛的状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchState {
    pub match_id: MatchId,
    pub phase: Phase,
    pub connection: Connection,
    /// 进入时比赛已经在进行（重连）。这种情况下日志给不出计时起点。
    pub rejoined: bool,
    pub map: Option<String>,
    pub scenario: Option<String>,
    pub deck: Option<String>,
    pub game_version: Option<String>,
    pub early: Vec<EarlyPlayer>,
    pub roster: Vec<RosterEntry>,
    /// 本机玩家在这场比赛里的名字（来自 `FID` 行）。
    pub self_name: Option<String>,
    pub self_id: Option<PlayerId>,
    pub clock: Option<BattleClock>,
    pub times: Timeline,
    /// 比赛结束的原因；未结束或原因未知时为 `None`。
    pub end_cause: Option<ExitCause>,
}

impl MatchState {
    fn new(
        match_id: MatchId,
        at: Timestamp,
        status: Option<RoomStatus>,
        reconnect_hint: bool,
    ) -> Self {
        let (phase, rejoined) = match status {
            Some(RoomStatus::WaitClients) => (Phase::WaitingRoom, false),
            Some(RoomStatus::DeploymentPhase) => (Phase::Deployment, true),
            Some(RoomStatus::InProcess) => (Phase::InBattle, true),
            Some(RoomStatus::End) => (Phase::Ended, true),
            // 旧版日志没有状态字段，只能靠“开始重连”的标记判断
            Some(RoomStatus::Unknown) | None if reconnect_hint => (Phase::InBattle, true),
            Some(RoomStatus::Unknown) | None => (Phase::WaitingRoom, false),
        };
        MatchState {
            match_id,
            phase,
            connection: if rejoined {
                Connection::Reconnecting
            } else {
                Connection::Connected
            },
            rejoined,
            map: None,
            scenario: None,
            deck: None,
            game_version: None,
            early: Vec::new(),
            roster: Vec::new(),
            self_name: None,
            self_id: None,
            clock: None,
            times: Timeline {
                discovered_at: at,
                loading_at: None,
                game_room_at: None,
                scenario_loaded_at: None,
                ended_at: None,
                left_at: None,
            },
            end_cause: None,
        }
    }

    /// 本机玩家所在的队伍。
    pub fn my_team(&self) -> Option<Team> {
        let me = self.self_id?;
        self.roster.iter().find(|p| p.id == me).map(|p| p.team)
    }

    /// 正式开局以来经过的毫秒数；尚未开局时为负，没有计时基准时为 `None`。
    pub fn elapsed_ms(&self, now: Timestamp) -> Option<i64> {
        self.clock.map(|c| now.millis_since(c.start_at))
    }

    /// 已知的全部玩家 ID：有名单用名单，否则用等待室里先到的玩家。
    pub fn known_player_ids(&self) -> Vec<PlayerId> {
        if self.roster.is_empty() {
            self.early.iter().map(|p| p.id).collect()
        } else {
            self.roster.iter().map(|p| p.id).collect()
        }
    }
}

/// [`Tracker::apply`] 报告的变化。调用方据此决定要做什么：查询玩家、保存记录、获取战报。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Update {
    /// 出现了一场新比赛。
    MatchDiscovered { match_id: MatchId },
    /// 上一场比赛没有等到结束信号就被新比赛取代。它可能在断线期间已经结束。
    MatchSuperseded { match_id: MatchId },
    /// 等待室里新出现的玩家。
    EarlyPlayers {
        match_id: MatchId,
        players: Vec<PlayerId>,
    },
    /// 完整名单已知，或者发生了变化。
    RosterReady { match_id: MatchId },
    /// 计时基准被设定或修改。
    ClockSet {
        match_id: MatchId,
        clock: BattleClock,
    },
    /// 比赛结束。每场比赛只报告一次。
    MatchEnded { match_id: MatchId },
    /// 与房间断开，比赛本身可能仍在进行。
    Disconnected { match_id: MatchId, cause: ExitCause },
    /// 其余变化：阶段、地图、卡组、本机身份、连接恢复。
    Changed { match_id: MatchId },
}

/// [`Tracker`] 的可调参数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackerConfig {
    /// 准备阶段的长度：场景加载完成到正式开局的间隔，毫秒。
    ///
    /// 41 场样本里，战报的开始时间比日志的场景加载完成晚 58.1～59.8 秒，中位数 59.2 秒。
    /// 默认取 60 秒，正式采用前还要对照游戏画面的倒计时校准。
    pub deployment_ms: i64,
}

impl Default for TrackerConfig {
    fn default() -> Self {
        TrackerConfig {
            deployment_ms: 60_000,
        }
    }
}

/// 对局跟踪器：把日志事件归并成当前比赛的状态。
#[derive(Debug, Clone)]
pub struct Tracker {
    config: TrackerConfig,
    current: Option<MatchState>,
    persona: Option<String>,
    game_version: Option<String>,
    deck: Option<String>,
    /// 见到了“开始重连”，下一次进入等待室是重连。
    reconnect_pending: bool,
    self_hint: Option<PlayerId>,
}

impl Default for Tracker {
    fn default() -> Self {
        Tracker::new(TrackerConfig::default())
    }
}

impl Tracker {
    pub fn new(config: TrackerConfig) -> Self {
        Tracker {
            config,
            current: None,
            persona: None,
            game_version: None,
            deck: None,
            reconnect_pending: false,
            self_hint: None,
        }
    }

    pub fn config(&self) -> &TrackerConfig {
        &self.config
    }

    /// 当前这场比赛。没有比赛，或者上一场已被清除时为 `None`。
    pub fn current(&self) -> Option<&MatchState> {
        self.current.as_ref()
    }

    /// 本机的 Steam 昵称。
    pub fn persona(&self) -> Option<&str> {
        self.persona.as_deref()
    }

    pub fn game_version(&self) -> Option<&str> {
        self.game_version.as_deref()
    }

    /// 忘掉当前比赛。
    pub fn clear(&mut self) {
        self.current = None;
    }

    /// 告诉跟踪器本机玩家的 ID。名单里有这个 ID 时直接采用，比按名字匹配可靠。
    pub fn set_self_hint(&mut self, hint: Option<PlayerId>) -> Vec<Update> {
        self.self_hint = hint;
        let mut out = Vec::new();
        self.refresh_self(&mut out);
        out
    }

    /// 设定某场比赛的计时基准，用于恢复已保存的基准、手动校准和战报修正。
    ///
    /// 来源不如现有基准可信时不生效。比赛不是当前这场时也不生效。
    pub fn set_clock(&mut self, match_id: MatchId, clock: BattleClock) -> Vec<Update> {
        let mut out = Vec::new();
        if let Some(m) = self.current.as_mut()
            && m.match_id == match_id
            && m.clock.is_none_or(|old| clock.source >= old.source)
            && m.clock != Some(clock)
        {
            m.clock = Some(clock);
            out.push(Update::ClockSet { match_id, clock });
        }
        out
    }

    /// 处理一条日志事件。`at` 是事件发生的时刻。
    pub fn apply(&mut self, at: Timestamp, event: &LogEvent) -> Vec<Update> {
        let mut out = Vec::new();
        match event {
            LogEvent::GameVersion { version } => {
                self.game_version = Some(version.clone());
            }
            LogEvent::Persona { name } => {
                self.persona = Some(name.clone());
                self.refresh_self(&mut out);
            }
            LogEvent::DeckSelected { name } => self.on_deck(name.as_deref(), &mut out),
            LogEvent::ReconnectStarted => self.reconnect_pending = true,
            LogEvent::RoomEntered {
                room,
                match_id,
                status,
            } => self.on_room_entered(at, *room, *match_id, *status, &mut out),
            LogEvent::RoomExited {
                room,
                match_id,
                cause,
                status,
            } => self.on_room_exited(at, *room, *match_id, *cause, *status, &mut out),
            LogEvent::RoomClient { room, player, name } => {
                self.on_room_client(*room, *player, name.as_deref(), &mut out)
            }
            LogEvent::LoadingStarted { map, scenario } => {
                self.on_loading(at, map, scenario.as_deref(), &mut out)
            }
            LogEvent::FightInfo {
                match_id,
                player_name,
            } => {
                if let Some(m) = self.current.as_mut()
                    && m.match_id == *match_id
                    && player_name.is_some()
                    && m.self_name != *player_name
                {
                    m.self_name = player_name.clone();
                    self.refresh_self(&mut out);
                }
            }
            LogEvent::PlayerList { players } => self.on_player_list(players, &mut out),
            LogEvent::ScenarioLoaded => self.on_scenario_loaded(at, &mut out),
            LogEvent::MatchEnded { match_id } => {
                if let Some(m) = self.current.as_mut()
                    && m.match_id == *match_id
                {
                    mark_ended(m, Some(at), Some(ExitCause::MatchEnded), &mut out);
                }
            }
        }
        out
    }

    /// 按当前时间推进由时间决定的阶段：准备阶段结束后进入正式对局。
    pub fn advance(&mut self, now: Timestamp) -> Vec<Update> {
        let mut out = Vec::new();
        let Some(m) = self.current.as_mut() else {
            return out;
        };
        let next = match (m.phase, m.clock) {
            (Phase::Deployment, Some(c)) if now >= c.start_at => Some(Phase::InBattle),
            // 手动把开局时刻往后校准时，退回准备阶段
            (Phase::InBattle, Some(c)) if now < c.start_at && !m.rejoined => {
                Some(Phase::Deployment)
            }
            // 在准备阶段中途进入的比赛没有计时基准，最迟一个准备阶段之后必然已经开局
            (Phase::Deployment, None)
                if now.millis_since(m.times.discovered_at) >= self.config.deployment_ms =>
            {
                Some(Phase::InBattle)
            }
            _ => None,
        };
        if let Some(phase) = next {
            m.phase = phase;
            out.push(Update::Changed {
                match_id: m.match_id,
            });
        }
        out
    }

    fn on_deck(&mut self, name: Option<&str>, out: &mut Vec<Update>) {
        self.deck = name.map(str::to_owned);
        // 进入等待室之后游戏会再报一次卡组；对局结束后会报空值，这时不能清掉比赛记录里的卡组
        if let (Some(m), Some(name)) = (self.current.as_mut(), name)
            && m.phase <= Phase::Loading
            && m.connection == Connection::Connected
            && m.deck.as_deref() != Some(name)
        {
            m.deck = Some(name.to_owned());
            out.push(Update::Changed {
                match_id: m.match_id,
            });
        }
    }

    fn on_room_entered(
        &mut self,
        at: Timestamp,
        room: RoomKind,
        match_id: MatchId,
        status: Option<RoomStatus>,
        out: &mut Vec<Update>,
    ) {
        let same = self
            .current
            .as_ref()
            .is_some_and(|m| m.match_id == match_id);
        if !same {
            if let Some(old) = self.current.take()
                && old.phase != Phase::Ended
            {
                out.push(Update::MatchSuperseded {
                    match_id: old.match_id,
                });
            }
            let hint = std::mem::take(&mut self.reconnect_pending);
            let mut m = MatchState::new(match_id, at, status, hint);
            m.game_version = self.game_version.clone();
            if !m.rejoined {
                m.deck = self.deck.clone();
            }
            if room == RoomKind::GameRoom {
                m.times.game_room_at = Some(at);
                m.connection = Connection::Connected;
            }
            let ended = m.phase == Phase::Ended;
            self.current = Some(m);
            out.push(Update::MatchDiscovered { match_id });
            if ended {
                out.push(Update::MatchEnded { match_id });
            }
            return;
        }

        let Some(m) = self.current.as_mut() else {
            return;
        };
        match room {
            RoomKind::WaitingRoom => {
                // 同一条消息的新旧两种写法，或者重读日志：什么都不用做
                let duplicate = m.connection == Connection::Reconnecting
                    || (m.connection == Connection::Connected && m.times.game_room_at.is_none());
                if duplicate {
                    return;
                }
                // 断线之后回到同一场比赛
                self.reconnect_pending = false;
                m.rejoined = true;
                m.connection = Connection::Reconnecting;
                if status == Some(RoomStatus::End) {
                    mark_ended(m, None, None, out);
                } else if m.phase < Phase::InBattle && status == Some(RoomStatus::InProcess) {
                    m.phase = Phase::InBattle;
                }
                out.push(Update::Changed { match_id });
            }
            RoomKind::GameRoom => {
                let mut changed = false;
                if m.times.game_room_at.is_none() {
                    m.times.game_room_at = Some(at);
                    changed = true;
                }
                if m.connection != Connection::Connected {
                    m.connection = Connection::Connected;
                    changed = true;
                }
                match status {
                    Some(RoomStatus::InProcess) if m.phase < Phase::InBattle => {
                        m.phase = Phase::InBattle;
                        m.rejoined = true;
                        changed = true;
                    }
                    Some(RoomStatus::DeploymentPhase) if m.phase < Phase::Deployment => {
                        m.phase = Phase::Deployment;
                        m.rejoined = true;
                        changed = true;
                    }
                    Some(RoomStatus::End) => mark_ended(m, None, None, out),
                    _ => {}
                }
                if changed {
                    out.push(Update::Changed { match_id });
                }
            }
        }
    }

    fn on_room_exited(
        &mut self,
        at: Timestamp,
        room: RoomKind,
        match_id: Option<MatchId>,
        cause: Option<ExitCause>,
        status: Option<RoomStatus>,
        out: &mut Vec<Update>,
    ) {
        let Some(m) = self.current.as_mut() else {
            return;
        };
        if match_id.is_some_and(|id| id != m.match_id) {
            return;
        }
        let ended = status == Some(RoomStatus::End) || cause == Some(ExitCause::MatchEnded);
        match (room, cause) {
            // 从等待室转入对局房间，或者旧版日志里不带原因的标记：连接没有断
            (RoomKind::WaitingRoom, None | Some(ExitCause::ServerExitInstance)) => {
                if status == Some(RoomStatus::End) {
                    mark_ended(m, None, None, out);
                }
            }
            _ if ended => {
                let already = m.phase == Phase::Ended;
                mark_ended(m, Some(at), cause, out);
                let was_connected = m.connection != Connection::Disconnected;
                m.connection = Connection::Disconnected;
                m.times.left_at = Some(at);
                if already && was_connected {
                    out.push(Update::Changed {
                        match_id: m.match_id,
                    });
                }
            }
            (_, Some(cause)) => {
                if m.connection != Connection::Disconnected {
                    m.connection = Connection::Disconnected;
                    m.times.left_at = Some(at);
                    out.push(Update::Disconnected {
                        match_id: m.match_id,
                        cause,
                    });
                }
            }
            // 旧版日志的 `GameRoom exited`：只知道离开了，不知道比赛是否结束
            (RoomKind::GameRoom, None) => {
                if m.connection != Connection::Disconnected {
                    m.connection = Connection::Disconnected;
                    m.times.left_at = Some(at);
                    out.push(Update::Changed {
                        match_id: m.match_id,
                    });
                }
            }
        }
    }

    fn on_room_client(
        &mut self,
        room: RoomKind,
        player: PlayerId,
        name: Option<&str>,
        out: &mut Vec<Update>,
    ) {
        if room != RoomKind::WaitingRoom {
            return;
        }
        if let Some(m) = self.current.as_mut()
            && m.phase <= Phase::Loading
            && m.connection == Connection::Connected
            && m.roster.is_empty()
            && !m.early.iter().any(|p| p.id == player)
        {
            m.early.push(EarlyPlayer {
                id: player,
                name: name.map(str::to_owned),
            });
            out.push(Update::EarlyPlayers {
                match_id: m.match_id,
                players: vec![player],
            });
        }
    }

    fn on_loading(
        &mut self,
        at: Timestamp,
        map: &str,
        scenario: Option<&str>,
        out: &mut Vec<Update>,
    ) {
        // 单人和遭遇战也会打印这条日志。只有刚进等待室、或正在重连时，它才属于当前比赛
        if let Some(m) = self.current.as_mut()
            && (m.phase == Phase::WaitingRoom || m.connection == Connection::Reconnecting)
        {
            m.map = Some(map.to_owned());
            m.scenario = scenario.map(str::to_owned);
            if m.times.loading_at.is_none() {
                m.times.loading_at = Some(at);
            }
            if m.phase == Phase::WaitingRoom {
                m.phase = Phase::Loading;
            }
            out.push(Update::Changed {
                match_id: m.match_id,
            });
        }
    }

    fn on_player_list(&mut self, players: &[RosterEntry], out: &mut Vec<Update>) {
        // 名单必须紧跟在进入对局房间之后；其他场合打印的名单不属于当前比赛
        if let Some(m) = self.current.as_mut()
            && m.times.game_room_at.is_some()
            && m.connection == Connection::Connected
            && m.phase != Phase::Ended
            && m.roster != players
        {
            m.roster = players.to_vec();
            m.self_id = identify_self(
                &m.roster,
                self.self_hint,
                [m.self_name.as_deref(), self.persona.as_deref()],
            );
            out.push(Update::RosterReady {
                match_id: m.match_id,
            });
        }
    }

    fn on_scenario_loaded(&mut self, at: Timestamp, out: &mut Vec<Update>) {
        // 重连时这条日志早于进入对局房间，而且比赛早已开始，不能用来计时
        if let Some(m) = self.current.as_mut()
            && !m.rejoined
            && m.phase == Phase::Loading
            && m.times.game_room_at.is_some()
            && m.times.scenario_loaded_at.is_none()
        {
            m.times.scenario_loaded_at = Some(at);
            m.phase = Phase::Deployment;
            let estimate = BattleClock {
                start_at: at.plus_millis(self.config.deployment_ms),
                source: ClockSource::LogEstimate,
            };
            if m.clock.is_none_or(|c| c.source == ClockSource::LogEstimate) {
                m.clock = Some(estimate);
                out.push(Update::ClockSet {
                    match_id: m.match_id,
                    clock: estimate,
                });
            } else {
                out.push(Update::Changed {
                    match_id: m.match_id,
                });
            }
        }
    }

    fn refresh_self(&mut self, out: &mut Vec<Update>) {
        if let Some(m) = self.current.as_mut()
            && !m.roster.is_empty()
        {
            let found = identify_self(
                &m.roster,
                self.self_hint,
                [m.self_name.as_deref(), self.persona.as_deref()],
            );
            if found != m.self_id {
                m.self_id = found;
                out.push(Update::Changed {
                    match_id: m.match_id,
                });
            }
        }
    }
}

fn mark_ended(
    m: &mut MatchState,
    at: Option<Timestamp>,
    cause: Option<ExitCause>,
    out: &mut Vec<Update>,
) {
    if m.phase == Phase::Ended {
        return;
    }
    m.phase = Phase::Ended;
    m.times.ended_at = at;
    m.end_cause = cause;
    out.push(Update::MatchEnded {
        match_id: m.match_id,
    });
}

/// 在名单里找出本机玩家。
///
/// 先用已知的 ID；没有或不在名单里时按名字找，名字必须恰好匹配到一人。
/// 日志里没有直接给出本机玩家的 ID，所以名字匹配是第一场比赛时唯一的办法。
fn identify_self(
    roster: &[RosterEntry],
    hint: Option<PlayerId>,
    names: [Option<&str>; 2],
) -> Option<PlayerId> {
    if let Some(hint) = hint
        && roster.iter().any(|p| p.id == hint)
    {
        return Some(hint);
    }
    for name in names.into_iter().flatten() {
        let mut matched = roster.iter().filter(|p| p.name.as_deref() == Some(name));
        if let (Some(p), None) = (matched.next(), matched.next()) {
            return Some(p.id);
        }
    }
    None
}

#[cfg(test)]
mod tests;
