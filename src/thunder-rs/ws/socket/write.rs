// server/ws/write.rs

use super::socket::Socket;
use crate::ws::error::SendError;
use crate::ws::frame::send_frame_inner;
use crate::ws::opcode::Opcode;
use serde::Serialize;

impl Socket {
    /// Sends a WebSocket frame with specified opcode and payload
    /// Returns SendError if sending fails, including the failed payload data
    pub async fn send(&self, opcode: Opcode, payload: &[u8]) -> Result<(), SendError> {
        let result = send_frame_inner(&self.writer, opcode as u8, payload).await;

        match result {
            Ok(()) => Ok(()),
            Err(e) => Err(SendError {
                error: e,
                data: payload.to_vec(),
            }),
        }
    }
    /// Serializes data to JSON and sends as text frame
    pub async fn send_json<T: Serialize>(&self, data: &T) -> Result<(), SendError> {
        let json_string = serde_json::to_string(data).map_err(|e| SendError {
            error: e.into(),
            data: Vec::new(),
        })?;

        let json_payload = json_string.as_bytes();

        self.send(Opcode::Text, json_payload).await
    }
    /// Convenience method for sending binary data
    pub async fn send_binary(&self, data: Vec<u8>) -> Result<(), SendError> {
        match self.send(Opcode::Binary, &data).await {
            Ok(()) => Ok(()),
            Err(err) => Err(err),
        }
    }
    /// Convenience method for sending text data
    pub async fn send_text(&self, data: String) -> Result<(), SendError> {
        match self.send(Opcode::Text, data.as_bytes()).await {
            Ok(()) => Ok(()),
            Err(err) => Err(err),
        }
    }
    /// Sends large payloads by splitting into smaller chunks
    /// Useful for avoiding memory issues with very large messages
    pub async fn send_large(
        &self,
        opcode: Opcode,
        payload: &[u8],
        max_chunk_size: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut offset = 0;
        let op_value = opcode;

        while offset < payload.len() {
            let end = std::cmp::min(offset + max_chunk_size, payload.len());
            let chunk = &payload[offset..end];

            self.send(op_value.clone(), chunk).await?;
            offset = end;
        }

        Ok(())
    }
}
