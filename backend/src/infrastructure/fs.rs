//! File-system adapter for log files.
//!
//! Large files are never loaded whole: reads seek to a byte offset and stop at
//! the page budget (`ReadWindow`), so memory use per request is bounded.

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::application::error::AppError;
use crate::application::ports::{LogFileSystem, ReadWindow};
use crate::application::views::{EntryKind, FileDetails, LogChunk, LogEntry, LogLine};
use crate::domain::{LogDirectory, RelativeLogPath};

const IO_BUFFER_BYTES: usize = 64 * 1024;

#[derive(Debug, Default, Clone, Copy)]
pub struct FsLogFileSystem;

impl LogFileSystem for FsLogFileSystem {
    fn is_directory(&self, directory: &LogDirectory) -> bool {
        directory.as_path().is_dir()
    }

    fn list_entries(
        &self,
        directory: &LogDirectory,
        path: &RelativeLogPath,
    ) -> Result<Vec<LogEntry>, AppError> {
        let root = canonical_root(directory)?;
        let base = resolve_within(&root, path)?;
        if !base.is_dir() {
            return Err(AppError::Validation("path is not a directory".into()));
        }

        let mut entries = Vec::new();
        for item in fs::read_dir(&base).map_err(storage)? {
            let item = item.map_err(storage)?;
            let child = item.path();
            // Skip dangling links and links that point outside the directory.
            let Ok(canonical) = fs::canonicalize(&child) else {
                continue;
            };
            if !canonical.starts_with(&root) {
                continue;
            }
            let Ok(meta) = fs::metadata(&canonical) else {
                continue;
            };
            let name = item.file_name().to_string_lossy().into_owned();
            let (kind, size) = if meta.is_dir() {
                (EntryKind::Directory, None)
            } else if meta.is_file() {
                (EntryKind::File, Some(meta.len()))
            } else {
                continue;
            };
            let relative_path = if path.is_root() {
                name.clone()
            } else {
                format!("{}/{}", path.as_path().to_string_lossy(), name)
            };
            entries.push(LogEntry {
                name,
                relative_path,
                kind,
                size,
            });
        }

        entries.sort_by(|a, b| {
            (a.kind != EntryKind::Directory, &a.name)
                .cmp(&(b.kind != EntryKind::Directory, &b.name))
        });
        Ok(entries)
    }

    fn read_chunk(
        &self,
        directory: &LogDirectory,
        path: &RelativeLogPath,
        offset: u64,
        window: ReadWindow,
    ) -> Result<LogChunk, AppError> {
        let root = canonical_root(directory)?;
        let file_path = resolve_within(&root, path)?;
        let meta = fs::metadata(&file_path).map_err(storage)?;
        if !meta.is_file() {
            return Err(AppError::Validation("path is not a regular file".into()));
        }
        let size = meta.len();
        if offset > size {
            return Err(AppError::Validation(format!(
                "offset {offset} is beyond end of file ({size} bytes)"
            )));
        }

        let mut file = File::open(&file_path).map_err(storage)?;
        file.seek(SeekFrom::Start(offset)).map_err(storage)?;
        let mut reader = BufReader::with_capacity(IO_BUFFER_BYTES, file);

        let mut budget = window.max_bytes as u64;
        let mut position = offset;
        let mut lines = Vec::new();
        let mut raw = Vec::new();

        while lines.len() < window.max_lines && budget > 0 {
            raw.clear();
            // `take` caps how many bytes one line may pull in, so a huge line cannot exhaust memory.
            let read = (&mut reader)
                .take(budget)
                .read_until(b'\n', &mut raw)
                .map_err(storage)?;
            if read == 0 {
                break;
            }
            budget -= read as u64;
            position += read as u64;

            let ends_with_newline = raw.last() == Some(&b'\n');
            let truncated = !ends_with_newline && budget == 0 && position < size;
            if ends_with_newline {
                raw.pop();
                if raw.last() == Some(&b'\r') {
                    raw.pop();
                }
            }
            lines.push(LogLine {
                text: String::from_utf8_lossy(&raw).into_owned(),
                truncated,
            });
        }

        Ok(LogChunk {
            file: FileDetails {
                size,
                modified_unix: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs()),
            },
            start_offset: offset,
            next_offset: position,
            eof: position >= size,
            lines,
        })
    }
}

