extern crate chrono_humanize;
extern crate discord;
extern crate rand;
extern crate time;

use arg;
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::Write;
use bot_utility::{extract_first_word, extract_preceding_arg, remove_non_command_characters,
                  BasicServerInfo, MessageRecipient};
use emojistats::{CustomEmoji, Database, Emoji};

use self::chrono_humanize::HumanTime;
use self::discord::model::{Channel, ChannelId, ChannelType, EmojiId, Event, Game, GameType,
                           LiveServer, Message, MessageType, OnlineStatus, PossibleServer,
                           PrivateChannel, PublicChannel, Reaction, ReactionEmoji, Server,
                           ServerId, ServerInfo, User, UserId};
use self::rand::{thread_rng, Rng};
use self::time::{get_time, Timespec};

const RESPONSE_STATS_ERR: &str =
    "Sorry! An error occurred while retrieving the statistics. :frowning:";
const RESPONSE_USE_COMMAND_IN_PUBLIC_CHANNEL: &str =
    "Please use this command in a public channel. :shrug:";

#[derive(Debug)]
pub enum BotError {
    FailedToAuthenticate = 101,
    FailedToConnect = 102,
}

#[derive(Debug)]
pub enum BotDisposition {
    Quit,
    Restart,
}

#[derive(Debug)]
enum BotLoopDisposition {
    Continue,
    Quit,
    Restart,
}

pub struct Bot {
    discord: discord::Discord,
    discord_conn: discord::Connection,
    online_since: Timespec,
    bot_user_id: UserId,
    bot_admin_password: String,
    bot_admins: HashMap<UserId, User>,
    about_text: Option<String>,
    help_text: Option<String>,
    feedback_file: Option<File>,
    servers: HashMap<ServerId, BasicServerInfo>,
    public_text_channels: HashMap<ChannelId, PublicChannel>,
    private_channels: HashMap<ChannelId, PrivateChannel>,
    unknown_public_text_channels: HashSet<ChannelId>,
    db: Database,
    emoji: HashSet<Emoji>,
}

impl Bot {
    pub fn new(bot_token: &str, bot_admin_password: &str, db: Database) -> Result<Bot, BotError> {
        let discord = match discord::Discord::from_bot_token(bot_token) {
            Ok(discord) => discord,
            Err(reason) => {
                error!("Failed to authenticate with Discord: {}", reason);
                return Err(BotError::FailedToAuthenticate);
            }
        };

        let (discord_conn, ready_event) = match discord.connect() {
            Ok((discord_conn, ready_event)) => (discord_conn, ready_event),
            Err(reason) => {
                error!(
                    "Failed to create websocket connection to Discord: {}",
                    reason
                );
                return Err(BotError::FailedToConnect);
            }
        };

        let bot_user_id = ready_event.user.id;
        let bot_admin_password = bot_admin_password.to_string();

        let mut bot_admins = HashMap::new();
        match discord.get_application_info() {
            Ok(application_info) => {
                debug!(
                    "Application owner = {}#{} ({})",
                    application_info.owner.name,
                    application_info.owner.discriminator,
                    application_info.owner.id
                );
                bot_admins.insert(application_info.owner.id, application_info.owner);
            }
            Err(_) => {
                debug!("No application info available");
            }
        }

        Ok(Bot {
            discord,
            discord_conn,
            online_since: get_time(),
            bot_user_id,
            bot_admin_password,
            bot_admins,
            about_text: None,
            help_text: None,
            feedback_file: None,
            servers: HashMap::new(),
            public_text_channels: HashMap::new(),
            private_channels: HashMap::new(),
            unknown_public_text_channels: HashSet::new(),
            db,
            emoji: HashSet::new(),
        })
    }

    pub fn set_about_text<S>(&mut self, text: S)
    where
        S: Into<String>,
    {
        self.about_text = Some(text.into());
    }

    pub fn set_help_text<S>(&mut self, text: S)
    where
        S: Into<String>,
    {
        self.help_text = Some(text.into());
    }

    pub fn set_feedback_file<S>(&mut self, filename: S)
    where
        S: Into<String>,
    {
        let filename = filename.into();

        match OpenOptions::new().append(true).create(true).open(&filename) {
            Ok(file) => {
                self.feedback_file = Some(file);
                info!("Logging feedback to file: <{}>", filename);
            }
            Err(reason) => {
                warn!(
                    "Unable to open file for logging feedback <{}>: {}",
                    filename, reason
                );
            }
        }
    }

    pub fn add_unicode_emoji(&mut self, emoji: String) {
        let emoji = Emoji::Unicode(emoji);

        match self.db.add_emoji(&emoji, None) {
            Ok(_) => {}
            Err(reason) => {
                warn!(
                    "Error adding Unicode emoji <{:?}> to database: {}",
                    emoji, reason
                );
            }
        }
        self.emoji.insert(emoji);
    }

