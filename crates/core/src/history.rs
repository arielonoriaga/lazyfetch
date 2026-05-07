use crate::exec::Executed;

pub trait HistoryRepo: Send + Sync {
    fn append(&self, e: &Executed) -> std::io::Result<()>;
    fn tail(&self, n: usize) -> std::io::Result<Vec<Executed>>;
}
