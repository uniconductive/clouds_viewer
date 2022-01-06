pub type RuntimeHolder = std::sync::Arc<std::sync::RwLock<Option<tokio::runtime::Runtime>>>;
pub type EventsSender<T> = tokio::sync::mpsc::UnboundedSender<T>;
pub type EventsReceiver<T> = tokio::sync::mpsc::UnboundedReceiver<T>;