    pub fn run(mut self) -> BotDisposition {
        self.set_game(format!(
            "{} version {}",
            env!("CARGO_PKG_NAME"),
            env!("CARGO_PKG_VERSION")
        ));

        let mut bot_loop_disposition = BotLoopDisposition::Continue;

        // Main loop
        let bot_disposition = loop {
            match self.discord_conn.recv_event() {
                Ok(Event::MessageCreate(message)) => {
                    bot_loop_disposition = self.process_message(message);
                }
                Ok(Event::ReactionAdd(reaction)) => {
                    self.log_reaction(&reaction);
                }
                Ok(Event::ServerCreate(server)) => match server {
                    PossibleServer::Online(server) => {
                        self.update_emoji_list(server.id, server.emojis.clone());
                        self.add_live_server(server);
                    }
                    PossibleServer::Offline(_) => {}
                },
                Ok(Event::ServerUpdate(server)) => {
                    self.update_server(server);
                }
                Ok(Event::ServerDelete(possible_server)) => match possible_server {
                    PossibleServer::Online(server) => {
                        self.remove_server_id(&server.id);
                    }
                    PossibleServer::Offline(server_id) => {
                        self.remove_server_id(&server_id);
                    }
                },
                Ok(Event::ChannelCreate(channel)) => {
                    self.add_channel(channel);
                }
                Ok(Event::ChannelDelete(channel)) => {
                    self.remove_channel(&channel);
                }
                Ok(Event::ChannelUpdate(channel)) => {
                    self.update_channel(channel);
                }
                Ok(Event::ChannelRecipientAdd(_, user)) => {
                    self.add_user(&user);
                }
                Ok(Event::ServerMemberUpdate { user, .. }) => {
                    self.add_user(&user);
                }
                Ok(Event::ServerEmojisUpdate(server_id, emoji_list)) => {
                    self.update_emoji_list(server_id, emoji_list);
                }
                _ => {}
            }

            match bot_loop_disposition {
                BotLoopDisposition::Continue => {}
                BotLoopDisposition::Quit => {
                    break BotDisposition::Quit;
                }
                BotLoopDisposition::Restart => {
                    break BotDisposition::Restart;
                }
            }
        };

        let _ = self.discord_conn.shutdown();
        bot_disposition
    }

    fn check_for_new_servers(&mut self) {
        if let Ok(servers) = self.discord.get_servers() {
            for server_info in servers {
                if !self.servers.contains_key(&server_info.id) {
                    self.add_server_info(server_info);
                }
            }
        }
    }

    fn add_live_server(&mut self, server: LiveServer) {
        if !self.servers.contains_key(&server.id) {
            debug!("Adding new server {} ({})", server.name, server.id);
        }

        for channel in &server.channels {
            self.add_channel(Channel::Public(channel.clone()));
        }

        for member in &server.members {
            self.add_user(&member.user);
        }

        self.servers
            .insert(server.id, BasicServerInfo::from(server));
    }

    fn add_server_info(&mut self, server: ServerInfo) {
        if !self.servers.contains_key(&server.id) {
            debug!("Adding new server {} ({})", server.name, server.id);
        }

        if let Ok(channels) = self.discord.get_server_channels(server.id) {
            for public_channel in channels {
                self.add_channel(Channel::Public(public_channel));
            }
        }

        self.servers
            .insert(server.id, BasicServerInfo::from(server));
    }

    fn remove_server_id(&mut self, server_id: &ServerId) {
        if self.servers.contains_key(server_id) {
            debug!("Removing server {} and all associated channels", server_id);

            self.servers.remove(server_id);

            self.public_text_channels
                .retain(|_, c| c.server_id != *server_id);
        }
    }

    fn update_server(&mut self, new_server_info: Server) {
        self.update_emoji_list(new_server_info.id, new_server_info.emojis);

        if let Some(server) = self.servers.get_mut(&new_server_info.id) {
            debug!(
                "Updating server info: {} -> {} ({})",
                server.name, new_server_info.name, server.id
            );

            server.name = new_server_info.name;
            server.icon = new_server_info.icon;
            return;
        }
    }

    fn add_channel(&mut self, channel: Channel) {
        match channel {
            Channel::Public(channel) => {
                if channel.kind != ChannelType::Text {
                    return;
                }

                if !self.public_text_channels.contains_key(&channel.id) {
                    debug!(
                        "Adding new public text channel #{} ({})",
                        channel.name, channel.id
                    );
                }

                if let Err(reason) = self.db.add_channel(&channel) {
                    warn!(
                        "Error adding channel ({}) to database: {}",
                        channel.id, reason
                    );
                }

                self.public_text_channels.insert(channel.id, channel);
            }
            Channel::Private(channel) => {
                if !self.private_channels.contains_key(&channel.id) {
                    debug!(
                        "Adding new private channel with {}#{} ({})",
                        channel.recipient.name, channel.recipient.discriminator, channel.id
                    );
                }

                self.private_channels.insert(channel.id, channel);
            }
            Channel::Group(_) => {}
        }
    }

    fn remove_channel(&mut self, channel: &Channel) {
        match *channel {
            Channel::Public(ref channel) => {
                debug!(
                    "Removing public text channel #{} ({})",
                    channel.name, channel.id
                );

                self.public_text_channels.remove(&channel.id);
            }
            Channel::Private(ref channel) => {
                debug!(
                    "Removing private channel with {}#{} ({})",
                    channel.recipient.name, channel.recipient.discriminator, channel.id
                );

                self.private_channels.remove(&channel.id);
            }
            Channel::Group(_) => {}
        }
    }

    fn update_channel(&mut self, channel: Channel) {
        match channel {
            Channel::Public(new_channel_info) => {
                if let Some(channel) = self.public_text_channels.get_mut(&new_channel_info.id) {
                    debug!(
                        "Updating existing public text channel #{} -> #{} ({})",
                        channel.name, new_channel_info.name, channel.id
                    );

                    channel.name = new_channel_info.name;
                    return;
                }

                self.add_channel(Channel::Public(new_channel_info));
            }
            Channel::Private(_) => {}
            Channel::Group(_) => {}
        }
    }

    fn add_user(&mut self, user: &User) {
        if let Err(reason) = self.db.add_user(user) {
            warn!(
                "Error adding user {}#{} ({}) to database: {}",
                user.name, user.discriminator, user.id, reason
            );
        }
    }

