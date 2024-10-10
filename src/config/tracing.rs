use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub fn subscribe_tracing() {
    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_file(true)
                .with_line_number(true)
                .with_thread_ids(true),
        )
        .with(
            EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into())
                .add_directive("sqlx=off".parse().unwrap()),
        )
        .init();
}
