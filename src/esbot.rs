extern crate discord;
extern crate postgres;

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
#[allow(unused)]
const PG_INSERT_EMOJI: &str = "
INSERT INTO emoji (name, is_custom_emoji)
VALUES ($1, FALSE);";
const PG_INSERT_CUSTOM_EMOJI: &str = "
INSERT INTO emoji (id, name, is_custom_emoji)
VALUES ($1, $2, TRUE)
ON CONFLICT (id) DO UPDATE
    SET name = excluded.name;";
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
    SUM(eu.use_count)
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
    SUM(eu.use_count)
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
    SUM(eu.use_count)
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

const MESSAGE_AUTH_ALREADY_AUTHENTICATED: &str = "\
You have already authenticated.";
const MESSAGE_AUTH_FAILURE: &str = "\
Authentication unsuccessful.";
const MESSAGE_AUTH_MISSING_PASSWORD: &str = "\
Please enter a password.";
const MESSAGE_AUTH_REQUIRES_DIRECT_MESSAGE: &str = "\
Please send me a direct message to authenticate.";
const MESSAGE_AUTH_SUCCESS: &str = "\
Authentication successful.";

const MESSAGE_ERROR_OBTAINING_STATS: &str = "\
An error occurred while obtaining the statistics.";
const MESSAGE_HELP: &str = "\
**Commands:**
**about**: See information about the bot
**stats**: Alias for **stats channel** in public text channels and for \
    **stats user** in direct messages
