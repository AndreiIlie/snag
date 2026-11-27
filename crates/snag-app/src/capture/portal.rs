use ashpd::desktop::screenshot::Screenshot as PortalScreenshot;
use snag_core::{export::decode_png, geometry::ScaleFactor, platform::Screenshot};
use std::{error::Error, fs};

pub fn capture() -> Result<Screenshot, Box<dyn Error>> {
    let request = PortalScreenshot::request().interactive(false).modal(false);
    let result = ashpd::zbus::block_on(request.send())?.response()?;
    let path = crate::file_uri::local_path(result.uri().as_str())?;
    let data = fs::read(&path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
    let image = decode_png(&data)?;
    let _ = fs::remove_file(path);
    Ok(Screenshot {
        image,
        scale: ScaleFactor::ONE,
    })
}
