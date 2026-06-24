use tracing::Level;
use tracing_subscriber::FmtSubscriber;

pub fn init_logging(level: Level) -> Result<(), tracing::subscriber::SetGlobalDefaultError> {
    let subscriber = FmtSubscriber::builder().with_max_level(level).finish();
    tracing::subscriber::set_global_default(subscriber)
}
