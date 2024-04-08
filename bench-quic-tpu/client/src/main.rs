use {
    clap::Parser,
    cli::ClientArgs,
    itertools::Itertools,
    rand::{
        distributions::{Alphanumeric, Distribution},
        SeedableRng,
    },
    solana_client::{
        connection_cache::ConnectionCache,
        nonblocking::quic_client::{QuicClientCertificate, QuicLazyInitializedEndpoint},
    },
    solana_sdk::{
        compute_budget::ComputeBudgetInstruction,
        hash::Hash,
        instruction::{AccountMeta, Instruction},
        message::v0,
        packet::TLSSupport,
        pubkey::Pubkey,
        signature::Keypair,
        signer::Signer,
        transaction::VersionedTransaction,
    },
    solana_streamer::tls_certificates::new_dummy_x509_certificate,
    std::{
        net::SocketAddr,
        str::FromStr,
        sync::{atomic::AtomicUsize, Arc},
        time::Duration,
    },
};

mod cli;

pub fn create_connection_cache(tpu_pool_size: usize, tls_support: TLSSupport) -> ConnectionCache {
    ConnectionCache::new_quic(
        "bench-tps-connection_cache_quic",
        tpu_pool_size,
        tls_support,
    )
}

#[inline]
pub fn generate_random_strings(
    num_of_txs: usize,
    random_seed: Option<u64>,
    n_chars: usize,
) -> Vec<Vec<u8>> {
    let seed = random_seed.map_or(0, |x| x);
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
    (0..num_of_txs)
        .map(|_| Alphanumeric.sample_iter(&mut rng).take(n_chars).collect())
        .collect()
}

pub fn create_memo_tx(
    memo_program_id: Pubkey,
    msg: &[u8],
    payer: &Keypair,
    blockhash: Hash,
    cu_price_micro_lamports: u64,
) -> Vec<u8> {
    let accounts = (0..8).map(|_| Keypair::new()).collect_vec();
    let cu_budget_ix: Instruction =
        ComputeBudgetInstruction::set_compute_unit_price(cu_price_micro_lamports);
    let cu_limit_ix: Instruction = ComputeBudgetInstruction::set_compute_unit_limit(14000);

    let instruction = Instruction::new_with_bytes(
        memo_program_id,
        msg,
        accounts
            .iter()
            .map(|keypair| AccountMeta::new_readonly(keypair.pubkey(), true))
            .collect_vec(),
    );
    let message = v0::Message::try_compile(
        &payer.pubkey(),
        &[cu_budget_ix, cu_limit_ix, instruction],
        &[],
        blockhash,
    )
    .unwrap();
    let versioned_message = solana_sdk::message::VersionedMessage::V0(message);
    let mut signers = vec![payer];
    signers.extend(accounts.iter());

    let tx = VersionedTransaction::try_new(versioned_message, &signers).unwrap();
    bincode::serialize(&tx).unwrap()
}

#[derive(Clone, Default)]
struct ClientStats {
    pub transactions_sent: Arc<AtomicUsize>,
    pub connection_failed: Arc<AtomicUsize>,
    pub unistream_failed: Arc<AtomicUsize>,
    pub write_failed: Arc<AtomicUsize>,
    pub finish_failed: Arc<AtomicUsize>,
}

impl ClientStats {
    pub fn print(&self) {
        println!(
            "Transactions successfully sent : {}",
            self.transactions_sent
                .load(std::sync::atomic::Ordering::Relaxed)
        );
        println!(
            "Connections failed : {}",
            self.connection_failed
                .load(std::sync::atomic::Ordering::Relaxed)
        );
        println!(
            "Unistream failed : {}",
            self.unistream_failed
                .load(std::sync::atomic::Ordering::Relaxed)
        );
        println!(
            "Write failed : {}",
            self.write_failed.load(std::sync::atomic::Ordering::Relaxed)
        );
        println!(
            "Finish failed : {}",
            self.finish_failed
                .load(std::sync::atomic::Ordering::Relaxed)
        );
    }
}

