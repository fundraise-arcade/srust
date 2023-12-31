use futures::io::{AsyncReadExt, AsyncWriteExt};
use srt_rs as srt;
use srt_rs::SrtAsyncStream;
use srt_rs::error::{SrtError, SrtRejectReason};
use std::collections::HashMap;
use std::fmt::Debug;
use std::net::SocketAddr;
use tokio::sync::broadcast;

use nom::{
    IResult,
    bytes::complete::{tag, take_while1},
    character::complete::{alphanumeric1, char},
    multi::separated_list0,
    sequence::separated_pair
};

type StreamIDMap<'a> = HashMap<&'a str, &'a str>;

fn is_key_character(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn key_value_parser(input: &str) -> IResult<&str, (&str, &str)> {
    let before = take_while1(is_key_character);
    separated_pair(before, char('='), alphanumeric1)(input)
}

fn stream_id_parser(input: &str) -> IResult<&str, StreamIDMap> {
    if input.is_empty() {
        Ok((input, HashMap::new()))
    } else {
        let (input, _) = tag("#!::")(input)?;
        let (input, entries) = separated_list0(char(','), key_value_parser)(input)?;
        Ok((input, HashMap::from_iter(entries)))
    }
}

#[derive(Debug)]
enum Error {
    IO(std::io::Error),
    SRT(SrtError)
}

impl From<std::io::Error> for Error {
    fn from(io_error: std::io::Error) -> Self {
        Self::IO(io_error)
    }
}

impl From<SrtError> for Error {
    fn from(srt_error: SrtError) -> Self {
        Self::SRT(srt_error)
    }
}

type Result<T> = std::result::Result<T, Error>;

type Payload = [u8; 1316 * 1];

struct Channel {
    tx: broadcast::Sender<Payload>
}

impl Channel {
    fn new() -> Self {
        let (tx, _) = broadcast::channel(1024);
        Self { tx }
    }

    fn receiver(&self) -> Receiver {
        Receiver { rx: self.tx.subscribe() }
    }

    fn sender(&self) -> Sender {
        Sender { tx: self.tx.clone() }
    }
}

struct Receiver {
    rx: broadcast::Receiver<Payload>
}

impl Receiver {
    async fn recv(&mut self) -> std::result::Result<Payload, broadcast::error::RecvError> {
        self.rx.recv().await
    }
}

struct Sender {
    tx: broadcast::Sender<Payload>
}

impl Sender {
    fn send(&self, buf: Payload) -> std::result::Result<usize, broadcast::error::SendError<Payload>> {
        self.tx.send(buf)
    }
}

async fn run() -> Result<()> {
    srt::startup()?;

    let mut channels: HashMap<String, Channel> = HashMap::new();

    channels.insert("default".to_string(), Channel::new());

    let addr = SocketAddr::from(([0, 0, 0, 0], 1337));
    let builder = srt::async_builder()
        .set_live_transmission_type();

    let listener = builder.listen(addr, 5, Some(|socket, stream_id| {
        match stream_id_parser(stream_id) {
            Ok((_, dict)) => {
                match dict.get("u") {
                    Some(user) => {
                        socket.set_passphrase("SecurePassphrase1").map_err(|_| SrtRejectReason::Predefined(500))?;
                        match dict.get("m").unwrap_or(&"request") {
                            &"request" => {
                                match dict.get("r") {
                                    Some(resource) => Ok(()),
                                    None => Err(SrtRejectReason::Predefined(403)) // default to resource forbidden
                                }
                            },
                            &"publish" => Ok(()),
                            _ => Err(SrtRejectReason::Predefined(405))
                        }
                    },
                    None => Err(SrtRejectReason::Predefined(401))
                }
            }
            Err(_) => Err(SrtRejectReason::Predefined(400))
        }
    }))?;

    loop {
        let (stream, client_addr) = listener.accept().await?;
        let stream_id = stream.get_stream_id()?;
        println!("Accepted connection from {} ({})", client_addr, stream_id);

        let (_, dict) = stream_id_parser(&stream_id).expect("stream_id_parser");
        match dict.get("m").unwrap_or(&"request") {
            &"request" => {
                let resource = dict.get("r").expect("dict.get(\"r\")").to_string();
                if let Some(channel) = channels.get(&resource) {
                    let receiver = channel.receiver();
                    tokio::spawn(async move {
                        if let Err(e) = handle_request(receiver, stream, client_addr).await {
                            println!("Error handling request from {}: {:?}", client_addr, e);
                        }
                    });
                }
            },
            &"publish" => {
                let user = dict.get("u").expect("dict.get(\"u\")").to_string();
                channels.insert(user.clone(), Channel::new());
                let sender = channels.get(&user).expect("channels.get(&user)").sender();
                tokio::spawn(async move {
                    if let Err(e) = handle_publish(sender, stream, client_addr).await {
                        println!("Error handling publish from {}: {:?}", client_addr, e);
                    }
                });
            },
            _ => panic!()
        }
    }

    // unreachable
    //Ok(srt::cleanup()?)
}

async fn handle_request(mut receiver: Receiver, mut stream: SrtAsyncStream, _client_addr: SocketAddr) -> Result<()> {
    loop {
        match receiver.recv().await {
            Ok(buffer) => {
                stream.write(&buffer).await?;
            },
            Err(broadcast::error::RecvError::Closed) => { break; },
            Err(broadcast::error::RecvError::Lagged(_)) => { println!("Client thread is lagging behind!"); }
        }
    }
    Ok(())
}

async fn handle_publish(sender: Sender, mut stream: SrtAsyncStream, _client_addr: SocketAddr) -> Result<()> {
    loop {
        let mut buffer = [0; 1316 * 1];
        stream.read(&mut buffer).await?;
        if let Err(_) = sender.send(buffer) {}
    }
}

#[tokio::main]
async fn main() {
    loop {
        match run().await {
            Ok(()) => (),
            Err(e) => println!("Error in main thread: {:?}", e)
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    }
}
