extern crate discord;
extern crate postgres;

use self::discord::model::{ChannelId, MessageId, PublicChannel, ServerId, User, UserId};
use super::model::{CustomEmoji, Emoji};
use postgres::rows::Rows;

pub struct Database {
    conn: postgres::Connection,
}

impl Database {
    pub fn new<T>(params: T) -> postgres::Result<Database>
    where
        T: postgres::params::IntoConnectParams,
    {
        let conn = postgres::Connection::connect(params, postgres::TlsMode::None)?;
        create_tables(&conn)?;

        Ok(Database { conn })
    }

    pub fn add_channel(&self, channel: &PublicChannel) -> postgres::Result<()> {
        const QUERY_INSERT_CHANNEL: &str = r#"
        INSERT INTO channel (id, server_id, name)
        VALUES ($1, $2, $3)
        ON CONFLICT (id) DO UPDATE
            SET name = excluded.name;"#;

        self.conn.execute(
            QUERY_INSERT_CHANNEL,
            &[
                &(channel.id.0 as i64),
                &(channel.server_id.0 as i64),
                &channel.name,
            ],
        )?;

        Ok(())
    }

    pub fn add_user(&self, user: &User) -> postgres::Result<()> {
        const QUERY_INSERT_USER: &str = r#"
        INSERT INTO user_ (id, name, discriminator)
        VALUES ($1, $2, $3)
        ON CONFLICT (id) DO UPDATE
            SET name = excluded.name,
                discriminator = excluded.discriminator;"#;

        self.conn.execute(
            QUERY_INSERT_USER,
            &[
                &(user.id.0 as i64),
                &user.name,
                &(user.discriminator as i32),
            ],
        )?;

        Ok(())
    }

    pub fn add_emoji(&self, emoji: &Emoji, server_id: Option<&ServerId>) -> postgres::Result<()> {
        const QUERY_INSERT_CUSTOM_EMOJI: &str = r#"
        INSERT INTO emoji (server_id, id, name, is_custom_emoji, is_animated)
        VALUES ($1, $2, $3, TRUE, $4)
        ON CONFLICT (id) DO UPDATE
            SET name = excluded.name, is_animated = excluded.is_animated;"#;

        const QUERY_INSERT_UNICODE_EMOJI: &str = r#"
        INSERT INTO emoji (server_id, name, is_custom_emoji, is_animated)
        VALUES (NULL, $1, FALSE, FALSE);"#;

        match *emoji {
            Emoji::Custom(ref emoji) => {
                self.conn.execute(
                    QUERY_INSERT_CUSTOM_EMOJI,
                    &[
                        &(server_id.unwrap().0 as i64),
                        &(emoji.id.0 as i64),
                        &emoji.name,
                        &emoji.animated,
                    ],
                )?;
            }
            Emoji::Unicode(ref emoji) => {
                // Only insert Unicode emoji if they aren't already in the database
                match self.get_emoji_id(emoji.clone())? {
                    Some(_) => {}
                    None => {
                        self.conn.execute(QUERY_INSERT_UNICODE_EMOJI, &[&emoji])?;
                    }
                }
            }
        }

        Ok(())
    }

