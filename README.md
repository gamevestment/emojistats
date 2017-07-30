# emojistats

A Discord bot that tracks and reports on emoji usage. Built with [discord-rs](https://github.com/SpaceManiac/discord-rs).

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

Copyright 2017 [Quailsoft](https://www.quailsoft.org/).

## Commands

To invoke a command, mention the bot at the beginning of the message; for example:

```
@EmojiStats global
```

|Command|Description|
|-|-|
|global|See global emoji statistics|
|server|See the top emoji and users on this server|
|channel|See the top emoji and users in this channel|
|me|See your favourite emoji|
|`#channel`|See the top emoji and users in that channel|
|`@user`|See the mentioned user's favourite emoji|
|*`(emoji)`*|See how many times that emoji was used|
|about|See information about the bot|
|help|See the bot commands|
|feedback &lt;message&gt;|Send feedback to the bot administrators|

Feedback is recorded to a log file and sent to bot administrators in private channels.

### Administrative commands

This bot has the four administrative commands below.

|Command|Description|
|-|-|
|auth &lt;password&gt;|Attempt to authenticate as a bot administrator using the bot administration password. This is required in order to invoke all of the other administrative commands.|
|botinfo|Display the program name, version, and uptime as well as the number of servers and public text channels to which the bot is connected.|
|restart|Attempt to restart the bot binary with the same arguments with which it was invoked.|
|quit|Halts program execution.|

## Configuration

1. Copy `config-EXAMPLE.toml` to `config.toml`.
2. Copy the bot token into `config.toml`.
3. Enter a bot administration password.

*NB: If you do not enter a bot administration password, you will be unable to shut down or restart the bot from Discord.*

*NB: Due to practical considerations, the bot administration password should not begin or end with whitespace (you will be unable to authenticate because whitespace is stripped), but it may contain whitespace between non-whitespace characters.*

```bash
[config]
bot_token = ""
bot_admin_password = ""
```

## Build notes

As of 30 July 2017, [discord](https://crates.io/crates/discord/0.8.0) relies on [websocket ^0.17](https://crates.io/crates/websocket/0.17.1), which in turn relies on [openssl ^0.7.6](https://crates.io/crates/websocket/0.17.1). If you run into difficulties with compiling [rust-openssl v0.7.x](https://github.com/sfackler/rust-openssl/blob/b8fb29db5c246175a096260eacca38180cd77dd0/README.md), try:

- `cargo clean`
- `export OPENSSL_INCLUDE_DIR=` (OpenSSL 1.0 include directory)
- `export OPENSSL_LIB_DIR=` (OpenSSL 1.0 lib directory)
- `cargo build`

The following values worked for me on Arch Linux:

```bash
export OPENSSL_INCLUDE_DIR=/usr/include/openssl-1.0
export OPENSSL_LIB_DIR=/usr/lib/openssl-1.0
```
