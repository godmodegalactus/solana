use {clap::Parser, std::str::FromStr};

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct ClientArgs {
    #[arg(short = 'm', long, default_value_t = 20000)]
    pub maximum_number_of_connections: usize,

    #[arg(short, long, default_value_t = false)]
    pub enable_tls_support: bool,

    #[arg(short = 'n', long, default_value_t = 1)]
    pub number_of_clients: usize,

    #[arg(short = 't', long, default_value_t = 1_000_000)]
    pub number_of_transactions_per_client: usize,

    #[arg(short = 'l', long, default_value_t = false)]
    pub large_transactions: bool,

    #[arg(short = 's', long, default_value_t = String::from_str("127.0.0.1:10800").unwrap() )]
    pub server: String,

    #[arg(short = 't', long, default_value_t = 60)]
    pub timeout_in_seconds: u64,

    #[arg(short = 'u', long, default_value_t = 128)]
    pub unistream_count: usize,
}
