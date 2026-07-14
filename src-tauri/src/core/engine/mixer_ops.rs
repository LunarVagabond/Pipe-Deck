use super::{CoreEngine, EngineError};

impl CoreEngine {
    pub fn set_device_volume(&mut self, device_id: &str, percent: u8) -> Result<(), EngineError> {
        if self.graph.data_source == "mock" {
            if let Some(device) = self.graph.devices.iter_mut().find(|device| device.id == device_id) {
                device.volume_percent = Some(percent.min(100));
                return Ok(());
            }
            return Err(EngineError::Adapter(format!("device not found: {device_id}")));
        }

        self.adapter
            .set_device_volume(&self.graph, device_id, percent)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        self.refresh_graph()?;
        Ok(())
    }

    pub fn set_device_mute(&mut self, device_id: &str, muted: bool) -> Result<(), EngineError> {
        if self.graph.data_source == "mock" {
            if let Some(device) = self.graph.devices.iter_mut().find(|device| device.id == device_id) {
                device.muted = Some(muted);
                return Ok(());
            }
            return Err(EngineError::Adapter(format!("device not found: {device_id}")));
        }

        self.adapter
            .set_device_mute(&self.graph, device_id, muted)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        self.refresh_graph()?;
        Ok(())
    }

    pub fn set_stream_volume(&mut self, stream_id: &str, percent: u8) -> Result<(), EngineError> {
        if self.graph.data_source == "mock" {
            if let Some(stream) = self.graph.streams.iter_mut().find(|stream| stream.id == stream_id) {
                stream.volume_percent = Some(percent.min(100));
                return Ok(());
            }
            return Err(EngineError::Adapter(format!("stream not found: {stream_id}")));
        }

        self.adapter
            .set_stream_volume(&self.graph, stream_id, percent)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        self.refresh_graph()?;
        Ok(())
    }

    pub fn set_stream_mute(&mut self, stream_id: &str, muted: bool) -> Result<(), EngineError> {
        if self.graph.data_source == "mock" {
            if let Some(stream) = self.graph.streams.iter_mut().find(|stream| stream.id == stream_id) {
                stream.muted = Some(muted);
                return Ok(());
            }
            return Err(EngineError::Adapter(format!("stream not found: {stream_id}")));
        }

        self.adapter
            .set_stream_mute(&self.graph, stream_id, muted)
            .map_err(|error| EngineError::Adapter(error.to_string()))?;
        self.refresh_graph()?;
        Ok(())
    }
}
