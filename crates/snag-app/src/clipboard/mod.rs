pub mod launch;
pub mod serve;
mod wayland;
mod x11;

use std::{error::Error, sync::Arc, time::Duration};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
type Png = Arc<Vec<u8>>;
const PNG_MIME: &str = "image/png";
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(10);

trait Owner {
    fn wait_until_lost(self: Box<Self>) -> Result<()>;
}
