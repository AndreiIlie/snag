use percent_encoding::percent_decode_str;
use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};

pub fn local_path(uri: &str) -> Result<PathBuf, &'static str> {
    let location = uri
        .strip_prefix("file://")
        .ok_or("expected a local file URI")?;
    let path = location.strip_prefix("localhost").unwrap_or(location);
    if !path.starts_with('/') || path.contains(['?', '#']) {
        return Err("expected a local file path in the portal URI");
    }
    Ok(OsString::from_vec(percent_decode_str(path).collect()).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn accepts_local_paths_and_decodes_filename_bytes() {
        for (uri, path) in [
            ("file:///a.png", &b"/a.png"[..]),
            ("file://localhost/a%20b%23%FF", &b"/a b#\xff"[..]),
        ] {
            assert_eq!(local_path(uri).unwrap().as_os_str().as_bytes(), path);
        }
        for uri in [
            "https://host/a",
            "file://remote/a",
            "file://localhost.example/a",
            "file:///a?b",
            "file:///a#c",
        ] {
            assert!(local_path(uri).is_err());
        }
    }
}
