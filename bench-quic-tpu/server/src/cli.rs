use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(author, version, about, long_about = None)]
pub struct ServerArgs {
    #[arg(short = 'm', long, default_value_t = 20000)]
    pub maximum_number_of_connections: usize,

    #[arg(short, long, default_value_t = false)]
    pub enable_tls_support: bool,

    #[arg(short = 'e', long, default_value_t = 1_000_000)]
    pub number_of_expected_transactions: usize,

    #[arg(short = 'e', long, default_value_t = 60)]
    pub timeout_in_seconds: usize,

    #[arg(short = 'p', long, default_value_t = 10800)]
    pub server_port: u16,
}
