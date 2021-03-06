extern crate config;
#[macro_use]
extern crate log;
extern crate log4rs;
extern crate nix;
extern crate postgres;

mod arg;
mod bot_utility;
mod emojistats;
mod bot;

use std::env::args;
use std::ffi::CString;
use std::process;
use nix::unistd::execv;
use log4rs::config::Logger;
use emojistats::Database;
use bot::BotDisposition;

const PROGRAM_NAME: &str = env!("CARGO_PKG_NAME");
const PROGRAM_VERSION: &str = env!("CARGO_PKG_VERSION");
const LOG_FILENAME: &str = "emojistats.log";
const LOG_FORMAT: &str = "{d(%Y-%m-%d %H:%M:%S %Z)(local)}: [{M}] {h([{l}])} {m}{n}";

enum ExitStatus {
    UnableToObtainConfig = 10,
    UnableToObtainExecutablePath = 11,
    UnableToRestart = 12,
    UnableToConvertCString = 13,
    UnableToCreateDatabaseConnection = 21,
}

// Initialize log4rs to log to LOG_FILENAME
fn init_logging() {
    let filename = LOG_FILENAME;

    // Set log level based on whether the program was built for debug or release
    let log_level_filter = if cfg!(debug_assertions) {
        log::LogLevelFilter::Debug
    } else {
        log::LogLevelFilter::Info
    };

    // Build appender for file log
    let file = log4rs::append::file::FileAppender::builder()
        .encoder(Box::new(log4rs::encode::pattern::PatternEncoder::new(LOG_FORMAT)))
        .build(filename)
        .expect("Failed to create log file");
    let file_appender = log4rs::config::Appender::builder().build("file", Box::new(file));

    // Put everything together
    let config = log4rs::config::Config::builder()
        .appender(file_appender)
        // No need to obtain debug detail for postgres
        .logger(Logger::builder()
                    .appender("file")
                    .build("postgres", log::LogLevelFilter::Info))
        .logger(Logger::builder()
                    .appender("file")
                    .build("discord", log_level_filter))
        .logger(Logger::builder()
                    .appender("file")
                    .build(PROGRAM_NAME, log_level_filter))
        .build(log4rs::config::Root::builder().build(log::LogLevelFilter::Off))
        .expect("Failed to build logging configuration");

    log4rs::init_config(config).expect("Failed to initialize log4rs");
}

fn str_ref_to_cstring(s: &str, string_descr: &str) -> CString {
    match CString::new(s) {
        Ok(cs) => cs,
        Err(reason) => {
            error!("Failed to convert {} \"{}\" to CString: {}",
                   string_descr,
                   s,
                   reason);
            process::exit(ExitStatus::UnableToConvertCString as i32);
        }
    }
}

fn restart() {
    let mut env_args = args();

    // Attempt to convert the executable path to a CString
    let exec_path = match env_args.next() {
        Some(exec_path) => exec_path,
        None => {
            error!("Unable to obtain executable path");
            process::exit(ExitStatus::UnableToObtainExecutablePath as i32);
        }
    };
    let exec_path_cstring = str_ref_to_cstring(&exec_path, "executable path");

    // Create a vector of the program arguments to pass on
    let mut execv_args = Vec::<CString>::new();
    execv_args.push(exec_path_cstring.clone());

    // Attempt to convert all program arguments to CStrings
    for arg in env_args {
        execv_args.push(str_ref_to_cstring(&arg, "program argument"));
    }

    match execv(&exec_path_cstring, &execv_args) {
        Err(reason) => {
            error!("Unable to restart \"{}\": {}", exec_path, reason);
            process::exit(ExitStatus::UnableToRestart as i32);
        }
        _ => {}
    }
}

fn load_unicode_emoji(config: &config::Config, bot: &mut bot::Bot) {
    let mut num_emoji_loaded = 0;

    if let Ok(emoji_list) = config.get_array("emojistats.emoji") {
        for emoji_value in emoji_list {
            if let Ok(emoji) = emoji_value.into_str() {
                bot.add_unicode_emoji(emoji);
                num_emoji_loaded += 1;
            }
        }
    }

    info!("Loaded {} Unicode emoji from config", num_emoji_loaded);
}

fn main() {
    init_logging();
    info!("Starting {} (version {}).", PROGRAM_NAME, PROGRAM_VERSION);

    // Discard nth(0), which is the name of the program
    // Use nth(1) as the config filename (without the suffix), or if it is not present, use "config"
    let config_filename = &args().nth(1).unwrap_or("config".to_string());

    // Attempt to load the config file
    let mut maybe_config = config::Config::new();
    let maybe_config = maybe_config.merge(config::File::with_name(&config_filename));
    let config = if maybe_config.is_ok() {
        maybe_config.unwrap()
    } else {
        error!("Unable to open any config files beginning with \"{}\".",
               config_filename);
        process::exit(ExitStatus::UnableToObtainConfig as i32);
    };

    // Get database settings and connect to the database
    let mut db_conn_params_builder = postgres::params::Builder::new();

    if let Ok(port) = config.get_int("database.port") {
        db_conn_params_builder.port(port as u16);
    }

    if let Ok(user) = config.get_str("database.username") {
        db_conn_params_builder.user(&user,
                                    config
                                        .get_str("database.password")
                                        .ok()
                                        .as_ref()
                                        .map(String::as_str));
    }

    if let Ok(database_name) = config.get_str("database.name") {
        db_conn_params_builder.database(&database_name);
    }

    let db_conn_params =
        db_conn_params_builder
            .build(postgres::params::Host::Tcp(config
                                                   .get_str("database.hostname")
                                                   .unwrap_or("localhost".to_string())));

    let db = match Database::new(db_conn_params) {
        Ok(db) => db,
        Err(reason) => {
            error!("Unable to connect to database: {}", reason);
            process::exit(ExitStatus::UnableToCreateDatabaseConnection as i32);
        }
    };

    // Get bot settings and connect to Discord
    let bot_token = config.get_str("config.bot_token").unwrap_or("".to_string());
    let bot_admin_password = config
        .get_str("config.bot_admin_password")
        .unwrap_or("".to_string());
    let mut bot = match bot::Bot::new(&bot_token, &bot_admin_password, db) {
        Ok(bot) => bot,
        Err(bot_error) => process::exit(bot_error as i32),
    };
    info!("Connected to Discord successfully");

    // Perform other setup tasks
    if let Ok(about_text) = config.get_str("config.about_text") {
        bot.set_about_text(about_text);
    }

    if let Ok(help_text) = config.get_str("config.help_text") {
        bot.set_help_text(help_text);
    }

    if let Ok(feedback_filename) = config.get_str("config.feedback_filename") {
        bot.set_feedback_file(feedback_filename);
    }
    load_unicode_emoji(&config, &mut bot);

    // Begin event loop
    match bot.run() {
        BotDisposition::Quit => {}
        BotDisposition::Restart => restart(),
    }
}
