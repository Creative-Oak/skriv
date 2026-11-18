// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! The two proofing languages.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Lang {
    #[serde(rename = "da")]
    Da,
    #[serde(rename = "en")]
    En,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::Da, Lang::En];

    pub fn code(self) -> &'static str {
        match self {
            Lang::Da => "da",
            Lang::En => "en",
        }
    }

    pub fn from_code(code: &str) -> Option<Lang> {
        match code {
            "da" => Some(Lang::Da),
            "en" => Some(Lang::En),
            _ => None,
        }
    }

    pub fn other(self) -> Lang {
        match self {
            Lang::Da => Lang::En,
            Lang::En => Lang::Da,
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }
}

impl std::fmt::Display for Lang {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
