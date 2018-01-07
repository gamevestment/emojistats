extern crate discord;

use arg;
use self::discord::Discord;
use self::discord::model::{ChannelId, LiveServer, Message, ServerId, ServerInfo};

pub struct BasicServerInfo {
    pub id: ServerId,
    pub name: String,
    pub icon: Option<String>,
}

impl From<LiveServer> for BasicServerInfo {
    fn from(live_server: LiveServer) -> Self {
        BasicServerInfo {
            id: live_server.id,
            name: live_server.name,
            icon: live_server.icon,
        }
    }
}

impl From<ServerInfo> for BasicServerInfo {
    fn from(server_info: ServerInfo) -> Self {
        BasicServerInfo {
            id: server_info.id,
            name: server_info.name,
            icon: server_info.icon,
        }
    }
}

pub trait MessageRecipient {
    fn send_message(&self, discord: &Discord, message: &str);
}

impl MessageRecipient for ChannelId {
    fn send_message(&self, discord: &Discord, message: &str) {
        let _ = discord.send_message(*self, message, "", false);
    }
}

impl MessageRecipient for Message {
    fn send_message(&self, discord: &Discord, message: &str) {
        let _ = discord.send_message(self.channel_id, message, "", false);
    }
}

// If s begins with a valid arg::Type::UserId, arg::Type::ChannelId, arg::Type::RoleId, or
// arg::Type::EmojiId, returns (that arg, the string without the arg)
// Otherwise, returns (None, s)
pub fn extract_preceding_arg<'a>(s: &'a str) -> (Option<arg::Type<'a>>, &str) {
    if s.trim_left().starts_with("<") && s.contains(">") {
        let maybe_end_of_arg = s.find(">").unwrap() + 1;
        let (maybe_arg, maybe_rest) = s.split_at(maybe_end_of_arg);

        match arg::get_type(maybe_arg.trim()) {
            arg::Type::Text(_) => {}
            arg => {
                return (Some(arg), maybe_rest);
            }
        }
    }
    (None, s)
}

// Removes all characters not used in commands from the beginning of a &str
pub fn remove_non_command_characters(s: &str) -> &str {
    let mut s_chars = s.chars();
    let mut skip_pos = 0;

    while let Some(c) = s_chars.next() {
        if c.is_whitespace() || c == ',' {
            skip_pos += c.len_utf8();
        } else {
            break;
        }
    }

    s.split_at(skip_pos).1
}

// Splits a &str into two parts: (The first word preceding whitespace, everything else)
// Trims whitespace at the beginning if there is any
// Used to split a command from its arguments
pub fn extract_first_word(s: &str) -> (&str, &str) {
    let s = s.trim_left();

    let mut s_chars = s.chars();
    let mut end_of_first_word = 0;

    while let Some(c) = s_chars.next() {
        if !c.is_whitespace() {
            end_of_first_word += c.len_utf8();
        } else {
            break;
        }
    }

    let (first_word, the_rest) = s.split_at(end_of_first_word);
    let the_rest = the_rest.trim_left();

    (first_word, the_rest)
}

mod tests {
    #[allow(unused_imports)]
    use super::{extract_first_word, extract_preceding_arg, remove_non_command_characters};
    #[allow(unused_imports)]
    use super::discord::model::{ChannelId, EmojiId, RoleId, UserId};
    #[allow(unused_imports)]
    use super::arg::Type;

    #[test]
    fn test_extract_preceding_arg() {
        macro_rules! test {
            ($test_string:expr => (None, $expected_command:expr)) => {
                let (arg, expected_command) = extract_preceding_arg($test_string);
                assert_eq!(arg.is_none(), true);
                assert_eq!(expected_command, $expected_command);
            };

            ($test_string:expr => ($expected_type:ident($value:expr), $expected_command:expr)) => {
                let (arg, expected_command) = extract_preceding_arg($test_string);
                assert_eq!(
                    match arg {
                        Some(Type::$expected_type(v)) => Some(v),
                        _ => None,
                    },
                    Some($expected_type($value)));
                assert_eq!(expected_command, $expected_command);
            };
        }

        test!("  abc  " => (None, "  abc  "));
        test!("  <@>  abc  " => (None, "  <@>  abc  "));
        test!("  <@123>  abc  " => (UserId(123), "  abc  "));
        test!("  <@!123>  abc  " => (UserId(123), "  abc  "));
        test!("  <@&123>  abc  " => (RoleId(123), "  abc  "));
        test!("  <#123>  abc  " => (ChannelId(123), "  abc  "));
        test!("  <:emoji:123>  abc  " => (EmojiId(123), "  abc  "));
    }

    #[test]
    fn test_remove_non_command_characters() {
        macro_rules! test {
            ($test_string:expr => $expected_value:expr) => {
                assert_eq!(remove_non_command_characters($test_string), $expected_value);
            };
        }

        test!("abcd " => "abcd ");
        test!(".abcd " => "abcd ");
        test!(".-_abcd " => "-_abcd ");
        test!("   _abcd " => "_abcd ");
        test!("  - . _abcd " => "- . _abcd ");
    }

    #[test]
    fn test_extract_first_word() {
        macro_rules! test {
            ($test_string:expr => ($s1:expr, $s2:expr)) => {
                assert_eq!(extract_first_word($test_string), ($s1, $s2));
            };
        }

        test!("" => ("", ""));
        test!(" \t " => ("", ""));
        test!(" ab " => ("ab", ""));
        test!(" ab \t " => ("ab", ""));
        test!("ab cd" => ("ab", "cd"));
        test!("ab  cd \t " => ("ab", "cd \t "));
    }
}
