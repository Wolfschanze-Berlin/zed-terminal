use anyhow::{Context as _, Result};
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use prost::Message as _;
use rpc::proto::Envelope;

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct MessageId(pub u32);

pub type MessageLen = u32;
pub const MESSAGE_LEN_SIZE: usize = size_of::<MessageLen>();

const COMPRESSION_FLAG_NONE: u8 = 0x00;
const COMPRESSION_FLAG_ZSTD: u8 = 0x01;

/// Messages smaller than this threshold are sent uncompressed to avoid
/// adding latency for small RPC calls where compression savings are negligible.
const COMPRESSION_THRESHOLD: usize = 256;

/// Zstd compression level (1 = fastest, good enough for wire protocol).
const ZSTD_COMPRESSION_LEVEL: i32 = 1;

pub fn message_len_from_buffer(buffer: &[u8]) -> MessageLen {
    MessageLen::from_le_bytes(buffer.try_into().unwrap())
}

pub async fn read_message_with_len<S: AsyncRead + Unpin>(
    stream: &mut S,
    buffer: &mut Vec<u8>,
    message_len: MessageLen,
) -> Result<Envelope> {
    buffer.resize(message_len as usize, 0);
    stream.read_exact(buffer).await?;
    Ok(Envelope::decode(buffer.as_slice())?)
}

pub async fn read_message<S: AsyncRead + Unpin>(
    stream: &mut S,
    buffer: &mut Vec<u8>,
) -> Result<Envelope> {
    buffer.resize(MESSAGE_LEN_SIZE, 0);
    stream.read_exact(buffer).await?;

    let len = message_len_from_buffer(buffer);

    read_message_with_len(stream, buffer, len).await
}

pub async fn write_message<S: AsyncWrite + Unpin>(
    stream: &mut S,
    buffer: &mut Vec<u8>,
    message: Envelope,
) -> Result<()> {
    let message_len = message.encoded_len() as u32;
    stream
        .write_all(message_len.to_le_bytes().as_slice())
        .await?;
    buffer.clear();
    buffer.reserve(message_len as usize);
    message.encode(buffer)?;
    stream.write_all(buffer).await?;
    Ok(())
}

/// Reads a message that may be zstd-compressed, with backward compatibility.
///
/// New wire format: `[u32 LE total_len][u8 flags][payload]`
/// - flags `0x00`: payload is raw protobuf
/// - flags `0x01`: payload is zstd-compressed protobuf
///
/// Legacy wire format: `[u32 LE len][protobuf]`
///
/// Detection: valid protobuf never starts with `0x00` or `0x01` (the lowest
/// valid protobuf first byte is `0x08` for field 1 varint), so the first byte
/// reliably distinguishes the two formats.
pub async fn read_message_compressed<S: AsyncRead + Unpin>(
    stream: &mut S,
    buffer: &mut Vec<u8>,
    decompress_buffer: &mut Vec<u8>,
) -> Result<Envelope> {
    buffer.resize(MESSAGE_LEN_SIZE, 0);
    stream.read_exact(buffer).await?;

    let total_len = message_len_from_buffer(buffer) as usize;
    anyhow::ensure!(total_len >= 1, "message too short");

    buffer.resize(total_len, 0);
    stream.read_exact(buffer).await?;

    let first_byte = buffer[0];

    match first_byte {
        COMPRESSION_FLAG_NONE => {
            let payload = &buffer[1..];
            Envelope::decode(payload).context("decoding uncompressed message")
        }
        COMPRESSION_FLAG_ZSTD => {
            let payload = &buffer[1..];
            let decompressed = zstd::decode_all(payload)
                .context("decompressing zstd message")?;
            *decompress_buffer = decompressed;
            Envelope::decode(decompress_buffer.as_slice())
                .context("decoding decompressed message")
        }
        _ => {
            // Legacy format: entire buffer is raw protobuf (no flag byte).
            // This handles messages from old servers that don't support compression.
            Envelope::decode(buffer.as_slice())
                .context("decoding legacy uncompressed message")
        }
    }
}

/// Writes a message with optional zstd compression.
///
/// Messages smaller than `COMPRESSION_THRESHOLD` are sent uncompressed
/// to avoid adding latency for small RPC calls.
///
/// Wire format: `[u32 LE total_len][u8 flags][payload]`
pub async fn write_message_compressed<S: AsyncWrite + Unpin>(
    stream: &mut S,
    buffer: &mut Vec<u8>,
    compress_buffer: &mut Vec<u8>,
    message: Envelope,
) -> Result<()> {
    buffer.clear();
    let encoded_len = message.encoded_len();
    buffer.reserve(encoded_len);
    message.encode(buffer)?;

    if encoded_len < COMPRESSION_THRESHOLD {
        let total_len = (1 + buffer.len()) as u32;
        stream
            .write_all(total_len.to_le_bytes().as_slice())
            .await?;
        stream.write_all(&[COMPRESSION_FLAG_NONE]).await?;
        stream.write_all(buffer).await?;
    } else {
        let compressed = zstd::encode_all(buffer.as_slice(), ZSTD_COMPRESSION_LEVEL)
            .context("compressing message with zstd")?;
        *compress_buffer = compressed;

        let total_len = (1 + compress_buffer.len()) as u32;
        stream
            .write_all(total_len.to_le_bytes().as_slice())
            .await?;
        stream.write_all(&[COMPRESSION_FLAG_ZSTD]).await?;
        stream.write_all(compress_buffer).await?;
    }

    Ok(())
}

