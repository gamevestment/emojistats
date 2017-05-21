extern crate discord;
extern crate postgres;

use arg;

use std::collections::HashMap;
use self::discord::model::{Event, LiveServer, ServerId, Channel, ChannelType, ChannelId,
                           PrivateChannel, PublicChannel, Message, MessageType, Emoji, EmojiId,
                           UserId};
use self::discord::model::PossibleServer::Online;

const PG_CREATE_TABLES: &str = "
CREATE TABLE IF NOT EXISTS emoji (
    id BIGSERIAL NOT NULL,
    name VARCHAR(512) NOT NULL,
    is_custom_emoji BOOL NOT NULL,
    PRIMARY KEY (id)
);
CREATE TABLE IF NOT EXISTS public_channel (
    id BIGINT NOT NULL,
    guild_id BIGINT NULL,
    name VARCHAR(512),
    PRIMARY KEY (id)
);
CREATE TABLE IF NOT EXISTS message (
    id BIGINT,
    public_channel_id BIGINT NOT NULL,
    user_id BIGINT NOT NULL,
    emoji_count INTEGER NOT NULL,
    PRIMARY KEY (id),
    FOREIGN KEY (public_channel_id) REFERENCES public_channel (id)
);
CREATE TABLE IF NOT EXISTS emoji_usage (
    public_channel_id BIGINT NOT NULL,
    user_id BIGINT NOT NULL,
    emoji_id BIGINT NOT NULL,
    use_count INTEGER NOT NULL,
    PRIMARY KEY (public_channel_id, emoji_id, user_id),
    FOREIGN KEY (public_channel_id) REFERENCES public_channel (id),
    FOREIGN KEY (emoji_id) REFERENCES emoji (id)
);";
const PG_INSERT_CUSTOM_EMOJI: &str = "
INSERT INTO emoji (id, name, is_custom_emoji)
VALUES ($1, $2, TRUE)
ON CONFLICT (id) DO UPDATE
    SET name = excluded.name;";
const PG_SELECT_UNICODE_EMOJI: &str = "
SELECT
    id
FROM
    emoji e
WHERE
    e.name = $1;";
const PG_INSERT_UNICODE_EMOJI: &str = "
INSERT INTO emoji (name, is_custom_emoji)
VALUES ($1, FALSE);";
const PG_INSERT_PUBLIC_CHANNEL: &str = "
INSERT INTO public_channel (id, guild_id, name)
VALUES ($1, $2, $3)
ON CONFLICT (id) DO UPDATE
    SET guild_id = excluded.guild_id,
        name = excluded.name";
const PG_INSERT_EMOJI_USAGE: &str = "
INSERT INTO emoji_usage (public_channel_id, user_id, emoji_id, use_count)
VALUES ($1, $2, $3, $4)
ON CONFLICT (public_channel_id, user_id, emoji_id) DO UPDATE
    SET use_count = emoji_usage.use_count + excluded.use_count;";
const PG_INSERT_MESSAGE: &str = "
INSERT INTO message (id, public_channel_id, user_id, emoji_count)
VALUES ($1, $2, $3, $4)
ON CONFLICT (id) DO UPDATE
    SET public_channel_id = excluded.public_channel_id,
        user_id = excluded.user_id,
        emoji_count = excluded.emoji_count;";
const PG_SELECT_STATS_GLOBAL: &str = "
SELECT
    e.name,
    SUM(eu.use_count)
FROM
    emoji_usage eu
    INNER JOIN emoji e ON eu.emoji_id = e.id
WHERE
    e.is_custom_emoji = FALSE
GROUP BY
    e.name
ORDER BY
    SUM(eu.use_count) DESC
LIMIT
    5
OFFSET
    0;";
const PG_SELECT_STATS_SERVER_TOP_EMOJI: &str = "
SELECT
    e.id,
    e.name,
    SUM(eu.use_count),
    e.is_custom_emoji
FROM
    emoji_usage eu
    INNER JOIN emoji e ON eu.emoji_id = e.id
    INNER JOIN public_channel pc ON eu.public_channel_id = pc.id
WHERE
    pc.guild_id = $1
GROUP BY
    e.id
ORDER BY
    SUM(eu.use_count) DESC
LIMIT
    5
OFFSET
    0;";
const PG_SELECT_STATS_SERVER_TOP_USERS: &str = "
SELECT
    eu.user_id,
    SUM(eu.use_count),
    (SELECT
        COUNT(*)
    FROM
        message m
        INNER JOIN public_channel pc2 ON m.public_channel_id = pc2.id
    WHERE
        m.user_id = eu.user_id AND
        pc2.guild_id = $1)
FROM
    emoji_usage eu
    INNER JOIN public_channel pc ON eu.public_channel_id = pc.id
WHERE
    pc.guild_id = $1
GROUP BY
    eu.user_id
ORDER BY
    SUM(eu.use_count) DESC
LIMIT
    5
OFFSET
    0;";
const PG_SELECT_STATS_CHANNEL_TOP_EMOJI: &str = "
SELECT
    e.id,
    e.name,
    SUM(eu.use_count),
    e.is_custom_emoji
FROM
    emoji_usage eu
    INNER JOIN emoji e ON eu.emoji_id = e.id
WHERE
    eu.public_channel_id = $1
GROUP BY
    e.id
ORDER BY
    SUM(eu.use_count) DESC
LIMIT
    5
OFFSET
    0;";
const PG_SELECT_STATS_CHANNEL_TOP_USERS: &str = "
SELECT
    eu.user_id,
    SUM(eu.use_count),
    (SELECT
        COUNT(*)
    FROM
        message m
    WHERE
        m.user_id = eu.user_id AND
        m.public_channel_id = $1)
FROM
    emoji_usage eu
WHERE
    eu.public_channel_id = $1
