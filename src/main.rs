mod channel;
mod error;
mod mpegts;

use bytes::BytesMut;
use crate::channel::*;
use crate::error::*;
use crate::mpegts::*;
use srt_rs as srt;
use srt_rs::SrtAsyncStream;
use srt_rs::error::SrtRejectReason;
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::broadcast;

use nom::{
    IResult,
    bytes::complete::{tag, take_while1},
    character::complete::char,
    multi::separated_list0,
    sequence::separated_pair
};

type StreamIDMap<'a> = HashMap<&'a str, &'a str>;

fn is_key_character(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_value_character(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '/'
}

fn key_value_parser(input: &str) -> IResult<&str, (&str, &str)> {
    let before = take_while1(is_key_character);
    let after = take_while1(is_value_character);
    separated_pair(before, char('='), after)(input)
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

async fn run() -> Result<()> {
    srt::startup()?;

    let mut channels: HashMap<String, Channel> = HashMap::new();

    channels.insert("default".to_string(), Channel::new());

    let addr = SocketAddr::from(([0, 0, 0, 0], 1337));
    let builder = srt::async_builder()
        .set_live_transmission_type();

    let listener = builder.listen(addr, 5, Some(|socket, stream_id| {
        //println!("{}", stream_id);
        match stream_id_parser(stream_id) {
            Ok((_, dict)) => {
                //println!("{:?}", dict);
                match dict.get("u") {
                    Some(_user) => {
                        socket.set_passphrase("SecurePassphrase1").map_err(|_| SrtRejectReason::Predefined(500))?;
                        match dict.get("m").unwrap_or(&"request") {
                            &"request" => {
                                match dict.get("r") {
                                    Some(_resource) => Ok(()),
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
                let resource = dict.get("r").expect("dict.get(\"r\")");
                let parts: Vec<&str> = resource.split('/').collect();
                //println!("parts = {:?}", parts);
                if parts.len() < 2 { return Err(Error::Panic) }

                if let Some(channel) = channels.get(&parts[0].to_string()) {
                    let maybe = match parts[1] {
                        "v0" => Some(channel.receiver_video()),
                        "a0" => Some(channel.receiver_audio0()),
                        "a1" => Some(channel.receiver_audio1()),
                        _ => None
                    };
                    match maybe {
                        Some(receiver) => {
                            tokio::spawn(async move {
                                if let Err(e) = handle_request(receiver, stream, client_addr).await {
                                    println!("Error handling request from {}: {:?}", client_addr, e);
                                }
                            });
                        },
                        None => return Err(Error::Panic)
                    }
                }
            },
            &"publish" => {
                let user = dict.get("u").expect("dict.get(\"u\")").to_string();
                channels.insert(user.clone(), Channel::new());
                let channel = channels.get(&user).expect("channels.get(&user)").clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_publish(channel, stream, client_addr).await {
                        println!("Error handling publish from {}: {:?}", client_addr, e);
                    }
                });
            },
            _ => return Err(Error::Panic)
        }
    }

    // unreachable
    //Ok(srt::cleanup()?)
}

async fn handle_request(mut receiver: Receiver, mut stream: SrtAsyncStream, _client_addr: SocketAddr) -> Result<()> {
    'outer: loop {
        let mut packets = Vec::with_capacity(7);
        while packets.len() < 7 {
            match receiver.recv().await {
                Ok(message) => { packets.push(message.packet); }
                Err(broadcast::error::RecvError::Closed) => { break 'outer; },
                Err(broadcast::error::RecvError::Lagged(_)) => { println!("Client thread is lagging behind!"); }
            }
        }

        const SIMULATED_PACKET_LOSS: f32 = 0.0;
        let r = rand::random::<f32>();
        if r > SIMULATED_PACKET_LOSS {
            let payload = SrtPayload { packets };

            let encoded_len = 1316;
            let mut buf = BytesMut::with_capacity(encoded_len);
            buf.resize(encoded_len, 0xFF);
            let mut slice: &mut [u8] = &mut buf;
            payload.encode(&mut slice)?;

            stream.write(&buf).await?;
        }
    }
    Ok(())
}

async fn handle_publish(mut channel: Channel, mut stream: SrtAsyncStream, _client_addr: SocketAddr) -> Result<()> {
    let mut pmt_pid: Option<u16> = None;
    loop {
        let mut buf = BytesMut::zeroed(1316);
        stream.read(&mut buf).await?;
        let frozen_buf = buf.freeze();
        let mut slice: &[u8] = &frozen_buf;

        let payload = SrtPayload::decode(&mut slice)?;

        /*
        println!("publish");
        for byte in &frozen_buf {
            print!("{:02X} ", byte);
        }
        println!("");
        println!("");

        let mut buf = BytesMut::zeroed(1316);
        let mut slice: &mut [u8] = &mut buf;
        payload.encode(&mut slice)?;

        println!("request");
        for byte in &buf {
            print!("{:02X} ", byte);
        }
        println!("");
        println!("");
        */

        for packet in &payload.packets {
            //println!("PID = 0x{:04X}", packet.pid);
            match packet.pid {
                0x0 => {
                    channel.send_video(ChannelMessage::new(packet.clone()));
                    channel.send_audio0(ChannelMessage::new(packet.clone()));
                    channel.send_audio1(ChannelMessage::new(packet.clone()));

                    let psi = packet.psi()?;
                    if let Some(PsiData::Pat(ref pat)) = psi.data {
                        pmt_pid = Some(pat.pmt_pid);
                    }
                }
                0x100 => {
                    channel.send_video(ChannelMessage::new(packet.clone()));
                },
                0x101 => {
                    channel.send_audio0(ChannelMessage::new(packet.clone()));
                }
                0x102 => {
                    channel.send_audio1(ChannelMessage::new(packet.clone()));
                }
                pid => {
                    if Some(pid) == pmt_pid {
                        let psi = packet.psi()?;
                        if let Some(PsiData::Pmt(ref pmt)) = psi.data {
                            let mut audio = 0;
                            for program_definition in &pmt.program_definitions {
                                if program_definition.stream_type == 0x1B {
                                    let mut new_packet = packet.clone();
                                    let mut new_psi = new_packet.psi()?;
                                    let Some(PsiData::Pmt(ref mut new_pmt)) = new_psi.data else { panic!() };
                                    new_pmt.program_definitions = vec!(program_definition.clone());
                                    new_packet.set_psi(new_psi)?;

                                    channel.send_video(ChannelMessage::new(new_packet));
                                } else if program_definition.stream_type == 0x0F && audio < 2 {
                                    let mut new_packet = packet.clone();
                                    let mut new_psi = new_packet.psi()?;
                                    let Some(PsiData::Pmt(ref mut new_pmt)) = new_psi.data else { panic!() };
                                    new_pmt.program_definitions = vec!(program_definition.clone());
                                    new_packet.set_psi(new_psi)?;

                                    match audio {
                                        0 => channel.send_audio0(ChannelMessage::new(new_packet)),
                                        1 => channel.send_audio1(ChannelMessage::new(new_packet)),
                                        _ => {}
                                    }

                                    audio += 1;
                                }
                                //println!("stream_type = 0x{:02X}", program_definition.stream_type);
                                //println!("stream_pid = 0x{:04X}", program_definition.stream_pid);
                            }
                        }
                    } else {
                        //println!("unhandled PID = 0x{:X}", pid);
                    }
                }
            }

        }
    }

    // unreachable
    //Ok(())
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
