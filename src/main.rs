extern crate dotenv;
#[macro_use]
extern crate log;
extern crate log4rs;
extern crate postgres;
extern crate serenity;

use std::process;

const PROGRAM_NAME: &'static str = env!("CARGO_PKG_NAME");
const PROGRAM_VERSION: &'static str = env!("CARGO_PKG_VERSION");
const DEFAULT_LOG_FILENAME: &str = "emojistats.log";

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

    let config = log4rs::config::Config::builder()
        .appender(appender)
        .build(log4rs::config::Root::builder()
                   .appender("emojistats")
                   .build(log::LogLevelFilter::Info))
        .expect("Failed to build logging configuration");

    log4rs::init_config(config).expect("Failed to initialize logger");
}

fn get_pg_connection_string() -> Result<(String, String), String> {
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

    let conn_string = format!("postgres://{}{}@{}{}/{}",
                              user,
                              password,
                              host,
                              port,
                              database);
    let conn_string_redacted = format!("postgres://{}{}@{}{}/{}",
                                       user,
                                       if password.len() > 0 {
                                           ":<REDACTED>"
                                       } else {
                                           ""
                                       },
                                       host,
                                       port,
                                       database);

    Ok((conn_string, conn_string_redacted))
}

fn main() {
    dotenv::dotenv().ok();
    init_logging();

    info!("Started {} v{}", PROGRAM_NAME, PROGRAM_VERSION);

    let (conn_string, conn_string_redacted) = match get_pg_connection_string() {
        Ok((conn_string, conn_string_redacted)) => (conn_string, conn_string_redacted),
        Err(reason) => {
            error!("Failed to build PostgreSQL connection string: {}", reason);
            process::exit(1);
        }
    };

    info!("Connecting to \"{}\"", conn_string_redacted);
}
