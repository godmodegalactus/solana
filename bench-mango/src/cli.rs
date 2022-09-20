use {
    clap::{crate_description, crate_name, App, Arg, ArgMatches},
    solana_clap_utils::input_validators::{is_url, is_url_or_moniker},
    solana_cli_config::{ConfigInput, CONFIG_FILE},
    solana_sdk::signature::{read_keypair_file, Keypair},
    std::{net::SocketAddr, process::exit, time::Duration},
};

/// Holds the configuration for a single run of the benchmark
pub struct Config {
    pub entrypoint_addr: SocketAddr,
    pub json_rpc_url: String,
    pub websocket_url: String,
    pub id: Keypair,
    pub duration: Duration,
    pub quotes_per_second: u64,
    pub account_keys: String,
    pub mango_keys: String,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            entrypoint_addr: SocketAddr::from(([127, 0, 0, 1], 8001)),
            json_rpc_url: ConfigInput::default().json_rpc_url,
            websocket_url: ConfigInput::default().websocket_url,
            id: Keypair::new(),
            duration: Duration::new(std::u64::MAX, 0),
            quotes_per_second: 1,
            account_keys: String::new(),
            mango_keys: String::new(),
        }
    }
}

/// Defines and builds the CLI args for a run of the benchmark
pub fn build_args<'a, 'b>(version: &'b str) -> App<'a, 'b> {
    App::new(crate_name!())
        .about(crate_description!())
        .version(version)
        .arg({
            let arg = Arg::with_name("config_file")
                .short("C")
                .long("config")
                .value_name("FILEPATH")
                .takes_value(true)
                .global(true)
                .help("Configuration file to use");
            if let Some(ref config_file) = *CONFIG_FILE {
                arg.default_value(config_file)
            } else {
                arg
            }
        })
        .arg(
            Arg::with_name("json_rpc_url")
                .short("u")
                .long("url")
                .value_name("URL_OR_MONIKER")
                .takes_value(true)
                .global(true)
                .validator(is_url_or_moniker)
                .help(
                    "URL for Solana's JSON RPC or moniker (or their first letter): \
                     [mainnet-beta, testnet, devnet, localhost]",
                ),
        )
        .arg(
            Arg::with_name("websocket_url")
                .long("ws")
                .value_name("URL")
                .takes_value(true)
                .global(true)
                .validator(is_url)
                .help("WebSocket URL for the solana cluster"),
        )
        .arg(
            Arg::with_name("entrypoint")
                .short("n")
                .long("entrypoint")
                .value_name("HOST:PORT")
                .takes_value(true)
                .help(
                    "Rendezvous with the cluster at this entry point; defaults to 127.0.0.1:8001",
                ),
        )
        .arg(
            Arg::with_name("identity")
                .short("i")
                .long("identity")
                .value_name("PATH")
                .takes_value(true)
                .help("File containing a client identity (keypair)"),
        )
        .arg(
            Arg::with_name("duration")
                .short("d")
                .long("duration")
                .value_name("SECS")
                .takes_value(true)
                .help("Seconds to run benchmark, then exit; default is forever"),
        )
        .arg(
            Arg::with_name("qoutes_per_second")
                .short("q")
                .long("qoutes_per_second")
                .value_name("QPS")
                .takes_value(true)
                .help("Number of quotes per second"),
        )
        .arg(
            Arg::with_name("account_keys")
                .short("a")
                .long("accounts")
                .value_name("FILENAME")
                .required(true)
                .takes_value(true)
                .help("Read account keys from JSON file generated with mango-client-v3"),
        )
        .arg(
            Arg::with_name("mango_keys")
                .short("m")
                .long("mango")
                .value_name("FILENAME")
                .required(true)
                .takes_value(true)
                .help("Read mango keys from JSON file generated with mango-client-v3"),
        )
}

/// Parses a clap `ArgMatches` structure into a `Config`
/// # Arguments
/// * `matches` - command line arguments parsed by clap
/// # Panics
/// Panics if there is trouble parsing any of the arguments
pub fn extract_args(matches: &ArgMatches) -> Config {
    let mut args = Config::default();

    let config = if let Some(config_file) = matches.value_of("config_file") {
        solana_cli_config::Config::load(config_file).unwrap_or_default()
    } else {
        solana_cli_config::Config::default()
    };
    let (_, json_rpc_url) = ConfigInput::compute_json_rpc_url_setting(
        matches.value_of("json_rpc_url").unwrap_or(""),
        &config.json_rpc_url,
    );
    args.json_rpc_url = json_rpc_url;

    let (_, websocket_url) = ConfigInput::compute_websocket_url_setting(
        matches.value_of("websocket_url").unwrap_or(""),
        &config.websocket_url,
        matches.value_of("json_rpc_url").unwrap_or(""),
        &config.json_rpc_url,
    );
    args.websocket_url = websocket_url;

    let (_, id_path) = ConfigInput::compute_keypair_path_setting(
        matches.value_of("identity").unwrap_or(""),
        &config.keypair_path,
    );
    if let Ok(id) = read_keypair_file(id_path) {
        args.id = id;
    } else if matches.is_present("identity") {
        panic!("could not parse identity path");
    }

    if let Some(addr) = matches.value_of("entrypoint") {
        args.entrypoint_addr = solana_net_utils::parse_host_port(addr).unwrap_or_else(|e| {
            eprintln!("failed to parse entrypoint address: {}", e);
            exit(1)
        });
    }

    if let Some(duration) = matches.value_of("duration") {
        args.duration = Duration::new(
            duration.to_string().parse().expect("can't parse duration"),
            0,
        );
    }

    if let Some(qps) = matches.value_of("qoutes_per_second") {
        args.quotes_per_second = qps.parse().expect("can't parse qoutes_per_second");
    }

    args.account_keys = matches.value_of("account_keys").unwrap().to_string();
    args.mango_keys = matches.value_of("mango_keys").unwrap().to_string();

    args
}
