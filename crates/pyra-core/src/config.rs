use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
pub struct GlobalConfig {
    #[serde(default)]
    pub default_python: Option<String>,
}
