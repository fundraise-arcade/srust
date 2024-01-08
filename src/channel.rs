use crate::error::*;
use crate::MpegTsPacket;
use tokio::sync::broadcast;

#[derive(Clone, Debug)]
pub struct ChannelMessage {
    pub packet: MpegTsPacket
}

impl ChannelMessage {
    pub fn new(packet: MpegTsPacket) -> Self {
        Self { packet }
    }
}

impl From<broadcast::error::SendError<ChannelMessage>> for Error {
    fn from(_: broadcast::error::SendError<ChannelMessage>) -> Self {
        Self::ChannelSend
    }
}

#[derive(Clone)]
pub struct Channel {
    video: broadcast::Sender<ChannelMessage>,
    audio0: broadcast::Sender<ChannelMessage>,
    audio1: broadcast::Sender<ChannelMessage>
}

impl Channel {
    pub fn new() -> Self {
        let (video, _) = broadcast::channel(512);
        let (audio0, _) = broadcast::channel(512);
        let (audio1, _) = broadcast::channel(512);
        Self { video, audio0, audio1 }
    }

    pub fn receiver_video(&self) -> Receiver {
        Receiver { rx: self.video.subscribe() }
    }

    pub fn receiver_audio0(&self) -> Receiver {
        Receiver { rx: self.audio0.subscribe() }
    }

    pub fn receiver_audio1(&self) -> Receiver {
        Receiver { rx: self.audio1.subscribe() }
    }

    pub fn send_video(&self, buf: ChannelMessage) -> std::result::Result<usize, broadcast::error::SendError<ChannelMessage>> {
        self.video.send(buf)
    }

    pub fn send_audio0(&self, buf: ChannelMessage) -> std::result::Result<usize, broadcast::error::SendError<ChannelMessage>> {
        self.audio0.send(buf)
    }

    pub fn send_audio1(&self, buf: ChannelMessage) -> std::result::Result<usize, broadcast::error::SendError<ChannelMessage>> {
        self.audio1.send(buf)
    }
}

pub struct Receiver {
    rx: broadcast::Receiver<ChannelMessage>
}

impl Receiver {
    pub async fn recv(&mut self) -> std::result::Result<ChannelMessage, broadcast::error::RecvError> {
        self.rx.recv().await
    }
}
