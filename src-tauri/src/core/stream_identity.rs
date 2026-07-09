use crate::core::models::{Stream, StreamRouteRule};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct StreamIdentityKey {
    pub app_name: String,
    pub executable: Option<String>,
    pub media_name: Option<String>,
}

pub fn stream_identity_key(stream: &Stream) -> StreamIdentityKey {
    StreamIdentityKey {
        app_name: stream.app_name.clone(),
        executable: stream.executable.clone(),
        media_name: stream.media_name.clone(),
    }
}

pub fn rule_identity_key(rule: &StreamRouteRule) -> StreamIdentityKey {
    StreamIdentityKey {
        app_name: rule.app_name.clone().unwrap_or_default(),
        executable: rule.executable.clone(),
        media_name: rule.media_name.clone(),
    }
}

pub fn identity_matches(stream_key: &StreamIdentityKey, override_key: &StreamIdentityKey) -> bool {
    if let (Some(stream_exe), Some(override_exe)) =
        (&stream_key.executable, &override_key.executable)
    {
        if stream_exe == override_exe {
            return true;
        }
    }

    stream_key.app_name == override_key.app_name
        && stream_key.media_name == override_key.media_name
}

pub fn stream_display_label(stream: &Stream) -> String {
    if let Some(executable) = &stream.executable {
        if executable != &stream.app_name {
            return format!("{} · {}", stream.app_name, executable);
        }
    }
    stream.app_name.clone()
}

pub fn parse_stream_identity(props: &serde_json::Map<String, serde_json::Value>) -> (String, Option<String>) {
    let app_name = prop_str(props, "application.name");
    let executable = {
        let binary = prop_str(props, "application.process.binary");
        if binary.is_empty() {
            None
        } else {
            Some(binary)
        }
    };

    let app_name = if !app_name.is_empty() {
        app_name
    } else if let Some(exe) = &executable {
        exe.clone()
    } else {
        for key in ["node.nick", "node.name", "media.name"] {
            let value = prop_str(props, key);
            if !value.is_empty() {
                return (value, executable);
            }
        }
        "Unknown Stream".into()
    };

    (app_name, executable)
}

fn prop_str(props: &serde_json::Map<String, serde_json::Value>, key: &str) -> String {
    props
        .get(key)
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_application_name_and_executable_separately() {
        let props = serde_json::json!({
            "application.name": "Firefox",
            "application.process.binary": "firefox",
            "media.name": "AudioCallback"
        });
        let (app_name, executable) =
            parse_stream_identity(props.as_object().expect("props object"));
        assert_eq!(app_name, "Firefox");
        assert_eq!(executable.as_deref(), Some("firefox"));
    }

    #[test]
    fn falls_back_to_executable_when_application_name_missing() {
        let props = serde_json::json!({
            "application.process.binary": "discord",
            "node.name": "discord-sink"
        });
        let (app_name, executable) =
            parse_stream_identity(props.as_object().expect("props object"));
        assert_eq!(app_name, "discord");
        assert_eq!(executable.as_deref(), Some("discord"));
    }
}