    pub fn update_server_emoji_list(
        &self,
        emoji_list: &Vec<Emoji>,
        server_id: &ServerId,
    ) -> postgres::Result<()> {
        const QUERY_SELECT_SERVER_CUSTOM_EMOJI: &str = r#"
        SELECT e.id
        FROM emoji e
        WHERE (e.server_id = $1) AND (e.is_active = TRUE);"#;

        const QUERY_PRUNE_EMOJI: &str = r#"
        UPDATE emoji
        SET is_active = FALSE
        WHERE id = $1;"#;

        let result = self.conn
            .query(QUERY_SELECT_SERVER_CUSTOM_EMOJI, &[&(server_id.0 as i64)]);

        match result {
            Ok(rows) => {
                debug!("Pruning old emoji on server {}", server_id);

                // Retrieve list of custom emoji for this server
                let mut db_emoji_ids = Vec::new();
                rows.into_iter().for_each(|row| {
                    db_emoji_ids.push(row.get::<usize, i64>(0) as u64);
                });

                // Prune emoji that are NOT in the updated emoji list
                let pruned_emoji_ids: Vec<u64> = db_emoji_ids
                    .into_iter()
                    .filter(|db_emoji_id| {
                        // Find the number of times the database emoji IS in the updated emoji list
                        let matches = emoji_list
                            .iter()
                            .filter(|emoji| match *emoji {
                                &Emoji::Custom(ref emoji) => emoji.id.0 == *db_emoji_id,
                                &Emoji::Unicode(_) => false,
                            })
                            .count();

                        // Return true if the database emoji IS NOT in the updated emoji list
                        matches == 0
                    })
                    .collect();

                pruned_emoji_ids.into_iter().for_each(|id| {
                    debug!("Pruning emoji with ID: {}", id);

                    match self.conn.execute(QUERY_PRUNE_EMOJI, &[&(id as i64)]) {
                        Ok(_) => {}
                        Err(reason) => {
                            warn!("Error pruning emoji with ID {}: {}", id, reason);
                        }
                    }
                });
            }
            Err(reason) => {
                warn!("Error retrieving list of emoji: {}", reason);
            }
        }

        for ref emoji in emoji_list {
            match self.add_emoji(emoji, Some(server_id)) {
                Ok(_) => {
                    debug!(
                        "Added custom emoji on server ({}): <{:?}>",
                        server_id, emoji
                    );
                }
                Err(reason) => {
                    warn!(
                        "Error adding custom emoji <{:?}> to database: {}",
                        emoji, reason
                    );
                }
            }
        }

        Ok(())
    }

    pub fn message_exists(&self, message_id: &MessageId) -> postgres::Result<bool> {
        const QUERY_GET_MESSAGE_EXIST: &str = r#"
        SELECT id
        FROM message
        WHERE id = $1;"#;

        let result = self.conn
            .query(QUERY_GET_MESSAGE_EXIST, &[&(message_id.0 as i64)])?;

        Ok(result.len() != 0)
    }

    pub fn record_message_stats(
        &self,
        message_id: &MessageId,
        channel_id: &ChannelId,
        user_id: &UserId,
        emoji_count: i32,
    ) -> postgres::Result<()> {
        const QUERY_RECORD_MESSAGE_STATS: &str = r#"
        INSERT INTO message (id, channel_id, user_id, emoji_count)
        VALUES ($1, $2, $3, $4);"#;

        self.conn.execute(
            QUERY_RECORD_MESSAGE_STATS,
            &[
                &(message_id.0 as i64),
                &(channel_id.0 as i64),
                &(user_id.0 as i64),
                &emoji_count,
            ],
        )?;

        Ok(())
    }

    pub fn record_emoji_usage(
        &self,
        channel_id: &ChannelId,
        user_id: &UserId,
        emoji: &Emoji,
        count: i32,
    ) -> postgres::Result<()> {
        const QUERY_RECORD_EMOJI_USAGE: &str = r#"
        INSERT INTO emoji_usage (channel_id, user_id, emoji_id, use_count)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (channel_id, user_id, emoji_id) DO UPDATE
            SET use_count = emoji_usage.use_count + excluded.use_count;"#;

        let emoji_id = match *emoji {
            Emoji::Custom(ref custom_emoji) => {
                debug!(
                    "Custom emoji {} used {} time{} by {} in channel {}",
                    custom_emoji.pattern,
                    count,
                    if count == 1 { "" } else { "s" },
                    user_id,
                    channel_id
                );
                custom_emoji.id.0 as i64
            }
            Emoji::Unicode(ref emoji) => {
                debug!(
                    "Emoji {} used {} time{} by {} in channel {}",
                    emoji,
                    count,
                    if count == 1 { "" } else { "s" },
                    user_id,
                    channel_id
                );
                self.get_emoji_id(emoji.clone())?.unwrap() as i64
            }
        };

        self.conn.execute(
            QUERY_RECORD_EMOJI_USAGE,
            &[
                &(channel_id.0 as i64),
                &(user_id.0 as i64),
                &emoji_id,
                &count,
            ],
        )?;

        Ok(())
    }

