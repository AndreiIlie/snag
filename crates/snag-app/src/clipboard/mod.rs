pub mod launch;
pub mod serve;
mod wayland;
mod x11;

use std::{error::Error, sync::Arc};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
type Png = Arc<Vec<u8>>;
const PNG_MIME: &str = "image/png";

trait Owner {
    fn wait_until_lost(self: Box<Self>) -> Result<()>;
}