GROUP BY
    eu.user_id
ORDER BY
    SUM(eu.use_count) DESC
LIMIT
    5
OFFSET
    0;";
const PG_SELECT_STATS_USER_FAVOURITE_EMOJI_FOR_SERVER: &str = "
SELECT
    e.id,
    e.name,
    SUM(eu.use_count),
    e.is_custom_emoji
FROM
    emoji_usage eu
    INNER JOIN emoji e ON eu.emoji_id = e.id
    INNER JOIN public_channel pc ON eu.public_channel_id = pc.id
WHERE
    eu.user_id = $1 AND
    pc.guild_id = $2
GROUP BY
    e.id,
    e.name
ORDER BY
    SUM(eu.use_count) DESC
LIMIT
    5
OFFSET
    0";
const PG_SELECT_STATS_USER_FAVOURITE_UNICODE_EMOJI: &str = "
SELECT
    e.id,
    e.name,
    SUM(eu.use_count)
FROM
    emoji_usage eu
    INNER JOIN emoji e ON eu.emoji_id = e.id
WHERE
    eu.user_id = $1 AND
    e.is_custom_emoji = FALSE
GROUP BY
    e.id,
    e.name
ORDER BY
    SUM(eu.use_count) DESC
LIMIT
    5
OFFSET
    0";
const PG_SELECT_STATS_EMOJI_TIMES_USED_GLOBALLY: &str = "
SELECT
    SUM(eu.use_count)
FROM
    emoji_usage eu
    INNER JOIN emoji e ON eu.emoji_id = e.id
WHERE
    eu.emoji_id = $1
GROUP BY
    eu.emoji_id";
const PG_SELECT_STATS_EMOJI_TIMES_USED_ON_SERVER: &str = "
SELECT
    SUM(eu.use_count)
FROM
    emoji_usage eu
    INNER JOIN emoji e ON eu.emoji_id = e.id
    INNER JOIN public_channel pc ON eu.public_channel_id = pc.id
WHERE
    eu.emoji_id = $1 AND
    pc.guild_id = $2
GROUP BY
    eu.emoji_id";
const PG_SELECT_STATS_EMOJI_TOP_USERS_ON_SERVER: &str = "
SELECT
    eu.user_id,
    SUM(eu.use_count)
FROM
    emoji_usage eu
    INNER JOIN emoji e ON eu.emoji_id = e.id
    INNER JOIN public_channel pc ON eu.public_channel_id = pc.id
WHERE
    eu.emoji_id = $1 AND
    pc.guild_id = $2
GROUP BY
    eu.user_id,
    eu.emoji_id
ORDER BY
    SUM(eu.use_count) DESC
LIMIT
    5
OFFSET
    0";

const MESSAGE_ERROR_OBTAINING_STATS: &str = "\
An error occurred while obtaining the statistics. \u{1F625}";
const MESSAGE_HELP: &str = "\
**\u{26A1} Commands**
**about:** See information about the bot
**global:** See the top used Unicode emoji globally
**server:** See the top used emoji on this server
**channel:** See the top used emoji on this channel
**<#channel>:** See the top used emoji on the specified channel
**me:** See your favourite emoji
**<@user>:** See the specified user's favourite emoji
**<emoji>:** See usage information for that emoji";
const MESSAGE_HELP_HINT: &str = "\
To see a list of commands, use `help`.";

const MESSAGE_COMMAND_REQUIRES_AUTH: &str = "\
\u{26D4} Please authenticate first.";
const MESSAGE_COMMAND_REQUIRES_PUBLIC_CHANNEL: &str = "\
\u{1F6AB} This command may only be used in public chat channels.";

const MESSAGE_ABOUT: &str = "\
I provide statistics on emoji usage! \u{1F4C8}
Made with \u{1F495} using Rust and discord-rs.
\u{1F310} https://github.com/quailiff/emojistats-bot";

const EMOJI_CHART: &str = "\u{1F4C8}";
const EMOJI_CROWN: &str = "\u{1F451}";
const EMOJI_DISAPPOINTED: &str = "\u{1F61E}";
const EMOJI_HEART: &str = "\u{2764}";
const EMOJI_ROBOT: &str = "\u{1F916}";
const EMOJI_QUITTING: &str = "\u{1F6D1}";
const EMOJI_RESTARTING: &str = "\u{1F504}";

const EXIT_STATUS_DB_COULDNT_CONNECT: i32 = 100;
const EXIT_STATUS_DB_COULDNT_CREATE_TABLES: i32 = 101;
const EXIT_STATUS_DISCORD_COULDNT_AUTHENTICATE: i32 = 110;
const EXIT_STATUS_DISCORD_COULDNT_CONNECT: i32 = 111;
pub const EXIT_STATUS_RESTART: i32 = 190;

pub struct EsBot {
    db_conn_str: String,
    bot_token: String,
    bot_control_password: String,

    servers: HashMap<ServerId, HashMap<EmojiId, String>>,
    private_channels: Vec<ChannelId>,
    public_channels: HashMap<ChannelId, ServerId>,
    unicode_emojis: HashMap<String, EmojiId>,
    control_users: Vec<UserId>,
    command_prefix: String,

    discord: Option<discord::Discord>,
    db_conn: Option<postgres::Connection>,
    bot_user_id: UserId,
    quit: bool,
    restart: bool,
}

impl EsBot {
    pub fn new<S>(db_conn_str: S, bot_token: S, bot_control_password: S) -> EsBot
        where S: Into<String>
    {
        EsBot {
            db_conn_str: db_conn_str.into(),
            bot_token: bot_token.into(),
            bot_control_password: bot_control_password.into(),

            servers: HashMap::new(),
            private_channels: Vec::new(),
            public_channels: HashMap::new(),
            unicode_emojis: HashMap::new(),
            control_users: Vec::new(),
            command_prefix: "".to_string(),

            discord: None,
            db_conn: None,
            bot_user_id: UserId(0),
            quit: false,
            restart: false,
        }
    }

