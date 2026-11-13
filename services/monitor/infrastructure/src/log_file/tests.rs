use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::RollingFile;

/// A folder of the test's own, removed when the test ends, even by a failed assertion.
struct Scratch(PathBuf);

impl Scratch {
    fn new(test: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("overlay-monitor-{test}-{}", std::process::id()));
        fs::remove_dir_all(&dir).ok();
        Self(dir)
    }

    fn dir(&self) -> &Path {
        &self.0
    }

    fn read(&self, name: &str) -> String {
        fs::read_to_string(self.0.join(name)).expect("read the log")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

#[test]
fn writing_past_the_limit_starts_a_new_file_and_keeps_the_old_one() {
    let scratch = Scratch::new("log-rotate");
    let mut log = RollingFile::open(scratch.dir(), 10, 3).expect("open");
    log.write_all(b"123456").expect("write");
    log.write_all(b"abcdef").expect("write");
    assert_eq!(scratch.read("overlay-monitor.log"), "abcdef");
    assert_eq!(scratch.read("overlay-monitor.1.log"), "123456");
}

#[test]
fn only_the_newest_old_files_are_kept() {
    let scratch = Scratch::new("log-keep");
    let mut log = RollingFile::open(scratch.dir(), 4, 2).expect("open");
    for text in ["aaaa", "bbbb", "cccc", "dddd", "eeee"] {
        log.write_all(text.as_bytes()).expect("write");
    }
    assert_eq!(scratch.read("overlay-monitor.log"), "eeee");
    assert_eq!(scratch.read("overlay-monitor.1.log"), "dddd");
    assert_eq!(scratch.read("overlay-monitor.2.log"), "cccc");
    assert!(!scratch.dir().join("overlay-monitor.3.log").exists());
}

#[test]
fn an_existing_log_is_appended_to() {
    let scratch = Scratch::new("log-append");
    {
        let mut log = RollingFile::open(scratch.dir(), 100, 3).expect("open");
        log.write_all(b"first ").expect("write");
    }
    let mut log = RollingFile::open(scratch.dir(), 100, 3).expect("open again");
    log.write_all(b"second").expect("write");
    assert_eq!(scratch.read("overlay-monitor.log"), "first second");
}