    fn resolve_unknown_channel(&mut self, channel_id: &ChannelId) {
        // Get an updated list of servers
        if let Ok(servers) = self.discord.get_servers() {
            let mut new_servers = HashSet::new();

            for server_info in &servers {
                // If this is a new server ID, add the information for the server
                if !self.servers.contains_key(&server_info.id) {
                    new_servers.insert(server_info.id);
                    self.add_server_info(server_info.clone());
                }
            }

            // If the channel ID is still unknown,
            if !self.public_text_channels.contains_key(channel_id) {
                // get updated data on all servers the bot already knew about
                for server_info in servers {
                    if !new_servers.contains(&server_info.id) {
                        self.add_server_info(server_info);
                    }
                }
            }
        }
    }

    fn update_emoji_list(&mut self, server_id: ServerId, emoji_list: Vec<discord::model::Emoji>) {
        let mut updated_emoji_list = Vec::<Emoji>::new();

        for emoji in emoji_list {
            let custom_emoji = Emoji::Custom(CustomEmoji::new(
                server_id,
                emoji.id,
                emoji.name,
                emoji.animated,
            ));
            self.emoji.insert(custom_emoji.clone());
            updated_emoji_list.push(custom_emoji);
        }

        match self.db
            .update_server_emoji_list(&updated_emoji_list, &server_id)
        {
            Ok(_) => {}
            Err(reason) => {
                warn!("Error updating server emoji list: {}", reason);
            }
        }
    }

    fn set_game<S>(&mut self, game_name: S)
    where
        S: Into<String>,
    {
        self.discord_conn.set_presence(
            Some(Game {
                name: game_name.into(),
                kind: GameType::Playing,
                url: None,
            }),
            OnlineStatus::Online,
            false,
        );
    }

    fn process_message(&mut self, message: Message) -> BotLoopDisposition {
        // If the channel is unknown, try and get information on it by refreshing the server list
        //
        // If the channel is still unknown after refreshing the server list, add it to a list of
        //     "unknown channels" so that we don't keep trying to get information on it
        if !self.public_text_channels.contains_key(&message.channel_id)
            && !self.private_channels.contains_key(&message.channel_id)
            && !self.unknown_public_text_channels
                .contains(&message.channel_id)
        {
            self.resolve_unknown_channel(&message.channel_id);

            if !self.public_text_channels.contains_key(&message.channel_id) {
                self.unknown_public_text_channels.insert(message.channel_id);
            }
        }

        // Ignore all messages except regular, text-based messages
        match message.kind {
            MessageType::Regular => {}
            _ => {
                return BotLoopDisposition::Continue;
            }
        }

        // Ignore messages sent by other bots
        if message.author.bot {
            return BotLoopDisposition::Continue;
        }

        // If the message begins with a user id,
        if let (Some(arg::Type::UserId(user_id)), command) = extract_preceding_arg(&message.content)
        {
            // And that user ID is the bot user ID, this message is a command
            if user_id == self.bot_user_id {
                return self.process_command(&message, command);
            }
        }

        // If the message was sent in a private channel to the bot, the entire message is a command
        if self.private_channels.contains_key(&message.channel_id) {
            return self.process_command(&message, &message.content);
        }

        // This is not a command; log the emoji and continue to the next event
        self.log_emoji_usage(&message);
        BotLoopDisposition::Continue
    }

    fn log_emoji_usage(&self, message: &Message) {
        match self.db.message_exists(&message.id) {
            Ok(message_exists) => {
                if message_exists {
                    return;
                }
            }
            Err(reason) => {
                warn!(
                    "Unable to determine whether message {} exists in database: {}",
                    message.id, reason
                );
                return;
            }
        }

        let mut total_emoji_used = 0;

        for emoji in &self.emoji {
            let pattern = match *emoji {
                Emoji::Custom(ref custom_emoji) => &custom_emoji.pattern,
                Emoji::Unicode(ref emoji) => emoji,
            };

            let count = message.content.matches(pattern).count() as i32;

            if count > 0 {
                total_emoji_used += count;

                match self.db.record_emoji_usage(
                    &message.channel_id,
                    &message.author.id,
                    emoji,
                    count,
                ) {
                    Ok(_) => {}
                    Err(reason) => warn!("Error recording emoji usage: {}", reason),
                }
            }
        }

        match self.db.record_message_stats(
            &message.id,
            &message.channel_id,
            &message.author.id,
            total_emoji_used,
        ) {
            Ok(_) => {}
            Err(reason) => {
                warn!(
                    "Error recording statistics for message {}: {}",
                    message.id, reason
                );
            }
        }
    }

    fn log_reaction(&self, reaction: &Reaction) {
        // Ignore reactions in private channels
        if !self.public_text_channels.contains_key(&reaction.channel_id) {
            return;
        }

        let reaction_emoji = match reaction.emoji {
            ReactionEmoji::Custom { ref id, .. } => match self.get_emoji_by_id(id) {
                Some(emoji) => emoji.clone(),
                None => {
                    // Unknown emoji; ignore
                    return;
                }
            },
            ReactionEmoji::Unicode(ref emoji_text) => Emoji::Unicode(emoji_text.clone()),
        };

        match self.db.record_reaction(
            &reaction.channel_id,
            &reaction.message_id,
            &reaction.user_id,
            &reaction_emoji,
        ) {
            Ok(_) => {}
            Err(reason) => warn!(
                "Error recording emoji usage for message {}: {}",
                reaction.message_id, reason
            ),
        }
    }

