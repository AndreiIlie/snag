use ashpd::desktop::screenshot::Screenshot as PortalScreenshot;
use percent_encoding::percent_decode_str;
use snag_core::{export::decode_png, geometry::ScaleFactor, platform::Screenshot};
use std::{error::Error, ffi::OsStr, fs, os::unix::ffi::OsStrExt, path::Path};

pub fn capture() -> Result<Screenshot, Box<dyn Error>> {
    let request = PortalScreenshot::request().interactive(false).modal(false);
    let result = ashpd::zbus::block_on(request.send())?.response()?;
    let uri = result.uri().as_str();
    let location = uri
        .strip_prefix("file://")
        .ok_or("expected a local file URI")?;
    let path = location.strip_prefix("localhost").unwrap_or(location);
    if !path.starts_with('/') || path.contains(['?', '#']) {
        return Err("expected a local file path in the screenshot URI".into());
    }
    let bytes = percent_decode_str(path).collect::<Vec<_>>();
    let path = Path::new(OsStr::from_bytes(&bytes));
    let data = fs::read(path).map_err(|e| format!("could not read {}: {e}", path.display()))?;
    let image = decode_png(&data)?;
    let _ = fs::remove_file(path);
    Ok(Screenshot {
        image,
        scale: ScaleFactor::ONE,
    })
}
