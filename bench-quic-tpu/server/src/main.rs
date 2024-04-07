mod cli;

use {
    clap::Parser,
    cli::ServerArgs,
    solana_sdk::{net::DEFAULT_TPU_COALESCE, packet::TLSSupport, signature::Keypair},
    solana_streamer::{
        nonblocking::quic::{spawn_server, DEFAULT_WAIT_FOR_CHUNK_TIMEOUT},
        quic::{MAX_STAKED_CONNECTIONS, MAX_UNSTAKED_CONNECTIONS},
        streamer::StakedNodes,
    },
    std::{
        net::UdpSocket,
        sync::{
            atomic::{AtomicBool, AtomicUsize},
            Arc, RwLock,
        },
        time::Duration,
    },
};

#[tokio::main]
pub async fn main() {
    solana_logger::setup_with_default_filter();
    let args = ServerArgs::parse();

    let (sender, receiver) = crossbeam_channel::unbounded();

    let staked_nodes = Arc::new(RwLock::new(StakedNodes::default()));
    // will create random free port
    let sock = UdpSocket::bind(format!("0.0.0.0:{}", args.server_port)).unwrap();
    let exit = Arc::new(AtomicBool::new(false));
    // keypair to derive the server tls certificate
    let keypair = Keypair::new();
    let packets_count = Arc::new(AtomicUsize::new(0));

    let tls_support = if args.enable_tls_support {
        TLSSupport::Enable
    } else {
        TLSSupport::SingleCert
    };

    let (_, stream_stats, jh_quic) = spawn_server(
        "quic_bencher",
        sock,
        &keypair,
        sender,
        exit,
        args.maximum_number_of_connections,
        staked_nodes,
        MAX_STAKED_CONNECTIONS,
        MAX_UNSTAKED_CONNECTIONS,
        DEFAULT_WAIT_FOR_CHUNK_TIMEOUT,
        DEFAULT_TPU_COALESCE,
        tls_support,
    )
    .unwrap();

    let jh_depiler = {
        let packets_count = packets_count.clone();
        let max_transactions = args.number_of_expected_transactions;
        tokio::spawn(async move {
            for _ in 0..max_transactions {
                match receiver.recv() {
                    Ok(_) => {
                        packets_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    Err(_) => {
                        break;
                    }
                }
            }
        })
    };

    let timeout_seconds = Duration::from_secs(args.timeout_in_seconds as u64);
    let _ = tokio::time::timeout(
        timeout_seconds,
        futures::future::join_all(vec![jh_quic, jh_depiler]),
    )
    .await;
    println!(
        "Got number of transactions : {}",
        packets_count.load(std::sync::atomic::Ordering::Relaxed)
    );

    stream_stats.print();
}
