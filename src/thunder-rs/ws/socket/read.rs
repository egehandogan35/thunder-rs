use crate::ws::{
    error::{Error, INTERNAL_ERROR, INVALID_FRAME_PAYLOAD_DATA, PROTOCOL_ERROR},
    frame::{read_and_unmask_payload, read_header},
    opcode::{Opcode, BINARY, CLOSE, CONTINUATION, PING, PONG, TEXT},
};
use encoding_rs::UTF_8;

use super::socket::Socket;
/// Possible results from reading a WebSocket frame
/// Close has a status code and an optional reason 
pub enum ReadResult {
    Text(Vec<u8>),
    Binary(Vec<u8>),
    Ping(Vec<u8>),
    Pong,
    Close(u16, Option<Vec<u8>>),
    Unknown,
}
impl Socket {
    /// Reads and processes a single WebSocket frame
    /// Handles fragmented messages and validates frame format
    /// Returns appropriate ReadResult or Error based on frame type
    pub async fn read_frame(&self) -> Result<ReadResult, Error> {
        let mut message_buffer = Vec::new();
        let mut current_opcode = None;
        let mut decoder = UTF_8.new_decoder();
        let mut temp_string = String::new();
        loop {
            let (fin, opcode, payload_length, masking_key) = read_header(&self.reader).await?;
            let payload =
                read_and_unmask_payload(&self.reader, payload_length, masking_key).await?;

            match opcode {
                CONTINUATION => {
                    if current_opcode.is_none() {
                        return Err(PROTOCOL_ERROR);
                    }

                    message_buffer.extend_from_slice(&payload);

                    if fin == 1 {
                        if let Some(opcode) = current_opcode {
                            return match opcode {
                                TEXT => {
                                    if decoder
                                        .decode_to_string(&message_buffer, &mut temp_string, true)
                                        .2
                                    {
                                        return Err(INVALID_FRAME_PAYLOAD_DATA);
                                    }
                                    Ok(ReadResult::Text(std::mem::take(&mut message_buffer)))
                                }
                                BINARY => {
                                    Ok(ReadResult::Binary(std::mem::take(&mut message_buffer)))
                                }
                                _ => return Err(PROTOCOL_ERROR),
                            };
                        } else {
                            return Err(PROTOCOL_ERROR);
                        }
                    }
                }
                TEXT | BINARY => {
                    if current_opcode.is_some() {
                        return Err(PROTOCOL_ERROR);
                    }
                    current_opcode = Some(opcode);
                    message_buffer.extend_from_slice(&payload);
                    if fin == 1 {
                        return match opcode {
                            TEXT => {
                                if decoder
                                    .decode_to_string(&message_buffer, &mut temp_string, true)
                                    .2
                                {
                                    return Err(INVALID_FRAME_PAYLOAD_DATA);
                                }
                                Ok(ReadResult::Text(std::mem::take(&mut message_buffer)))
                            }
                            BINARY => Ok(ReadResult::Binary(std::mem::take(&mut message_buffer))),
                            _ => return Err(PROTOCOL_ERROR),
                        };
                    }
                }
                CLOSE => return Self::handle_close_frame(&payload),
                PING => {
                    if payload.len() > 125 || fin != 1 {
                        return Err(PROTOCOL_ERROR);
                    } else if current_opcode.is_some() {
                        self.send(Opcode::Pong, &payload)
                            .await
                            .map_err(|_e| INTERNAL_ERROR)?;
                        continue;
                    } else {
                        return Ok(ReadResult::Ping(payload));
                    }
                }
                PONG => {
                    if current_opcode.is_none() {
                        return Ok(ReadResult::Pong);
                    } else {
                        return Err(PROTOCOL_ERROR);
                    }
                }
                _ => return Err(PROTOCOL_ERROR),
            }
        }
    }
    /// Processes WebSocket close frame payload
    /// Validates close code (1000-1015, 3000-4999) and optional UTF-8 reason
    fn handle_close_frame(payload: &[u8]) -> Result<ReadResult, Error> {
        match payload.len() {
            0 => Ok(ReadResult::Close(1000, None)),
            1 => Err(PROTOCOL_ERROR),
            len if len > 125 => Err(PROTOCOL_ERROR),
            _ => {
                let close_code = if payload.len() >= 2 {
                    u16::from_be_bytes([payload[0], payload[1]])
                } else {
                    1005
                };
                if close_code == 1004 || close_code == 1005 || close_code == 1006 {
                    return Err(PROTOCOL_ERROR);
                }
                if (1000..=1015).contains(&close_code) || (3000..=4999).contains(&close_code) {
                    if payload.len() > 2 {
                        let close_reason = &payload[2..];
                        if std::str::from_utf8(close_reason).is_err() {
                            return Err(INVALID_FRAME_PAYLOAD_DATA);
                        }
                    }
                    Ok(ReadResult::Close(close_code, Some(payload.to_vec())))
                } else {
                    Err(PROTOCOL_ERROR)
                }
            }
        }
    }
    /// High-level read function that handles frame processing and error cases
    /// Automatically responds to control frames (ping/pong) and close requests
    /// Validates UTF-8 for text frames

