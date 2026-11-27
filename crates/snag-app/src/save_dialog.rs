use crate::{file_uri::local_path, run::SaveDialog};
use ashpd::desktop::{
    ResponseError,
    file_chooser::{FileFilter, SelectedFiles},
};
use std::{error::Error, path::PathBuf};

pub struct Portal;

impl SaveDialog for Portal {
    fn path(self) -> Result<Option<PathBuf>, Box<dyn Error>> {
        let request = SelectedFiles::save_file()
            .title("Save screenshot")
            .current_name("screenshot.png")
            .filter(FileFilter::new("PNG image").mimetype("image/png"));
        let result = ashpd::zbus::block_on(request.send()).and_then(|r| r.response());
        match result {
            Ok(files) => {
                let uri = files.uris().first().ok_or("save dialog returned no file")?;
                Ok(Some(local_path(uri.as_str())?))
            }
            Err(ashpd::Error::Response(ResponseError::Cancelled)) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}
