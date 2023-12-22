use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use srt_rs as srt;
use srt_rs::{SrtTransmissionType, SrtSocket};
use srt_rs::error::SrtError;

#[derive(Debug)]
enum SRSError {
    SRTStartup,
    SRTCleanup,
    New,
    SetSockFlag(SrtError),
    Bind(SrtError),
    Listen,
    Accept
}

type SRSResult<T> = std::result::Result<T, SRSError>;

fn create_listener() -> SRSResult<SrtSocket> {
    let sock = SrtSocket::new().map_err(|_| SRSError::New)?;

    //sock.set_transmission_type(SrtTransmissionType::Live).map_err(|err| SRSError::SetTransmissionType(err))?;
    sock.set_receive_blocking(true).map_err(|err| SRSError::SetSockFlag(err))?;

    let addr = SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080);
    println!("binding to {:?}", addr);

    let sock = sock.bind(addr).map_err(|err| SRSError::Bind(err))?;

    sock.listen(5).map_err(|_| SRSError::Listen)?;

    Ok(sock)
}

fn run() -> SRSResult<()> {
    srt::startup().map_err(|_| SRSError::SRTStartup)?;

    let sock = create_listener()?;

    let _accepted = sock.accept().map_err(|_| SRSError::Accept)?;
    println!("accepted connection");

    srt::cleanup().map_err(|_| SRSError::SRTCleanup)
}

fn main() {
    //match run() {
    //    Ok(()) => (),
    //    Err(e) => println!("error: {:?}", e)
    //}

    let addr: SocketAddr = "127.0.0.1:1337".parse().expect("invalid addr:port syntax");

    srt::startup().expect("startup");
    let ss = SrtSocket::new().expect("create_socket");
    ss.bind(addr).expect("bind");
}
