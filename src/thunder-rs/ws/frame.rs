use std::sync::Arc;
use tokio::io::{self};

use super::{
    error::{Error, ErrorType, PROTOCOL_ERROR},
    socket::socket::{Reader, Writer},
};
const FIN_BIT: u8 = 0x80;
const RSV1_BIT: u8 = 0x40;
const RSV2_BIT: u8 = 0x20;
const RSV3_BIT: u8 = 0x10;
const MASK_BIT: u8 = 0x80;
const PAYLOAD_LEN_126: u8 = 126;
const PAYLOAD_LEN_127: u8 = 127;
const MAX_PAYLOAD_SIZE: usize = 1024;

// let header = [0b10000001, 0b00000001];
// let (fin, opcode, payload_length_indicator) = parse_opcode(&header)?;
// assert_eq!(fin, 1);
// assert_eq!(opcode, 1);
// assert_eq!(payload_length_indicator, 1);
//
#[inline]
pub fn parse_opcode(header: &[u8]) -> Result<(u8, u8, u8), Error> {
    if header.len() < 2 {
        return Err(PROTOCOL_ERROR);
    }
    let fin = (header[0] & FIN_BIT) >> 7;
    let opcode = header[0] & 0x0F;
    let payload_length_indicator = header[1] & 0x7F;

    Ok((fin, opcode, payload_length_indicator))
}
/// This function sends a WebSocket frame by writing the opcode and payload to the writer.
/// It handles splitting the payload into smaller frames if needed and ensures the data is flushed.
/// Each frame is written and flushed immediately after construction.
#[inline]
pub async fn send_frame_inner(
    writer: &Arc<tokio::sync::Mutex<Writer>>,
    opcode: u8,
    payload: &[u8],
) -> io::Result<()> {
    let payload_len = payload.len();

    if payload.is_empty() {
        let mut buffer = Vec::with_capacity(2);
        buffer.push(FIN_BIT | opcode);
        buffer.push(0);

        let mut writer = writer.lock().await;
        writer.write_all(&buffer).await?;
        writer.flush().await?;
        return Ok(());
    }

    let mut start = 0;
    while start < payload_len {
        let end = std::cmp::min(start + MAX_PAYLOAD_SIZE, payload_len);
        let slice = &payload[start..end];

        let mut buffer = Vec::with_capacity(
            2 + slice.len()
                + match slice.len() {
                    0..=125 => 0,
                    126..=65535 => 2,
                    _ => 8,
                },
        );

        let fin = (end == payload_len) as u8 * FIN_BIT;
        let frame_opcode = (start == 0) as u8 * opcode;
        buffer.push(fin | frame_opcode);

        let slice_len = slice.len();
        match slice_len {
            0..=125 => buffer.push(slice_len as u8),
            126..=65535 => {
                buffer.push(PAYLOAD_LEN_126);
                buffer.extend_from_slice(&(slice_len as u16).to_be_bytes());
            }
            _ => {
                buffer.push(PAYLOAD_LEN_127);
                buffer.extend_from_slice(&(slice_len as u64).to_be_bytes());
            }
        }

        buffer.extend_from_slice(slice);

        {
            let mut writer = writer.lock().await;
            writer.write_all(&buffer).await?;
            writer.flush().await?;
        }

        start = end;
    }

    Ok(())
}

pub async fn read_header(
    reader: &Arc<tokio::sync::Mutex<Reader>>,
) -> Result<(u8, u8, usize, [u8; 4]), Error> {
    let mut header = [0; 2];
    let mut length_bytes_2 = [0; 2];
    let mut length_bytes_8 = [0; 8];
    let mut masking_key = [0; 4];

    {
        let mut reader = reader.lock().await;
        reader.read_exact(&mut header).await.map_err(|_e| Error {
            code: 1002,
            message: "Failed to read header",
            error_type: ErrorType::NonRecoverable,
        })?;
        //Determines the additional bytes to read
        let payload_length_indicator = header[1] & 0x7F;
        match payload_length_indicator {
            PAYLOAD_LEN_126 => {
                reader
                    .read_exact(&mut length_bytes_2)
                    .await
                    .map_err(|_e| Error {
                        code: 1002,
                        message: "Failed to read extended payload length",
                        error_type: ErrorType::NonRecoverable,
                    })?;
            }
            PAYLOAD_LEN_127 => {
                reader
                    .read_exact(&mut length_bytes_8)
                    .await
                    .map_err(|_e| Error {
                        code: 1002,
                        message: "Failed to read extended payload length",
                        error_type: ErrorType::NonRecoverable,
                    })?;
            }
            _ => {}
        }

        if header[1] & MASK_BIT != 0 {
            reader
                .read_exact(&mut masking_key)
                .await
                .map_err(|_e| Error {
                    code: 1002,
                    message: "Failed to read masking key",
                    error_type: ErrorType::NonRecoverable,
                })?;
        }
    }

    let (fin, opcode, payload_length_indicator) = parse_opcode(&header)?;

    if header[0] & (RSV1_BIT | RSV2_BIT | RSV3_BIT) != 0 {
        return Err(Error {
            code: 1002,
            message: "RSV1, RSV2, or RSV3 bit is set",
            error_type: ErrorType::NonRecoverable,
        });
    }
    //Actual payload length calculation
    let payload_length = match payload_length_indicator {
        PAYLOAD_LEN_126 => u16::from_be_bytes(length_bytes_2) as usize,
        PAYLOAD_LEN_127 => {
            let length = u64::from_be_bytes(length_bytes_8);
            if length > usize::MAX as u64 {
                return Err(Error {
                    code: 1002,
                    message: "Payload size exceeds the maximum allowed size",
                    error_type: ErrorType::NonRecoverable,
                });
            }
            usize::try_from(length).map_err(|_| Error {
                code: 1002,
                message: "Payload size exceeds the maximum allowed size",
                error_type: ErrorType::NonRecoverable,
            })?
        }
        len => len as usize,
    };

    Ok((fin, opcode, payload_length, masking_key))
}

#[inline]
pub async fn read_and_unmask_payload(
    reader: &Arc<tokio::sync::Mutex<Reader>>,
    payload_length: usize,
    masking_key: [u8; 4],
) -> Result<Vec<u8>, Error> {
    let mut payload_data = vec![0; payload_length];

    {
        let mut reader = reader.lock().await;
        reader
            .read_exact(&mut payload_data)
            .await
            .map_err(|_e| Error {
                code: 1002,
                message: "Failed to read payload data",
                error_type: ErrorType::NonRecoverable,
            })?;
    }

    if masking_key == [0; 4] {
        return Err(Error {
            code: 1002,
            message: "Missing masking key",
            error_type: ErrorType::NonRecoverable,
        });
    } else {
        for (i, byte) in payload_data.iter_mut().enumerate() {
            *byte ^= masking_key[i & 3];
        }
    }

    Ok(payload_data)
}
