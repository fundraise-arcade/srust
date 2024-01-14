use base64::Engine;
use base64::engine::general_purpose::STANDARD_NO_PAD;
use byteorder::{BigEndian, ReadBytesExt};
use bytes::BytesMut;

#[derive(Debug)]
pub enum Error {
    InvalidPrefix,
    InvalidEncoding,
    InvalidTrack,
    IO(std::io::Error),
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::IO(err)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub fn decode_stream_id(input: &str) -> Result<StreamId> {
    if !input.starts_with("#!R") {
        return Err(Error::InvalidPrefix);
    }
    let input = &input[3..];

    let mut buf = BytesMut::zeroed(16);
    let size = STANDARD_NO_PAD.decode_slice(input, &mut buf).map_err(|_| Error::InvalidEncoding)?;
    buf.truncate(size);
    StreamId::decode(&buf.freeze())
}

#[derive(Debug)]
pub struct StreamId {
    pub version: u16,
    pub user: u16,
    pub target: Option<StreamTarget>
}

#[derive(Debug)]
pub struct StreamTarget {
    pub user: u16,
    pub track: StreamTrack
}

#[derive(Debug)]
pub enum StreamTrack {
    Video,
    ContentAudio,
    CommentaryAudio
}

impl TryFrom<u8> for StreamTrack {
    type Error = Error;

    fn try_from(value: u8) -> std::result::Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Video),
            1 => Ok(Self::ContentAudio),
            2 => Ok(Self::CommentaryAudio),
            _ => Err(Error::InvalidTrack)
        }
    }
}

impl StreamId {
    pub fn decode(mut buf: &[u8]) -> Result<Self> {
        let flags = buf.read_u16::<BigEndian>()?;
        let publisher = ((flags >> 15) & 0b1) != 0;
        let version = flags & 0x7FFF;
        let user = buf.read_u16::<BigEndian>()?;
        let target = if !publisher {
            Some(StreamTarget {
                user: buf.read_u16::<BigEndian>()?,
                track: StreamTrack::try_from(buf.read_u8()?)?
            })
        } else {
            None
        };

        Ok(Self {
            version,
            user,
            target
        })
    }

    pub fn is_publisher(&self) -> bool {
        self.target.is_none()
    }
}
