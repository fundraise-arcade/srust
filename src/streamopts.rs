use byteorder::{BigEndian, ReadBytesExt};
use crate::error::*;

#[derive(Debug)]
pub struct StreamOpts {
    pub version: u16,
    pub user: u16,
    pub target: Option<u16>
}

impl StreamOpts {
    pub fn decode(mut buf: &[u8]) -> Result<Self> {
        let flags = buf.read_u16::<BigEndian>()?;
        let publisher = ((flags >> 15) & 0b1) != 0;
        let version = flags & 0x7FFF;
        let user = buf.read_u16::<BigEndian>()?;
        let target = if !publisher {
            Some(buf.read_u16::<BigEndian>()?)
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
