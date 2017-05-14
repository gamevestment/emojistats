# emojistats-bot

A Discord bot that provides statistics on emoji usage.

## Commands

To use a command, mention the bot at the beginning of the message; for example:

```
@EmojiStats stats global
```

Alternatively, you may directly message the bot. When you send a direct message, you do not need to mention the bot. You may not use `stats server` or `stats channel` through direct messages.

When obtaining server, channel, or user stats in a public text channel, the stats returned will include custom emoji on that server.

When obtaining global stats or when obtaining stats for yourself in a direct message, the stats returned will be for Unicode emoji only.

### General

|           Command           |                                   Description                                   |
|-----------------------------|---------------------------------------------------------------------------------|
|`stats global`               |See the top used Unicode emoji globally                                          |
|`stats server`               |See the top used emoji on this server                                            |
|`stats channel [<#channel>]]`|See the top used emoji on the specified channel (defaults to the current channel)|
|`stats user [<@user>]`       |See the specified user's favourite emoji (defaults to yourself)                  |

### Bot control

|             Command             |                            Description                              |
|---------------------------------|---------------------------------------------------------------------|
|`auth \| authenticate <password>`|Attempts to authenticate with the bot using the bot control password.|
|`quit`                           |Shuts down the bot. You must be authenticated to use this command.   |


## Requirements

- PostgreSQL

## Configuration

1. Copy `.env.example` to `.env`.
2. Enter the PostgreSQL connection information in `.env`.
3. Copy the bot token into `.env`.
4. Enter a bot control password. To shut down the bot from Discord, you must `authenticate` using the bot control password and then use the `quit` command. If you do not enter a bot control password, you will not be able to authenticate or shut down the bot from Discord.

```bash
ES_LOG_FILENAME=emojistats.log

ES_DB_HOST=localhost
ES_DB_PORT=5432
ES_DB_USER=user
ES_DB_PASS=password
ES_DB_NAME=database_name

ES_BOT_TOKEN=Discord.Bot.Token

# Use this password to control the bot through private messages
ES_BOT_CONTROL_PASSWORD=MySuperSecretPassword
```

## Build notes

As of 13 May 2017, [discord](https://crates.io/crates/discord/0.8.0) relies on [websocket ^0.17](https://crates.io/crates/websocket/0.17.1), which in turn relies on [openssl ^0.7.6](https://crates.io/crates/websocket/0.17.1). If you run into difficulties with compiling [rust-openssl v0.7.x](https://github.com/sfackler/rust-openssl/blob/b8fb29db5c246175a096260eacca38180cd77dd0/README.md), try:

- `cargo clean`
- `export OPENSSL_INCLUDE_DIR=` (OpenSSL 1.0 include directory)
- `export OPENSSL_LIB_DIR=` (OpenSSL 1.0 lib directory)
- `cargo build`

The following values worked for me on Arch Linux:

```bash
export OPENSSL_INCLUDE_DIR=/usr/include/openssl-1.0
export OPENSSL_LIB_DIR=/usr/lib/openssl-1.0
```