    pub fn record_reaction(
        &self,
        channel_id: &ChannelId,
        message_id: &MessageId,
        user_id: &UserId,
        reaction_emoji: &Emoji,
    ) -> postgres::Result<()> {
        const QUERY_RECORD_REACTION: &str = r#"
        INSERT INTO reactions (channel_id, message_id, user_id, emoji_id)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (channel_id, message_id, user_id, emoji_id) DO NOTHING;"#;

        let emoji_id = match *reaction_emoji {
            Emoji::Custom(ref custom_emoji) => {
                debug!(
                    "User {} responded to message {} in channel {} with emoji {}",
                    user_id, message_id, channel_id, custom_emoji.pattern
                );
                custom_emoji.id.0 as i64
            }
            Emoji::Unicode(ref emoji) => {
                debug!(
                    "User {} responded to message {} in channel {} with emoji {}",
                    user_id, message_id, channel_id, emoji
                );
                self.get_emoji_id(emoji.clone())?.unwrap() as i64
            }
        };

        self.conn.execute(
            QUERY_RECORD_REACTION,
            &[
                &(channel_id.0 as i64),
                &(message_id.0 as i64),
                &(user_id.0 as i64),
                &emoji_id,
            ],
        )?;

        Ok(())
    }

    pub fn get_emoji_id<S>(&self, name: S) -> postgres::Result<Option<u64>>
    where
        S: Into<String>,
    {
        const QUERY_GET_EMOJI_ID: &str = r#"
        SELECT id
        FROM emoji
        WHERE name = $1;"#;

        let result = self.conn.query(QUERY_GET_EMOJI_ID, &[&name.into()])?;

        if result.len() == 0 {
            Ok(None)
        } else {
            Ok(Some(result.get(0).get::<usize, i64>(0) as u64))
        }
    }

    pub fn get_global_top_emoji(&self) -> postgres::Result<Vec<(Emoji, i64)>> {
        const QUERY_SELECT_TOP_GLOBAL_EMOJI: &str = r#"
        SELECT e.is_custom_emoji, e.id, e.name, e.is_animated, SUM(eu.use_count)
        FROM emoji_usage eu
            INNER JOIN emoji e ON eu.emoji_id = e.id
        WHERE e.is_custom_emoji = FALSE
        GROUP BY e.is_custom_emoji, e.id, e.name
        ORDER BY SUM(eu.use_count) DESC
        LIMIT 5;"#;

        let result = self.conn.query(QUERY_SELECT_TOP_GLOBAL_EMOJI, &[])?;

        Ok(result_into_vec_emoji(result)?)
    }

    pub fn get_global_top_reaction_emoji(&self) -> postgres::Result<Vec<(Emoji, i64)>> {
        const QUERY_SELECT_TOP_GLOBAL_REACTION_EMOJI: &str = r#"
        SELECT e.is_custom_emoji, e.id, e.name, e.is_animated, COUNT(*)
        FROM reactions r
            INNER JOIN emoji e ON r.emoji_id = e.id
        WHERE e.is_custom_emoji = FALSE
        GROUP BY e.is_custom_emoji, e.id, e.name
        ORDER BY COUNT(*) DESC
        LIMIT 5;"#;

        let result = self.conn
            .query(QUERY_SELECT_TOP_GLOBAL_REACTION_EMOJI, &[])?;

        Ok(result_into_vec_emoji(result)?)
    }

    pub fn get_global_emoji_use_count(&self) -> postgres::Result<i64> {
        const QUERY_SELECT_GLOBAL_EMOJI_USE_COUNT: &str = r#"
        SELECT COALESCE(SUM(eu.use_count), 0)
        FROM emoji_usage eu
            INNER JOIN emoji e ON eu.emoji_id = e.id
        WHERE e.is_custom_emoji = FALSE;"#;

        let result = self.conn.query(QUERY_SELECT_GLOBAL_EMOJI_USE_COUNT, &[])?;

        Ok(result.get(0).get::<usize, i64>(0))
    }

    pub fn get_global_reaction_count(&self) -> postgres::Result<i64> {
        const QUERY_SELECT_GLOBAL_REACTION_COUNT: &str = r#"
        SELECT COALESCE(COUNT(*), 0)
        FROM reactions r
            INNER JOIN emoji e ON r.emoji_id = e.id
        WHERE e.is_custom_emoji = FALSE;"#;

        let result = self.conn.query(QUERY_SELECT_GLOBAL_REACTION_COUNT, &[])?;

        Ok(result.get(0).get::<usize, i64>(0))
    }

