use super::{CoreEngine, EngineError};

impl CoreEngine {
    pub fn set_device_volume(&mut self, device_id: &str, percent: u8) -> Result<(), EngineError> {
        self.adapter
            .set_device_volume(&self.graph, device_id, percent)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        self.refresh_graph()?;
        Ok(())
    }

    pub fn set_device_mute(&mut self, device_id: &str, muted: bool) -> Result<(), EngineError> {
        self.adapter
            .set_device_mute(&self.graph, device_id, muted)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        self.refresh_graph()?;
        Ok(())
    }

    pub fn set_stream_volume(&mut self, stream_id: &str, percent: u8) -> Result<(), EngineError> {
        self.adapter
            .set_stream_volume(&self.graph, stream_id, percent)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        self.refresh_graph()?;
        Ok(())
    }

    pub fn set_stream_mute(&mut self, stream_id: &str, muted: bool) -> Result<(), EngineError> {
        self.adapter
            .set_stream_mute(&self.graph, stream_id, muted)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        self.refresh_graph()?;
        Ok(())
    }
}