fn _create_transactions(count: usize, is_large: bool) -> Vec<Vec<u8>> {
    let blockhash = Hash::default();
    let payer_keypair = Keypair::new();
    let seed = 42;
    let size = if is_large { 232 } else { 5 };
    let random_strings = generate_random_strings(1, Some(seed), size);
    let rand_string = random_strings.first().unwrap();

    let memo_program_id = Pubkey::new_unique();
    (0..count)
        .map(|_| create_memo_tx(memo_program_id, rand_string, &payer_keypair, blockhash, 300))
        .collect_vec()
}

fn create_dummy_data(count: usize, is_large: bool) -> Vec<Vec<u8>> {
    let size: usize = if is_large {
        2000
    } else {
        200
    };
    let vec = (0..size).map(|x| (x%(u8::MAX as usize)) as u8).collect_vec();
    (0..count).map(|_| vec.clone()).collect_vec()
}

#[tokio::main]
pub async fn main() {
    solana_logger::setup_with_default_filter();

    let args = ClientArgs::parse();

    let nb_transactions = args.number_of_clients * args.number_of_transactions_per_client;

    println!("Creating transactions");
    let transactions = create_dummy_data(nb_transactions, args.large_transactions);

    //let mut jhs = vec![];
    let tls_support = if args.enable_tls_support {
        TLSSupport::Enable
    } else {
        TLSSupport::SingleCert
    };
    let transactions_per_connections = (args.number_of_transactions_per_client
        / args.maximum_number_of_connections)
        .max(args.number_of_transactions_per_client)
        .min(1);

    let addr = SocketAddr::from_str(args.server.as_str()).unwrap();
    let client_stats = ClientStats::default();
    let unistream_count = args.unistream_count;

    println!(
        "Creating {} clients, sending :{}, through {} connections and {} unistreams",
        args.number_of_clients,
        args.number_of_transactions_per_client,
        args.maximum_number_of_connections,
        args.unistream_count
    );

    let mut connection_tasks = vec![];
    // batch by client
    for chunk in transactions.chunks(args.number_of_transactions_per_client) {
        let (certificate, key) = new_dummy_x509_certificate(&Keypair::new());
        let client_certificate = Arc::new(QuicClientCertificate { certificate, key });
        let lazy_endpoint = QuicLazyInitializedEndpoint::new(client_certificate, None);
        let endpoint = Arc::new(lazy_endpoint.create_endpoint(tls_support));
        // batch by connections
        for client_transactions in chunk.chunks(transactions_per_connections) {
            let client_transactions = client_transactions.to_vec();
            let connecting = endpoint.connect(addr, "connect").unwrap();
            let client_stats = client_stats.clone();
            let connection_task = tokio::spawn(async move {
                if let Ok(connection) = connecting.await {
                    let connection = Arc::new(connection);

                    // batch by unistream
                    for transaction_batch in client_transactions.chunks(unistream_count) {
                        let mut uni_tasks = vec![];
                        let transaction_batch = transaction_batch.to_vec();
                        for transaction in transaction_batch {
                            let client_stats = client_stats.clone();
                            let connection = connection.clone();
                            let uni_task = tokio::spawn(async move {
                                if let Ok(mut unistream) = connection.open_uni().await {
                                    if unistream.write_all(&transaction).await.is_err() {
                                        client_stats
                                            .write_failed
                                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                    } else if unistream.finish().await.is_err() {
                                        client_stats
                                            .finish_failed
                                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                    } else {
                                        client_stats
                                            .transactions_sent
                                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                    }
                                } else {
                                    client_stats
                                        .unistream_failed
                                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                }
                            });
                            uni_tasks.push(uni_task)
                        }
                        futures::future::join_all(uni_tasks).await;
                    }
                } else {
                    client_stats
                        .connection_failed
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            });
            connection_tasks.push(connection_task);
        }
    }

    if tokio::time::timeout(
        Duration::from_secs(args.timeout_in_seconds),
        futures::future::join_all(connection_tasks),
    )
    .await
    .is_err()
    {
        println!("Operation timed out");
    }
    client_stats.print();
}
