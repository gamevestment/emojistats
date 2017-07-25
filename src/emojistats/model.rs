extern crate discord;

use self::discord::model::{EmojiId, ServerId};

#[derive(Debug)]
pub enum Emoji {
    Custom(CustomEmoji),
    Unicode(String), // Some emoji span multiple chars
}

#[derive(Debug)]
pub struct CustomEmoji {
    server_id: ServerId,
    id: EmojiId,
    name: String,
    pattern: String,
}

impl CustomEmoji {
    pub fn new<S>(server_id: ServerId, id: EmojiId, name: S) -> CustomEmoji
        where S: Into<String>
    {
        let name = name.into();
        let pattern = format!("<:{}:{}>", id, name);

        CustomEmoji {
            server_id,
            id,
            name,
            pattern,
        }
    }
}
