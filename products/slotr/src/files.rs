use std::{
    fs::{File, OpenOptions},
    io::{self, Read},
    path::Path,
};

// Validate the descriptor, not the path, which may be replaced during open.
pub fn readable(path: impl AsRef<Path>) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(
            (rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC).bits() as i32,
        );
    }
    let file = options.open(path)?;
    regular(&file)?;
    Ok(file)
}

pub fn regular(file: &File) -> io::Result<()> {
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a regular file",
        ));
    }
    Ok(())
}

pub fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
    let mut text = String::new();
    readable(path)?.read_to_string(&mut text)?;
    Ok(text)
}
