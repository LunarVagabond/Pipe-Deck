use crate::core::models::{RecentStreamIdentity, Stream, StreamDirection};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_ENTRIES: usize = 64;
const MAX_AGE_SECS: u64 = 60 * 60;

#[derive(Debug, Default)]
pub struct RecentStreamCache {
    entries: HashMap<String, RecentStreamIdentity>,
}

impl RecentStreamCache {
    pub fn record_streams(&mut self, streams: &[Stream]) {
        let now = now_secs();
        for stream in streams {
            let key = fingerprint(stream);
            self.entries.insert(
                key,
                RecentStreamIdentity {
                    app_name: stream.app_name.clone(),
                    executable: stream.executable.clone(),
                    window_class: stream.window_class.clone(),
                    system_name: stream.system_name.clone(),
                    media_name: stream.media_name.clone(),
                    direction: stream.direction.clone(),
                    is_system: stream.is_system,
                    last_seen_secs: now,
                    is_live: false,
                },
            );
        }
        self.prune(now);
    }

    pub fn list(&self, live_streams: &[Stream]) -> Vec<RecentStreamIdentity> {
        let live_keys: std::collections::HashSet<String> =
            live_streams.iter().map(fingerprint).collect();

        let mut entries: Vec<RecentStreamIdentity> = self
            .entries
            .values()
            .cloned()
            .map(|mut entry| {
                entry.is_live = live_keys.contains(&fingerprint_from_identity(&entry));
                entry
            })
            .collect();

        entries.sort_by(|left, right| {
            right
                .last_seen_secs
                .cmp(&left.last_seen_secs)
                .then_with(|| left.app_name.cmp(&right.app_name))
        });
        entries
    }

    pub fn synthetic_streams(&self, live_streams: &[Stream]) -> Vec<Stream> {
        let live_keys: std::collections::HashSet<String> =
            live_streams.iter().map(fingerprint).collect();

        self.entries
            .values()
            .filter(|entry| !live_keys.contains(&fingerprint_from_identity(entry)))
            .map(|entry| Stream {
                id: format!("recent-{}", fingerprint_from_identity(entry)),
                app_name: entry.app_name.clone(),
                executable: entry.executable.clone(),
                window_class: entry.window_class.clone(),
                system_name: entry.system_name.clone(),
                direction: entry.direction.clone(),
                current_target: None,
                current_targets: Vec::new(),
                media_name: entry.media_name.clone(),
                is_system: entry.is_system,
                volume_percent: None,
                muted: None,
                route_explanation: None,
            })
            .collect()
    }

    fn prune(&mut self, now: u64) {
        self.entries
            .retain(|_, entry| now.saturating_sub(entry.last_seen_secs) <= MAX_AGE_SECS);

        if self.entries.len() <= MAX_ENTRIES {
            return;
        }

        let mut ranked: Vec<(String, u64)> = self
            .entries
            .iter()
            .map(|(key, entry)| (key.clone(), entry.last_seen_secs))
            .collect();
        ranked.sort_by_key(|(_, seen)| std::cmp::Reverse(*seen));
        ranked.truncate(MAX_ENTRIES);
        let keep: std::collections::HashSet<String> = ranked.into_iter().map(|(key, _)| key).collect();
        self.entries.retain(|key, _| keep.contains(key));
    }
}

pub fn fingerprint(stream: &Stream) -> String {
    fingerprint_parts(
        &stream.app_name,
        stream.executable.as_deref(),
        stream.media_name.as_deref(),
        stream.window_class.as_deref(),
        &stream.direction,
    )
}

fn fingerprint_from_identity(entry: &RecentStreamIdentity) -> String {
    fingerprint_parts(
        &entry.app_name,
        entry.executable.as_deref(),
        entry.media_name.as_deref(),
        entry.window_class.as_deref(),
        &entry.direction,
    )
}

fn fingerprint_parts(
    app_name: &str,
    executable: Option<&str>,
    media_name: Option<&str>,
    window_class: Option<&str>,
    direction: &StreamDirection,
) -> String {
    format!(
        "{app_name}|{}|{}|{}|{direction:?}",
        executable.unwrap_or(""),
        media_name.unwrap_or(""),
        window_class.unwrap_or(""),
    )
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::StreamDirection;

    fn sample_stream(app_name: &str) -> Stream {
        Stream {
            id: "node-1".into(),
            app_name: app_name.into(),
            executable: None,
            window_class: None,
            system_name: None,
            direction: StreamDirection::Playback,
            current_target: None,
            current_targets: Vec::new(),
            media_name: None,
            is_system: false,
            volume_percent: None,
            muted: None,
            route_explanation: None,
        }
    }

    #[test]
    fn remembers_streams_after_they_disappear() {
        let mut cache = RecentStreamCache::default();
        cache.record_streams(&[sample_stream("mpv")]);
        let synthetic = cache.synthetic_streams(&[]);
        assert_eq!(synthetic.len(), 1);
        assert_eq!(synthetic[0].app_name, "mpv");

        cache.record_streams(&[]);
        let still = cache.synthetic_streams(&[]);
        assert_eq!(still.len(), 1);
    }

    #[test]
    fn live_streams_are_not_duplicated_in_synthetic_list() {
        let mut cache = RecentStreamCache::default();
        let live = sample_stream("firefox");
        cache.record_streams(std::slice::from_ref(&live));
        let synthetic = cache.synthetic_streams(std::slice::from_ref(&live));
        assert!(synthetic.is_empty());
    }
}