    pub fn get_server_top_emoji(
        &self,
        server_id: &ServerId,
    ) -> postgres::Result<Vec<(Emoji, i64)>> {
        const QUERY_SELECT_TOP_SERVER_EMOJI: &str = r#"
        SELECT e.is_custom_emoji, e.id, e.name, e.is_animated, SUM(eu.use_count)
        FROM emoji_usage eu
            INNER JOIN emoji e ON eu.emoji_id = e.id
            INNER JOIN channel c ON eu.channel_id = c.id
        WHERE (c.server_id = $1) AND (e.is_active = TRUE)
        GROUP BY e.is_custom_emoji, e.id, e.name
        ORDER BY SUM(eu.use_count) DESC
        LIMIT 5;"#;

        let result = self.conn
            .query(QUERY_SELECT_TOP_SERVER_EMOJI, &[&(server_id.0 as i64)])?;

        Ok(result_into_vec_emoji(result)?)
    }

    pub fn get_server_least_used_custom_emoji(
        &self,
        server_id: &ServerId,
    ) -> postgres::Result<Vec<(Emoji, i64)>> {
        const QUERY_SELECT_LEAST_USED_SERVER_CUSTOM_EMOJI: &str = r#"
        SELECT e.is_custom_emoji, e.id, e.name, e.is_animated, COALESCE(SUM(eu.use_count), 0)
        FROM emoji e
            LEFT JOIN emoji_usage eu ON e.id = eu.emoji_id
            LEFT JOIN channel c ON eu.channel_id = c.id
        WHERE (e.server_id = $1) AND (e.is_custom_emoji = TRUE) AND (e.is_active = TRUE)
        GROUP BY e.is_custom_emoji, e.id, e.name
        ORDER BY COALESCE(SUM(eu.use_count), 0) ASC
        LIMIT 5;"#;

        let result = self.conn.query(
            QUERY_SELECT_LEAST_USED_SERVER_CUSTOM_EMOJI,
            &[&(server_id.0 as i64)],
        )?;

        Ok(result_into_vec_emoji(result)?)
    }

    pub fn get_server_least_used_custom_reaction_emoji(
        &self,
        server_id: &ServerId,
    ) -> postgres::Result<Vec<(Emoji, i64)>> {
        const QUERY_SELECT_LEAST_USED_SERVER_REACTION_EMOJI: &str = r#"
        SELECT e.is_custom_emoji, e.id, e.name, e.is_animated, COALESCE(COUNT(r.*), 0)
        FROM emoji e
            LEFT JOIN reactions r ON e.id = r.emoji_id
            LEFT JOIN channel c ON r.channel_id = c.id
        WHERE (e.server_id = $1) AND (e.is_custom_emoji = TRUE) AND (e.is_active = TRUE)
        GROUP BY e.is_custom_emoji, e.id, e.name
        ORDER BY COALESCE(COUNT(r.*), 0) ASC
        LIMIT 5;"#;

        let result = self.conn.query(
            QUERY_SELECT_LEAST_USED_SERVER_REACTION_EMOJI,
            &[&(server_id.0 as i64)],
        )?;

        Ok(result_into_vec_emoji(result)?)
    }

    pub fn get_server_top_custom_emoji(
        &self,
        server_id: &ServerId,
    ) -> postgres::Result<Vec<(Emoji, i64)>> {
        const QUERY_SELECT_TOP_SERVER_CUSTOM_EMOJI: &str = r#"
        SELECT e.is_custom_emoji, e.id, e.name, e.is_animated, SUM(eu.use_count)
        FROM emoji_usage eu
            INNER JOIN emoji e ON eu.emoji_id = e.id
            INNER JOIN channel c ON eu.channel_id = c.id
        WHERE (c.server_id = $1) AND (e.is_custom_emoji = TRUE) AND (e.is_active = TRUE)
        GROUP BY e.is_custom_emoji, e.id, e.name
        ORDER BY SUM(eu.use_count) DESC
        LIMIT 5;"#;

        let result = self.conn.query(
            QUERY_SELECT_TOP_SERVER_CUSTOM_EMOJI,
            &[&(server_id.0 as i64)],
        )?;

        Ok(result_into_vec_emoji(result)?)
    }

