extern crate discord;
extern crate postgres;

use std::collections::HashMap;
use self::discord::model::{Event, LiveServer, ServerId, Channel, ChannelType, ChannelId,
                           PrivateChannel, PublicChannel, Message, MessageType, Emoji, EmojiId,
                           UserId};
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
    bot_control_password: String,

    private_channels: Vec<ChannelId>,
    public_channels: HashMap<ChannelId, ServerId>,
    custom_emojis: HashMap<EmojiId, String>,
    control_users: Vec<UserId>,
    command_prefix: String,
    command_prefix_skip: usize,

    discord: Option<discord::Discord>,
    quit: bool,
}

impl EsBot {
    pub fn new<S>(db_conn_str: S, bot_token: S, bot_control_password: S) -> EsBot
        where S: Into<String>
    {
        EsBot {
            db_conn_str: db_conn_str.into(),
            bot_token: bot_token.into(),
            bot_control_password: bot_control_password.into(),

            private_channels: Vec::new(),
            public_channels: HashMap::new(),
            custom_emojis: HashMap::new(),
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
                Ok(Event::ChannelCreate(channel)) => {
                    self.add_channel(&channel);
                }
                Ok(Event::ChannelUpdate(channel)) => {
                    self.add_channel(&channel);
                }
                Ok(Event::ServerEmojisUpdate(_, custom_emojis)) => {
                    self.add_custom_emojis(&custom_emojis);
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
        // TODO: Ensure that the message channel ID is known

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
            "auth" | "authenticate" => {
                if !self.private_channels.contains(&message.channel_id) {
                    self.send_message(&message.channel_id,
                                      format!("Please send me a direct message to authenticate."));

                    return;
                }

                if self.control_users.contains(&message.author.id) {
                    self.send_message(&message.channel_id,
                                      format!("You have already authenticated."));
                    return;
                }

                match parts.next() {
                    Some(try_password) => {
                        if try_password == self.bot_control_password {
                            debug!("Authentication successful for {}:\"{}\"",
                                   message.author.id.0,
                                   message.author.name);

                            self.control_users.push(message.author.id);
                            self.send_message(&message.channel_id,
                                              format!("Authentication successful."));
                        } else {
                            info!("Failed authentication attempt by {}:\"{}\" with password \"{}\"",
                                  message.author.id.0,
                                  message.author.name,
                                  try_password);

                            self.send_message(&message.channel_id,
                                              format!("Authentication unsuccessful."));
                        }
                    }
                    None => {
                        self.send_message(&message.channel_id, format!("Please enter a password."));
                    }
                };
            }
            "quit" => {
                if !self.control_users.contains(&message.author.id) {
                    self.send_message(&message.channel_id, "Please authenticate first.");
                    return;
                }

                info!("Quitting per {}:\"{}\"",
                      &message.author.id.0,
                      &message.author.name);
                self.quit = true;
            }
            _ => {
                self.send_message(&message.channel_id,
                                  format!("Unknown command `{}`.", command));
            }
        }
    }

    fn add_server(&mut self, server: &LiveServer) {
        debug!("Adding from server: \"{}\"", server.name);

        for channel in &server.channels {
            self.add_public_channel(channel);
        }

        self.add_custom_emojis(&server.emojis);
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
            debug!("Added private channel: \"{}\"", channel.recipient.name);

            self.private_channels.push(channel.id);
        }
    }

    fn add_public_channel(&mut self, channel: &PublicChannel) {
        if channel.kind == ChannelType::Text {
            debug!("Added public channel: \"#{}\"", channel.name);

            self.public_channels.insert(channel.id, channel.server_id);
        }
    }

    fn add_custom_emojis(&mut self, custom_emojis: &Vec<Emoji>) {
        for custom_emoji in custom_emojis {
            let custom_emoji_name = format!("<:{}:{}>", custom_emoji.name, custom_emoji.id.0);

            debug!("Added custom emoji: \"{}\"", custom_emoji_name);

            self.custom_emojis
                .insert(custom_emoji.id, custom_emoji_name);
        }
    }

    fn send_message<S>(&self, channel_id: &ChannelId, message: S)
        where S: Into<String>
    {
        let _ = self.discord
            .as_ref()
            .unwrap()
            .send_message(*channel_id, &message.into(), "", false);
    }
}