pub async fn write_size_prefixed_buffer<S: AsyncWrite + Unpin>(
    stream: &mut S,
    buffer: &mut Vec<u8>,
) -> Result<()> {
    let len = buffer.len() as u32;
    stream.write_all(len.to_le_bytes().as_slice()).await?;
    stream.write_all(buffer).await?;
    Ok(())
}

pub async fn read_message_raw<S: AsyncRead + Unpin>(
    stream: &mut S,
    buffer: &mut Vec<u8>,
) -> Result<()> {
    buffer.resize(MESSAGE_LEN_SIZE, 0);
    stream.read_exact(buffer).await?;

    let message_len = message_len_from_buffer(buffer);
    buffer.resize(message_len as usize, 0);
    stream.read_exact(buffer).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rpc::proto;

    #[test]
    fn test_roundtrip_uncompressed_small_message() {
        smol::block_on(async {
            let envelope = Envelope {
                id: 1,
                payload: Some(proto::envelope::Payload::Ping(proto::Ping {})),
                ..Default::default()
            };

            let mut wire = Vec::new();
            let mut encode_buf = Vec::new();
            let mut compress_buf = Vec::new();
            write_message_compressed(
                &mut wire,
                &mut encode_buf,
                &mut compress_buf,
                envelope.clone(),
            )
            .await
            .unwrap();

            let mut cursor = futures::io::Cursor::new(&wire);
            let mut read_buf = Vec::new();
            let mut decompress_buf = Vec::new();
            let decoded =
                read_message_compressed(&mut cursor, &mut read_buf, &mut decompress_buf)
                    .await
                    .unwrap();

            assert_eq!(envelope.id, decoded.id);
            assert_eq!(envelope.payload, decoded.payload);
            // Small message should use the uncompressed flag
            assert_eq!(wire[MESSAGE_LEN_SIZE], COMPRESSION_FLAG_NONE);
        });
    }

    #[test]
    fn test_roundtrip_compressed_large_message() {
        smol::block_on(async {
            // Create a large error message to exceed the compression threshold
            let large_message = "x".repeat(512);
            let envelope = Envelope {
                id: 42,
                payload: Some(proto::envelope::Payload::Error(proto::Error {
                    message: large_message.clone(),
                    ..Default::default()
                })),
                ..Default::default()
            };

            let mut wire = Vec::new();
            let mut encode_buf = Vec::new();
            let mut compress_buf = Vec::new();
            write_message_compressed(
                &mut wire,
                &mut encode_buf,
                &mut compress_buf,
                envelope.clone(),
            )
            .await
            .unwrap();

            // Large message should use zstd flag
            assert_eq!(wire[MESSAGE_LEN_SIZE], COMPRESSION_FLAG_ZSTD);

            // Compressed wire size should be smaller than uncompressed
            let uncompressed_size = envelope.encoded_len();
            let wire_payload_size = wire.len() - MESSAGE_LEN_SIZE;
            assert!(
                wire_payload_size < uncompressed_size,
                "compressed ({wire_payload_size}) should be smaller than uncompressed ({uncompressed_size})"
            );

            let mut cursor = futures::io::Cursor::new(&wire);
            let mut read_buf = Vec::new();
            let mut decompress_buf = Vec::new();
            let decoded =
                read_message_compressed(&mut cursor, &mut read_buf, &mut decompress_buf)
                    .await
                    .unwrap();

            assert_eq!(envelope.id, decoded.id);
            assert_eq!(envelope.payload, decoded.payload);
        });
    }

    #[test]
    fn test_read_legacy_uncompressed_message() {
        smol::block_on(async {
            let envelope = Envelope {
                id: 7,
                payload: Some(proto::envelope::Payload::Ping(proto::Ping {})),
                ..Default::default()
            };

            // Write in the OLD format: [u32 len][protobuf] — no flag byte
            let mut wire = Vec::new();
            let mut buf = Vec::new();
            write_message(&mut wire, &mut buf, envelope.clone())
                .await
                .unwrap();

            // The new read_message_compressed should handle this gracefully
            let mut cursor = futures::io::Cursor::new(&wire);
            let mut read_buf = Vec::new();
            let mut decompress_buf = Vec::new();
            let decoded =
                read_message_compressed(&mut cursor, &mut read_buf, &mut decompress_buf)
                    .await
                    .unwrap();

            assert_eq!(envelope.id, decoded.id);
            assert_eq!(envelope.payload, decoded.payload);
        });
    }
}
