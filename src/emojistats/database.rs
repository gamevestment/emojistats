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

    pub fn add_emoji(&self, _emoji: &Emoji) -> postgres::Result<()> {
        Ok(())
    }

    pub fn message_exists(&self, _message_id: &MessageId) -> postgres::Result<bool> {
        Ok(false)
    }

    pub fn record_emoji_usage(&self,
                              _channel_id: &ChannelId,
                              _user_id: &UserId,
                              _emoji: Emoji,
                              _count: i64)
                              -> postgres::Result<()> {
        Ok(())
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

fn create_tables(_db_conn: &postgres::Connection) -> postgres::Result<()> {
    Ok(())
}
