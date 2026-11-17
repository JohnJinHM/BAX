//! 玩家与比赛的标识。

use std::fmt;
use std::num::ParseIntError;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// 游戏内部的玩家 ID。玩家以它为准，名字只是附加信息。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PlayerId(pub u64);

/// 比赛 ID，日志里叫 fight id，BATrace 里叫 match id。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct MatchId(pub u64);

macro_rules! id_impls {
    ($t:ident) => {
        impl $t {
            pub const fn get(self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $t {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $t {
            type Err = ParseIntError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                s.trim().parse().map($t)
            }
        }

        impl From<u64> for $t {
            fn from(v: u64) -> Self {
                $t(v)
            }
        }
    };
}

id_impls!(PlayerId);
id_impls!(MatchId);
