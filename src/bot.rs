extern crate discord;
extern crate time;
extern crate chrono_humanize;

use arg;
use std::collections::{HashMap, HashSet};
use bot_utility::{extract_preceding_arg, remove_non_command_characters, extract_first_word,
                  MessageRecipient};
use emojistats::{CustomEmoji, Database, Emoji};

use self::chrono_humanize::HumanTime;
use self::discord::model::{Event, Channel, ChannelId, ChannelType, Game, GameType, Message,
                           MessageType, OnlineStatus, PossibleServer, PublicChannel, Server,
                           ServerId, ServerInfo, UserId};
use self::time::{Timespec, get_time};

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
    bot_admins: HashSet<UserId>,
    servers: HashMap<ServerId, ServerInfo>,
    public_text_channels: HashMap<ChannelId, PublicChannel>,
    private_channels: HashSet<ChannelId>,
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
                error!("Failed to create websocket connection to Discord: {}",
                       reason);
                return Err(BotError::FailedToConnect);
            }
        };

        let bot_user_id = ready_event.user.id;
        let bot_admin_password = bot_admin_password.to_string();

        let mut bot_admins = HashSet::new();
        match discord.get_application_info() {
            Ok(application_info) => {
                bot_admins.insert(application_info.owner.id);
                debug!("Application owner = {}#{} ({})",
                       application_info.owner.name,
                       application_info.owner.discriminator,
                       application_info.owner.id);
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
               servers: HashMap::new(),
               public_text_channels: HashMap::new(),
               private_channels: HashSet::new(),
               unknown_public_text_channels: HashSet::new(),
               db,
               emoji: HashSet::new(),
           })
    }

    pub fn add_unicode_emoji(&mut self, emoji: String) {
        let emoji = Emoji::Unicode(emoji);

        match self.db.add_emoji(&emoji) {
            Ok(_) => {}
            Err(reason) => {
                warn!("Error adding Unicode emoji <{:?}> to database: {}",
                      emoji,
                      reason);
            }
        }
        self.emoji.insert(emoji);
    }

    pub fn run(mut self) -> BotDisposition {
        self.set_game(format!("{} version {}",
                              env!("CARGO_PKG_NAME"),
                              env!("CARGO_PKG_VERSION")));

        let mut bot_loop_disposition = BotLoopDisposition::Continue;

        self.refresh_servers();

        // Main loop
        let bot_disposition = loop {
            match self.discord_conn.recv_event() {
                Ok(Event::MessageCreate(message)) => {
                    bot_loop_disposition = self.process_message(message);
                }
                Ok(Event::ServerCreate(server)) => {
                    // Don't call refresh_servers() - when the bot is starting up, this will spam
                    // Discord because on startup, an event is generated for every server
                    match server {
                        PossibleServer::Online(server) => {
                            self.add_emoji_list(server.id, server.emojis);
                        }
                        PossibleServer::Offline(_) => {}
                    }
                }
                Ok(Event::ServerUpdate(server)) => {
                    self.update_server(server);
                }
                Ok(Event::ServerDelete(possible_server)) => {
                    match possible_server {
                        PossibleServer::Online(server) => {
                            self.remove_server_id(&server.id);
                        }
                        PossibleServer::Offline(server_id) => {
                            self.remove_server_id(&server_id);
                        }
                    }
                }
                Ok(Event::ChannelCreate(channel)) => {
                    self.add_channel(channel);
                }
                Ok(Event::ChannelDelete(channel)) => {
                    self.remove_channel(&channel);
                }
                Ok(Event::ChannelUpdate(channel)) => {
                    self.update_channel(channel);
                }
                Ok(Event::ServerEmojisUpdate(server_id, emoji_list)) => {
                    self.add_emoji_list(server_id, emoji_list);
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

    fn refresh_servers(&mut self) {
        if let Ok(servers) = self.discord.get_servers() {
            for server in servers {
                self.add_server(server);
            }
        }
    }

    fn add_server(&mut self, server: ServerInfo) {
        if !self.servers.contains_key(&server.id) {
            debug!("Adding new server {} ({})", server.name, server.id);
        }

        if let Ok(channels) = self.discord.get_server_channels(server.id) {
            for public_channel in channels {
                self.add_channel(Channel::Public(public_channel));
            }
        }

        self.servers.insert(server.id, server);
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
        self.add_emoji_list(new_server_info.id, new_server_info.emojis);

        if let Some(server) = self.servers.get_mut(&new_server_info.id) {
            debug!("Updating server info: {} -> {} ({})",
                   server.name,
                   new_server_info.name,
                   server.id);

            server.name = new_server_info.name;
            server.icon = new_server_info.icon;
            return;
        }

        self.refresh_servers();
    }

    fn add_channel(&mut self, channel: Channel) {
        match channel {
            Channel::Public(channel) => {
                if channel.kind != ChannelType::Text {
                    return;
                }

                if !self.public_text_channels.contains_key(&channel.id) {
                    debug!("Adding new public text channel #{} ({})",
                           channel.name,
                           channel.id);
                }

                self.public_text_channels.insert(channel.id, channel);
            }
            Channel::Private(channel) => {
                if !self.private_channels.contains(&channel.id) {
                    debug!("Adding new private channel with {}#{} ({})",
                           channel.recipient.name,
                           channel.recipient.discriminator,
                           channel.id);
                }

                self.private_channels.insert(channel.id);
            }
            Channel::Group(_) => {}
        }
    }

    fn remove_channel(&mut self, channel: &Channel) {
        match *channel {
            Channel::Public(ref channel) => {
                debug!("Removing public text channel #{} ({})",
                       channel.name,
                       channel.id);

                self.public_text_channels.remove(&channel.id);
            }
            Channel::Private(ref channel) => {
                debug!("Removing private channel with {}#{} ({})",
                       channel.recipient.name,
                       channel.recipient.discriminator,
                       channel.id);

                self.private_channels.remove(&channel.id);
            }
            Channel::Group(_) => {}
        }
    }

    fn update_channel(&mut self, channel: Channel) {
        match channel {
            Channel::Public(new_channel_info) => {
                if let Some(channel) = self.public_text_channels.get_mut(&new_channel_info.id) {
                    debug!("Updating existing public text channel #{} -> #{} ({})",
                           channel.name,
                           new_channel_info.name,
                           channel.id);

                    channel.name = new_channel_info.name;
                    return;
                }

                self.add_channel(Channel::Public(new_channel_info));
            }
            Channel::Private(_) => {}
            Channel::Group(_) => {}
        }
    }

    fn add_emoji_list(&mut self, server_id: ServerId, emoji_list: Vec<discord::model::Emoji>) {
        for emoji in emoji_list {
            let custom_emoji = &Emoji::Custom(CustomEmoji::new(server_id, emoji.id, emoji.name));

            match self.db.add_emoji(&custom_emoji) {
                Ok(_) => {
                    debug!("Added custom emoji on server ({}): <{:?}>",
                           server_id,
                           custom_emoji);
                }
                Err(reason) => {
                    warn!("Error adding custom emoji <{:?}> to database: {}",
                          custom_emoji,
                          reason);
                }
            }
        }
    }

    fn set_game<S>(&mut self, game_name: S)
        where S: Into<String>
    {
        self.discord_conn
            .set_presence(Some(Game {
                                   name: game_name.into(),
                                   kind: GameType::Playing,
                                   url: None,
                               }),
                          OnlineStatus::Online,
                          false);
    }

    fn process_message(&mut self, message: Message) -> BotLoopDisposition {
        // If the channel is unknown, try and get information on it by refreshing the server list
        //
        // If the channel is still unknown after refreshing the server list, add it to a list of
        //     "unknown channels" so that we don't keep trying to get information on it
        if !self.public_text_channels.contains_key(&message.channel_id) &&
           !self.private_channels.contains(&message.channel_id) &&
           !self.unknown_public_text_channels
                .contains(&message.channel_id) {
            self.refresh_servers();

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
        if let (Some(arg::Type::UserId(user_id)), command) =
            extract_preceding_arg(&message.content) {
            // And that user ID is the bot user ID, this message is a command
            if user_id == self.bot_user_id {
                return self.process_command(&message, command);
            }
        }

        // If the message was sent in a private channel to the bot, the entire message is a command
        if self.private_channels.contains(&message.channel_id) {
            return self.process_command(&message, &message.content);
        }

        // This is not a command; continue to the next event
        BotLoopDisposition::Continue
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
                    _ => BotLoopDisposition::Continue, // Unknown command
                }
            }
            _ => BotLoopDisposition::Continue, // No command was provided; continue to next event
        }
    }

    fn send_message(&self, recipient: &MessageRecipient, text: &str) {
        recipient.send_message(&self.discord, text);
    }

    fn send_response(&self, message: &Message, text: &str) {
        self.send_message(message, &format!("<@{}>: {}", message.author.id, text));
    }

    fn attempt_auth(&mut self, message: &Message, password_attempt: &str) -> BotLoopDisposition {
        if self.bot_admins.contains(&message.author.id) {
            self.send_response(message,
                               "You are already authenticated as a bot administrator.");
        } else if !self.private_channels.contains(&message.channel_id) {
            self.send_response(message, "Please use this command in a private message.");
        } else {
            if password_attempt.is_empty() {
                self.send_response(message, "Please enter the bot administration password.");
            } else if password_attempt == self.bot_admin_password {
                self.send_response(message, "Authenticated successfully.");
                self.bot_admins.insert(message.author.id);
            } else {
                self.send_response(message, "Unable to authenticate.");
            }
        }

        BotLoopDisposition::Continue
    }

    fn bot_info(&mut self, message: &Message) -> BotLoopDisposition {
        if self.bot_admins.contains(&message.author.id) {
            self.refresh_servers();

            let online_time = HumanTime::from(self.online_since - get_time());

            self.send_response(message,
                               &format!("**{} version {}**\n\
                                       Online since {} on {} server{} comprising \
                                       {} text channel{}.",
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
                                       }));
        } else {
            self.respond_auth_required(message);
        }

        BotLoopDisposition::Continue
    }

    fn quit(&self, message: &Message) -> BotLoopDisposition {
        if self.bot_admins.contains(&message.author.id) {
            self.send_response(message, "Quitting.");
            info!("Quit command issued by {}.", message.author.name);
            BotLoopDisposition::Quit
        } else {
            self.respond_auth_required(message);
            BotLoopDisposition::Continue
        }
    }

    fn restart(&self, message: &Message) -> BotLoopDisposition {
        if self.bot_admins.contains(&message.author.id) {
            self.send_response(message, "Restarting.");
            info!("Restart command issued by {}.", message.author.name);
            BotLoopDisposition::Restart
        } else {
            self.respond_auth_required(message);
            BotLoopDisposition::Continue
        }
    }

    fn respond_auth_required(&self, message: &Message) {
        self.send_response(message, "Please authenticate first.");
    }
}
