use tokio_tun;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UdpSocket;
use std::str::FromStr;
use clap::Parser;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::watch;

const MTU: usize = 1500;

#[derive(Parser, Debug)]
#[command(name = "veth", about = "veth: virtual L2 connection")]
struct Args {
    tun_name: String,

    local_addr: String,

    #[arg(short = 'c')]
    remote_addr: Option<String>,
}


#[tokio::main]
async fn main() {
    let args = Args::parse();

    let tun = tokio_tun::TunBuilder::new()
        .name(&args.tun_name)
        .tap(true)
        .mtu(MTU.try_into().unwrap())
        .try_build().unwrap();
    let (mut tun_reader, mut tun_writer) = tokio::io::split(tun);

    let is_server = args.remote_addr.is_none();

    let remote_addr: Option<SocketAddr> = args.remote_addr.map(|a| SocketAddr::from_str(&a).unwrap());

    let (addr_tx, mut addr_rx) = watch::channel(remote_addr);

    let sock = Arc::new(UdpSocket::bind(args.local_addr).await.unwrap());

    let recv = {
        let sock = sock.clone();
        async move {
            let mut last_addr = None;
            let mut buf = [0u8; MTU];
            while let Ok((len, addr)) = sock.recv_from(&mut buf).await {
                println!("rcvd: {:?}", len);
                tun_writer.write(&buf[..len]).await.unwrap();
                if is_server {
                    if last_addr != Some(addr) {
                        addr_tx.send(Some(addr)).unwrap();
                        last_addr = Some(addr);
                    }
                }
            }
        }
    };

    let send = async move {
        let mut buf = [0u8; MTU];
        let mut addr = addr_rx.borrow().clone();
        loop {
            tokio::select! {
                res = tun_reader.read(&mut buf) => {
                    let len = res.unwrap();
                    println!("send: {}", len);
                    if let Some(addr) = addr {
                        sock.send_to(&buf[..len], addr).await.unwrap();
                    }
                }
                _ = addr_rx.changed() => {
                    addr = addr_rx.borrow().clone();
                    println!("addr changed: {:?}", addr);
                }
            }
        }
    };

    tokio::join!(recv, send);
}
