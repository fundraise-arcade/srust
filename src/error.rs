use srt_rs::error::SrtError;

#[derive(Debug)]
pub enum Error {
    Panic,
    InvalidStreamId,
    NoChannel,
    NoSync,
    NoPayload,
    ChannelSend,
    IO(std::io::Error),
    SRT(SrtError)
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::IO(err)
    }
}

impl From<SrtError> for Error {
    fn from(err: SrtError) -> Self {
        Self::SRT(err)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
