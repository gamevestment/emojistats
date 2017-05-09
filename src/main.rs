#[macro_use]
extern crate log;
extern crate log4rs;
extern crate config;
extern crate discord;

use log4rs::encode::pattern::PatternEncoder;
use std::process;

const PROGRAM_NAME: &'static str = env!("CARGO_PKG_NAME");
const PROGRAM_VERSION: &'static str = env!("CARGO_PKG_VERSION");
const LOG_FILE: &str = "emojistats.log";
const CONFIG_FILE: &str = "config.toml";

// TODO: fn to count Emoji

fn main() {
    fn init_logging_config() {
        let log_file = log4rs::append::file::FileAppender::builder()
            .encoder(Box::new(PatternEncoder::new("{d(%Y-%m-%d %H:%M:%S %Z)(local)}: {h({l})}: {m}{n}")))
            .build(LOG_FILE)
            .unwrap();

        let log_config = log4rs::config::Config::builder()
            .appender(log4rs::config::Appender::builder()
                .build("all", Box::new(log_file)))
            .build(log4rs::config::Root::builder()
                .appender("all")
                .build(log::LogLevelFilter::Info))
            .unwrap();

        log4rs::init_config(log_config).unwrap();
    }

    init_logging_config();

    info!("Started {} v{}", PROGRAM_NAME, PROGRAM_VERSION);

    // Read config.toml
    let mut config = config::Config::new();

    match config.merge(config::File::new(CONFIG_FILE, config::FileFormat::Toml)) {
        Ok(_) => {},
        Err(err) => {
            error!("{}", err);
            process::exit(1)
        }
    }

    let host = match config.get_str("database.host") {
        Some(val) => val,
        None => {
            error!("The configuration is missing a database host.");
            process::exit(1)
        }
    };

    let port = match config.get_str("database.port") {
        Some(val) => val,
        None => {
            error!("The configuration is missing a database port.");
            process::exit(1)
        }
    };

    let user = match config.get_str("database.user") {
        Some(val) => val,
        None => {
            error!("The configuration is missing a database username.");
            process::exit(1)
        }
    };

    let password = match config.get_int("database.port") {
        Some(val) => val,
        None => {
            error!("The configuration is missing a database password.");
            process::exit(1)
        }
    };

    let database = match config.get_str("database.database") {
        Some(val) => val,
        None => {
            error!("The configuration is missing a database name.");
            process::exit(1)
        }
    };

    let bot_token = match config.get_str("bot.bot_token") {
        Some(val) => val,
        None => {
            error!("The configuration is missing a bot token.");
            process::exit(1)
        }
    };

    // Print config
    println!("Host: {}", host);
    println!("Port: {}", port);
    println!("User: {}", user);
    println!("Password: {}", password);
    println!("Database: {}", database);

    // Connect to database

    // Connect to Discord
    let bot = match discord::Discord::from_bot_token(&bot_token[..]) {
        Ok(bot) => bot,
        Err(err) => {
            error!("{}", err);
            process::exit(1)
        }
    };

    let mut connection = match bot.connect() {
        Ok((connection, _)) => connection,
        Err(err) => {
            error!("{}", err);
            process::exit(1)
        }
    };

    loop {
        match connection.recv_event() {
            Ok(discord::model::Event::MessageCreate(message)) => {
                println!("{} says: {}", message.author.name, message.content);
                if message.content == "!test" {
                    let _ = bot.send_message(message.channel_id, "This is a reply to the test.", "", false);
                } else if message.content == "!quit" {
                    connection.shutdown().unwrap();
                    info!("Quitting");
                    println!("Quitting.");
                    break
                }
            }
            Ok(_) => {}
            Err(discord::Error::Closed(code, body)) => {
                println!("Gateway closed with code {:?}: {}", code, body);
                break
            }
            Err(err) => println!("Receive error: {:?}", err)
        }
    }
}