    pub fn get_server_top_custom_reaction_emoji(
        &self,
        server_id: &ServerId,
    ) -> postgres::Result<Vec<(Emoji, i64)>> {
        const QUERY_SELECT_TOP_SERVER_REACTION_EMOJI: &str = r#"
        SELECT e.is_custom_emoji, e.id, e.name, e.is_animated, COUNT(*)
        FROM reactions r
            INNER JOIN emoji e ON r.emoji_id = e.id
            INNER JOIN channel c ON r.channel_id = c.id
        WHERE (c.server_id = $1) AND (e.is_custom_emoji = TRUE) AND (e.is_active = TRUE)
        GROUP BY e.is_custom_emoji, e.id, e.name
        ORDER BY COUNT(*) DESC
        LIMIT 5;"#;

        let result = self.conn.query(
            QUERY_SELECT_TOP_SERVER_REACTION_EMOJI,
            &[&(server_id.0 as i64)],
        )?;

        Ok(result_into_vec_emoji(result)?)
    }

    pub fn get_server_custom_emoji_reaction_use_count(
        &self,
        server_id: &ServerId,
    ) -> postgres::Result<i64> {
        const QUERY_SELECT_SERVER_EMOJI_USE_COUNT: &str = r#"
        SELECT COALESCE(COUNT(*), 0)
        FROM reactions r
            INNER JOIN channel c ON r.channel_id = c.id
            INNER JOIN emoji e ON r.emoji_id = e.id
        WHERE (c.server_id = $1) AND (e.is_custom_emoji = TRUE);"#;

        let result = self.conn.query(
            QUERY_SELECT_SERVER_EMOJI_USE_COUNT,
            &[&(server_id.0 as i64)],
        )?;

        Ok(result.get(0).get::<usize, i64>(0))
    }

    pub fn get_server_top_reaction_emoji(
        &self,
        server_id: &ServerId,
    ) -> postgres::Result<Vec<(Emoji, i64)>> {
        const QUERY_SELECT_TOP_SERVER_REACTION_EMOJI: &str = r#"
        SELECT e.is_custom_emoji, e.id, e.name, e.is_animated, COUNT(*)
        FROM reactions r
            INNER JOIN emoji e ON r.emoji_id = e.id
            INNER JOIN channel c ON r.channel_id = c.id
        WHERE (c.server_id = $1) AND (e.is_active = TRUE)
        GROUP BY e.is_custom_emoji, e.id, e.name
        ORDER BY COUNT(*) DESC
        LIMIT 5;"#;

        let result = self.conn.query(
            QUERY_SELECT_TOP_SERVER_REACTION_EMOJI,
            &[&(server_id.0 as i64)],
        )?;

        Ok(result_into_vec_emoji(result)?)
    }

    pub fn get_server_emoji_use_count(&self, server_id: &ServerId) -> postgres::Result<i64> {
        const QUERY_SELECT_SERVER_EMOJI_USE_COUNT: &str = r#"
        SELECT COALESCE(SUM(eu.use_count), 0)
        FROM emoji_usage eu
            INNER JOIN channel c ON eu.channel_id = c.id
        WHERE c.server_id = $1;"#;

        let result = self.conn.query(
            QUERY_SELECT_SERVER_EMOJI_USE_COUNT,
            &[&(server_id.0 as i64)],
        )?;

        Ok(result.get(0).get::<usize, i64>(0))
    }

    pub fn get_server_reaction_count(&self, server_id: &ServerId) -> postgres::Result<i64> {
        const QUERY_SELECT_SERVER_REACTION_COUNT: &str = r#"
        SELECT COALESCE(COUNT(*), 0)
        FROM reactions r
            INNER JOIN channel c ON r.channel_id = c.id
        WHERE c.server_id = $1;"#;

        let result = self.conn
            .query(QUERY_SELECT_SERVER_REACTION_COUNT, &[&(server_id.0 as i64)])?;

        Ok(result.get(0).get::<usize, i64>(0))
    }

    pub fn get_server_custom_emoji_use_count(&self, server_id: &ServerId) -> postgres::Result<i64> {
        const QUERY_SELECT_SERVER_CUSTOM_EMOJI_USE_COUNT: &str = r#"
        SELECT SUM(eu.use_count)
        FROM emoji_usage eu
            INNER JOIN channel c ON eu.channel_id = c.id
            INNER JOIN emoji e ON eu.emoji_id = e.id
        WHERE c.server_id = $1 AND e.is_custom_emoji = TRUE;"#;

        let result = self.conn.query(
            QUERY_SELECT_SERVER_CUSTOM_EMOJI_USE_COUNT,
            &[&(server_id.0 as i64)],
        )?;

        Ok(result.get(0).get::<usize, i64>(0))
    }

