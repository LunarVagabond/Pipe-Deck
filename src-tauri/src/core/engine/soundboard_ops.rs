use std::path::Path;

use super::{CoreEngine, EngineError};

impl CoreEngine {
    /// Plays `path` into `target_device_id`'s underlying device (a virtual
    /// input or a hardware input passthrough). Fire-and-forget — see
    /// `AudioBackend::play_sound` for what that does and doesn't guarantee.
    pub fn play_sound(&self, path: &Path, target_device_id: &str) -> Result<(), EngineError> {
        let device = self
            .graph
            .devices
            .iter()
            .find(|device| device.id == target_device_id)
            .ok_or_else(|| EngineError::NotFound(format!("device not found: {target_device_id}")))?;

        self.adapter
            .play_sound(path, &device.system_name)
            .map_err(|error| EngineError::Adapter(error.to_string()))
    }
}

#[cfg(test)]
mod live_tests {
    //! `#[ignore]`d: hits a real PipeWire session, same rationale as
    //! `virtual_ops::live_tests`. Creates and tears down its own disposable
    //! virtual input.
    use super::*;

    #[test]
    #[ignore]
    fn play_sound_starts_playback_into_a_real_virtual_input() {
        assert_ne!(std::env::var("PIPE_DECK_USE_MOCK").as_deref(), Ok("1"));

        let mut engine = CoreEngine::new();
        engine.refresh_graph().expect("initial graph refresh");

        let created = engine
            .create_virtual_input("Pipe Deck Soundboard Playback Test")
            .expect("create disposable test device");

        let clip = Path::new("/usr/share/sounds/speech-dispatcher/test.wav");
        assert!(clip.is_file(), "expected a system test wav to exist at {}", clip.display());

        let result = engine.play_sound(clip, &created.device_id);

        let _ = engine.remove_virtual_device(&created.system_name);

        result.expect("play_sound should succeed against a real virtual input");
    }
}