    pub fn run(&mut self, unicode_emojis: &Vec<(String, String)>) -> i32 {
        // Connect to database
        let db_conn = match postgres::Connection::connect(self.db_conn_str.as_str(),
                                                          postgres::TlsMode::None) {
            Ok(db_conn) => db_conn,
            Err(reason) => {
                error!("Failed to connect to PostgreSQL: {}", reason);
                return EXIT_STATUS_DB_COULDNT_CONNECT;
            }
        };

        if let Err(reason) = db_conn.batch_execute(PG_CREATE_TABLES) {
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
        // TODO: This will not work if the bot has a nickname; the message will
        // begin with <!@ID> instead of <!ID>
        self.command_prefix = format!("<@{}>", ready_event.user.id.0);
        self.bot_user_id = ready_event.user.id;

        // Main loop
        self.discord = Some(discord);
        self.db_conn = Some(db_conn);
        self.add_unicode_emojis(unicode_emojis);

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
                Ok(Event::ServerEmojisUpdate(server_id, custom_emojis)) => {
                    self.add_custom_emojis(&server_id, &custom_emojis);
                }
                Ok(Event::MessageCreate(message)) => {
                    // Process messages sent by people; ignore messages sent by bots
                    if &message.kind == &MessageType::Regular && !message.author.bot {
                        self.process_message(&message);
                    }
                }
                _ => {}
            }

            if self.quit || self.restart {
                break;
            }
        }

        let _ = self.db_conn.take().unwrap().finish();
        let _ = discord_conn.shutdown();