    pub fn get_channel_top_emoji(
        &self,
        channel_id: &ChannelId,
    ) -> postgres::Result<Vec<(Emoji, i64)>> {
        const QUERY_SELECT_TOP_CHANNEL_EMOJI: &str = r#"
        SELECT e.is_custom_emoji, e.id, e.name, e.is_animated, SUM(eu.use_count)
        FROM emoji_usage eu
            INNER JOIN emoji e ON eu.emoji_id = e.id
        WHERE (eu.channel_id = $1) AND (e.is_active = TRUE)
        GROUP BY e.is_custom_emoji, e.id, e.name
        ORDER BY SUM(eu.use_count) DESC
        LIMIT 5;"#;

        let result = self.conn
            .query(QUERY_SELECT_TOP_CHANNEL_EMOJI, &[&(channel_id.0 as i64)])?;

        Ok(result_into_vec_emoji(result)?)
    }

    pub fn get_channel_emoji_use_count(&self, channel_id: &ChannelId) -> postgres::Result<i64> {
        const QUERY_SELECT_CHANNEL_EMOJI_USE_COUNT: &str = r#"
        SELECT COALESCE(SUM(eu.use_count), 0)
        FROM emoji_usage eu
        WHERE eu.channel_id = $1;"#;

        let result = self.conn.query(
            QUERY_SELECT_CHANNEL_EMOJI_USE_COUNT,
            &[&(channel_id.0 as i64)],
        )?;

        Ok(result.get(0).get::<usize, i64>(0))
    }

    pub fn get_user_top_emoji(
        &self,
        user_id: &UserId,
        server_id: Option<&ServerId>,
    ) -> postgres::Result<Vec<(Emoji, i64)>> {
        const QUERY_SELECT_TOP_USER_UNICODE_EMOJI: &str = r#"
        SELECT e.is_custom_emoji, e.id, e.name, e.is_animated, SUM(eu.use_count)
        FROM emoji_usage eu
            INNER JOIN emoji e ON eu.emoji_id = e.id
        WHERE (e.is_custom_emoji = FALSE) AND (eu.user_id = $1) AND (e.is_active = TRUE)
        GROUP BY e.is_custom_emoji, e.id, e.name
        ORDER BY SUM(eu.use_count) DESC
        LIMIT 5;"#;

        const QUERY_SELECT_TOP_USER_SERVER_EMOJI: &str = r#"
        SELECT e.is_custom_emoji, e.id, e.name, e.is_animated, SUM(eu.use_count)
        FROM emoji_usage eu
            INNER JOIN emoji e ON eu.emoji_id = e.id
        WHERE (eu.user_id = $1) AND (e.server_id IS NULL OR e.server_id = $2) AND (e.is_active = TRUE)
        GROUP BY e.is_custom_emoji, e.id, e.name
        ORDER BY SUM(eu.use_count) DESC
        LIMIT 5;"#;

        let result = match server_id {
            Some(server_id) => self.conn.query(
                QUERY_SELECT_TOP_USER_SERVER_EMOJI,
                &[&(user_id.0 as i64), &(server_id.0 as i64)],
            )?,
            None => self.conn
                .query(QUERY_SELECT_TOP_USER_UNICODE_EMOJI, &[&(user_id.0 as i64)])?,
        };

        Ok(result_into_vec_emoji(result)?)
    }

    pub fn get_user_emoji_use_count(
        &self,
        user_id: &UserId,
        server_id: Option<&ServerId>,
    ) -> postgres::Result<i64> {
        const QUERY_SELECT_USER_UNICODE_EMOJI_USE_COUNT: &str = r#"
        SELECT COALESCE(SUM(eu.use_count), 0)
        FROM emoji_usage eu
        WHERE eu.user_id = $1;"#;

        const QUERY_SELECT_USER_SERVER_EMOJI_USE_COUNT: &str = r#"
        SELECT COALESCE(SUM(eu.use_count), 0)
        FROM emoji_usage eu
        INNER JOIN channel c ON eu.channel_id = c.id
        WHERE eu.user_id = $1 AND c.server_id = $2;"#;

        let result = match server_id {
            Some(server_id) => self.conn.query(
                QUERY_SELECT_USER_SERVER_EMOJI_USE_COUNT,
                &[&(user_id.0 as i64), &(server_id.0 as i64)],
            )?,
            None => self.conn.query(
                QUERY_SELECT_USER_UNICODE_EMOJI_USE_COUNT,
                &[&(user_id.0 as i64)],
            )?,
        };

        Ok(result.get(0).get::<usize, i64>(0))
    }

