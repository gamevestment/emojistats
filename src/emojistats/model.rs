extern crate discord;

use std::hash::{Hash, Hasher};
use self::discord::model::{EmojiId, ServerId};

#[derive(Debug, Hash, PartialEq, Eq)]
pub enum Emoji {
    Custom(CustomEmoji),
    Unicode(String), // Some emoji span multiple chars
}

#[derive(Debug, Eq)]
pub struct CustomEmoji {
    pub server_id: ServerId,
    pub id: EmojiId,
    pub name: String,
    pub pattern: String,
}

impl Hash for CustomEmoji {
    fn hash<H>(&self, state: &mut H)
        where H: Hasher
    {
        self.id.hash(state);
    }
}

impl PartialEq for CustomEmoji {
    fn eq(&self, other: &CustomEmoji) -> bool {
        self.id == other.id
    }
}

impl CustomEmoji {
    pub fn new<S>(server_id: ServerId, id: EmojiId, name: S) -> CustomEmoji
        where S: Into<String>
    {
        let name = name.into();
        let pattern = format!("<:{}:{}>", name, id);

        CustomEmoji {
            server_id,
            id,
            name,
            pattern,
        }
    }
}
