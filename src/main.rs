mod channel;
mod error;
mod mpegts;

use bytes::BytesMut;
use crate::channel::*;
use crate::error::*;
use crate::mpegts::*;
use streamid::{decode_streamid, StreamTrack};
use srt_rs as srt;
use srt_rs::{SrtAsyncListener, SrtAsyncStream};
use srt_rs::error::SrtRejectReason;
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::broadcast;

async fn accept(channels: &mut HashMap<u16, Channel>, listener: &SrtAsyncListener) -> Result<()> {
    loop {
        let (stream, client_addr) = listener.accept().await?;
        let stream_id = stream.get_stream_id()?;
        println!("Accepted connection from {} ({})", client_addr, stream_id);

        let streamopts = decode_streamid(&stream_id).expect("decode_streamid");

        if streamopts.is_publisher() {
            let channel = channels.entry(streamopts.user).or_insert(Channel::new()).clone();
            tokio::spawn(async move {
                if let Err(e) = handle_publish(channel, stream, client_addr).await {
                    println!("Error handling publish from {}: {:?}", client_addr, e);
                }
            });
        } else {
            let target = &streamopts.target.expect("streamopts.target");
            let channel = channels.entry(target.user).or_insert(Channel::new());
            let receiver = match target.track {
                StreamTrack::Video => channel.receiver_video(),
                StreamTrack::ContentAudio => channel.receiver_audio0(),
                StreamTrack::CommentaryAudio => channel.receiver_audio1(),
            };
            tokio::spawn(async move {
                if let Err(e) = handle_request(receiver, stream, client_addr).await {
                    println!("Error handling request from {}: {:?}", client_addr, e);
                }
            });
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }
}

async fn run() -> Result<()> {
    srt::startup()?;

    let mut channels: HashMap<u16, Channel> = HashMap::new();
    //channels.insert("default".to_string(), Channel::new());

    let addr = SocketAddr::from(([0, 0, 0, 0], 1337));
    let builder = srt::async_builder()
        .set_live_transmission_type()
        .set_receive_latency(1000);

    let listener = builder.listen(addr, 5, Some(|socket, stream_id| {
        match decode_streamid(&stream_id) {
            Ok(streamopts) => {
                println!("Accepting stream ID: {:?}", streamopts);
                socket.set_passphrase("SecurePassphrase1").map_err(|_| SrtRejectReason::Predefined(500))?;
                Ok(())
            },
            Err(e) => {
                println!("Rejecting stream ID: {:?}", e);
                Err(SrtRejectReason::Predefined(400))
            }
        }
    }))?;

    loop {
        match accept(&mut channels, &listener).await {
            Ok(()) => (),
            Err(e) => println!("Error in accepter: {:?}", e)
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
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
        if r >= SIMULATED_PACKET_LOSS {
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
                                match program_definition.stream_type {
                                    0x1B | 0x24 => {
                                        let mut new_packet = packet.clone();
                                        let mut new_psi = new_packet.psi()?;
                                        let Some(PsiData::Pmt(ref mut new_pmt)) = new_psi.data else { panic!() };
                                        new_pmt.program_definitions = vec!(program_definition.clone());
                                        new_packet.set_psi(new_psi)?;

                                        channel.send_video(ChannelMessage::new(new_packet));
                                    },
                                    0x0F => if audio < 2 {
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
                                    },
                                    _ => {
                                        println!("stream_type = 0x{:02X}", program_definition.stream_type);
                                        //println!("stream_pid = 0x{:04X}", program_definition.stream_pid);
                                    }
                                }
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