    pub fn get_server_top_users(
        &self,
        server_id: &ServerId,
    ) -> postgres::Result<Vec<(String, i64)>> {
        const QUERY_SELECT_TOP_SERVER_USERS: &str = r#"
        SELECT u.name, u.discriminator, SUM(m.emoji_count)
        FROM message m
            INNER JOIN user_ u ON m.user_id = u.id
            INNER JOIN channel c ON m.channel_id = c.id
        WHERE c.server_id = $1
        GROUP BY u.name, u.discriminator
        HAVING SUM(m.emoji_count) > 0
        ORDER BY SUM(m.emoji_count) DESC
        LIMIT 5;"#;

        let result = self.conn
            .query(QUERY_SELECT_TOP_SERVER_USERS, &[&(server_id.0 as i64)])?;

        Ok(result_into_vec_users(result)?)
    }

    pub fn get_server_top_custom_emoji_users(
        &self,
        server_id: &ServerId,
    ) -> postgres::Result<Vec<(String, i64)>> {
        const QUERY_SELECT_TOP_SERVER_CUSTOM_EMOJI_USERS: &str = r#"
        SELECT u.name, u.discriminator, SUM(eu.use_count)
        FROM emoji_usage eu
            INNER JOIN user_ u ON eu.user_id = u.id
            INNER JOIN channel c ON eu.channel_id = c.id
            INNER JOIN emoji e ON eu.emoji_id = e.id
        WHERE (c.server_id = $1) AND (e.is_custom_emoji = TRUE)
        GROUP BY u.name, u.discriminator
        HAVING SUM(eu.use_count) > 0
        ORDER BY SUM(eu.use_count) DESC
        LIMIT 5;"#;

        let result = self.conn.query(
            QUERY_SELECT_TOP_SERVER_CUSTOM_EMOJI_USERS,
            &[&(server_id.0 as i64)],
        )?;

        Ok(result_into_vec_users(result)?)
    }

    pub fn get_channel_top_users(
        &self,
        channel_id: &ChannelId,
    ) -> postgres::Result<Vec<(String, i64)>> {
        const QUERY_SELECT_TOP_CHANNEL_USERS: &str = r#"
        SELECT u.name, u.discriminator, SUM(m.emoji_count)
        FROM message m
            INNER JOIN user_ u ON m.user_id = u.id
        WHERE m.channel_id = $1
        GROUP BY u.name, u.discriminator
        HAVING SUM(m.emoji_count) > 0
        ORDER BY SUM(m.emoji_count) DESC
        LIMIT 5;"#;

        let result = self.conn
            .query(QUERY_SELECT_TOP_CHANNEL_USERS, &[&(channel_id.0 as i64)])?;

        Ok(result_into_vec_users(result)?)
    }

    pub fn get_emoji_usage(&self, emoji: &Emoji) -> postgres::Result<Option<i64>> {
        const QUERY_EMOJI_USAGE: &str = r#"
        SELECT COALESCE(SUM(eu.use_count), 0)
        FROM emoji_usage eu
        WHERE eu.emoji_id = $1;"#;

        let emoji_id = match *emoji {
            Emoji::Custom(ref emoji) => emoji.id.0 as i64,
            Emoji::Unicode(ref emoji) => {
                match self.get_emoji_id(emoji.clone())? {
                    Some(id) => id as i64,
                    None => {
                        // This Unicode emoji is not in the database
                        info!(
                            "Couldn't get statistics for unknown Unicode emoji <{}>",
                            emoji
                        );
                        return Ok(None);
                    }
                }
            }
        };

        let result = self.conn.query(QUERY_EMOJI_USAGE, &[&emoji_id])?;

        match result.iter().next() {
            Some(row) => match row.get::<usize, Option<i64>>(0) {
                Some(count) => Ok(Some(count)),
                None => Ok(None),
            },
            None => Ok(None),
        }
    }

