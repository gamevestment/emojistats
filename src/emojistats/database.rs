extern crate discord;
extern crate postgres;

use std::collections::HashMap;
use self::discord::model::{ChannelId, MessageId, PublicChannel, ServerId, User, UserId};
use super::model::{Emoji, CustomEmoji};
use postgres::rows::Rows;

#[allow(dead_code)]
pub struct Database {
    conn: postgres::Connection,
    unicode_emoji_ids: HashMap<String, i64>,
}

#[allow(dead_code)]
impl Database {
    pub fn new<T>(params: T) -> postgres::Result<Database>
        where T: postgres::params::IntoConnectParams
    {
        let conn = postgres::Connection::connect(params, postgres::TlsMode::None)?;
        create_tables(&conn)?;

        Ok(Database {
               conn,
               unicode_emoji_ids: HashMap::new(),
           })
    }

    pub fn add_channel(&self, channel: &PublicChannel) -> postgres::Result<()> {
        const QUERY_INSERT_CHANNEL: &str = r#"
        INSERT INTO channel (id, server_id, name)
        VALUES ($1, $2, $3)
        ON CONFLICT (id) DO UPDATE
            SET name = excluded.name;"#;

        self.conn
            .execute(QUERY_INSERT_CHANNEL,
                     &[&(channel.id.0 as i64),
                       &(channel.server_id.0 as i64),
                       &channel.name])?;

        Ok(())
    }

    pub fn add_user(&self, _user: &User) -> postgres::Result<()> {
        Ok(())
    }

