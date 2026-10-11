use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Signal {
    IceConfig {
        ice_servers: Vec<crate::connectivity::IceServer>,
    },
    Offer {
        sdp: String,
    },
    Answer {
        sdp: String,
    },
    Ice {
        candidate: String,
        sdp_mline_index: u32,
    },
    Keyframe {},
    Error {
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_signals_are_rejected() {
        assert!(serde_json::from_str::<Signal>(r#"{"type":"ice","candidate":"x"}"#).is_err());
        assert!(serde_json::from_str::<Signal>(r#"{"type":"keyframe","unexpected":1}"#).is_err());
    }
}
