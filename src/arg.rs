extern crate discord;

use std::fmt;

#[derive(Debug)]
pub enum Type<'a> {
    UserId(discord::model::UserId),
    ChannelId(discord::model::ChannelId),
    CustomEmoji(discord::model::EmojiId),
    Text(&'a str),
}

impl<'a> fmt::Display for Type<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

// See: https://discordapp.com/developers/docs/resources/channel#message-formatting
pub fn get_type(arg: &str) -> Type {
    if arg.ends_with(">") {
        if arg.starts_with("<!@") {
            match arg[3..(arg.len() - 1)].parse::<u64>() {
                Ok(id) => {
                    return Type::UserId(discord::model::UserId(id));
                }
                Err(_) => {
                    // Fall through to Type::Text
                }
            }
        } else if arg.starts_with("<@") {
            match arg[2..(arg.len() - 1)].parse::<u64>() {
                Ok(id) => {
                    return Type::UserId(discord::model::UserId(id));
                }
                Err(_) => {
                    // Fall through to Type::Text
                }
            }
        } else if arg.starts_with("<#") {
            match arg[2..(arg.len() - 1)].parse::<u64>() {
                Ok(id) => {
                    return Type::ChannelId(discord::model::ChannelId(id));
                }
                Err(_) => {
                    // Fall through to Type::Text
                }
            }
        } else if arg.starts_with("<:") && arg.len() >= 6 {
            // The minimum possible length of a custom emoji reference is 6:
            // <:a:1>
            // 012345

            // The emoji ID, if there is a valid one, will begin after the
            // second colon and end at the closing angle bracket

            // The second colon will be somewhere at or after &arg[3]
            let maybe_arg = &arg[3..];
            match maybe_arg.find(":") {
                Some(pos) => {
                    match maybe_arg[(pos + 1)..(maybe_arg.len() - 1)].parse::<u64>() {
                        Ok(id) => {
                            return Type::CustomEmoji(discord::model::EmojiId(id));
                        }
                        Err(_) => {}
                    }
                }
                None => {
                    // Fall through to Type::Text
                }
            }
        }
    }

    Type::Text(arg)
}

#[cfg(test)]
mod tests {
    extern crate discord;
    use super::Type;

    #[test]
    fn user_id() {
        assert_eq!(Some(discord::model::UserId(1)),
                   match super::get_type("<!@1>") {
                       Type::UserId(id) => Some(id),
                       _ => None,
                   });

        assert_eq!(Some(discord::model::UserId(123)),
                   match super::get_type("<!@123>") {
                       Type::UserId(id) => Some(id),
                       _ => None,
                   });

        assert_eq!(Some(discord::model::UserId(1)),
                   match super::get_type("<@1>") {
                       Type::UserId(id) => Some(id),
                       _ => None,
                   });

        assert_eq!(Some(discord::model::UserId(123)),
                   match super::get_type("<@123>") {
                       Type::UserId(id) => Some(id),
                       _ => None,
                   });
    }

    #[test]
    fn not_user_id() {
        assert_eq!(Some("<!123>"), match super::get_type("<!123>") {
            Type::Text(text) => Some(text),
            _ => None,
        });

        assert_eq!(Some("<!#123>"), match super::get_type("<!#123>") {
            Type::Text(text) => Some(text),
            _ => None,
        });

        assert_eq!(Some("<!@>"), match super::get_type("<!@>") {
            Type::Text(text) => Some(text),
            _ => None,
        });

        assert_eq!(Some("<!@.>"), match super::get_type("<!@.>") {
            Type::Text(text) => Some(text),
            _ => None,
        });

        assert_eq!(Some("<@>"), match super::get_type("<@>") {
            Type::Text(text) => Some(text),
            _ => None,
        });

        assert_eq!(Some("<@1.>"), match super::get_type("<@1.>") {
            Type::Text(text) => Some(text),
            _ => None,
        });
    }

    #[test]
    fn channel_id() {
        assert_eq!(Some(discord::model::ChannelId(1)),
                   match super::get_type("<#1>") {
                       Type::ChannelId(id) => Some(id),
                       _ => None,
                   });

        assert_eq!(Some(discord::model::ChannelId(123)),
                   match super::get_type("<#123>") {
                       Type::ChannelId(id) => Some(id),
                       _ => None,
                   });
    }

    #[test]
    fn not_channel_id() {
        assert_eq!(Some("<#>"), match super::get_type("<#>") {
            Type::Text(text) => Some(text),
            _ => None,
        });

        assert_eq!(Some("<#1.0>"), match super::get_type("<#1.0>") {
            Type::Text(text) => Some(text),
            _ => None,
        });

    }

    #[test]
    fn custom_emoji() {
        assert_eq!(Some(discord::model::EmojiId(1)),
                   match super::get_type("<:a:1>") {
                       Type::CustomEmoji(id) => Some(id),
                       _ => None,
                   });

        assert_eq!(Some(discord::model::EmojiId(123)),
                   match super::get_type("<:abc:123>") {
                       Type::CustomEmoji(id) => Some(id),
                       _ => None,
                   });
    }

    #[test]
    fn not_custom_emoji() {
        assert_eq!(Some("<::>"), match super::get_type("<::>") {
            Type::Text(text) => Some(text),
            _ => None,
        });

        assert_eq!(Some("<:a:>"), match super::get_type("<:a:>") {
            Type::Text(text) => Some(text),
            _ => None,
        });

        assert_eq!(Some("<:a:.>"), match super::get_type("<:a:.>") {
            Type::Text(text) => Some(text),
            _ => None,
        });

        assert_eq!(Some("<:a:1.>"), match super::get_type("<:a:1.>") {
            Type::Text(text) => Some(text),
            _ => None,
        });

        assert_eq!(Some("<::1>"), match super::get_type("<::1>") {
            Type::Text(text) => Some(text),
            _ => None,
        });
    }

    #[test]
    fn text() {
        assert_eq!(Some("some text"), match super::get_type("some text") {
            Type::Text(text) => Some(text),
            _ => None,
        });

        assert_eq!(Some(""), match super::get_type("") {
            Type::Text(text) => Some(text),
            _ => None,
        });
    }
}
