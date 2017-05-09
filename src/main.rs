#[macro_use]
extern crate log;
extern crate log4rs;
extern crate config;
extern crate postgres;
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
                .build(log::LogLevelFilter::Debug))
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
            process::exit(1);
        }
    }

    let db_conn_str: String;
    let bot_token: String;

    {
        let host = match config.get_str("database.host") {
            Some(val) => val,
            None => {
                error!("The configuration is missing a database host.");
                process::exit(1);
            }
        };

        let port = match config.get_str("database.port") {
            Some(val) => val,
            None => {
                error!("The configuration is missing a database port.");
                process::exit(1);
            }
        };

        let user = match config.get_str("database.user") {
            Some(val) => val,
            None => {
                error!("The configuration is missing a database username.");
                process::exit(1);
            }
        };

        let password = match config.get_str("database.password") {
            Some(val) => val,
            None => {
                error!("The configuration is missing a database password.");
                process::exit(1);
            }
        };

        let database = match config.get_str("database.database") {
            Some(val) => val,
            None => {
                error!("The configuration is missing a database name.");
                process::exit(1);
            }
        };

        db_conn_str = format!("postgres://{user}:{password}@{host}:{port}/{database}",
            user = user,
            password = password,
            host = host,
            port = port,
            database = database
        );

        debug!("Database connection string: postgres://{user}:{password}@{host}:{port}/{database}",
            user = user,
            password = "<PASSWORD_REDACTED>",
            host = host,
            port = port,
            database = database
        );

        bot_token = match config.get_str("bot.bot_token") {
            Some(val) => val,
            None => {
                error!("The configuration is missing a bot token.");
                process::exit(1);
            }
        };
    }

    // Connect to database
    let db_conn = match postgres::Connection::connect(db_conn_str, postgres::TlsMode::None) {
        Ok(conn) => {
            debug!("Connected to database successfully");
            conn
        }
        Err(err) => {
            error!("{}", err);
            process::exit(1);
        }
    };

    // Create tables
    {
        let create_statements =
            "
            CREATE TABLE IF NOT EXISTS emoji (
                id SERIAL,
                discord_id NUMERIC NOT NULL,
                name VARCHAR(512),
                PRIMARY KEY (id)
            );

            CREATE TABLE IF NOT EXISTS message (
                id NUMERIC,
                guild_id NUMERIC NOT NULL,
                channel_id NUMERIC NOT NULL,
                user_id NUMERIC NOT NULL,
                emoji_count NUMERIC NOT NULL,
                PRIMARY KEY (id)
            );

            CREATE TABLE IF NOT EXISTS emoji_usage (
                emoji_id INTEGER NOT NULL,
                guild_id NUMERIC NOT NULL,
                channel_id NUMERIC NOT NULL,
                user_id NUMERIC NOT NULL,
                use_count NUMERIC NOT NULL,
                PRIMARY KEY (emoji_id, guild_id, channel_id, user_id),
                FOREIGN KEY (emoji_id) REFERENCES emoji (id)
            );
            ";
        match db_conn.batch_execute(create_statements) {
            Ok(_) => {},
            Err(err) => {
                error!("{}", err);
                process::exit(1);
            }
        }
    }

    // Connect to Discord
    let bot = match discord::Discord::from_bot_token(&bot_token[..]) {
        Ok(bot) => bot,
        Err(err) => {
            error!("{}", err);
            process::exit(1);
        }
    };

    let mut connection = match bot.connect() {
        Ok((connection, _)) => connection,
        Err(err) => {
            error!("{}", err);
            process::exit(1);
        }
    };

    debug!("Connected to Discord");

    loop {
        match connection.recv_event() {
            Ok(discord::model::Event::MessageCreate(message)) => {
                // If the message was set by a person-user, scrape the message for emojis

                println!("{} says: {}", message.author.name, message.content);
                if message.content == "!test" {
                    let _ = bot.send_message(message.channel_id, "This is a reply to the test.", "", false);
                } else if message.content == "!quit" {
                    connection.shutdown().unwrap();
                    info!("Quitting");
                    break;
                }
            }
            Ok(_) => {}
            Err(discord::Error::Closed(code, body)) => {
                println!("Gateway closed with code {:?}: {}", code, body);
                break;
            }
            Err(err) => {
                warn!("Receive error: {:?}", err);
            }
        }
    }

    db_conn.finish().unwrap();
}
