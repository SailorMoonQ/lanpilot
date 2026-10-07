//! Length-prefixed protobuf messages on byte streams (4-byte big-endian length).

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const MAX_FRAME: usize = 64 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("frame of {0} bytes exceeds limit")]
    TooLarge(usize),
    #[error("decode: {0}")]
    Decode(#[from] prost::DecodeError),
}

pub async fn write_msg<M, W>(w: &mut W, msg: &M) -> Result<(), FrameError>
where
    M: prost::Message,
    W: AsyncWrite + Unpin,
{
    let len = msg.encoded_len();
    if len > MAX_FRAME {
        return Err(FrameError::TooLarge(len));
    }
    let mut buf = Vec::with_capacity(4 + len);
    buf.extend_from_slice(&(len as u32).to_be_bytes());
    msg.encode(&mut buf).expect("Vec has capacity");
    w.write_all(&buf).await?;
    Ok(())
}

pub async fn read_msg<M, R>(r: &mut R) -> Result<Option<M>, FrameError>
where
    M: prost::Message + Default,
    R: AsyncRead + Unpin,
{
    let mut first_byte = [0u8; 1];
    match r.read_exact(&mut first_byte).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let mut rest_bytes = [0u8; 3];
    r.read_exact(&mut rest_bytes).await?;
    let mut len_buf = [0u8; 4];
    len_buf[0] = first_byte[0];
    len_buf[1..].copy_from_slice(&rest_bytes);
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME {
        return Err(FrameError::TooLarge(len));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).await?;
    Ok(Some(M::decode(body.as_slice())?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::v1::{Hello, Text};

    #[tokio::test]
    async fn round_trips_several_messages() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        write_msg(&mut a, &Text { text: "one".into() })
            .await
            .unwrap();
        write_msg(&mut a, &Text { text: "two".into() })
            .await
            .unwrap();
        drop(a);
        let m1: Text = read_msg(&mut b).await.unwrap().unwrap();
        let m2: Text = read_msg(&mut b).await.unwrap().unwrap();
        assert_eq!((m1.text.as_str(), m2.text.as_str()), ("one", "two"));
        assert!(read_msg::<Text, _>(&mut b).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn rejects_oversized_write() {
        let (mut a, _b) = tokio::io::duplex(16);
        let big = Text {
            text: "x".repeat(MAX_FRAME + 1),
        };
        assert!(matches!(
            write_msg(&mut a, &big).await,
            Err(FrameError::TooLarge(_))
        ));
    }

    #[tokio::test]
    async fn rejects_oversized_length_prefix() {
        let (mut a, mut b) = tokio::io::duplex(16);
        use tokio::io::AsyncWriteExt;
        a.write_all(&((MAX_FRAME as u32) + 1).to_be_bytes())
            .await
            .unwrap();
        assert!(matches!(
            read_msg::<Hello, _>(&mut b).await,
            Err(FrameError::TooLarge(_))
        ));
    }

    #[tokio::test]
    async fn truncated_body_is_an_error() {
        let (mut a, mut b) = tokio::io::duplex(16);
        use tokio::io::AsyncWriteExt;
        a.write_all(&10u32.to_be_bytes()).await.unwrap();
        a.write_all(&[1, 2]).await.unwrap();
        drop(a);
        assert!(matches!(
            read_msg::<Hello, _>(&mut b).await,
            Err(FrameError::Io(_))
        ));
    }

    #[tokio::test]
    async fn truncated_length_prefix_is_an_error() {
        let (mut a, mut b) = tokio::io::duplex(16);
        use tokio::io::AsyncWriteExt;
        a.write_all(&[1, 2]).await.unwrap();
        drop(a);
        assert!(matches!(
            read_msg::<Hello, _>(&mut b).await,
            Err(FrameError::Io(_))
        ));
    }
}
