extern crate discord;
extern crate postgres;

use std::collections::HashMap;
use self::discord::model::{Event, LiveServer, ServerId, Channel, ChannelType, ChannelId,
                           PrivateChannel, PublicChannel, Message, MessageType, UserId};
use self::discord::model::PossibleServer::Online;

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
    public_channels: HashMap<ChannelId, ServerId>,
    control_users: Vec<UserId>,
    command_prefix: String,
    command_prefix_skip: usize,

    discord: Option<discord::Discord>,
    quit: bool,
}

impl EsBot {
    pub fn new<S>(db_conn_str: S, bot_token: S) -> EsBot
        where S: Into<String>
    {
        EsBot {
            db_conn_str: db_conn_str.into(),
            bot_token: bot_token.into(),

            private_channels: Vec::new(),
            public_channels: HashMap::new(),
            control_users: Vec::new(),
            command_prefix: "".to_string(),
            command_prefix_skip: 0,

            discord: None,
            quit: false,
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

        let (mut discord_conn, ready_event) = match discord.connect() {
            Ok((discord_conn, ready_event)) => (discord_conn, ready_event),
            Err(reason) => {
                error!("Failed to create websocket connection to Discord: {}",
                       reason);
                return EXIT_STATUS_DISCORD_COULDNT_CONNECT;
            }
        };

        // Add servers and channels
        for server in &ready_event.servers {
            match *server {
                Online(ref server) => {
                    self.add_server(server);
                }
                _ => {}
            }
        }

        for channel in &ready_event.private_channels {
            self.add_channel(channel);
        }

        // Let users invoke commands from public channels
        self.command_prefix = format!("<@{}>", ready_event.user.id.0);
        self.command_prefix_skip = self.command_prefix.len() + 1;

        // Main loop
        self.discord = Some(discord);

        loop {
            match discord_conn.recv_event() {
                Ok(Event::ServerCreate(Online(server))) => {
                    self.add_server(&server);
                }
                Ok(Event::MessageCreate(message)) => {
                    // Process messages sent by people; ignore messages sent by bots
                    if &message.kind == &MessageType::Regular && !message.author.bot {
                        self.process_message(&message);
                    }
                }
                _ => {}
            }

            if self.quit {
                break;
            }

        }

        let _ = db_conn.finish();
        let _ = discord_conn.shutdown();
        0
    }

    fn process_message(&mut self, message: &Message) {
        if message.content.starts_with(&self.command_prefix) {
            let (_, command_str) = message.content.split_at(self.command_prefix_skip);
            self.process_command(message, command_str);
            return;
        }

        // Treat all private messages as commands
        if self.private_channels.contains(&message.channel_id) {
            self.process_command(message, &message.content);
            return;
        }

        debug!("Message from \"{}\": \"{}\"",
               message.author.name,
               message.content);
    }

    fn process_command(&mut self, message: &Message, command_str: &str) {
        debug!("Command from \"{}\": \"{}\"",
               message.author.name,
               command_str);

        let mut parts = command_str.split_whitespace();

        let command = match parts.next() {
            Some(command) => command,
            None => {
                return;
            }
        };

        match command {
            "quit" => {
                self.quit = true;
            }
            _ => {
                let _ = self.discord
                    .as_ref()
                    .unwrap()
                    .send_message(message.channel_id,
                                  &format!("Unknown command `{}`.", command),
                                  "",
                                  false);
            }
        }
    }

    fn add_server(&mut self, server: &LiveServer) {
        for channel in &server.channels {
            self.add_public_channel(channel);
        }
    }

    fn add_channel(&mut self, channel: &Channel) {
        match *channel {
            Channel::Private(ref channel) => {
                self.add_private_channel(channel);
            }
            Channel::Public(ref channel) => {
                self.add_public_channel(channel);
            }
            _ => {}
        }
    }

    fn add_private_channel(&mut self, channel: &PrivateChannel) {
        if channel.kind == ChannelType::Private {
            self.private_channels.push(channel.id);
        }
    }

    fn add_public_channel(&mut self, channel: &PublicChannel) {
        if channel.kind == ChannelType::Text {
            self.public_channels.insert(channel.id, channel.server_id);
        }
    }
}