    pub fn add_emoji(&self, emoji: &Emoji, server_id: Option<&ServerId>) -> postgres::Result<()> {
        const QUERY_INSERT_CUSTOM_EMOJI: &str = r#"
        INSERT INTO emoji (server_id, id, name, is_custom_emoji)
        VALUES ($1, $2, $3, TRUE)
        ON CONFLICT (id) DO UPDATE
            SET name = excluded.name;"#;

        const QUERY_INSERT_UNICODE_EMOJI: &str = r#"
        INSERT INTO emoji (server_id, name, is_custom_emoji)
        VALUES (NULL, $1, FALSE);"#;

        match *emoji {
            Emoji::Custom(ref emoji) => {
                self.conn
                    .execute(QUERY_INSERT_CUSTOM_EMOJI,
                             &[&(server_id.unwrap().0 as i64),
                               &(emoji.id.0 as i64),
                               &emoji.name])?;
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

    pub fn message_exists(&self, message_id: &MessageId) -> postgres::Result<bool> {
        const QUERY_GET_MESSAGE_EXIST: &str = r#"
        SELECT id
        FROM message
        WHERE id = $1;"#;

        let result = self.conn
            .query(QUERY_GET_MESSAGE_EXIST, &[&(message_id.0 as i64)])?;

        Ok(result.len() != 0)
    }

    pub fn record_message_stats(&self,
                                message_id: &MessageId,
                                channel_id: &ChannelId,
                                user_id: &UserId,
                                emoji_count: i32)
                                -> postgres::Result<()> {
        const QUERY_RECORD_MESSAGE_STATS: &str = r#"
        INSERT INTO message (id, channel_id, user_id, emoji_count)
        VALUES ($1, $2, $3, $4);"#;

        self.conn
            .execute(QUERY_RECORD_MESSAGE_STATS,
                     &[&(message_id.0 as i64),
                       &(channel_id.0 as i64),
                       &(user_id.0 as i64),
                       &emoji_count])?;

        Ok(())
    }

    pub fn record_emoji_usage(&self,
                              channel_id: &ChannelId,
                              user_id: &UserId,
                              emoji: &Emoji,
                              count: i32)
                              -> postgres::Result<()> {
        const QUERY_RECORD_EMOJI_USAGE: &str =
            r#"
        INSERT INTO emoji_usage (channel_id, user_id, emoji_id, use_count)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (channel_id, user_id, emoji_id) DO UPDATE
            SET use_count = emoji_usage.use_count + excluded.use_count;"#;

        let emoji_id = match *emoji {
            Emoji::Custom(ref custom_emoji) => {
                debug!("Custom emoji {} used {} time{} by {} in channel {}",
                       custom_emoji.pattern,
                       count,
                       if count == 1 { "" } else { "s" },
                       user_id,
                       channel_id);
                custom_emoji.id.0 as i64
            }
            Emoji::Unicode(ref emoji) => {
                debug!("Emoji {} used {} time{} by {} in channel {}",
                       emoji,
                       count,
                       if count == 1 { "" } else { "s" },
                       user_id,
                       channel_id);
                self.get_emoji_id(emoji.clone())?.unwrap() as i64
            }
        };

        self.conn
            .execute(QUERY_RECORD_EMOJI_USAGE,
                     &[&(channel_id.0 as i64),
                       &(user_id.0 as i64),
                       &emoji_id,
                       &count])?;

        Ok(())
    }

    pub fn get_emoji_id<S>(&self, name: S) -> postgres::Result<Option<u64>>
        where S: Into<String>
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
        SELECT e.is_custom_emoji, e.id, e.name, SUM(eu.use_count)
        FROM emoji_usage eu
            INNER JOIN emoji e ON eu.emoji_id = e.id
        WHERE e.is_custom_emoji = FALSE
        GROUP BY e.is_custom_emoji, e.id, e.name
        ORDER BY SUM(eu.use_count) DESC
        LIMIT 5;"#;

        let result = self.conn.query(QUERY_SELECT_TOP_GLOBAL_EMOJI, &[])?;

        Ok(result_into_vec_emoji(result)?)
    }

    pub fn get_server_top_emoji(&self,
                                server_id: &ServerId)
                                -> postgres::Result<Vec<(Emoji, i64)>> {
        const QUERY_SELECT_TOP_SERVER_EMOJI: &str = r#"
        SELECT e.is_custom_emoji, e.id, e.name, SUM(eu.use_count)
        FROM emoji_usage eu
            INNER JOIN emoji e ON eu.emoji_id = e.id
            INNER JOIN channel c ON eu.channel_id = c.id
        WHERE c.server_id = $1
        GROUP BY e.is_custom_emoji, e.id, e.name
        ORDER BY SUM(eu.use_count) DESC
        LIMIT 5;"#;

        let result = self.conn
            .query(QUERY_SELECT_TOP_SERVER_EMOJI, &[&(server_id.0 as i64)])?;

        Ok(result_into_vec_emoji(result)?)
    }

    pub fn get_channel_top_emoji(&self,
                                 channel_id: &ChannelId)
                                 -> postgres::Result<Vec<(Emoji, i64)>> {
        const QUERY_SELECT_TOP_CHANNEL_EMOJI: &str = r#"
        SELECT e.is_custom_emoji, e.id, e.name, SUM(eu.use_count)
        FROM emoji_usage eu
            INNER JOIN emoji e ON eu.emoji_id = e.id
        WHERE eu.channel_id = $1
        GROUP BY e.is_custom_emoji, e.id, e.name
        ORDER BY SUM(eu.use_count) DESC
        LIMIT 5;"#;

        let result = self.conn
            .query(QUERY_SELECT_TOP_CHANNEL_EMOJI, &[&(channel_id.0 as i64)])?;

        Ok(result_into_vec_emoji(result)?)
    }

    pub fn get_user_top_emoji(&self,
                              user_id: &UserId,
                              server_id: Option<&ServerId>)
                              -> postgres::Result<Vec<(Emoji, i64)>> {
        const QUERY_SELECT_TOP_USER_UNICODE_EMOJI: &str = r#"
        SELECT e.is_custom_emoji, e.id, e.name, SUM(eu.use_count)
        FROM emoji_usage eu
            INNER JOIN emoji e ON eu.emoji_id = e.id
        WHERE e.is_custom_emoji = FALSE AND eu.user_id = $1
        GROUP BY e.is_custom_emoji, e.id, e.name
        ORDER BY SUM(eu.use_count) DESC
        LIMIT 5;"#;

        const QUERY_SELECT_TOP_USER_SERVER_EMOJI: &str = r#"
        SELECT e.is_custom_emoji, e.id, e.name, SUM(eu.use_count)
        FROM emoji_usage eu
            INNER JOIN emoji e ON eu.emoji_id = e.id
        WHERE (eu.user_id = $1) AND (e.server_id IS NULL OR e.server_id = $2)
        GROUP BY e.is_custom_emoji, e.id, e.name
        ORDER BY SUM(eu.use_count) DESC
        LIMIT 5;"#;

        let result = match server_id {
            Some(server_id) => {
                debug!("Usre {} server {}", user_id, server_id);
                self.conn
                    .query(QUERY_SELECT_TOP_USER_SERVER_EMOJI,
                           &[&(user_id.0 as i64), &(server_id.0 as i64)])?
            }
            None => {
                self.conn
                    .query(QUERY_SELECT_TOP_USER_UNICODE_EMOJI, &[&(user_id.0 as i64)])?
            }
        };

        Ok(result_into_vec_emoji(result)?)
    }

    pub fn get_server_top_users(&self,
                                _server_id: &ServerId)
                                -> postgres::Result<Vec<(UserId, i64)>> {
        Ok(Vec::new())
    }

    pub fn get_channel_top_users(&self,
                                 _channel_id: &ChannelId)
                                 -> postgres::Result<Vec<(UserId, i64)>> {
        Ok(Vec::new())
    }

    pub fn get_user_fav_emoji(&self,
                              _user_id: &UserId,
                              _server_id: &ServerId)
                              -> postgres::Result<Vec<(Emoji, i64)>> {
        Ok(Vec::new())
    }

    pub fn get_user_fav_unicode_emoji(&self,
                                      _user_id: &UserId)
                                      -> postgres::Result<Vec<(Emoji, i64)>> {
        Ok(Vec::new())
    }
}

fn create_tables(db_conn: &postgres::Connection) -> postgres::Result<()> {
    const QUERY_CREATE_TABLES: &str = r#"
    CREATE TABLE IF NOT EXISTS emoji (
        server_id BIGINT NULL,
        id BIGSERIAL NOT NULL,
        name VARCHAR(512) NOT NULL,
        is_custom_emoji BOOL NOT NULL,
        PRIMARY KEY (id)
    );
    CREATE TABLE IF NOT EXISTS channel (
        id BIGINT NOT NULL,
        server_id BIGINT NOT NULL,
        name VARCHAR(512),
        PRIMARY KEY (id)
    );
    CREATE TABLE IF NOT EXISTS message (
        id BIGINT,
        channel_id BIGINT NOT NULL,
        user_id BIGINT NOT NULL,
        emoji_count INTEGER NOT NULL,
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
    );"#;

    db_conn.batch_execute(QUERY_CREATE_TABLES)?;

    Ok(())
}

fn result_into_vec_emoji(result: Rows) -> postgres::Result<Vec<(Emoji, i64)>> {
    // row
    // column 0: is_custom_emoji
    // column 1: emoji ID
    // column 2: emoji name
    // column 3: use count
    let mut vec_emoji = Vec::new();

    for row in result.iter() {
        let emoji = match row.get::<usize, bool>(0) {
            true => {
                Emoji::Custom(CustomEmoji::new(ServerId(0),
                                               discord::model::EmojiId(row.get::<usize, i64>(1) as
                                                                       u64),
                                               row.get::<usize, String>(2)))
            }
            false => Emoji::Unicode(row.get::<usize, String>(2)),
        };

        vec_emoji.push((emoji, row.get::<usize, i64>(3)));
    }

    Ok(vec_emoji)
}