    fn process_command(&mut self, message: &Message, command: &str) -> BotLoopDisposition {
        let command = remove_non_command_characters(command);

        match extract_first_word(command) {
            (command, args) if !command.is_empty() => {
                // Commands are case-insensitive
                match command.to_lowercase().as_ref() {
                    "auth" => self.attempt_auth(message, args),
                    "botinfo" => self.bot_info(message),
                    "quit" => self.quit(message),
                    "restart" => self.restart(message),
                    "feedback" => self.feedback(message, args),
                    "about" | "info" => self.about(message),
                    "help" | "commands" => self.help(message),
                    "g" | "global" => self.stats_global(message),
                    "s" | "server" => self.stats_server(message),
                    "c" | "channel" => self.stats_channel(message, None),
                    "m" | "me" => self.stats_user(message, None),
                    "u" | "custom" => self.stats_server_custom(message),
                    "l" | "least-used" => self.stats_server_least_used_custom_emoji(message),
                    _ => {
                        // Something else
                        // Did the user begin the message with a #channel or mention a user?
                        match arg::get_type(command) {
                            arg::Type::UserId(user_id) => {
                                self.stats_user(message, Some(&user_id));
                            }
                            arg::Type::ChannelId(channel_id) => {
                                self.stats_channel(message, Some(&channel_id));
                            }
                            _ => {
                                let mut matches =
                                    self.emoji.iter().filter(|e| e.pattern() == command);

                                if let Some(emoji) = matches.next() {
                                    self.stats_emoji(message, &emoji);
                                } else {
                                    self.help(message);
                                }
                            }
                        }

                        BotLoopDisposition::Continue
                    }
                }
            }
            _ => {
                // No command was provided
                self.help(message);
                BotLoopDisposition::Continue
            }
        }
    }

    fn send_message(&self, recipient: &MessageRecipient, text: &str) {
        recipient.send_message(&self.discord, text);
    }

    fn send_response(&self, message: &Message, text: &str) {
        self.send_message(message, &format!("**{}**: {}", message.author.name, text));
    }

    fn attempt_auth(&mut self, message: &Message, password_attempt: &str) -> BotLoopDisposition {
        if self.bot_admins.contains_key(&message.author.id) {
            self.send_response(
                message,
                "You are already authenticated as a bot administrator. :unlock:",
            );
        } else if !self.private_channels.contains_key(&message.channel_id) {
            self.send_response(
                message,
                "Please use this command in a private message. :lock:",
            );
        } else {
            if password_attempt.is_empty() {
                self.send_response(
                    message,
                    "Please enter the bot administration password. :lock:",
                );
            } else if password_attempt == self.bot_admin_password {
                self.send_response(message, "Authenticated successfully. :white_check_mark:");
                self.bot_admins
                    .insert(message.author.id, message.author.clone());
            } else {
                self.send_response(message, "Unable to authenticate. :x:");
            }
        }

        BotLoopDisposition::Continue
    }

    fn bot_info(&mut self, message: &Message) -> BotLoopDisposition {
        if self.bot_admins.contains_key(&message.author.id) {
            self.check_for_new_servers();

            let online_time = HumanTime::from(self.online_since - get_time());

            self.send_response(
                message,
                &format!(
                    "**{} version {}**\n\
                     Online since {} on {} server{} comprising \
                     {} text channel{}. :clock2:",
                    env!("CARGO_PKG_NAME"),
                    env!("CARGO_PKG_VERSION"),
                    online_time,
                    self.servers.len(),
                    if self.servers.len() == 1 { "" } else { "s" },
                    self.public_text_channels.len(),
                    if self.public_text_channels.len() == 1 {
                        ""
                    } else {
                        "s"
                    }
                ),
            );
        } else {
            self.respond_auth_required(message);
        }

        BotLoopDisposition::Continue
    }

    fn quit(&self, message: &Message) -> BotLoopDisposition {
        if self.bot_admins.contains_key(&message.author.id) {
            self.send_response(message, "Quitting. :octagonal_sign:");
            info!("Quit command issued by {}.", message.author.name);
            BotLoopDisposition::Quit
        } else {
            self.respond_auth_required(message);
            BotLoopDisposition::Continue
        }
    }

    fn restart(&self, message: &Message) -> BotLoopDisposition {
        if self.bot_admins.contains_key(&message.author.id) {
            self.send_response(message, "Restarting. :repeat:");
            info!("Restart command issued by {}.", message.author.name);
            BotLoopDisposition::Restart
        } else {
            self.respond_auth_required(message);
            BotLoopDisposition::Continue
        }
    }

    fn feedback(&self, message: &Message, feedback: &str) -> BotLoopDisposition {
        self.send_response(
            message,
            "Thanks. Your feedback has been logged for review. :smiley:",
        );

        // Write the feedback to log files
        // If the the feedback spans multiple lines, indent the subsequent lines
        let log_feedback = feedback.replace(
            "\n",
            &format!(
                "\n              {}#{}> ",
                message.author.name, message.author.discriminator
            ),
        );

        let log_feedback = format!(
            "Feedback from {}#{}: {}\n",
            message.author.name, message.author.discriminator, log_feedback
        );
        info!("{}", log_feedback);

        if self.feedback_file.is_some() {
            match self.feedback_file
                .as_ref()
                .unwrap()
                .write(log_feedback.as_bytes())
            {
                Ok(_) => {}
                Err(reason) => {
                    warn!("Error writing feedback \"{}\" to log: {}", feedback, reason);
                }
            }
        }

        // Send the feedback to administrators
        let feedback = format!(
            "Feedback from {}#{}:\n```\n{}```",
            message.author.name, message.author.discriminator, feedback
        );

        for (user_id, user) in &self.bot_admins {
            let mut num_channels_sent_to = 0;

            // Look for an existing private channel for each administrator
            for (channel_id, _) in self.private_channels
                .iter()
                .filter(|&(_, c)| c.recipient.id == *user_id)
            {
                num_channels_sent_to += 1;
                self.send_message(channel_id, &feedback);
            }

            // If there wasn't an existing private channel, create one
            if num_channels_sent_to == 0 {
                if let Ok(private_channel) = self.discord.create_private_channel(*user_id) {
                    self.send_message(&private_channel.id, &feedback);
                } else {
                    warn!(
                        "Unable to create private channel to send feedback to bot administrator \
                         {}#{}.",
                        user.name, user.discriminator
                    );
                }
            }
        }

        BotLoopDisposition::Continue
    }

