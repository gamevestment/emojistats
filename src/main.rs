extern crate dotenv;
#[macro_use]
extern crate log;
extern crate log4rs;
extern crate postgres;
extern crate serenity;

use log4rs::encode::pattern::PatternEncoder;
use std::process;

const PROGRAM_NAME: &'static str = env!("CARGO_PKG_NAME");
const PROGRAM_VERSION: &'static str = env!("CARGO_PKG_VERSION");
const LOG_FILE: &str = "emojistats.log";

fn init_logging() {
    let file = log4rs::append::file::FileAppender::builder()
            .encoder(Box::new(PatternEncoder::new(
                    "{d(%Y-%m-%d %H:%M:%S %Z)(local)}: {h({l})}: {m}{n}")))
            .build(LOG_FILE)
            .expect("Failed to create log file");

    let appender = log4rs::config::Appender::builder().build("emojistats", Box::new(file));

    let config = log4rs::config::Config::builder()
        .appender(appender)
        .build(log4rs::config::Root::builder()
                   .appender("emojistats")
                   .build(log::LogLevelFilter::Info))
        .expect("Failed to build logging configuration");

    log4rs::init_config(config).expect("Failed to initialize logger");

}

fn main() {
    init_logging();
}
