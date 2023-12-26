use srt_rs as srt;
use srt_rs::SrtAsyncStream;
use srt_rs::error::{SrtError, SrtRejectReason};
use std::collections::HashMap;
use std::fmt::Debug;
use std::net::SocketAddr;
use futures::io::{AsyncReadExt, AsyncWriteExt};

use nom::{
    IResult,
    bytes::complete::tag,
    character::complete::{alphanumeric1, char},
    multi::separated_list0,
    sequence::separated_pair
};

type StreamIDMap<'a> = HashMap<&'a str, &'a str>;

fn key_value_parser(input: &str) -> IResult<&str, (&str, &str)> {
    separated_pair(alphanumeric1, char('='), alphanumeric1)(input)
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

async fn run() -> Result<()> {
    srt::startup()?;

    let addr = SocketAddr::from(([0, 0, 0, 0], 1337));
    let builder = srt::async_builder()
        .set_live_transmission_type();
    let listener = builder.listen(addr, 5, Some(|socket, stream_id| {
        match stream_id_parser(stream_id) {
            Ok((_, dict)) => {
                println!("dict = {:?}", dict);
                println!("m = {}", dict.get("m").unwrap_or(&"request"));
                match dict.get("m").unwrap_or(&"request") {
                    &"publish" | &"bidirectional" => {
                        let result = socket.set_passphrase("SecurePassphrase1");
                        println!("{:?}", result);
                        result.map_err(|_| SrtRejectReason::Predefined(500))
                    },
                    &"request" => Ok(()),
                    _ => Err(SrtRejectReason::Predefined(405))
                }
            }
            Err(_) => Err(SrtRejectReason::Predefined(400))
        }
    }))?;

    loop {
        let (stream, client_addr) = listener.accept().await?;
        let stream_id = stream.get_stream_id()?;
        println!("accepted connection from {} ({})", client_addr, stream_id);

        let _dict = stream_id_parser(&stream_id).expect("stream_id_parser");
        tokio::spawn(async move {
            if let Err(e) = handle_push(stream, client_addr).await {
                println!("error handling connection from {}: {:?}", client_addr, e);
            }
        });
    }

    // unreachable
    //Ok(srt::cleanup()?)
}

async fn handle_push(mut stream: SrtAsyncStream, client_addr: SocketAddr) -> Result<()> {
    loop {
        let mut buffer = [0; 2048];
        stream.read(&mut buffer[..]).await?;
        println!("read {} ({})", client_addr, stream.get_stream_id()?);
    }
}

#[tokio::main]
async fn main() {
    loop {
        match run().await {
            Ok(()) => (),
            Err(e) => println!("error: {:?}", e)
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
    }
}
