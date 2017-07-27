extern crate discord;
extern crate postgres;

use self::discord::model::{ChannelId, MessageId, PublicChannel, ServerId, ServerInfo, User, UserId};
use super::model::Emoji;

#[allow(dead_code)]
pub struct Database {
    conn: postgres::Connection,
}

#[allow(dead_code)]
impl Database {
    pub fn new<T>(params: T) -> postgres::Result<Database>
        where T: postgres::params::IntoConnectParams
    {
        let conn = postgres::Connection::connect(params, postgres::TlsMode::None)?;
        create_tables(&conn)?;

        Ok(Database { conn })
    }

    pub fn add_server(&self, _server_info: &ServerInfo) -> postgres::Result<()> {
        Ok(())
    }

    pub fn add_channel(&self, _channel: &PublicChannel) -> postgres::Result<()> {
        Ok(())
    }

    pub fn add_user(&self, _user: &User) -> postgres::Result<()> {
        Ok(())
    }

    pub fn add_emoji(&self, emoji: &Emoji) -> postgres::Result<()> {
        const QUERY_INSERT_CUSTOM_EMOJI: &str = r#"
        INSERT INTO emoji (id, name, is_custom_emoji)
        VALUES ($1, $2, TRUE)
        ON CONFLICT (id) DO UPDATE
            SET name = excluded.name;"#;

        const QUERY_INSERT_UNICODE_EMOJI: &str = r#"
        INSERT INTO emoji (name, is_custom_emoji)
        VALUES ($1, FALSE);"#;

        match *emoji {
            Emoji::Custom(ref emoji) => {
                self.conn
                    .execute(QUERY_INSERT_CUSTOM_EMOJI,
                             &[&(emoji.id.0 as i64), &emoji.name])?;
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

    pub fn message_exists(&self, _message_id: &MessageId) -> postgres::Result<bool> {
        Ok(false)
    }

    pub fn record_emoji_usage(&self,
                              channel_id: &ChannelId,
                              user_id: &UserId,
                              emoji: &Emoji,
                              count: i64)
                              -> postgres::Result<()> {
        match *emoji {
            Emoji::Custom(ref custom_emoji) => {
                debug!("Custom emoji {} used {} time{} by {} in channel {}",
                       custom_emoji.pattern,
                       count,
                       if count == 1 { "" } else { "s" },
                       user_id,
                       channel_id);
            }
            Emoji::Unicode(ref emoji) => {
                debug!("Emoji {} used {} time{} by {} in channel {}",
                       emoji,
                       count,
                       if count == 1 { "" } else { "s" },
                       user_id,
                       channel_id);
            }
        }

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
        Ok(Vec::new())
    }

    pub fn get_server_top_emoji(&self,
                                _server_id: &ServerId)
                                -> postgres::Result<Vec<(Emoji, i64)>> {
        Ok(Vec::new())
    }

    pub fn get_channel_top_emoji(&self,
                                 _channel_id: &ChannelId)
                                 -> postgres::Result<Vec<(Emoji, i64)>> {
        Ok(Vec::new())
    }

    pub fn get_user_top_emoji(&self, _user_id: &UserId) -> postgres::Result<Vec<(Emoji, i64)>> {
        Ok(Vec::new())
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
    );"#;

    db_conn.batch_execute(QUERY_CREATE_TABLES)?;

    Ok(())
}