    pub fn get_user_name(&self, user_id: &UserId) -> postgres::Result<Option<String>> {
        const QUERY_SELECT_USER: &str = r#"
        SELECT u.name, u.discriminator
        FROM user_ u
        WHERE u.id = $1;"#;

        let result = self.conn.query(QUERY_SELECT_USER, &[&(user_id.0 as i64)])?;

        if result.len() == 0 {
            Ok(None)
        } else {
            let row = result.get(0);

            Ok(Some(format!("{}", row.get::<usize, String>(0))))
        }
    }
}

fn create_tables(db_conn: &postgres::Connection) -> postgres::Result<()> {
    const QUERY_CREATE_TABLES: &str = r#"
    CREATE TABLE IF NOT EXISTS emoji (
        server_id BIGINT NULL,
        id BIGSERIAL NOT NULL,
        name VARCHAR(512) NOT NULL,
        is_custom_emoji BOOL NOT NULL,
        is_active BOOL NOT NULL DEFAULT TRUE,
        is_animated BOOL NOT NULL,
        PRIMARY KEY (id)
    );
    CREATE TABLE IF NOT EXISTS channel (
        id BIGINT NOT NULL,
        server_id BIGINT NOT NULL,
        name VARCHAR(512),
        PRIMARY KEY (id)
    );
    CREATE TABLE IF NOT EXISTS user_ (
        id BIGINT NOT NULL,
        name VARCHAR(512),
        discriminator INTEGER,
        PRIMARY KEY (id)
    );
    CREATE TABLE IF NOT EXISTS message (
        id BIGINT,
        channel_id BIGINT NOT NULL,
        user_id BIGINT NOT NULL,
        emoji_count INTEGER NOT NULL,
        posted TIMESTAMP NOT NULL DEFAULT NOW(),
        PRIMARY KEY (id),
        FOREIGN KEY (channel_id) REFERENCES channel (id)
    );
    CREATE TABLE IF NOT EXISTS emoji_usage (
        channel_id BIGINT NOT NULL,
        user_id BIGINT NOT NULL,
        emoji_id BIGINT NOT NULL,
        use_count INTEGER NOT NULL,
        PRIMARY KEY (channel_id, emoji_id, user_id),
        FOREIGN KEY (channel_id) REFERENCES channel (id),
        FOREIGN KEY (emoji_id) REFERENCES emoji (id)
    );
    CREATE TABLE IF NOT EXISTS reactions (
        channel_id BIGINT NOT NULL,
        message_id BIGINT NOT NULL,
        user_id BIGINT NOT NULL,
        emoji_id BIGINT NOT NULL,
        PRIMARY KEY (channel_id, message_id, user_id, emoji_id),
        FOREIGN KEY (channel_id) REFERENCES channel (id),
        FOREIGN KEY (emoji_id) REFERENCES emoji (id)
    );"#;

    db_conn.batch_execute(QUERY_CREATE_TABLES)?;

    Ok(())
}

fn result_into_vec_emoji(result: Rows) -> postgres::Result<Vec<(Emoji, i64)>> {
    // row
    // column 0: is_custom_emoji
    // column 1: emoji ID
    // column 2: emoji name
    // column 3: is animated
    // column 4: use count
    let mut vec_emoji = Vec::new();

    for row in result.iter() {
        let emoji = match row.get::<usize, bool>(0) {
            true => Emoji::Custom(CustomEmoji::new(
                ServerId(0),
                discord::model::EmojiId(row.get::<usize, i64>(1) as u64),
                row.get::<usize, String>(2),
                row.get::<usize, bool>(3),
            )),
            false => Emoji::Unicode(row.get::<usize, String>(2)),
        };

        vec_emoji.push((emoji, row.get::<usize, i64>(4)));
    }

    Ok(vec_emoji)
}

fn result_into_vec_users(result: Rows) -> postgres::Result<Vec<(String, i64)>> {
    // row
    // column 0: user name
    // column 1: user discriminator
    // column 2: number of emoji used
    let mut vec_emoji = Vec::new();

    for row in result.iter() {
        let name = format!("{}", row.get::<usize, String>(0));

        vec_emoji.push((name, row.get::<usize, i64>(2)));
    }

    Ok(vec_emoji)
}