    fn about(&self, message: &Message) -> BotLoopDisposition {
        if self.about_text.is_some() {
            self.send_response(message, self.about_text.as_ref().unwrap());
        }

        BotLoopDisposition::Continue
    }

    fn help(&self, message: &Message) -> BotLoopDisposition {
        if self.help_text.is_some() {
            self.send_response(message, self.help_text.as_ref().unwrap());
        }

        BotLoopDisposition::Continue
    }

    fn stats_global(&self, message: &Message) -> BotLoopDisposition {
        let top_emoji = match self.db.get_global_top_emoji() {
            Ok(results) => results,
            Err(reason) => {
                warn!("Unable to retrieve global top used emoji: {}", reason);
                self.send_response(message, RESPONSE_STATS_ERR);
                return BotLoopDisposition::Continue;
            }
        };

        let top_reaction_emoji = match self.db.get_global_top_reaction_emoji() {
            Ok(results) => results,
            Err(reason) => {
                warn!(
                    "Unable to retrieve global top used reaction emoji: {}",
                    reason
                );
                self.send_response(message, RESPONSE_STATS_ERR);
                return BotLoopDisposition::Continue;
            }
        };

        let top_emoji_ct = top_emoji.len();
        let top_reaction_emoji_ct = top_reaction_emoji.len();

        if (top_emoji_ct == 0) && (top_reaction_emoji_ct == 0) {
            self.send_response(message, "I've never seen anyone use any emoji. :shrug:");
        } else {
            let emoji_stats = create_emoji_usage_line(top_emoji);
            let reaction_emoji_stats = create_emoji_usage_line(top_reaction_emoji);

            let earth_emoji_list = [":earth_africa:", ":earth_americas:", ":earth_asia:"];
            let earth = thread_rng().choose(&earth_emoji_list).unwrap();

            let emoji_header = match self.db.get_global_emoji_use_count() {
                Ok(count) => format!(
                    "Top Emoji ({} total use{})",
                    count,
                    if count == 1 { "" } else { "s" }
                ),
                Err(reason) => {
                    warn!("Unable to retrieve global emoji use count: {}", reason);
                    "Top Emoji".to_string()
                }
            };

            let reaction_emoji_header = match self.db.get_global_reaction_count() {
                Ok(count) => format!(
                    "Top Reaction Emoji ({} total use{})",
                    count,
                    if count == 1 { "" } else { "s" }
                ),
                Err(reason) => {
                    warn!("Unable to retrieve global reaction count: {}", reason);
                    "Top Reaction Emoji".to_string()
                }
            };

            let _ = self.discord.send_embed(
                message.channel_id,
                &format!("**{}**", message.author.name),
                |e| {
                    e.title(&format!("Global Statistics {}", earth))
                        .fields(|f| {
                            if (top_emoji_ct > 0) && (top_reaction_emoji_ct > 0) {
                                f.field(&emoji_header, &emoji_stats, true).field(
                                    &reaction_emoji_header,
                                    &reaction_emoji_stats,
                                    true,
                                )
                            } else if top_emoji_ct > 0 {
                                f.field(&emoji_header, &emoji_stats, true)
                            } else {
                                f.field(&reaction_emoji_header, &reaction_emoji_stats, true)
                            }
                        })
                },
            );
        }

        BotLoopDisposition::Continue
    }

