extern crate dotenv;
#[macro_use]
extern crate log;
extern crate log4rs;
extern crate postgres;
extern crate serenity;

const PROGRAM_NAME: &'static str = env!("CARGO_PKG_NAME");
const PROGRAM_VERSION: &'static str = env!("CARGO_PKG_VERSION");
const DEFAULT_LOG_FILENAME: &str = "emojistats.log";

fn init_logging() {
    let filename = dotenv::var("ES_LOG_FILENAME").unwrap_or(DEFAULT_LOG_FILENAME.to_string());

    let file = log4rs::append::file::FileAppender::builder()
            .encoder(Box::new(log4rs::encode::pattern::PatternEncoder::new(
                    "{d(%Y-%m-%d %H:%M:%S %Z)(local)}: {h({l})}: {m}{n}")))
            .build(filename)
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
    dotenv::dotenv().ok();
    init_logging();

    info!("Started {} v{}", PROGRAM_NAME, PROGRAM_VERSION);
}
