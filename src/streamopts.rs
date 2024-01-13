use byteorder::{BigEndian, ReadBytesExt};
use crate::error::*;

#[derive(Debug)]
pub struct StreamOpts {
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
            _ => Err(Error::InvalidStreamId)
        }
    }
}

impl StreamOpts {
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

        Ok(StreamOpts {
            version,
            user,
            target
        })
    }

    pub fn is_publisher(&self) -> bool {
        self.target.is_none()
    }
}