        if self.restart {
            return EXIT_STATUS_RESTART;
        }
        0
    }

    fn process_message(&mut self, message: &Message) {
        // If the bot doesn't know about this channel for some reason, add it
        self.add_channel_id(&message.channel_id);

        // Check whether the message is a command
        if message.content.starts_with(&self.command_prefix) {
            // Remove the command prefix
            let command_str = &message.content[self.command_prefix.len()..];
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

        self.process_message_emojis(message);
    }

    fn process_command(&mut self, message: &Message, command_str: &str) {
        debug!("Command from \"{}\": \"{}\"",
               message.author.name,
               command_str);

        let mut command_parts = command_str.split_whitespace();
        let command = match command_parts.next() {
            Some(command) => command,
            None => {
                self.send_message(&message.channel_id,
                                  format!("{} {}", EMOJI_ROBOT, MESSAGE_HELP_HINT));
                return;
            }
        };

        match arg::get_type(command) {
            arg::Type::ChannelId(channel_id) => {
                self.command_channel(&message.channel_id, &channel_id);
            }
            arg::Type::UserId(user_id) => {
                self.command_user(&message.channel_id, &user_id);
            }
            arg::Type::CustomEmoji(custom_emoji_id) => {
                self.command_custom_emoji(&message.channel_id, &custom_emoji_id);
            }
            arg::Type::Text(command) => {
                match command {
                    "help" => {
                        self.send_message(&message.channel_id, MESSAGE_HELP);
                    }
                    "about" => {
                        self.send_message(&message.channel_id, MESSAGE_ABOUT);
                    }
                    "auth" => {
                        self.command_auth(&message.channel_id,
                                          &message.author.id,
                                          &message.author.name,
                                          command_parts.next().unwrap_or(""));
                    }
                    "global" => {
                        self.command_global(&message.channel_id);
                    }
                    "server" => {
                        self.command_server(&message.channel_id);
                    }
                    "channel" => {
                        self.command_channel(&message.channel_id, &message.channel_id);
                    }
                    "me" => {
                        self.command_user(&message.channel_id, &message.author.id);
                    }
                    "meta" => {
                        self.command_meta(&message.channel_id);
                    }
                    "quit" => {
                        self.command_quit(&message.channel_id,
                                          &message.author.id,
                                          &message.author.name);
                    }
                    "restart" => {
                        self.command_restart(&message.channel_id,
                                             &message.author.id,
                                             &message.author.name);
                    }
                    other => {
                        if self.unicode_emojis.contains_key(other) {
                            self.command_unicode_emoji(&message.channel_id, other);
                            return;
                        }

                        self.send_message(&message.channel_id,
                                          format!("{} Unknown command `{}`. {}",
                                                  EMOJI_ROBOT,
                                                  other,
                                                  MESSAGE_HELP_HINT));
                    }
                }
            }
        }
    }

    fn process_message_emojis(&self, message: &Message) {
        let db_conn = self.db_conn.as_ref().unwrap();
        let insert_emoji_usage = match db_conn.prepare_cached(PG_INSERT_EMOJI_USAGE) {
            Ok(insert_emoji_usage) => insert_emoji_usage,
            Err(reason) => {
                error!("Error creating prepared statement: {}", reason);
                return;
            }
        };
        let insert_message = match db_conn.prepare_cached(PG_INSERT_MESSAGE) {
            Ok(insert_message) => insert_message,
            Err(reason) => {
                error!("Error creating prepared statement: {}", reason);
                return;
            }
        };

        let mut total_emoji_count = 0;

        let message_id = message.id.0 as i64;
        let channel_id = message.channel_id.0 as i64;
        let user_id = message.author.id.0 as i64;

        if let Some(server_id) = self.public_channels.get(&message.channel_id) {
            if let Some(custom_emojis) = self.servers.get(server_id) {
                for (custom_emoji_id, custom_emoji_name) in custom_emojis {
                    let ct = message.content.matches(custom_emoji_name).count() as i32;

                    if ct > 0 {
                        debug!("{} instances of \"{}\"", ct, custom_emoji_name);

                        let emoji_id = custom_emoji_id.0 as i64;

                        if let Err(reason) =
                            insert_emoji_usage.execute(&[&channel_id, &user_id, &emoji_id, &ct]) {
                            error!("Error inserting emoji usage: {}", reason);
                        }
                    }

                    total_emoji_count += ct;
                }
            }
        }

        for (unicode_emoji, unicode_emoji_id) in &self.unicode_emojis {
            let count = message.content.matches(unicode_emoji).count() as i32;

            if count > 0 {
                debug!("{} instances of \"{}\"", count, unicode_emoji);

                let emoji_id = unicode_emoji_id.0 as i64;

                if let Err(reason) = insert_emoji_usage
                       .execute(&[&channel_id, &user_id, &emoji_id, &count]) {
                    error!("Error inserting emoji usage: {}", reason);
                }
            }

            total_emoji_count += count;
        }

        debug!("Message {} had {} emojis", message_id, total_emoji_count);

        if let Err(reason) =
            insert_message.execute(&[&message_id, &channel_id, &user_id, &total_emoji_count]) {
            error!("Error inserting message: {}", reason);
        }
    }

    fn command_auth(&mut self,
                    response_channel_id: &ChannelId,
                    user_id: &UserId,
                    user_name: &str,
                    try_password: &str) {
        if !self.private_channels.contains(response_channel_id) {
            self.send_message(response_channel_id,
                              format!("{} Please send me a direct message to authenticate.",
                                      EMOJI_ROBOT));
            return;
        }

        if self.control_users.contains(user_id) {
            self.send_message(response_channel_id,
                              format!("{} You have already authenticated.", EMOJI_ROBOT));
            return;
        }

        if try_password == self.bot_control_password {
            debug!("Authentication successful for <@{}> = \"{}\"",
                   user_id,
                   user_name);

            self.control_users.push(*user_id);

            self.send_message(response_channel_id, "\u{2705} Authentication successful.");
        } else if try_password.len() == 0 {
            self.send_message(response_channel_id, "Please enter a password.");
        } else {
            info!("Failed authentication attempt by <@{}> = \"{}\" with password \"{}\"",
                  user_id,
                  user_name,
                  try_password);

            self.send_message(response_channel_id, "\u{26D4} Authentication unsuccessful.");
        }
    }

    fn command_global(&self, response_channel_id: &ChannelId) {
        match self.command_stats_global() {
            Ok(stats) => {
                self.send_message(response_channel_id, stats);
            }
            Err(_) => {
                self.send_message(response_channel_id, MESSAGE_ERROR_OBTAINING_STATS);
            }
        }
    }

    fn command_server(&self, response_channel_id: &ChannelId) {
        if let Some(server_id) = self.public_channels.get(response_channel_id) {
            match self.command_stats_server(server_id) {
                Ok(stats) => {
                    self.send_message(response_channel_id, stats);
                }
                Err(_) => {
                    self.send_message(response_channel_id, MESSAGE_ERROR_OBTAINING_STATS);
                }
            }
        } else {
            self.send_message(response_channel_id, MESSAGE_COMMAND_REQUIRES_PUBLIC_CHANNEL);
            return;
        }
    }

    fn command_channel(&self, response_channel_id: &ChannelId, channel_id: &ChannelId) {
        if !self.public_channels.contains_key(channel_id) {
            self.send_message(response_channel_id, MESSAGE_COMMAND_REQUIRES_PUBLIC_CHANNEL);
            return;
        }

        match self.command_stats_channel(channel_id) {
            Ok(stats) => {
                self.send_message(response_channel_id, stats);
            }
            Err(_) => {
                self.send_message(response_channel_id, MESSAGE_ERROR_OBTAINING_STATS);
            }
        }
    }

    fn command_user(&self, response_channel_id: &ChannelId, user_id: &UserId) {
        let (is_public_channel, stats) =
            if let Some(server_id) = self.public_channels.get(response_channel_id) {
                (true, self.command_stats_user_favourite_emoji_for_server(user_id, &server_id))
            } else if self.private_channels.contains(response_channel_id) {
                (false, self.command_stats_user_favourite_unicode_emoji(user_id))
            } else {
                (false, Err(()))
            };

        match stats {
            Ok(stats) => {
                self.send_message(response_channel_id,
                                  format!("**<@{}>'s favourite {} {}**\n{}",
                                          user_id.0,
                                          if is_public_channel {
                                              "emoji on this server"
                                          } else {
                                              "Unicode emoji"
                                          },
                                          EMOJI_HEART,
                                          stats));
            }
            Err(_) => {
                self.send_message(response_channel_id, MESSAGE_ERROR_OBTAINING_STATS);
            }
        }
    }

    fn command_custom_emoji(&self, response_channel_id: &ChannelId, emoji_id: &EmojiId) {
        if let Some(server_id) = self.public_channels.get(response_channel_id) {
            if let Some(server_emojis) = self.servers.get(server_id) {
                if let Some(custom_emoji) = server_emojis.get(emoji_id) {
                    if let Ok(times_used) = self.command_stats_emoji_times_used_globally(emoji_id) {
                        if times_used == 0 {
                            self.send_message(response_channel_id,
                                              format!("{} has never been used. {}",
                                                      custom_emoji,
                                                      EMOJI_DISAPPOINTED));
                            return;
                        } else if let Ok(top_user_stats) =
                            self.command_stats_emoji_top_users_on_server(emoji_id, server_id) {
                            self.send_message(response_channel_id,
                                              format!("**{} Stats for {}**\n\
                                                      Used {} time{}\n\
                                                      {}",
                                                      EMOJI_CHART,
                                                      custom_emoji,
                                                      times_used,
                                                      if times_used == 1 { "" } else { "s" },
                                                      top_user_stats));
                            return;
                        }
                    }
                }
            }
        }

        self.send_message(response_channel_id, MESSAGE_ERROR_OBTAINING_STATS);
    }

    fn command_unicode_emoji(&self, response_channel_id: &ChannelId, emoji: &str) {
        let emoji_id = match self.unicode_emojis.get(emoji) {
            Some(emoji_id) => emoji_id,
            None => {
                self.send_message(response_channel_id, MESSAGE_ERROR_OBTAINING_STATS);
                return;
            }
        };

        let times_used_globally = match self.command_stats_emoji_times_used_globally(emoji_id) {
            Ok(times_used_globally) => times_used_globally,
            Err(_) => {
                self.send_message(response_channel_id, MESSAGE_ERROR_OBTAINING_STATS);
                return;
            }
        };

        let mut stats;

        if times_used_globally == 0 {
            stats = format!("{} has never been used. {}", emoji, EMOJI_DISAPPOINTED);
        } else {
            stats = format!("**{} Stats for {}**\n\
                            Used {} time{} globally",
                            EMOJI_CHART,
                            emoji,
                            times_used_globally,
                            if times_used_globally == 1 { "" } else { "s" });

            // If the command was invoked on a server, get usage statistics for
            // that server
            if let Some(server_id) = self.public_channels.get(response_channel_id) {
                match self.command_stats_emoji_times_used_on_server(emoji_id, server_id) {
                    Ok(times_used_on_server) => {
                        if times_used_on_server == 0 {
                            stats += &format!("\nNever used on this server. {}",
                                    EMOJI_DISAPPOINTED);
                        } else {
                            match self.command_stats_emoji_top_users_on_server(emoji_id,
                                                                               server_id) {
                                Ok(server_stats) => {
                                    stats += &format!("\n{}", server_stats);
                                }
                                Err(_) => {
                                    self.send_message(response_channel_id,
                                                      MESSAGE_ERROR_OBTAINING_STATS);
                                    return;
                                }
                            }
                        }
                    }
                    Err(_) => {
                        self.send_message(response_channel_id, MESSAGE_ERROR_OBTAINING_STATS);
                        return;
                    }
                }
            }
        }

        self.send_message(response_channel_id, stats);
    }

    fn command_meta(&mut self, response_channel_id: &ChannelId) {
        let num_channels = self.public_channels.len();
        let num_servers = self.servers.len();

        self.send_message(response_channel_id,
                          format!("I'm tracking emoji in {} channel{} on {} server{}. {}",
                                  num_channels,
                                  if num_channels == 1 { "" } else { "s" },
                                  num_servers,
                                  if num_servers == 1 { "" } else { "s" },
                                  EMOJI_CHART));
    }

    fn command_quit(&mut self,
                    response_channel_id: &ChannelId,
                    user_id: &UserId,
                    user_name: &str) {
        if !self.control_users.contains(user_id) {
            self.send_message(response_channel_id, MESSAGE_COMMAND_REQUIRES_AUTH);
            return;
        }

        self.send_message(response_channel_id, format!("{} Quitting.", EMOJI_QUITTING));

        info!("Quitting per <@{}> = \"{}\"", user_id.0, user_name);
        self.quit = true;
    }

    fn command_restart(&mut self,
                       response_channel_id: &ChannelId,
                       user_id: &UserId,
                       user_name: &str) {
        if !self.control_users.contains(user_id) {
            self.send_message(response_channel_id, MESSAGE_COMMAND_REQUIRES_AUTH);
            return;
        }

        self.send_message(response_channel_id,
                          format!("{} Restarting.", EMOJI_RESTARTING));

        info!("Restarting per <@{}> = \"{}\"", user_id.0, user_name);
        self.restart = true;
    }

    fn command_stats_global(&self) -> Result<String, ()> {
        let db_conn = self.db_conn.as_ref().unwrap();
        let stats_global = match db_conn.prepare_cached(PG_SELECT_STATS_GLOBAL) {
            Ok(stats_global) => stats_global,
            Err(reason) => {
                error!("Error creating prepared statement: {}", reason);
                return Err(());
            }
        };

        let mut stats = format!("**Top Unicode emoji used globally {}**", EMOJI_CHART);

        match stats_global.query(&[]) {
            Ok(rows) => {
                if rows.len() == 0 {
                    stats = format!("No Unicode emoji have been used. {}", EMOJI_DISAPPOINTED);
                } else {
                    for row in &rows {
                        let emoji_name = row.get::<usize, String>(0);
                        let use_count = row.get::<usize, i64>(1);

                        stats += &format!("\n{} was used {} time{}",
                                emoji_name,
                                use_count,
                                if use_count == 1 { "" } else { "s" });
                    }
                }
            }
            Err(reason) => {
                error!("Error selecting global stats: {}", reason);
                return Err(());
            }
        }

        Ok(stats)
    }

    fn command_stats_server(&self, server_id: &ServerId) -> Result<String, ()> {
        let db_conn = self.db_conn.as_ref().unwrap();
        let select_stats_server_top_emoji =
            match db_conn.prepare_cached(PG_SELECT_STATS_SERVER_TOP_EMOJI) {
                Ok(select_stats_server_top_emoji) => select_stats_server_top_emoji,
                Err(reason) => {
                    error!("Error creating prepared statement: {}", reason);
                    return Err(());
                }
            };
        let select_stats_server_top_users =
            match db_conn.prepare_cached(PG_SELECT_STATS_SERVER_TOP_USERS) {
                Ok(select_stats_server_top_users) => select_stats_server_top_users,
                Err(reason) => {
                    error!("Error creating prepared statement: {}", reason);
                    return Err(());
                }
            };

        let mut stats = format!("**Top emoji used on this server {}**", EMOJI_CHART);

        let server_id = &(server_id.0 as i64);

        match select_stats_server_top_emoji.query(&[server_id]) {
            Ok(rows) => {
                if rows.len() == 0 {
                    stats = format!("No emoji have been used on this server. {}",
                                    EMOJI_DISAPPOINTED);
                } else {
                    for row in &rows {
                        let emoji_id = row.get::<usize, i64>(0) as u64;
                        let emoji_name = row.get::<usize, String>(1);
                        let use_count = row.get::<usize, i64>(2);
                        let is_custom_emoji = row.get::<usize, bool>(3);

                        if is_custom_emoji {
                            stats += &format!("\n<:{}:{}> was used {} time{}",
                                    emoji_name,
                                    emoji_id,
                                    use_count,
                                    if use_count == 1 { "" } else { "s" });
                        } else {
                            stats += &format!("\n{} was used {} time{}",
                                    emoji_name,
                                    use_count,
                                    if use_count == 1 { "" } else { "s" });
                        }
                    }
                }
            }
            Err(reason) => {
                error!("Error selecting server stats: {}", reason);
                return Err(());
            }
        }

        match select_stats_server_top_users.query(&[server_id]) {
            Ok(rows) => {
                if rows.len() > 0 {
                    let mut first_row = true;

                    for row in &rows {
                        let user_id = row.get::<usize, i64>(0) as u64;
                        let use_count = row.get::<usize, i64>(1);
                        let message_count = row.get::<usize, i64>(2);

                        stats += &format!("\n<@{}> has used {} emoji in {} message{} ",
                                user_id,
                                use_count,
                                message_count,
                                if message_count == 1 { "" } else { "s" });

                        if first_row {
                            stats += EMOJI_CROWN;
                            first_row = false;
                        }
                    }
                }
            }
            Err(reason) => {
                error!("Error selecting server stats: {}", reason);
                return Err(());
            }
        }

        Ok(stats)
    }

    fn command_stats_channel(&self, channel_id: &ChannelId) -> Result<String, ()> {
        let db_conn = self.db_conn.as_ref().unwrap();
        let select_stats_channel_top_emoji =
            match db_conn.prepare_cached(PG_SELECT_STATS_CHANNEL_TOP_EMOJI) {
                Ok(select_stats_channel_top_emoji) => select_stats_channel_top_emoji,
                Err(reason) => {
                    error!("Error creating prepared statement: {}", reason);
                    return Err(());
                }
            };
        let select_stats_channel_top_users =
            match db_conn.prepare_cached(PG_SELECT_STATS_CHANNEL_TOP_USERS) {
                Ok(select_stats_channel_top_users) => select_stats_channel_top_users,
                Err(reason) => {
                    error!("Error creating prepared statement: {}", reason);
                    return Err(());
                }
            };

        let mut stats = format!("**Top used emoji and emoji users in <#{}> {}**",
                                channel_id.0,
                                EMOJI_CHART);

        let channel_id = &(channel_id.0 as i64);

        match select_stats_channel_top_emoji.query(&[channel_id]) {
            Ok(rows) => {
                if rows.len() == 0 {
                    stats = format!("No emoji have been used in <#{}>. {}",
                                    channel_id,
                                    EMOJI_DISAPPOINTED);
                } else {
                    for row in &rows {
                        let emoji_id = row.get::<usize, i64>(0) as u64;
                        let emoji_name = row.get::<usize, String>(1);
                        let use_count = row.get::<usize, i64>(2);
                        let is_custom_emoji = row.get::<usize, bool>(3);

                        if is_custom_emoji {
                            stats += &format!("\n<:{}:{}> was used {} time{}",
                                    emoji_name,
                                    emoji_id,
                                    use_count,
                                    if use_count == 1 { "" } else { "s" });
                        } else {
                            stats += &format!("\n{} was used {} time{}",
                                    emoji_name,
                                    use_count,
                                    if use_count == 1 { "" } else { "s" });
                        }
                    }
                }
            }
            Err(reason) => {
                error!("Error selecting channel stats: {}", reason);
                return Err(());
            }
        }

        match select_stats_channel_top_users.query(&[channel_id]) {
            Ok(rows) => {
                if rows.len() > 0 {
                    let mut first_record = true;

                    for row in &rows {
                        let user_id = row.get::<usize, i64>(0) as u64;
                        let use_count = row.get::<usize, i64>(1);
                        let message_count = row.get::<usize, i64>(2);

                        stats += &format!("\n<@{}> has used {} emoji in {} message{} ",
                                user_id,
                                use_count,
                                message_count,
                                if message_count == 1 { "" } else { "s" });

                        if first_record {
                            stats += EMOJI_CROWN;
                            first_record = false;
                        }
                    }
                }
            }
            Err(reason) => {
                error!("Error selecting channel stats: {}", reason);
                return Err(());
            }
        }

        Ok(stats)
    }

    fn command_stats_user_favourite_emoji_for_server(&self,
                                                     user_id: &UserId,
                                                     server_id: &ServerId)
                                                     -> Result<String, ()> {
        let db_conn = self.db_conn.as_ref().unwrap();
        let stats_user_favourite_emoji_for_server =
            match db_conn.prepare_cached(PG_SELECT_STATS_USER_FAVOURITE_EMOJI_FOR_SERVER) {
                Ok(stats_user_favourite_emoji_for_server) => stats_user_favourite_emoji_for_server,
                Err(reason) => {
                    error!("Error creating prepared statement: {}", reason);
                    return Err(());
                }
            };

        let mut stats = "".to_string();

        let user_id = &(user_id.0 as i64);
        let server_id = &(server_id.0 as i64);

        match stats_user_favourite_emoji_for_server.query(&[user_id, server_id]) {
            Ok(rows) => {
                if rows.len() == 0 {
                    stats = format!("This user has not used any emoji. {}", EMOJI_DISAPPOINTED);
                } else {
                    for row in &rows {
                        let emoji_id = row.get::<usize, i64>(0) as u64;
                        let emoji_name = row.get::<usize, String>(1);
                        let use_count = row.get::<usize, i64>(2);
                        let is_custom_emoji = row.get::<usize, bool>(3);

                        if is_custom_emoji {
                            stats += &format!("<:{}:{}> was used {} time{}\n",
                                    emoji_name,
                                    emoji_id,
                                    use_count,
                                    if use_count == 1 { "" } else { "s" });
                        } else {
                            stats += &format!("{} was used {} time{}\n",
                                    emoji_name,
                                    use_count,
                                    if use_count == 1 { "" } else { "s" });
                        }
                    }
                }
            }
            Err(reason) => {
                error!("Error selecting user stats: {}", reason);
                return Err(());
            }
        }

        Ok(stats)
    }

    fn command_stats_user_favourite_unicode_emoji(&self, user_id: &UserId) -> Result<String, ()> {
        let db_conn = self.db_conn.as_ref().unwrap();
        let select_stats_user_favourite_unicode_emoji =
            match db_conn.prepare_cached(PG_SELECT_STATS_USER_FAVOURITE_UNICODE_EMOJI) {
                Ok(select_stats_user_favourite_unicode_emoji) => {
                    select_stats_user_favourite_unicode_emoji
                }
                Err(reason) => {
                    error!("Error creating prepared statement: {}", reason);
                    return Err(());
                }
            };

        let mut stats = "".to_string();

        let user_id = &(user_id.0 as i64);

        match select_stats_user_favourite_unicode_emoji.query(&[user_id]) {
            Ok(rows) => {
                if rows.len() == 0 {
                    stats = format!("This user has not used any Unicode emoji. {}",
                                    EMOJI_DISAPPOINTED);
                } else {
                    for row in &rows {
                        let emoji_name = row.get::<usize, String>(1);
                        let use_count = row.get::<usize, i64>(2);

                        stats += &format!("{} was used {} time{}\n",
                                emoji_name,
                                use_count,
                                if use_count == 1 { "" } else { "s" });
                    }
                }
            }
            Err(reason) => {
                error!("Error selecting user stats: {}", reason);
                return Err(());
            }
        }

        Ok(stats)
    }

    fn command_stats_emoji_times_used_globally(&self, emoji_id: &EmojiId) -> Result<i64, ()> {
        let db_conn = self.db_conn.as_ref().unwrap();
        let select_stats_emoji_times_used_globally =
            match db_conn.prepare_cached(PG_SELECT_STATS_EMOJI_TIMES_USED_GLOBALLY) {
                Ok(select_stats_emoji_times_used_globally) => {
                    select_stats_emoji_times_used_globally
                }
                Err(reason) => {
                    error!("Error creating prepared statement: {}", reason);
                    return Err(());
                }
            };

        let emoji_id = emoji_id.0 as i64;
        let times_used;

        match select_stats_emoji_times_used_globally.query(&[&emoji_id]) {
            Ok(rows) => {
                if rows.len() == 0 {
                    times_used = 0;
                } else {
                    let row = rows.get(0);
                    times_used = row.get::<usize, i64>(0);
                }
            }
            Err(reason) => {
                error!("Error selecting global usage stats for emoji: {}", reason);
                return Err(());
            }
        }

        Ok(times_used)
    }

    fn command_stats_emoji_times_used_on_server(&self,
                                                emoji_id: &EmojiId,
                                                server_id: &ServerId)
                                                -> Result<i64, ()> {
        let db_conn = self.db_conn.as_ref().unwrap();
        let select_stats_emoji_times_used_on_server =
            match db_conn.prepare_cached(PG_SELECT_STATS_EMOJI_TIMES_USED_ON_SERVER) {
                Ok(select_stats_emoji_times_used_on_server) => {
                    select_stats_emoji_times_used_on_server
                }
                Err(reason) => {
                    error!("Error creating prepared statement: {}", reason);
                    return Err(());
                }
            };

        let emoji_id = emoji_id.0 as i64;
        let server_id = server_id.0 as i64;
        let times_used;

        match select_stats_emoji_times_used_on_server.query(&[&emoji_id, &server_id]) {
            Ok(rows) => {
                if rows.len() == 0 {
                    times_used = 0;
                } else {
                    let row = rows.get(0);
                    times_used = row.get::<usize, i64>(0);
                }
            }
            Err(reason) => {
                error!("Error selecting server usage stats for emoji: {}", reason);
                return Err(());
            }
        }

        Ok(times_used)
    }

    fn command_stats_emoji_top_users_on_server(&self,
                                               emoji_id: &EmojiId,
                                               server_id: &ServerId)
                                               -> Result<String, ()> {
        let db_conn = self.db_conn.as_ref().unwrap();
        let select_stats_emoji_top_users_on_server =
            match db_conn.prepare_cached(PG_SELECT_STATS_EMOJI_TOP_USERS_ON_SERVER) {
                Ok(select_stats_emoji_top_users_on_server) => {
                    select_stats_emoji_top_users_on_server
                }
                Err(reason) => {
                    error!("Error creating prepared statement: {}", reason);
                    return Err(());
                }
            };

        let mut stats = "".to_string();

        let emoji_id = emoji_id.0 as i64;
        let server_id = server_id.0 as i64;

        match select_stats_emoji_top_users_on_server.query(&[&emoji_id, &server_id]) {
            Ok(rows) => {
                let mut first_row = true;

                for row in &rows {
                    let user_id = row.get::<usize, i64>(0);
                    let use_count = row.get::<usize, i64>(1);

                    stats += &format!("Used by <@{}> {} time{} ",
                            user_id,
                            use_count,
                            if use_count == 1 { "" } else { "s" });

                    if first_row {
                        stats += EMOJI_CROWN;
                        first_row = false;
                    }

                    stats += "\n";
                }
            }
            Err(reason) => {
                error!("Error selecting server usage stats for emoji: {}", reason);
                return Err(());
            }
        }

        Ok(stats)
    }

    fn add_server(&mut self, server: &LiveServer) {
        debug!("Adding from server: \"{}\"", server.name);

        for channel in &server.channels {
            self.add_public_channel(channel);
        }

        self.add_custom_emojis(&server.id, &server.emojis);
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

    fn add_channel_id(&mut self, channel_id: &ChannelId) {
        // If this channel ID is unknown
        if !self.private_channels.contains(channel_id) &&
           self.public_channels.get(channel_id).is_none() {
            // Attempt to get the channel for this channel ID
            match self.discord.as_ref().unwrap().get_channel(*channel_id) {
                Ok(channel) => {
                    self.add_channel(&channel);
                }
                Err(reason) => {
                    warn!("Received message from unknown channel {}", channel_id);
                    warn!("Failed to look up channel: {}", reason);
                    return;
                }
            }
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
            debug!("Adding public channel: \"#{}\"", channel.name);

            let db_conn = self.db_conn.as_ref().unwrap();
            let insert_public_channel = match db_conn.prepare_cached(PG_INSERT_PUBLIC_CHANNEL) {
                Ok(insert_public_channel) => insert_public_channel,
                Err(reason) => {
                    error!("Error creating prepared statement: {}", reason);
                    return;
                }
            };

            let channel_id = channel.id.0 as i64;
            let guild_id = channel.server_id.0 as i64;

            if let Err(reason) = insert_public_channel
                   .execute(&[&channel_id, &guild_id, &channel.name]) {
                error!("Error inserting channel: {}", reason);
            }

            self.public_channels.insert(channel.id, channel.server_id);
        }
    }

    fn add_custom_emojis(&mut self, server_id: &ServerId, custom_emojis: &Vec<Emoji>) {
        let db_conn = self.db_conn.as_ref().unwrap();
        let insert_custom_emoji = match db_conn.prepare_cached(PG_INSERT_CUSTOM_EMOJI) {
            Ok(insert_custom_emoji) => insert_custom_emoji,
            Err(reason) => {
                error!("Error creating prepared statement: {}", reason);
                return;
            }
        };

        let mut server_emojis = HashMap::<EmojiId, String>::new();

        for custom_emoji in custom_emojis {
            let custom_emoji_name = format!("<:{}:{}>", custom_emoji.name, custom_emoji.id.0);
            debug!("Adding custom emoji: \"{}\"", custom_emoji_name);

            let emoji_id = custom_emoji.id.0 as i64;
            if let Err(reason) = insert_custom_emoji.execute(&[&emoji_id, &custom_emoji.name]) {
                error!("Error inserting custom emoji: {}", reason);
            }

            server_emojis.insert(custom_emoji.id, custom_emoji_name.clone());
        }

        self.servers.insert(*server_id, server_emojis);
    }

    fn add_unicode_emojis(&mut self, unicode_emojis: &Vec<(String, String)>) {
        let db_conn = self.db_conn.as_ref().unwrap();
        let select_unicode_emoji = match db_conn.prepare_cached(PG_SELECT_UNICODE_EMOJI) {
            Ok(select_unicode_emoji) => select_unicode_emoji,
            Err(reason) => {
                error!("Error creating prepared statement: {}", reason);
                return;
            }
        };
        let insert_unicode_emoji = match db_conn.prepare_cached(PG_INSERT_UNICODE_EMOJI) {
            Ok(insert_unicode_emoji) => insert_unicode_emoji,
            Err(reason) => {
                error!("Error creating prepared statement: {}", reason);
                return;
            }
        };

        for emoji_info in unicode_emojis {
            let emoji = &emoji_info.0;
            let emoji_desc = &emoji_info.1;

            // Check whether the emoji is already in the database
            match select_unicode_emoji.query(&[emoji]) {
                Ok(rows) => {
                    if rows.len() > 0 {
                        let emoji_id = rows.get(0).get::<usize, i64>(0) as u64;
                        self.unicode_emojis.insert(emoji.clone(), EmojiId(emoji_id));
                        continue;
                    }
                }
                Err(reason) => {
                    warn!("Error selecting ID for Unicode emoji \"{}\" (\"{}\"): {}",
                          emoji,
                          emoji_desc,
                          reason);
                    continue;
                }
            }

            // Insert the emoji into the database
            match insert_unicode_emoji.execute(&[emoji]) {
                Ok(rows_affected) => {
                    if rows_affected < 1 {
                        warn!("Unicode emoji \"{}\" (\"{}\") not inserted into database",
                              emoji,
                              emoji_desc);
                        continue;
                    }
                }
                Err(reason) => {
                    warn!("Error inserting Unicode emoji \"{}\" (\"{}\") into database: {}",
                          emoji,
                          emoji_desc,
                          reason);
                    continue;
                }
            }

            match select_unicode_emoji.query(&[emoji]) {
                Ok(rows) => {
                    for row in &rows {
                        let emoji_id = row.get::<usize, i64>(0) as u64;
                        self.unicode_emojis.insert(emoji.clone(), EmojiId(emoji_id));
                        continue;
                    }
                }
                Err(reason) => {
                    warn!("Error selecting ID for inserted Unicode emoji \"{}\" (\"{}\"): {}",
                          emoji,
                          emoji_desc,
                          reason);
                    continue;
                }
            }
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

    // TODO: Helper function that returns prepared statements?
}