fn canonical_root(directory: &LogDirectory) -> Result<PathBuf, AppError> {
    fs::canonicalize(directory.as_path()).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => {
            AppError::NotFound(format!("directory {}", directory.as_path().display()))
        }
        _ => storage(e),
    })
}

/// Resolves `path` under `root` and fails if the real location (after following
/// symlinks) escapes `root`. Escapes are reported as "not found" to avoid leaking layout.
fn resolve_within(root: &Path, path: &RelativeLogPath) -> Result<PathBuf, AppError> {
    let not_found = || AppError::NotFound(format!("path {}", path.as_path().display()));
    let resolved = fs::canonicalize(root.join(path.as_path())).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => not_found(),
        _ => storage(e),
    })?;
    if !resolved.starts_with(root) {
        return Err(not_found());
    }
    Ok(resolved)
}

fn storage(error: io::Error) -> AppError {
    AppError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn dir_of(tmp: &TempDir) -> LogDirectory {
        LogDirectory::parse(tmp.path().to_str().unwrap()).unwrap()
    }

    fn rel(p: &str) -> RelativeLogPath {
        RelativeLogPath::parse(p).unwrap()
    }

    fn window(max_lines: usize, max_bytes: usize) -> ReadWindow {
        ReadWindow {
            max_lines,
            max_bytes,
        }
    }

    fn write(tmp: &TempDir, name: &str, bytes: &[u8]) {
        let p = tmp.path().join(name);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        File::create(p).unwrap().write_all(bytes).unwrap();
    }

    #[test]
    fn reads_page_and_reports_next_offset() {
        let tmp = TempDir::new().unwrap();
        write(&tmp, "app.log", b"a\nb\nc\n");
        let fs_ = FsLogFileSystem;
        let dir = dir_of(&tmp);

        let first = fs_
            .read_chunk(&dir, &rel("app.log"), 0, window(2, 1024))
            .unwrap();
        let texts: Vec<&str> = first.lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, vec!["a", "b"]);
        assert_eq!(first.next_offset, 4);
        assert!(!first.eof);

        let second = fs_
            .read_chunk(&dir, &rel("app.log"), first.next_offset, window(2, 1024))
            .unwrap();
        assert_eq!(second.lines[0].text, "c");
        assert_eq!(second.next_offset, 6);
        assert!(second.eof);
        assert_eq!(second.file.size, 6);
        assert!(second.file.modified_unix.is_some());
    }

    #[test]
    fn strips_crlf_and_replaces_invalid_utf8() {
        let tmp = TempDir::new().unwrap();
        write(&tmp, "mixed.log", b"windows\r\nbad\xff\n");
        let chunk = FsLogFileSystem
            .read_chunk(&dir_of(&tmp), &rel("mixed.log"), 0, window(10, 1024))
            .unwrap();
        assert_eq!(chunk.lines[0].text, "windows");
        assert_eq!(chunk.lines[1].text, "bad\u{fffd}");
    }

    #[test]
    fn rejects_offset_past_end_of_file() {
        let tmp = TempDir::new().unwrap();
        write(&tmp, "app.log", b"abc\n");
        let err = FsLogFileSystem
            .read_chunk(&dir_of(&tmp), &rel("app.log"), 100, window(10, 1024))
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[test]
    fn pages_through_large_file_without_loss() {
        // ~5 MB file read in 64 KiB pages: every byte must come back exactly once.
        let tmp = TempDir::new().unwrap();
        let mut original = String::new();
        for i in 0..200_000 {
            original.push_str(&format!("2026-11-25 line {i} some payload text\n"));
        }
        write(&tmp, "big.log", original.as_bytes());

        let fs_ = FsLogFileSystem;
        let dir = dir_of(&tmp);
        let mut rebuilt = String::new();
        let mut offset = 0;
        let mut pages = 0;
        loop {
            let chunk = fs_
                .read_chunk(&dir, &rel("big.log"), offset, window(1_000, 64 * 1024))
                .unwrap();
            assert!(chunk.lines.len() <= 1_000);
            for line in &chunk.lines {
                rebuilt.push_str(&line.text);
                if !line.truncated {
                    rebuilt.push('\n');
                }
            }
            pages += 1;
            if chunk.eof {
                break;
            }
            assert!(chunk.next_offset > offset, "paging must make progress");
            offset = chunk.next_offset;
        }
        assert!(pages > 50);
        assert_eq!(rebuilt, original);
    }

    #[test]
    fn splits_a_line_longer_than_the_page_budget() {
        let tmp = TempDir::new().unwrap();
        let long = "x".repeat(300);
        write(&tmp, "long.log", format!("{long}\nend\n").as_bytes());
        let fs_ = FsLogFileSystem;
        let dir = dir_of(&tmp);

        let first = fs_
            .read_chunk(&dir, &rel("long.log"), 0, window(10, 100))
            .unwrap();
        assert_eq!(first.lines.len(), 1);
        assert!(first.lines[0].truncated);
        assert_eq!(first.next_offset, 100);

        let mut collected = String::new();
        let mut offset = 0;
        loop {
            let chunk = fs_
                .read_chunk(&dir, &rel("long.log"), offset, window(10, 100))
                .unwrap();
            for l in &chunk.lines {
                collected.push_str(&l.text);
            }
            if chunk.eof {
                break;
            }
            offset = chunk.next_offset;
        }
        assert_eq!(collected, format!("{long}end"));
    }

    #[test]
    fn lists_directories_first_then_files_sorted() {
        let tmp = TempDir::new().unwrap();
        write(&tmp, "b.log", b"12345");
        write(&tmp, "a.log", b"1");
        fs::create_dir(tmp.path().join("archive")).unwrap();
        let entries = FsLogFileSystem
            .list_entries(&dir_of(&tmp), &RelativeLogPath::root())
            .unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["archive", "a.log", "b.log"]);
        assert_eq!(entries[0].kind, EntryKind::Directory);
        assert_eq!(entries[2].size, Some(5));
    }

    #[test]
    fn lists_nested_directory_with_relative_paths() {
        let tmp = TempDir::new().unwrap();
        write(&tmp, "2026/11/app.log", b"x");
        let entries = FsLogFileSystem
            .list_entries(&dir_of(&tmp), &rel("2026/11"))
            .unwrap();
        assert_eq!(entries[0].relative_path, "2026/11/app.log");
    }

    #[test]
    fn missing_file_is_not_found() {
        let tmp = TempDir::new().unwrap();
        let err = FsLogFileSystem
            .read_chunk(&dir_of(&tmp), &rel("nope.log"), 0, window(10, 100))
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }

    #[test]
    fn directory_is_not_a_readable_file() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("sub")).unwrap();
        let err = FsLogFileSystem
            .read_chunk(&dir_of(&tmp), &rel("sub"), 0, window(10, 100))
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escaping_the_log_directory_is_not_readable() {
        let outside = TempDir::new().unwrap();
        write(&outside, "secret.txt", b"top secret");
        let tmp = TempDir::new().unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("secret.txt"),
            tmp.path().join("link.log"),
        )
        .unwrap();

        let fs_ = FsLogFileSystem;
        let dir = dir_of(&tmp);
        let err = fs_
            .read_chunk(&dir, &rel("link.log"), 0, window(10, 100))
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));

        let listing = fs_.list_entries(&dir, &RelativeLogPath::root()).unwrap();
        assert!(listing.iter().all(|e| e.name != "link.log"));
    }

    #[test]
    fn is_directory_reflects_host_state() {
        let tmp = TempDir::new().unwrap();
        assert!(FsLogFileSystem.is_directory(&dir_of(&tmp)));
        let missing = LogDirectory::parse("/definitely/not/here").unwrap();
        assert!(!FsLogFileSystem.is_directory(&missing));
    }
}
