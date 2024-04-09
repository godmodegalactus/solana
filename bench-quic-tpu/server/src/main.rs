mod cli;

use {
    clap::Parser,
    cli::ServerArgs,
    solana_sdk::{
        native_token::LAMPORTS_PER_SOL, net::DEFAULT_TPU_COALESCE, packet::TLSSupport,
        signature::Keypair, signer::Signer,
    },
    solana_streamer::{
        nonblocking::quic::{spawn_server, DEFAULT_WAIT_FOR_CHUNK_TIMEOUT},
        streamer::StakedNodes,
    },
    std::{
        collections::HashMap,
        net::UdpSocket,
        sync::{
            atomic::{AtomicBool, AtomicUsize},
            Arc, RwLock,
        },
        time::Duration,
    },
};

pub async fn load_identity_keypair(identity_keyfile_path: String) -> Keypair {
    let bytes = tokio::fs::read_to_string(identity_keyfile_path).await;

    if let Ok(bytes) = bytes {
        let identity_bytes: Vec<u8> = serde_json::from_str(&bytes).unwrap();
        println!("Loading identity from file");
        Keypair::from_bytes(identity_bytes.as_slice()).unwrap()
    } else {
        println!("Kp file does not exist so creating a new one");
        Keypair::new()
    }
}

#[tokio::main]
pub async fn main() {
    solana_logger::setup_with_default_filter();
    let args = ServerArgs::parse();

    let (sender, receiver) = crossbeam_channel::unbounded();

    // will create random free port
    let sock = UdpSocket::bind(format!("0.0.0.0:{}", args.server_port)).unwrap();
    let exit = Arc::new(AtomicBool::new(false));
    // keypair to derive the server tls certificate
    let keypair = load_identity_keypair(args.identity).await;

    let mut hashmap = HashMap::new();
    hashmap.insert(keypair.pubkey(), LAMPORTS_PER_SOL);
    let nodes = StakedNodes::new(Arc::new(hashmap), HashMap::new());
    let staked_nodes = Arc::new(RwLock::new(nodes));
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
        args.maximum_number_of_connections,
        args.maximum_number_of_connections,
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
