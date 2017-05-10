#[macro_use]
extern crate log;
extern crate log4rs;
extern crate config;
extern crate postgres;
extern crate discord;

use log4rs::encode::pattern::PatternEncoder;
use std::process;
use std::collections::{ HashMap, HashSet };
use std::hash::{Hash, Hasher};
use discord::model::{
    Event,
    ChannelType,
    MessageType,
    ServerId,
    PublicChannel,
    ChannelId,
    Message,
    MessageId,
    Emoji,
    EmojiId
};
use discord::model::PossibleServer::Online;

const PROGRAM_NAME: &'static str = env!("CARGO_PKG_NAME");
const PROGRAM_VERSION: &'static str = env!("CARGO_PKG_VERSION");
const LOG_FILE: &str = "emojistats.log";
const CONFIG_FILE: &str = "config.toml";

#[derive(Debug)]
struct FlattenedEmoji {
    id: EmojiId,
    text: String
}

impl Hash for FlattenedEmoji {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl PartialEq for FlattenedEmoji {
    fn eq(&self, other: &FlattenedEmoji) -> bool {
        self.id.0 == other.id.0
    }
}

impl Eq for FlattenedEmoji {}

#[derive(Debug)]
struct EmojiTracker {
    db_conn: postgres::Connection,
    channels: HashMap<ChannelId, ServerId>,
    emojis: HashSet<FlattenedEmoji>
}

impl EmojiTracker {
    fn new(db_conn: postgres::Connection) -> EmojiTracker {
        EmojiTracker {
            db_conn: db_conn,
            channels: HashMap::new(),
            emojis: HashSet::new()
        }
    }

    fn add_channel(&mut self, channel: &PublicChannel) {
        self.channels.insert(channel.id, channel.server_id);
    }

    fn add_emojis(&mut self, emojis: &Vec<Emoji>) {
        for emoji in emojis {
            self.emojis.replace(FlattenedEmoji {
                id: emoji.id,
                text: format!("<:{}:{}>", emoji.name, emoji.id.0)
            });
        }
    }

    fn process_message(&self, message: &Message) {
        println!("{}", &message.content);
        for emoji in &self.emojis {
            let count = message.content.matches(&emoji.text[..]).count();
            println!("{} instances of {}", count, emoji.text);
        }
    }

    fn finish(self) -> () {
        println!("{:?}", &self);
        self.db_conn.finish().unwrap();
    }
}

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
        Ok(_) => {}
        Err(err) => {
            error!("{}", err);
            process::exit(1);
        }
    }

    let db_conn_str: String;
    let bot_token: String;

    {
        let host = match config.get_str("database.host") {
            Some(host) => host,
            None => "localhost".to_string()
        };

        let port = match config.get_int("database.port") {
            Some(port) => port,
            None => 5432
        };

        let user = match config.get_str("database.user") {
            Some(user) => user,
            None => {
                error!("The configuration is missing a database username.");
                process::exit(1);
            }
        };

        let password = match config.get_str("database.password") {
            Some(password) => format!(":{}", password),
            None => "".to_string()
        };

        let database = match config.get_str("database.database") {
            Some(val) => val,
            None => {
                error!("The configuration is missing a database name.");
                process::exit(1);
            }
        };

        db_conn_str = format!("postgres://{user}{password}@{host}:{port}/{database}",
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
        let create_statements = "\
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
            Ok(_) => {}
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

    let mut et = EmojiTracker::new(db_conn);

    loop {
        match connection.recv_event() {
            Ok(Event::ServerCreate(Online(server))) => {
                // Map text channel IDs to server IDs
                for channel in &server.channels {
                    match &channel.kind {
                        &ChannelType::Text => {
                            et.add_channel(&channel);
                        }
                        _ => {}
                    }
                }

                et.add_emojis(&server.emojis);
            }
            Ok(Event::ServerEmojisUpdate(_, emojis)) => {
                et.add_emojis(&emojis);
            }
            Ok(Event::MessageCreate(message)) => {
                // If the message was set by a person-user, scrape the message for emojis
                match &message.kind {
                    &MessageType::Regular => {
                        if !message.author.bot {
                            match &message.content {
                                _ if &message.content == "!quit" => {
                                    break;
                                }
                                _ => {
                                    et.process_message(&message);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(something) => {
                println!("Got something: {:?}", something);
            }
            Err(discord::Error::Closed(code, body)) => {
                println!("Gateway closed with code {:?}: {}", code, body);
                break;
            }
            Err(err) => {
                warn!("Receive error: {:?}", err);
            }
        }
    }

    et.finish();
}