    pub async fn read(&self) -> Result<Option<ReadResult>, Error> {
        let frame_result = self.read_frame().await;

        let frame = match frame_result {
            Ok(frame) => frame,
            Err(e) => {
                return self.handle_error_close(e).await;
            }
        };

        match frame {
            ReadResult::Text(data) => {
                if std::str::from_utf8(&data).is_ok() {
                    Ok(Some(ReadResult::Text(data)))
                } else {
                    let utf8_error = Error {
                        code: 1007,
                        message: "Invalid UTF-8 sequence",
                        error_type: crate::ws::error::ErrorType::NonRecoverable,
                    };
                    self.handle_error_close(utf8_error).await
                }
            }
            ReadResult::Binary(data) => Ok(Some(ReadResult::Binary(data))),
            ReadResult::Ping(data) => {
                self.send(Opcode::Pong, &data).await.map_err(|_e| Error {
                    code: 1011,
                    message: "Internal error, connection closed",
                    error_type: crate::ws::error::ErrorType::NonRecoverable,
                })?;
                Ok(Some(ReadResult::Ping(data)))
            }
            ReadResult::Pong => Ok(Some(ReadResult::Pong)),
            ReadResult::Close(status, data) => {
                let data = data.unwrap_or_default();
                self.close(true, status, Some(data.clone()))
                    .await
                    .map_err(|_e| Error {
                        code: 1011,
                        message: "Internal error, connection closed",
                        error_type: crate::ws::error::ErrorType::NonRecoverable,
                    })?;
                Ok(Some(ReadResult::Close(status, Some(data))))
            }
            ReadResult::Unknown => Ok(Some(ReadResult::Unknown)),
        }
    }
    /// Handles various WebSocket error conditions
    /// Ensures proper connection closure with appropriate status codes
    async fn handle_error_close(&self, e: Error) -> Result<Option<ReadResult>, Error> {
        match e.code {
            1007 => {
                self.close(false, 1007, None).await.map_err(|_e| Error {
                    code: 1011,
                    message: "Internal error, connection closed",
                    error_type: crate::ws::error::ErrorType::NonRecoverable,
                })?;
                Err(Error {
                    code: 1007,
                    message: "Invalid UTF-8 sequence, connection closed",
                    error_type: crate::ws::error::ErrorType::NonRecoverable,
                })
            }
            1002 => {
                self.close(false, 1002, None).await.map_err(|_e| Error {
                    code: 1011,
                    message: "Internal error, connection closed",
                    error_type: crate::ws::error::ErrorType::NonRecoverable,
                })?;
                Err(Error {
                    code: 1002,
                    message: "Protocol error, connection closed",
                    error_type: crate::ws::error::ErrorType::NonRecoverable,
                })
            }
            1009 => {
                self.close(false, 1009, None).await.map_err(|_e| Error {
                    code: 1011,
                    message: "Internal error, connection closed",
                    error_type: crate::ws::error::ErrorType::NonRecoverable,
                })?;
                Err(Error {
                    code: 1009,
                    message: "Message too big, connection closed",
                    error_type: crate::ws::error::ErrorType::NonRecoverable,
                })
            }
            1011 => {
                self.close(false, 1011, None).await.map_err(|_e| Error {
                    code: 1011,
                    message: "Internal error, connection closed",
                    error_type: crate::ws::error::ErrorType::NonRecoverable,
                })?;
                Err(Error {
                    code: 1011,
                    message: "Internal error, connection closed",
                    error_type: crate::ws::error::ErrorType::NonRecoverable,
                })
            }
            _ => Err(e),
        }
    }
}
