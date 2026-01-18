use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    pub name: String,
    pub retries: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            name: "fixture".to_string(),
            retries: 3,
        }
    }
}