**stats global**: See the top used Unicode emoji globally
**stats server**: See the top used emoji on this server
**stats channel [<#channel>]**: See the top used emoji on the specified channel \
    (defaults to the current channel)
**stats user [<@user>]**: See the specified user's favourite emoji (defaults to yourself)";
const MESSAGE_HELP_HINT: &str = "\
To see a list of commands, use `help`.";
const MESSAGE_HELP_STATS: &str = "\
Usage: `stats [global | server | channel [<#channel>] | user [<@user>]]`";

#[allow(unused)]
const MESSAGE_COMMAND_NOT_IMPLEMENTED: &str = "\
This command has not yet been implemented.";
const MESSAGE_COMMAND_REQUIRES_AUTH: &str = "\
Please authenticate first.";
const MESSAGE_COMMAND_REQUIRES_PUBLIC_CHANNEL: &str = "\
This command may only be used in public chat channels.";
const MESSAGE_COMMAND_UNKNOWN: &str = "\
Unknown command";

const MESSAGE_ABOUT: &str = "\
**EmojiStats**
A Discord bot that provides statistics on emoji usage. Built with discord-rs.
https://github.com/quailiff/emojistats-bot";

const MESSAGE_STATS_GLOBAL: &str = "\
Top used Unicode emoji globally:";
const MESSAGE_STATS_SERVER: &str = "\
Top used Unicode emoji on this server:";
const MESSAGE_STATS_CHANNEL_THIS_CHANNEL: &str = "\
Top used emoji and emoji users in this channel:";
const MESSAGE_STATS_USER_FOR_SERVER: &str = "\
's favourite emoji on this server:";
const MESSAGE_STATS_USER_UNICODE: &str = "\
's favourite Unicode emoji:";

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
    emojis: HashMap<char, u64>,
    custom_emojis: HashMap<EmojiId, String>,
    control_users: Vec<UserId>,
    command_prefix: String,

    discord: Option<discord::Discord>,
    db_conn: Option<postgres::Connection>,
    bot_user_id: UserId,
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
            emojis: HashMap::new(),
            custom_emojis: HashMap::new(),
            control_users: Vec::new(),
            command_prefix: "".to_string(),

            discord: None,
            db_conn: None,
            bot_user_id: UserId(0),
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
        self.command_prefix = format!("<@{}>", ready_event.user.id.0);
        self.bot_user_id = ready_event.user.id;

        // Main loop
        self.discord = Some(discord);
        self.db_conn = Some(db_conn);

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

        let _ = self.db_conn.take().unwrap().finish();
        let _ = discord_conn.shutdown();
        0
    }

    fn process_message(&mut self, message: &Message) {
        // If the bot doesn't know about this channel for some reason, add it
        if !self.private_channels.contains(&message.channel_id) &&
           self.public_channels.get(&message.channel_id).is_none() {
            match self.discord
                      .as_ref()
                      .unwrap()
                      .get_channel(message.channel_id) {
                Ok(channel) => {
                    self.add_channel(&channel);
                }
                Err(reason) => {
                    warn!("Received message from unknown channel {}",
                          &message.channel_id);
                    warn!("Failed to look up channel: {}", reason);
                    return;
                }
            }
        }

        if message.content.starts_with(&self.command_prefix) {
            let mut command_str = "";
            if message.content.len() > (self.command_prefix.len()) {
                let (_, command_str_) = message.content.split_at(self.command_prefix.len() + 1);
                command_str = command_str_;
            }
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

        let mut parts = command_str.split_whitespace();
        let is_control_user = self.control_users.contains(&message.author.id);
        let is_private_channel = self.private_channels.contains(&message.channel_id);
        let is_public_channel = self.public_channels.contains_key(&message.channel_id);

        match parts.next().unwrap_or("") {
            "" => {
                self.send_message(&message.channel_id, MESSAGE_HELP_HINT);
            }
            "help" => {
                self.send_message(&message.channel_id, MESSAGE_HELP);
            }
            "about" => {
                self.send_message(&message.channel_id, MESSAGE_ABOUT);
            }
            "auth" | "authenticate" => {
                if !is_private_channel {
                    self.send_message(&message.channel_id, MESSAGE_AUTH_REQUIRES_DIRECT_MESSAGE);
                    return;
                }

                if is_control_user {
                    self.send_message(&message.channel_id, MESSAGE_AUTH_ALREADY_AUTHENTICATED);
                    return;
                }

                match parts.next() {
                    Some(try_password) => {
                        if try_password == self.bot_control_password {
                            debug!("Authentication successful for {}:\"{}\"",
                                   message.author.id.0,
                                   message.author.name);

                            self.control_users.push(message.author.id);
                            self.send_message(&message.channel_id, MESSAGE_AUTH_SUCCESS);
                        } else {
                            info!("Failed authentication attempt by {}:\"{}\" with password \"{}\"",
                                  message.author.id.0,
                                  message.author.name,
                                  try_password);

                            self.send_message(&message.channel_id, MESSAGE_AUTH_FAILURE);
                        }
                    }
                    None => {
                        self.send_message(&message.channel_id, MESSAGE_AUTH_MISSING_PASSWORD);
                    }
                };
            }
            "stats" => {
                let default_stats = if is_public_channel { "channel" } else { "user" };

                match parts
                          .next()
                          .unwrap_or(default_stats)
                          .to_lowercase()
                          .as_str() {
                    "global" => {
                        match self.command_stats_global() {
                            Ok(stats) => {
                                self.send_message(&message.channel_id,
                                                  format!("**{}**\n{}",
                                                          MESSAGE_STATS_GLOBAL,
                                                          stats));
                            }
                            Err(_) => {
                                self.send_message(&message.channel_id,
                                                  MESSAGE_ERROR_OBTAINING_STATS);
                            }
                        }
                    }
                    "server" | "guild" => {
                        if !is_public_channel {
                            self.send_message(&message.channel_id,
                                              MESSAGE_COMMAND_REQUIRES_PUBLIC_CHANNEL);
                            return;
                        }

                        if let Some(server_id) = self.public_channels.get(&message.channel_id) {
                            match self.command_stats_server(server_id) {
                                Ok(stats) => {
                                    self.send_message(&message.channel_id,
                                                      format!("**{}**\n{}",
                                                              MESSAGE_STATS_SERVER,
                                                              stats));
                                }
                                Err(_) => {
                                    self.send_message(&message.channel_id,
                                                      MESSAGE_ERROR_OBTAINING_STATS);
                                }
                            }
                        }
                    }
                    "channel" => {
                        // TODO: Obtain channel ID from command argument
                        if !is_public_channel {
                            self.send_message(&message.channel_id,
                                              MESSAGE_COMMAND_REQUIRES_PUBLIC_CHANNEL);
                            return;
                        }

                        match self.command_stats_channel(&message.channel_id) {
                            Ok(stats) => {
                                self.send_message(&message.channel_id,
                                                  format!("**{}**\n{}",
                                                          MESSAGE_STATS_CHANNEL_THIS_CHANNEL,
                                                          stats));
                            }
                            Err(_) => {
                                self.send_message(&message.channel_id,
                                                  MESSAGE_ERROR_OBTAINING_STATS);
                            }
                        }
                    }
                    "user" => {
                        let mut user_id = &message.author.id;
                        let stats;

                        if is_public_channel {
                            // message.mentions might not be sorted in order of
                            // mentions in the message, but that's okay
                            for mentioned_user in &message.mentions {
                                if mentioned_user.id == self.bot_user_id {
                                    continue;
                                }

                                user_id = &mentioned_user.id;
                                break;
                            }

                            let server_id = &self.public_channels.get(&message.channel_id).unwrap();
                            stats =
                                self.command_stats_user_favourite_emoji_for_server(user_id,
                                                                                   server_id);
                        } else {
                            stats = self.command_stats_user_favourite_unicode_emoji(&user_id);
                        }

                        match stats {
                            Ok(stats) => {
                                self.send_message(&message.channel_id,
                                                  format!("**<@{}>{}**\n{}",
                                                          user_id.0,
                                                          if is_public_channel {
                                                              MESSAGE_STATS_USER_FOR_SERVER
                                                          } else {
                                                              MESSAGE_STATS_USER_UNICODE
                                                          },
                                                          stats));
                            }
                            Err(_) => {
                                self.send_message(&message.channel_id,
                                                  MESSAGE_ERROR_OBTAINING_STATS);
                            }
                        }

                    }
                    _ => {
                        self.send_message(&message.channel_id, MESSAGE_HELP_STATS);
                    }
                };
            }
            "quit" => {
                if !self.control_users.contains(&message.author.id) {
                    self.send_message(&message.channel_id, MESSAGE_COMMAND_REQUIRES_AUTH);
                    return;
                }

                info!("Quitting per {}:\"{}\"",
                      &message.author.id.0,
                      &message.author.name);
                self.quit = true;
            }
            unknown_command => {
                self.send_message(&message.channel_id,
                                  format!("{} `{}`. {}",
                                          MESSAGE_COMMAND_UNKNOWN,
                                          unknown_command,
                                          MESSAGE_HELP_HINT));
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

        for (custom_emoji_id, custom_emoji_name) in &self.custom_emojis {
            let count = message.content.matches(custom_emoji_name).count() as i32;

            if count > 0 {
                debug!("{} instances of \"{}\"", count, custom_emoji_name);

                let emoji_id = custom_emoji_id.0 as i64;

                if let Err(reason) = insert_emoji_usage
                       .execute(&[&channel_id, &user_id, &emoji_id, &count]) {
                    error!("Eror inserting emoji usage: {}", reason);
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

    fn command_stats_global(&self) -> Result<String, ()> {
        let db_conn = self.db_conn.as_ref().unwrap();
        let stats_global = match db_conn.prepare_cached(PG_SELECT_STATS_GLOBAL) {
            Ok(stats_global) => stats_global,
            Err(reason) => {
                error!("Error creating prepared statement: {}", reason);
                return Err(());
            }
        };

        let mut stats: String = "".to_string();

        match stats_global.query(&[]) {
            Ok(rows) => {
                if rows.len() == 0 {
                    stats = "No Unicode emoji have been used.".to_string();
                } else {
                    for row in &rows {
                        let emoji_name = row.get::<usize, String>(0);
                        let use_count = row.get::<usize, i64>(1);

                        stats += &format!("{} was used {} time{}\n",
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

        let mut stats: String = "".to_string();

        let server_id = &(server_id.0 as i64);

        match select_stats_server_top_emoji.query(&[server_id]) {
            Ok(rows) => {
                if rows.len() == 0 {
                    stats = "No emoji have been used on this server.".to_string();
                } else {
                    for row in &rows {
                        let emoji_id = row.get::<usize, i64>(0) as u64;
                        let emoji_name = row.get::<usize, String>(1);
                        let use_count = row.get::<usize, i64>(2);

                        stats += &format!("<:{}:{}> was used {} time{}\n",
                                emoji_name,
                                emoji_id,
                                use_count,
                                if use_count == 1 { "" } else { "s" });
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
                    for row in &rows {
                        let user_id = row.get::<usize, i64>(0) as u64;
                        let use_count = row.get::<usize, i64>(1);
                        let message_count = row.get::<usize, i64>(2);

                        stats += &format!("<@{}> has used {} emoji in {} message{}\n",
                                user_id,
                                use_count,
                                message_count,
                                if message_count == 1 { "" } else { "s" });
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

        let mut stats: String = "".to_string();

        let channel_id = &(channel_id.0 as i64);

        match select_stats_channel_top_emoji.query(&[channel_id]) {
            Ok(rows) => {
                if rows.len() == 0 {
                    stats = "No emoji have been used in this channel.".to_string();
                } else {
                    for row in &rows {
                        let emoji_id = row.get::<usize, i64>(0) as u64;
                        let emoji_name = row.get::<usize, String>(1);
                        let use_count = row.get::<usize, i64>(2);

                        stats += &format!("<:{}:{}> was used {} time{}\n",
                                emoji_name,
                                emoji_id,
                                use_count,
                                if use_count == 1 { "" } else { "s" });
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
                    for row in &rows {
                        let user_id = row.get::<usize, i64>(0) as u64;
                        let use_count = row.get::<usize, i64>(1);
                        let message_count = row.get::<usize, i64>(2);

                        stats += &format!("<@{}> has used {} emoji in {} message{}\n",
                                user_id,
                                use_count,
                                message_count,
                                if message_count == 1 { "" } else { "s" });
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

        let mut stats: String = "".to_string();

        let user_id = &(user_id.0 as i64);
        let server_id = &(server_id.0 as i64);

        match stats_user_favourite_emoji_for_server.query(&[user_id, server_id]) {
            Ok(rows) => {
                if rows.len() == 0 {
                    stats = "This user has not used any emoji.".to_string();
                } else {
                    for row in &rows {
                        let emoji_id = row.get::<usize, i64>(0) as u64;
                        let emoji_name = row.get::<usize, String>(1);
                        let use_count = row.get::<usize, i64>(2);

                        stats += &format!("<:{}:{}> was used {} time{}\n",
                                emoji_name,
                                emoji_id,
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

        let mut stats: String = "".to_string();

        let user_id = &(user_id.0 as i64);

        match select_stats_user_favourite_unicode_emoji.query(&[user_id]) {
            Ok(rows) => {
                if rows.len() == 0 {
                    stats = "This user has not used any Unicode emoji.".to_string();
                } else {
                    for row in &rows {
                        let emoji_id = row.get::<usize, i64>(0) as u64;
                        let emoji_name = row.get::<usize, String>(1);
                        let use_count = row.get::<usize, i64>(2);

                        stats += &format!("<:{}:{}> was used {} time{}\n",
                                emoji_name,
                                emoji_id,
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
            debug!("Adding public channel: \"#{}\"", channel.name);

            let db_conn = self.db_conn.as_ref().unwrap();
            let insert_public_channel = match db_conn.prepare_cached(PG_INSERT_PUBLIC_CHANNEL) {
                Ok(prepared_stmt) => prepared_stmt,
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

    fn add_custom_emojis(&mut self, custom_emojis: &Vec<Emoji>) {
        let db_conn = self.db_conn.as_ref().unwrap();
        let insert_custom_emoji = match db_conn.prepare_cached(PG_INSERT_CUSTOM_EMOJI) {
            Ok(prepared_stmt) => prepared_stmt,
            Err(reason) => {
                error!("Error creating prepared statement: {}", reason);
                return;
            }
        };

        for custom_emoji in custom_emojis {
            let custom_emoji_name = format!("<:{}:{}>", custom_emoji.name, custom_emoji.id.0);
            debug!("Adding custom emoji: \"{}\"", custom_emoji_name);

            let emoji_id = custom_emoji.id.0 as i64;
            if let Err(reason) = insert_custom_emoji.execute(&[&emoji_id, &custom_emoji.name]) {
                error!("Error inserting custom emoji: {}", reason);
            }

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