    fn stats_server(&self, message: &Message) -> BotLoopDisposition {
        if self.private_channels.contains_key(&message.channel_id) {
            self.send_response(message, RESPONSE_USE_COMMAND_IN_PUBLIC_CHANNEL);
            return BotLoopDisposition::Continue;
        }

        let server_id = match self.public_text_channels.get(&message.channel_id) {
            Some(channel) => channel.server_id,
            None => {
                warn!("Unknown public text channel ({})", message.channel_id);
                self.send_response(message, RESPONSE_STATS_ERR);
                return BotLoopDisposition::Continue;
            }
        };

        let top_emoji = match self.db.get_server_top_emoji(&server_id) {
            Ok(results) => results,
            Err(reason) => {
                warn!(
                    "Unable to retrieve top used emoji on server ({}): {}",
                    server_id, reason
                );
                self.send_response(message, RESPONSE_STATS_ERR);
                return BotLoopDisposition::Continue;
            }
        };

        let top_reaction_emoji = match self.db.get_server_top_reaction_emoji(&server_id) {
            Ok(results) => results,
            Err(reason) => {
                warn!(
                    "Unable to retrieve top used reaction emoji on server ({}): {}",
                    server_id, reason
                );
                return BotLoopDisposition::Continue;
            }
        };

        let top_emoji_ct = top_emoji.len();
        let top_reaction_emoji_ct = top_reaction_emoji.len();

        if (top_emoji_ct == 0) && (top_reaction_emoji_ct == 0) {
            self.send_response(
                message,
                "I've never seen anyone use any emoji on this server. :shrug:",
            );
        } else {
            let top_users = match self.db.get_server_top_users(&server_id) {
                Ok(results) => results,
                Err(reason) => {
                    warn!(
                        "Unable to retrieve top users on server ({}): {}",
                        server_id, reason
                    );
                    self.send_response(message, RESPONSE_STATS_ERR);
                    return BotLoopDisposition::Continue;
                }
            };

            let user_stats = create_top_users_line(top_users);

            let emoji_stats = create_emoji_usage_line(top_emoji);
            let reaction_emoji_stats = create_emoji_usage_line(top_reaction_emoji);

            let emoji_header = match self.db.get_server_emoji_use_count(&server_id) {
                Ok(count) => format!(
                    "Top Emoji ({} total use{})",
                    count,
                    if count == 1 { "" } else { "s" }
                ),
                Err(reason) => {
                    warn!("Unable to retrieve server emoji use count: {}", reason);
                    "Top Emoji".to_string()
                }
            };

            let reaction_emoji_header = match self.db.get_server_reaction_count(&server_id) {
                Ok(count) => format!(
                    "Top Reaction Emoji ({} total use{})",
                    count,
                    if count == 1 { "" } else { "s" }
                ),
                Err(reason) => {
                    warn!("Unable to retrieve server reaction count: {}", reason);
                    "Top Reaction Emoji".to_string()
                }
            };

            let _ = self.discord.send_embed(
                message.channel_id,
                &format!("**{}**", message.author.name),
                |e| {
                    e.title("Server Statistics :chart_with_upwards_trend:")
                        .fields(|f| {
                            if (top_emoji_ct > 0) && (top_reaction_emoji_ct > 0) {
                                f.field(&emoji_header, &emoji_stats, true)
                                    .field(&reaction_emoji_header, &reaction_emoji_stats, true)
                                    .field("Top Emoji Users", &user_stats, true)
                            } else if top_emoji_ct > 0 {
                                f.field(&emoji_header, &emoji_stats, true).field(
                                    "Top Emoji Users",
                                    &user_stats,
                                    true,
                                )
                            } else {
                                f.field(&reaction_emoji_header, &reaction_emoji_stats, true)
                                // If there aren't any emoji stats, there aren't any top emoji users
                            }
                        })
                },
            );
        }

        BotLoopDisposition::Continue
    }

    fn stats_server_custom(&self, message: &Message) -> BotLoopDisposition {
        if self.private_channels.contains_key(&message.channel_id) {
            self.send_response(message, RESPONSE_USE_COMMAND_IN_PUBLIC_CHANNEL);
            return BotLoopDisposition::Continue;
        }

        let server_id = match self.public_text_channels.get(&message.channel_id) {
            Some(channel) => channel.server_id,
            None => {
                warn!("Unknown public text channel ({})", message.channel_id);
                self.send_response(message, RESPONSE_STATS_ERR);
                return BotLoopDisposition::Continue;
            }
        };

        let top_emoji = match self.db.get_server_top_custom_emoji(&server_id) {
            Ok(results) => results,
            Err(reason) => {
                warn!(
                    "Unable to retrieve top used custom emoji on server ({}): {}",
                    server_id, reason
                );
                self.send_response(message, RESPONSE_STATS_ERR);
                return BotLoopDisposition::Continue;
            }
        };

        let top_reaction_emoji = match self.db.get_server_top_custom_reaction_emoji(&server_id) {
            Ok(results) => results,
            Err(reason) => {
                warn!(
                    "Unable to retrieve top used custom reaction emoji on server ({}): {}",
                    server_id, reason
                );
                self.send_response(message, RESPONSE_STATS_ERR);
                return BotLoopDisposition::Continue;
            }
        };

        let top_emoji_ct = top_emoji.len();
        let top_reaction_emoji_ct = top_reaction_emoji.len();

        if (top_emoji_ct == 0) && (top_reaction_emoji_ct == 0) {
            self.send_response(
                message,
                "I've never seen anyone use any custom emoji on this server. :shrug:",
            );
        } else {
            let top_users = match self.db.get_server_top_custom_emoji_users(&server_id) {
                Ok(results) => results,
                Err(reason) => {
                    warn!(
                        "Unable to retrieve top custom emoji users on server ({}): {}",
                        server_id, reason
                    );
                    self.send_response(message, RESPONSE_STATS_ERR);
                    return BotLoopDisposition::Continue;
                }
            };

            let user_stats = create_top_users_line(top_users);

            let emoji_stats = create_emoji_usage_line(top_emoji);
            let reaction_emoji_stats = create_emoji_usage_line(top_reaction_emoji);

            let emoji_header = match self.db.get_server_custom_emoji_use_count(&server_id) {
                Ok(count) => format!(
                    "Top Emoji ({} total use{})",
                    count,
                    if count == 1 { "" } else { "s" }
                ),
                Err(reason) => {
                    warn!(
                        "Unable to retrieve server custom emoji use count: {}",
                        reason
                    );
                    "Top Emoji".to_string()
                }
            };

            let reaction_emoji_header = match self.db
                .get_server_custom_emoji_reaction_use_count(&server_id)
            {
                Ok(count) => format!(
                    "Top Reaction Emoji ({} total use{})",
                    count,
                    if count == 1 { "" } else { "s" }
                ),
                Err(reason) => {
                    warn!(
                        "Unable to retrieve server custom emoji reaction count: {}",
                        reason
                    );
                    "Top Reaction Emoji".to_string()
                }
            };

            let _ = self.discord.send_embed(
                message.channel_id,
                &format!("**{}**", message.author.name),
                |e| {
                    e.title("Server Statistics (Custom Emoji) :chart_with_upwards_trend:")
                        .fields(|f| {
                            if (top_emoji_ct > 0) && (top_reaction_emoji_ct > 0) {
                                f.field(&emoji_header, &emoji_stats, true)
                                    .field(&reaction_emoji_header, &reaction_emoji_stats, true)
                                    .field("Top Emoji Users", &user_stats, true)
                            } else if top_emoji_ct > 0 {
                                f.field(&emoji_header, &emoji_stats, true).field(
                                    "Top Emoji Users",
                                    &user_stats,
                                    true,
                                )
                            } else {
                                f.field(&reaction_emoji_header, &reaction_emoji_stats, true)
                                // If there aren't any emoji stats, there aren't any top emoji users
                            }
                        })
                },
            );
        }

        BotLoopDisposition::Continue
    }

