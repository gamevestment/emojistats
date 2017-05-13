extern crate dotenv;
#[macro_use]
extern crate log;
extern crate log4rs;

mod esbot;

const PROGRAM_NAME: &str = env!("CARGO_PKG_NAME");
const PROGRAM_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_LOG_FILENAME: &str = "emojistats.log";

const EXIT_STATUS_BOT_TOKEN_MISSING: i32 = 1;
const EXIT_STATUS_DB_CONFIG_INVALID: i32 = 2;

fn get_env_string(key: &str) -> Option<String> {
    let value = dotenv::var(key)
        .unwrap_or("".to_string())
        .trim()
        .to_string();

    if value.len() > 0 { Some(value) } else { None }
}

fn init_logging() {
    let filename = get_env_string("ES_LOG_FILENAME").unwrap_or(DEFAULT_LOG_FILENAME.to_string());

    let file = log4rs::append::file::FileAppender::builder()
            .encoder(Box::new(log4rs::encode::pattern::PatternEncoder::new(
                    "{d(%Y-%m-%d %H:%M:%S %Z)(local)}: {h({l})}: {m}{n}")))
            .build(filename)
            .expect("Failed to create log file");

    let appender = log4rs::config::Appender::builder().build("emojistats", Box::new(file));

    let log_level_filter: log::LogLevelFilter;
    if cfg!(debug_assertions) {
        log_level_filter = log::LogLevelFilter::Debug;
    } else {
        log_level_filter = log::LogLevelFilter::Info;
    }

    let config = log4rs::config::Config::builder()
        .appender(appender)
        .logger(log4rs::config::Logger::builder()
                    .appender("emojistats")
                    .build("emojistats", log_level_filter))
        .build(log4rs::config::Root::builder().build(log_level_filter))
        .expect("Failed to build logging configuration");

    log4rs::init_config(config).expect("Failed to initialize logger");
}

fn pg_get_conn_str() -> Result<(String, String), String> {
    let user = match get_env_string("ES_DB_USER") {
        Some(user) => user,
        None => {
            return Err("No user specified".to_string());
        }
    };

    let mut password = get_env_string("ES_DB_PASS").unwrap_or("".to_string());
    if password.len() > 0 {
        password = format!(":{}", password);
    }

    let host = dotenv::var("ES_DB_HOST").unwrap_or("localhost".to_string());

    let port = if let Some(port_str) = get_env_string("ES_DB_PORT") {
        match port_str.parse::<u16>() {
            Ok(port) => format!(":{}", port),
            Err(_) => {
                return Err(format!("Invalid port number \"{}\"", port_str));
            }
        }
    } else {
        "".to_string()
    };

    let database = match get_env_string("ES_DB_NAME") {
        Some(database) => database,
        None => {
            return Err("No database specified".to_string());
        }
    };

    let pg_conn_str = format!("postgres://{}{}@{}{}/{}",
                              user,
                              password,
                              host,
                              port,
                              database);
    let pg_conn_str_redacted = format!("postgres://{}{}@{}{}/{}",
                                       user,
                                       if password.len() > 0 {
                                           ":<REDACTED>"
                                       } else {
                                           ""
                                       },
                                       host,
                                       port,
                                       database);

    Ok((pg_conn_str, pg_conn_str_redacted))
}

fn main() {
    dotenv::dotenv().ok();
    init_logging();

    info!("Started {} v{}", PROGRAM_NAME, PROGRAM_VERSION);

    let (pg_conn_str, pg_conn_str_redacted) = match pg_get_conn_str() {
        Ok((pg_conn_str, pg_conn_str_redacted)) => (pg_conn_str, pg_conn_str_redacted),
        Err(reason) => {
            error!("Failed to build PostgreSQL connection string: {}", reason);
            std::process::exit(EXIT_STATUS_DB_CONFIG_INVALID);
        }
    };

    let bot_token = match get_env_string("ES_BOT_TOKEN") {
        Some(bot_token) => bot_token,
        None => {
            error!("No bot token found");
            std::process::exit(EXIT_STATUS_BOT_TOKEN_MISSING);
        }
    };

    info!("Connecting to \"{}\"", pg_conn_str_redacted);

    let mut eb = esbot::EsBot::new(pg_conn_str, bot_token);
    let exit_status = eb.run();

    std::process::exit(exit_status);
}
