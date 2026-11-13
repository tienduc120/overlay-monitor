//! The app's log file, rotated by size: the newest in `overlay-monitor.log`, older ones in `overlay-monitor.1.log` up.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const NAME: &str = "overlay-monitor";

/// A log file that moves to the next name before a write would take it past its limit, and drops the oldest.
#[derive(Debug)]
pub struct RollingFile {
    dir: PathBuf,
    limit_bytes: u64,
    keep: usize,
    /// `None` only while the file is being renamed, as Windows will not rename an open file.
    file: Option<File>,
    size: u64,
}

impl RollingFile {
    /// Opens the log in `dir`, creating the folder, and appends to a log that is already there.
    ///
    /// # Errors
    /// The folder cannot be created or the file cannot be opened.
    pub fn open(dir: &Path, limit_bytes: u64, keep: usize) -> io::Result<Self> {
        fs::create_dir_all(dir)?;
        let file = open_append(&dir.join(current_name()))?;
        let size = file.metadata()?.len();
        Ok(Self {
            dir: dir.to_owned(),
            limit_bytes,
            keep,
            file: Some(file),
            size,
        })
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.file = None;
        let moved = self.move_old_files();
        // A failed rename must not stop the log: the current file is opened again and grows past the limit.
        self.file = Some(open_append(&self.dir.join(current_name()))?);
        self.size = self
            .file
            .as_ref()
            .map_or(0, |file| file.metadata().map_or(0, |meta| meta.len()));
        moved
    }

    fn move_old_files(&self) -> io::Result<()> {
        if self.keep == 0 {
            return fs::remove_file(self.dir.join(current_name()));
        }
        remove_if_there(&self.dir.join(old_name(self.keep)))?;
        for number in (1..self.keep).rev() {
            rename_if_there(&self.dir.join(old_name(number)), &self.dir.join(old_name(number + 1)))?;
        }
        rename_if_there(&self.dir.join(current_name()), &self.dir.join(old_name(1)))
    }
}

impl Write for RollingFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let incoming = u64::try_from(buf.len()).unwrap_or(u64::MAX);
        if self.size > 0 && self.size.saturating_add(incoming) > self.limit_bytes {
            // The line is worth more than the rotation: write it even when the old files could not move.
            self.rotate().ok();
        }
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("the log file is closed"))?;
        let written = file.write(buf)?;
        self.size = self.size.saturating_add(u64::try_from(written).unwrap_or(u64::MAX));
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.as_mut().map_or(Ok(()), Write::flush)
    }
}

fn current_name() -> String {
    format!("{NAME}.log")
}

fn old_name(number: usize) -> String {
    format!("{NAME}.{number}.log")
}

fn open_append(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

fn remove_if_there(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

fn rename_if_there(from: &Path, to: &Path) -> io::Result<()> {
    match fs::rename(from, to) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests;