    fn stats_server_least_used_custom_emoji(&self, message: &Message) -> BotLoopDisposition {
        if self.private_channels.contains_key(&message.channel_id) {
            self.send_response(message, RESPONSE_USE_COMMAND_IN_PUBLIC_CHANNEL);
            return BotLoopDisposition::Continue;
        }

        let server_id = match self.public_text_channels.get(&message.channel_id) {
            Some(channel) => channel.server_id,
            None => {
                warn!("Unknown public text channel ({})", message.channel_id);
                self.send_response(message, RESPONSE_STATS_ERR);
                return BotLoopDisposition::Continue;
            }
        };

        let least_used_emoji = match self.db.get_server_least_used_custom_emoji(&server_id) {
            Ok(results) => results,
            Err(reason) => {
                warn!(
                    "Unable to retrieve least used custom emoji on server ({}): {}",
                    server_id, reason
                );
                self.send_response(message, RESPONSE_STATS_ERR);
                return BotLoopDisposition::Continue;
            }
        };

        let least_used_reaction_emoji = match self.db
            .get_server_least_used_custom_reaction_emoji(&server_id)
        {
            Ok(results) => results,
            Err(reason) => {
                warn!(
                    "Unable to retrieve least used custom reaction emoji on server ({}): {}",
                    server_id, reason
                );
                self.send_response(message, RESPONSE_STATS_ERR);
                return BotLoopDisposition::Continue;
            }
        };

        let least_used_emoji_ct = least_used_emoji.len();
        let least_used_reaction_emoji_ct = least_used_reaction_emoji.len();

        if (least_used_emoji_ct == 0) && (least_used_reaction_emoji_ct == 0) {
            self.send_response(
                message,
                "It looks like there aren't any custom emoji on this server. :shrug:",
            );
        } else {
            let emoji_stats = create_emoji_usage_line(least_used_emoji);
            let reaction_emoji_stats = create_emoji_usage_line(least_used_reaction_emoji);

            let _ = self.discord.send_embed(
                message.channel_id,
                &format!("**{}**", message.author.name),
                |e| {
                    e.title(
                        "Server Statistics (Least Used Custom Emoji) :chart_with_downwards_trend:",
                    ).fields(|f| {
                        if (least_used_emoji_ct > 0) && (least_used_reaction_emoji_ct > 0) {
                            f.field(&"Emoji", &emoji_stats, true).field(
                                &"Reactions",
                                &reaction_emoji_stats,
                                true,
                            )
                        } else if least_used_emoji_ct > 0 {
                            f.field(&"Emoji", &emoji_stats, true)
                        } else {
                            f.field(&"Reactions", &reaction_emoji_stats, true)
                        }
                    })
                },
            );
        }

        BotLoopDisposition::Continue
    }

    fn stats_channel(
        &self,
        message: &Message,
        channel_id: Option<&ChannelId>,
    ) -> BotLoopDisposition {
        if self.private_channels.contains_key(&message.channel_id) {
            self.send_response(message, RESPONSE_USE_COMMAND_IN_PUBLIC_CHANNEL);
            return BotLoopDisposition::Continue;
        }

        let channel_id = channel_id.unwrap_or(&message.channel_id);

        let stats_description = match self.public_text_channels.get(&channel_id) {
            Some(channel) => format!(
                "Statistics for #{} :chart_with_upwards_trend:",
                channel.name
            ),
            None => "Channel statistics :chart_with_upwards_trend:".to_string(),
        };

        let top_emoji = match self.db.get_channel_top_emoji(&channel_id) {
            Ok(results) => results,
            Err(reason) => {
                warn!(
                    "Unable to retrieve top used emoji on channel ({}): {}",
                    message.channel_id, reason
                );
                self.send_response(message, RESPONSE_STATS_ERR);
                return BotLoopDisposition::Continue;
            }
        };

        if top_emoji.len() == 0 {
            self.send_response(
                message,
                "I've never seen anyone use any emoji in that channel. :shrug:",
            );
        } else {
            let top_users = match self.db.get_channel_top_users(&channel_id) {
                Ok(results) => results,
                Err(reason) => {
                    warn!(
                        "Unable to retrieve top users in channel ({}): {}",
                        channel_id, reason
                    );
                    self.send_response(message, RESPONSE_STATS_ERR);
                    return BotLoopDisposition::Continue;
                }
            };

            let user_stats = create_top_users_line(top_users);

            let emoji_stats = create_emoji_usage_line(top_emoji);

            let emoji_header = match self.db.get_channel_emoji_use_count(&channel_id) {
                Ok(count) => format!(
                    "Top Emoji ({} total use{})",
                    count,
                    if count == 1 { "" } else { "s" }
                ),
                Err(reason) => {
                    warn!("Unable to retrieve channel emoji use count: {}", reason);
                    "Top Emoji".to_string()
                }
            };

            let _ = self.discord.send_embed(
                message.channel_id,
                &format!("**{}**", message.author.name),
                |e| {
                    e.title(&stats_description).fields(|f| {
                        f.field(&emoji_header, &emoji_stats, true).field(
                            "Top Emoji Users",
                            &user_stats,
                            true,
                        )
                    })
                },
            );
        }

        BotLoopDisposition::Continue
    }

