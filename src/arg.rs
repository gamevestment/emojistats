extern crate discord;

use std::fmt;

#[derive(Debug)]
pub enum Type<'a> {
    UserId(discord::model::UserId),
    ChannelId(discord::model::ChannelId),
    RoleId(discord::model::RoleId),
    EmojiId(discord::model::EmojiId),
    Text(&'a str),
}

impl<'a> fmt::Display for Type<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

// Reference: https://discordapp.com/developers/docs/resources/channel#message-formatting
pub fn get_type(arg: &str) -> Type {
    if arg.ends_with(">") {
        // Nickname
        if arg.starts_with("<@!") && arg.len() >= 5 {
            // The minimum possible length of a nickname reference is 5:
            // <@!1>
            // 01234
            match arg[3..(arg.len() - 1)].parse::<u64>() {
                Ok(id) => {
                    return Type::UserId(discord::model::UserId(id));
                }
                Err(_) => {
                    // Unable to parse as u64; fall through to Type::Text
                }
            }
        }
        // Role
        else if arg.starts_with("<@&") && arg.len() >= 5 {
            // The minimum possible length of a role reference is 5:
            // <@&1>
            // 01234
            match arg[3..(arg.len() - 1)].parse::<u64>() {
                Ok(id) => {
                    return Type::RoleId(discord::model::RoleId(id));
                }
                Err(_) => {
                    // Unable to parse as u64; fall through to Type::Text
                }
            }
        }
        // User
        else if arg.starts_with("<@") && arg.len() >= 4 {
            // The minimum possible length of a user reference is 4:
            // <@1>
            // 0123
            match arg[2..(arg.len() - 1)].parse::<u64>() {
                Ok(id) => {
                    return Type::UserId(discord::model::UserId(id));
                }
                Err(_) => {
                    // Unable to parse as u64; fall through to Type::Text
                }
            }
        }
        // Channel
        else if arg.starts_with("<#") && arg.len() >= 4 {
            // The minimum possible length of a channel reference is 4:
            // <#1>
            // 0123
            match arg[2..(arg.len() - 1)].parse::<u64>() {
                Ok(id) => {
                    return Type::ChannelId(discord::model::ChannelId(id));
                }
                Err(_) => {
                    // Unable to parse as u64; fall through to Type::Text
                }
            }
        }
        // Custom emoji
        else if arg.starts_with("<:") && arg.len() >= 6 {
            // The minimum possible length of a custom emoji reference is 6:
            // <:a:1>
            // 012345

            // The emoji ID, if there is a valid one, will begin after the
            // second colon and end at the closing angle bracket

            // The second colon, if present, will be somewhere at or after &arg[3]
            let maybe_arg = &arg[3..];
            match maybe_arg.find(":") {
                Some(pos) => {
                    // Attempt to parse the string that
                    // begins right after the colon (pos + 1) and
                    // ends just before the closing angle bracket (maybe_arg.len() - 1)
                    match maybe_arg[(pos + 1)..(maybe_arg.len() - 1)].parse::<u64>() {
                        Ok(id) => {
                            return Type::EmojiId(discord::model::EmojiId(id));
                        }
                        Err(_) => {
                            // Unable to parse as u64; fall through to Type::Text
                        }
                    }
                }
                None => {
                    // String does not contain a second colon; fall through to Type::Text
                }
            }
        }
    }

    Type::Text(arg)
}

#[cfg(test)]
mod tests {
    extern crate discord;

    use super::{get_type, Type};
    use self::discord::model::{ChannelId, EmojiId, RoleId, UserId};

    macro_rules! test {
        ($test_string:expr => Text) => {
            assert_eq!(
                match get_type($test_string) {
                    Type::Text(v) => Some(v),
                    _ => None,
                },
                Some($test_string));
        };

        ($test_string:expr => $expected_type:ident($value:expr)) => {
            assert_eq!(
                match get_type($test_string) {
                    Type::$expected_type(v) => Some(v),
                    _ => None,
                },
                Some($expected_type($value)));
        };
    }

    #[test]
    fn user_id() {
        test!("<@!1>" => UserId(1));
        test!("<@!123>" => UserId(123));
        test!("<@1>" => UserId(1));
        test!("<@123>" => UserId(123));
    }

    #[test]
    fn not_user_id() {
        test!("<!123>" => Text);
        test!("<!#123>" => Text);
        test!("<@!>" => Text);
        test!("<@!.>" => Text);
        test!("<@>" => Text);
        test!("<@1.>" => Text);
        test!("<@a>" => Text);
        test!("<@1" => Text);
    }

    #[test]
    fn channel_id() {
        test!("<#1>" => ChannelId(1));
        test!("<#123>" => ChannelId(123));
    }

    #[test]
    fn not_channel_id() {
        test!("<#>" => Text);
        test!("<#1.>" => Text);
        test!("<#1.0>" => Text);
        test!("<#a>" => Text);
        test!("<#12" => Text);
    }

    #[test]
    fn role_id() {
        test!("<@&1>" => RoleId(1));
        test!("<@&123>" => RoleId(123));
    }

    #[test]
    fn not_role_id() {
        test!("<@&>" => Text);
        test!("<@&1.>" => Text);
        test!("<@&1.0>" => Text);
        test!("<@&a>" => Text);
    }

    #[test]
    fn custom_emoji() {
        test!("<:a:1>" => EmojiId(1));
        test!("<:abc:123>" => EmojiId(123));
    }

    #[test]
    fn not_custom_emoji() {
        test!("::" => Text);
        test!(":a:" => Text);
        test!(":a:." => Text);
        test!("::1" => Text);
        test!("::1." => Text);
        test!(":a:1." => Text);
    }

    #[test]
    fn text() {
        test!("" => Text);
        test!("some text" => Text);
    }
}
