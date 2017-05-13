extern crate discord;
extern crate postgres;

use self::discord::model::{Channel, ChannelId};

const PG_CREATE_TABLE_STATEMENTS: &str = "
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
);";

const EXIT_STATUS_DB_COULDNT_CONNECT: i32 = 3;
const EXIT_STATUS_DB_COULDNT_CREATE_TABLES: i32 = 4;
const EXIT_STATUS_DISCORD_COULDNT_AUTHENTICATE: i32 = 10;
const EXIT_STATUS_DISCORD_COULDNT_CONNECT: i32 = 11;

pub struct EsBot {
    db_conn_str: String,
    bot_token: String,

    private_channels: Vec<ChannelId>,
    public_channels: Vec<ChannelId>,
}

impl EsBot {
    pub fn new<S>(db_conn_str: S, bot_token: S) -> EsBot
        where S: Into<String>
    {
        EsBot {
            db_conn_str: db_conn_str.into(),
            bot_token: bot_token.into(),

            private_channels: Vec::new(),
            public_channels: Vec::new(),
        }
    }

    pub fn run(&mut self) -> i32 {
        // Connect to database
        let db_conn = match postgres::Connection::connect(self.db_conn_str.as_str(),
                                                          postgres::TlsMode::None) {
            Ok(db_conn) => db_conn,
            Err(reason) => {
                error!("Failed to connect to PostgreSQL: {}", reason);
                return EXIT_STATUS_DB_COULDNT_CONNECT;
            }
        };

        if let Err(reason) = db_conn.batch_execute(PG_CREATE_TABLE_STATEMENTS) {
            error!("Failed to create tables: {}", reason);
            let _ = db_conn.finish();
            return EXIT_STATUS_DB_COULDNT_CREATE_TABLES;
        }

        // Connect to Discord
        let discord = match discord::Discord::from_bot_token(&self.bot_token) {
            Ok(discord) => discord,
            Err(reason) => {
                error!("Failed to authenticate with Discord: {}", reason);
                let _ = db_conn.finish();
                return EXIT_STATUS_DISCORD_COULDNT_AUTHENTICATE;
            }
        };

        let (discord_conn, ready_event) = match discord.connect() {
            Ok((discord_conn, ready_event)) => (discord_conn, ready_event),
            Err(reason) => {
                error!("Failed to create websocket connection to Discord: {}",
                       reason);
                return EXIT_STATUS_DISCORD_COULDNT_CONNECT;
            }
        };

        // Add channels
        for channel in &ready_event.private_channels {
            self.add_channel(channel);
        }

        let _ = db_conn.finish();
        let _ = discord_conn.shutdown();
        0
    }

    fn add_channel(&mut self, channel: &Channel) {
        match *channel {
            Channel::Private(ref channel) => {
                self.private_channels.push(channel.id);
            }
            Channel::Public(ref channel) => {
                self.public_channels.push(channel.id);
            }
            _ => {}
        }
    }
}
