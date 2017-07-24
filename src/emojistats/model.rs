extern crate discord;

pub enum Emoji {
    Custom(discord::model::Emoji),
    Unicode(char),
}
