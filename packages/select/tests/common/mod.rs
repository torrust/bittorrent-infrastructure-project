//----------------------------------------------------------------------------------//

use std::sync::Once;

use tracing::level_filters::LevelFilter;

#[allow(dead_code)]
pub static INIT: Once = Once::new();

#[allow(dead_code)]
#[derive(PartialEq, Eq, Debug)]
pub enum TimeoutResult {
    TimedOut,
    GotResult,
}

#[allow(dead_code)]
pub fn tracing_stderr_init(filter: LevelFilter) {
    let builder = tracing_subscriber::fmt()
        .with_max_level(filter)
        .with_ansi(true)
        .with_writer(std::io::stderr);

    builder.pretty().with_file(true).init();

    tracing::info!("Logging initialized");
}
