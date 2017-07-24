extern crate config;
#[macro_use]
extern crate log;
extern crate log4rs;
extern crate nix;

mod arg;
mod bot_utility;
mod bot;

use std::env::args;
use std::ffi::CString;
use std::process;
use nix::unistd::execv;
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
    let root = log4rs::config::Root::builder()
        .appender("file")
        .build(log_level_filter);
    let config = log4rs::config::Config::builder()
        .appender(file_appender)
        .build(root)
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

    // Get bot settings and connect to Discord
    let bot_token = config.get_str("config.bot_token").unwrap_or("".to_string());
    let bot_admin_password = config
        .get_str("config.bot_admin_password")
        .unwrap_or("".to_string());
    let bot = match bot::Bot::new(&bot_token, &bot_admin_password) {
        Ok(bot) => bot,
        Err(bot_error) => process::exit(bot_error as i32),
    };
    info!("Connected to Discord successfully");

    // Perform other setup tasks

    // Begin event loop
    match bot.run() {
        BotDisposition::Quit => {}
        BotDisposition::Restart => restart(),
    }
}