    fn stats_user(&self, message: &Message, user_id: Option<&UserId>) -> BotLoopDisposition {
        let user_id = user_id.unwrap_or(&message.author.id);

        if *user_id == self.bot_user_id {
            self.send_response(message, "You're so silly! :smile:");
            return BotLoopDisposition::Continue;
        }

        // If the bot knows which server is associated with the public text channel, get statistics
        // for both Unicode emoji and custom emoji on the same server
        // Otherwise, just get statistics for Unicode emoji
        let server = match self.public_text_channels.get(&message.channel_id) {
            Some(channel) => Some(&channel.server_id),
            None => None,
        };

        let user_name = match self.db.get_user_name(user_id) {
            Ok(maybe_user_name) => match maybe_user_name {
                Some(user_name) => user_name,
                None => "(Unknown user)".to_string(),
            },
            Err(reason) => {
                debug!(
                    "Error retrieving user name for user ({}) from database: {}",
                    user_id, reason
                );
                "(Unknown user)".to_string()
            }
        };

        let stats_description = if *user_id == message.author.id {
            "Your favourite emoji :two_hearts:".to_string()
        } else {
            format!("{}'s favourite emoji :two_hearts:", user_name)
        };

        let top_emoji = match self.db.get_user_top_emoji(&user_id, server) {
            Ok(results) => results,
            Err(reason) => {
                warn!(
                    "Unable to retrieve top emoji used by user {} ({}): {}",
                    user_name, user_id, reason
                );
                self.send_response(message, RESPONSE_STATS_ERR);
                return BotLoopDisposition::Continue;
            }
        };

        if top_emoji.len() == 0 {
            self.send_response(
                message,
                &format!("I've never seen <@{}> use any emoji. :shrug:", user_id),
            );
        } else {
            let stats = create_emoji_usage_line(top_emoji);

            let stats_header = match self.db.get_user_emoji_use_count(&user_id, server) {
                Ok(count) => format!(
                    "All in all, {} {} used {} emoji{}:",
                    if *user_id == message.author.id {
                        "you"
                    } else {
                        &user_name
                    },
                    if *user_id == message.author.id {
                        "have"
                    } else {
                        "has"
                    },
                    count,
                    if server.is_some() {
                        " on this server"
                    } else {
                        ""
                    },
                ),
                Err(reason) => {
                    warn!("Unable to retrieve user emoji use count: {}", reason);
                    "Your top emoji:".to_string()
                }
            };

            let _ = self.discord.send_embed(
                message.channel_id,
                &format!("**{}**", message.author.name),
                |e| {
                    e.title(&stats_description)
                        .fields(|f| f.field(&stats_header, &stats, false))
                },
            );
        }

        BotLoopDisposition::Continue
    }

    fn stats_emoji(&self, message: &Message, emoji: &Emoji) {
        match self.db.get_emoji_usage(emoji) {
            Ok(maybe_count) => match maybe_count {
                Some(count) if count > 0 => {
                    self.send_response(
                        message,
                        &format!(
                            "{} has been used {} time{}.",
                            emoji.pattern(),
                            count,
                            if count == 1 { "" } else { "s" }
                        ),
                    );
                }
                _ => {
                    self.send_response(
                        message,
                        &format!("I've never seen anyone use {}.", emoji.pattern()),
                    );
                }
            },
            Err(reason) => {
                warn!(
                    "Error obtaining emoji usage stats for emoji {}: {}",
                    emoji.pattern(),
                    reason
                );
                self.send_response(message, RESPONSE_STATS_ERR);
            }
        }
    }

    fn respond_auth_required(&self, message: &Message) {
        self.send_response(message, "Please authenticate first. :lock:");
    }

    fn get_emoji_by_id(&self, id: &EmojiId) -> Option<&Emoji> {
        return self.emoji
            .iter()
            .filter(|emoji| match *emoji {
                &Emoji::Custom(ref emoji) => emoji.id == *id,
                &Emoji::Unicode(_) => false,
            })
            .next();
    }
}

fn create_emoji_usage_line(emoji_usage: Vec<(Emoji, i64)>) -> String {
    let mut stats = String::new();

    for (emoji, count) in emoji_usage {
        match emoji {
            Emoji::Custom(emoji) => {
                stats += &format!(
                    "{} used {} time{}\n",
                    emoji.pattern,
                    count,
                    if count == 1 { "" } else { "s" }
                );
            }
            Emoji::Unicode(emoji) => {
                stats += &format!(
                    "{} used {} time{}\n",
                    emoji,
                    count,
                    if count == 1 { "" } else { "s" }
                );
            }
        }
    }

    stats
}

fn create_top_users_line(emoji_usage: Vec<(String, i64)>) -> String {
    let mut stats = String::new();

    for (user_name, count) in emoji_usage {
        stats += &format!("{} used {} emoji\n", user_name, count)
    }

    stats
}
