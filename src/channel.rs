use crate::error::*;
use crate::MpegTsPacket;
use std::collections::VecDeque;
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

type InternalChannelMessage = VecDeque<ChannelMessage>;

/*
#[derive(Clone, Debug)]
struct InternalChannelMessage {
    pub messages: Vec<ChannelMessage>
}

impl InternalChannelMessage {
    pub fn new() -> Self {
        Self {
            messages: Vec::new()
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            messages: Vec::with_capacity(capacity)
        }
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn push(&mut self, message: ChannelMessage) {
        self.messages.push(message)
    }

    pub fn pop(&mut self) -> Option<ChannelMessage> {
        self.messages.pop()
    }

    pub fn clear(&mut self) {
        self.messages.clear()
    }
}
*/

impl From<broadcast::error::SendError<InternalChannelMessage>> for Error {
    fn from(_: broadcast::error::SendError<InternalChannelMessage>) -> Self {
        Self::ChannelSend
    }
}

#[derive(Clone)]
pub struct Channel {
    video_buf: InternalChannelMessage,
    audio0_buf: InternalChannelMessage,
    audio1_buf: InternalChannelMessage,
    video: broadcast::Sender<InternalChannelMessage>,
    audio0: broadcast::Sender<InternalChannelMessage>,
    audio1: broadcast::Sender<InternalChannelMessage>
}

const CHANNEL_INTERNAL_BUF_LEN: usize = 14;

impl Channel {
    pub fn new() -> Self {
        let (video, _) = broadcast::channel(512);
        let (audio0, _) = broadcast::channel(512);
        let (audio1, _) = broadcast::channel(512);
        Self {
            video_buf: InternalChannelMessage::with_capacity(CHANNEL_INTERNAL_BUF_LEN),
            audio0_buf: InternalChannelMessage::with_capacity(CHANNEL_INTERNAL_BUF_LEN),
            audio1_buf: InternalChannelMessage::with_capacity(CHANNEL_INTERNAL_BUF_LEN),
            video, audio0, audio1
        }
    }

    pub fn receiver_video(&self) -> Receiver {
        Receiver::new(self.video.subscribe())
    }

    pub fn receiver_audio0(&self) -> Receiver {
        Receiver::new(self.audio0.subscribe())
    }

    pub fn receiver_audio1(&self) -> Receiver {
        Receiver::new(self.audio1.subscribe())
    }

    pub fn send_video(&mut self, buf: ChannelMessage) {
        self.video_buf.push_back(buf);
        if self.video_buf.len() >= CHANNEL_INTERNAL_BUF_LEN {
            if let Err(_) = self.video.send(self.video_buf.clone()) {}
            self.video_buf.clear();
        }
    }

    pub fn send_audio0(&mut self, buf: ChannelMessage) {
        self.audio0_buf.push_back(buf);
        if self.audio0_buf.len() >= CHANNEL_INTERNAL_BUF_LEN {
            if let Err(_) = self.audio0.send(self.audio0_buf.clone()) {}
            self.audio0_buf.clear();
        }
    }

    pub fn send_audio1(&mut self, buf: ChannelMessage) {
        self.audio1_buf.push_back(buf);
        if self.audio1_buf.len() >= CHANNEL_INTERNAL_BUF_LEN {
            if let Err(_) = self.audio1.send(self.audio1_buf.clone()) {}
            self.audio1_buf.clear();
        }
    }
}

pub struct Receiver {
    buf: InternalChannelMessage,
    rx: broadcast::Receiver<InternalChannelMessage>
}

impl Receiver {
    fn new(rx: broadcast::Receiver<InternalChannelMessage>) -> Self{
        Self {
            buf: InternalChannelMessage::new(),
            rx
        }
    }

    pub async fn recv(&mut self) -> std::result::Result<ChannelMessage, broadcast::error::RecvError> {
        if self.buf.is_empty() {
            self.buf = self.rx.recv().await?;
        }
        Ok(self.buf.pop_front().unwrap())
    }
}
