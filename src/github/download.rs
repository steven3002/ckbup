//! Resumable download of one release asset into `<asset>.part`.
//!
//! A download that stops part-way keeps its bytes and its record
//! (`part_record.rs`), and the next call asks only for the rest. GitHub's
//! download host ignores `If-Range`, so the `Content-Range`, the size and the
//! validators of each response are compared with the record here instead.

use std::fs::{self, OpenOptions};
use std::io::{self, BufWriter, ErrorKind, Read, Write};
use std::path::Path;

use super::client::{AssetHead, AssetResponse};
use super::part_record::PartRecord;
use super::{Asset, GithubClient, GithubError};

/// What a response means for the bytes already on disk.
#[derive(Debug, PartialEq, Eq)]
enum Answer {
    /// A 206 for exactly the missing bytes of the same file.
    Append,
    /// A 200 with the whole file.
    Rewrite,
    /// A 416 saying that nothing is left, with the whole file on disk.
    Complete,
    /// A response that does not continue the bytes on disk.
    Discard(String),
    /// A success status that carries no file.
    Unexpected(u16),
}

/// Downloads `asset` into `part`, resuming from the bytes already there when
/// the record at `record_path` says they belong to the same asset.
///
/// `on_request` receives the offset of each request before it is sent: 0 for
/// a download from the start. When a response does not continue the bytes on
/// disk, both files are deleted and the asset is downloaded once more from the
/// start. Returns once `part` holds exactly `asset.size` bytes; a download that
/// stops leaves `part` and its record for the next call.
pub fn download_asset(
    client: &GithubClient,
    asset: &Asset,
    part: &Path,
    record_path: &Path,
    on_request: &mut dyn FnMut(u64),
) -> Result<(), GithubError> {
    let mut record = prepare(asset, part, record_path)?;
    let mut restarted = false;
    loop {
        let offset = file_len(part)?;
        if offset == asset.size {
            return Ok(());
        }
        on_request(offset);
        let response = client.get_asset(&asset.browser_download_url, offset)?;
        match classify(&response.head, offset, asset.size, &record) {
            Answer::Complete => return Ok(()),
            Answer::Unexpected(status) => {
                return Err(GithubError::Status {
                    url: asset.browser_download_url.clone(),
                    status,
                });
            }
            Answer::Discard(reason) => {
                discard(part, record_path)?;
                if restarted {
                    return Err(GithubError::AssetChanged {
                        name: asset.name.clone(),
                        reason,
                    });
                }
                restarted = true;
                record = PartRecord::for_asset(asset);
                save(&record, record_path)?;
            }
            answer => {
                let rewrite = answer == Answer::Rewrite;
                if rewrite || record.etag.is_none() {
                    record.etag = response.head.etag.clone();
                }
                if rewrite || record.last_modified.is_none() {
                    record.last_modified = response.head.last_modified.clone();
                }
                save(&record, record_path)?;
                let from = if rewrite { 0 } else { offset };
                return write_body(response, asset, part, record_path, from);
            }
        }
    }
}

/// Keeps the bytes on disk only when the record belongs to `asset` and they
/// are not more than the asset holds; otherwise both files start over.
fn prepare(asset: &Asset, part: &Path, record_path: &Path) -> Result<PartRecord, GithubError> {
    if let Some(record) = PartRecord::read(record_path).filter(|record| record.describes(asset))
        && file_len(part)? <= asset.size
    {
        return Ok(record);
    }
    discard(part, record_path)?;
    let record = PartRecord::for_asset(asset);
    save(&record, record_path)?;
    Ok(record)
}

fn classify(head: &AssetHead, offset: u64, size: u64, record: &PartRecord) -> Answer {
    if matches!(head.status, 200 | 206)
        && let Some(encoding) = &head.content_encoding
    {
        return Answer::Discard(format!("it was sent with Content-Encoding {encoding}"));
    }
    let range = head.content_range.as_deref();
    match head.status {
        200 => Answer::Rewrite,
        206 => match range.and_then(parse_content_range) {
            Some(ContentRange::Bytes { start, total }) if start == offset && total == size => {
                match changed_validator(record, head) {
                    Some(reason) => Answer::Discard(reason),
                    None => Answer::Append,
                }
            }
            _ => Answer::Discard(format!(
                "a request from byte {offset} got Content-Range {}, expected a {size}-byte file",
                range.unwrap_or("none")
            )),
        },
        416 => match range.and_then(parse_content_range) {
            Some(ContentRange::Unsatisfied { total }) if total == size && offset == size => {
                Answer::Complete
            }
            _ => Answer::Discard(format!(
                "a request from byte {offset} got HTTP 416 with Content-Range {}",
                range.unwrap_or("none")
            )),
        },
        status => Answer::Unexpected(status),
    }
}

/// An ETag or Last-Modified that both the record and the response carry, and
/// that differs between them.
fn changed_validator(record: &PartRecord, head: &AssetHead) -> Option<String> {
    for (name, kept, sent) in [
        ("ETag", &record.etag, &head.etag),
        ("Last-Modified", &record.last_modified, &head.last_modified),
    ] {
        if let (Some(kept), Some(sent)) = (kept, sent)
            && kept != sent
        {
            return Some(format!("its {name} changed from {kept} to {sent}"));
        }
    }
    None
}

#[derive(Debug, PartialEq, Eq)]
enum ContentRange {
    /// `bytes <start>-<end>/<total>`
    Bytes { start: u64, total: u64 },
    /// `bytes */<total>`
    Unsatisfied { total: u64 },
}

fn parse_content_range(value: &str) -> Option<ContentRange> {
    let (range, total) = value.trim().strip_prefix("bytes ")?.split_once('/')?;
    let total = total.parse().ok()?;
    if range == "*" {
        return Some(ContentRange::Unsatisfied { total });
    }
    let (start, end) = range.split_once('-')?;
    let start: u64 = start.parse().ok()?;
    let end: u64 = end.parse().ok()?;
    (start <= end && end < total).then_some(ContentRange::Bytes { start, total })
}

/// Writes the body to `part` from byte `from` on, then checks that `part`
/// holds exactly the asset. At most one byte more than the asset is read, so
/// a body that runs on cannot fill the disk.
fn write_body(
    response: AssetResponse,
    asset: &Asset,
    part: &Path,
    record_path: &Path,
    from: u64,
) -> Result<(), GithubError> {
    let mut options = OpenOptions::new();
    if from == 0 {
        options.write(true).create(true).truncate(true);
    } else {
        options.create(true).append(true);
    }
    let file = options
        .open(part)
        .map_err(GithubError::file("cannot write", part))?;
    let mut writer = BufWriter::new(file);
    let mut body = response.into_reader().take(asset.size - from + 1);
    let copied = io::copy(&mut body, &mut writer);
    let flushed = writer.flush();
    drop(writer);

    let received = file_len(part)?;
    if received > asset.size {
        discard(part, record_path)?;
        return Err(GithubError::DownloadTooLong {
            name: asset.name.clone(),
            size: asset.size,
        });
    }
    flushed.map_err(GithubError::file("cannot write", part))?;
    let cause = match copied {
        Err(err) => err.to_string(),
        Ok(_) if received < asset.size => "the connection closed early".to_string(),
        Ok(_) => return Ok(()),
    };
    Err(GithubError::DownloadStopped {
        name: asset.name.clone(),
        received,
        size: asset.size,
        cause,
    })
}

fn save(record: &PartRecord, path: &Path) -> Result<(), GithubError> {
    record
        .write(path)
        .map_err(GithubError::file("cannot write", path))
}

fn file_len(path: &Path) -> Result<u64, GithubError> {
    match fs::metadata(path) {
        Ok(meta) => Ok(meta.len()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(0),
        Err(err) => Err(GithubError::file("cannot read", path)(err)),
    }
}

/// Deletes the partial download and its record.
fn discard(part: &Path, record_path: &Path) -> Result<(), GithubError> {
    for path in [part, record_path] {
        match fs::remove_file(path) {
            Err(err) if err.kind() != ErrorKind::NotFound => {
                return Err(GithubError::file("cannot delete", path)(err));
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::github::test_server::{Reply, TestServer};

    const SIZE: u64 = 100;

    /// The 100 bytes of the release asset in these tests.
    fn file() -> Vec<u8> {
        (0..SIZE).map(|i| (i * 7 % 251) as u8).collect()
    }

    fn etag(value: &str) -> (&'static str, String) {
        ("ETag", value.to_string())
    }

    struct Download {
        _dir: tempfile::TempDir,
        part: PathBuf,
        record: PathBuf,
        asset: Asset,
    }

    impl Download {
        fn new(server: &TestServer) -> Self {
            let dir = tempfile::tempdir().unwrap();
            Download {
                part: dir.path().join("tool.tar.gz.part"),
                record: dir.path().join("tool.tar.gz.part.toml"),
                asset: Asset {
                    id: 7,
                    name: "tool.tar.gz".to_string(),
                    size: SIZE,
                    browser_download_url: server.url("/download/tool.tar.gz"),
                    digest: Some(format!("sha256:{}", "ab".repeat(32))),
                },
                _dir: dir,
            }
        }

        /// `bytes` on disk, with a record for this asset that kept `etag`.
        fn with_part(self, bytes: &[u8], etag: Option<&str>) -> Self {
            fs::write(&self.part, bytes).unwrap();
            let mut record = PartRecord::for_asset(&self.asset);
            record.etag = etag.map(str::to_string);
            record.write(&self.record).unwrap();
            self
        }

        /// Runs the download and returns its result with the offset of each request.
        fn run(&self) -> (Result<(), GithubError>, Vec<u64>) {
            let mut offsets = Vec::new();
            let result = download_asset(
                &GithubClient::for_test_server(),
                &self.asset,
                &self.part,
                &self.record,
                &mut |offset| offsets.push(offset),
            );
            (result, offsets)
        }

        fn part_bytes(&self) -> Vec<u8> {
            fs::read(&self.part).unwrap()
        }
    }

    fn ranges(server: &TestServer) -> Vec<Option<String>> {
        server
            .requests()
            .iter()
            .map(|request| request.header("range").map(str::to_string))
            .collect()
    }

    #[test]
    fn a_fresh_download_sends_no_range_and_keeps_the_validators() {
        let server = TestServer::start(vec![Reply::full(&file(), &[etag("\"v1\"")])]);
        let download = Download::new(&server);

        let (result, offsets) = download.run();

        result.unwrap();
        assert_eq!(offsets, [0]);
        assert_eq!(ranges(&server), [None]);
        assert_eq!(
            server.requests()[0].header("accept-encoding"),
            Some("identity")
        );
        assert_eq!(download.part_bytes(), file());
        let record = PartRecord::read(&download.record).unwrap();
        assert_eq!(record.etag.as_deref(), Some("\"v1\""));
    }

    #[test]
    fn resumes_after_the_bytes_on_disk() {
        let server = TestServer::start(vec![Reply::partial(
            &file()[40..],
            40,
            SIZE,
            &[etag("\"v1\"")],
        )]);
        let download = Download::new(&server).with_part(&file()[..40], Some("\"v1\""));

        let (result, offsets) = download.run();

        result.unwrap();
        assert_eq!(offsets, [40]);
        assert_eq!(ranges(&server), [Some("bytes=40-".to_string())]);
        assert_eq!(download.part_bytes(), file());
    }

    #[test]
    fn a_redirect_keeps_the_range() {
        let server = TestServer::start(vec![
            Reply::redirect("/signed/blob"),
            Reply::partial(&file()[40..], 40, SIZE, &[]),
        ]);
        let download = Download::new(&server).with_part(&file()[..40], None);

        download.run().0.unwrap();

        let requests = server.requests();
        assert_eq!(requests[1].path, "/signed/blob");
        assert_eq!(requests[1].header("range"), Some("bytes=40-"));
        assert_eq!(requests[1].header("accept-encoding"), Some("identity"));
        assert_eq!(download.part_bytes(), file());
    }

    #[test]
    fn a_200_to_a_range_request_rewrites_from_0() {
        let server = TestServer::start(vec![Reply::full(&file(), &[])]);
        let download = Download::new(&server).with_part(&[0xff; 40], None);

        download.run().0.unwrap();

        assert_eq!(ranges(&server), [Some("bytes=40-".to_string())]);
        assert_eq!(download.part_bytes(), file());
    }

    #[test]
    fn a_complete_part_makes_no_request() {
        let server = TestServer::start(vec![]);
        let download = Download::new(&server).with_part(&file(), Some("\"v1\""));

        let (result, offsets) = download.run();

        result.unwrap();
        assert!(offsets.is_empty());
        assert!(server.requests().is_empty());
        assert_eq!(download.part_bytes(), file());
    }

    #[test]
    fn responses_that_do_not_continue_the_part_are_discarded() {
        let cases = [
            ("416 with another total", Reply::unsatisfiable(200)),
            (
                "a Content-Range total that differs",
                Reply::partial(&file()[40..], 40, 120, &[]),
            ),
            (
                "a different ETag",
                Reply::partial(&file()[40..], 40, SIZE, &[etag("\"v2\"")]),
            ),
            (
                "a Content-Encoding",
                Reply::partial(
                    &file()[40..],
                    40,
                    SIZE,
                    &[("Content-Encoding", "br".to_string())],
                ),
            ),
        ];
        for (case, reply) in cases {
            let server = TestServer::start(vec![reply, Reply::full(&file(), &[])]);
            let download = Download::new(&server).with_part(&file()[..40], Some("\"v1\""));

            let (result, offsets) = download.run();

            result.unwrap_or_else(|err| panic!("{case}: {err}"));
            assert_eq!(offsets, [40, 0], "{case}");
            assert_eq!(
                ranges(&server),
                [Some("bytes=40-".to_string()), None],
                "{case}"
            );
            assert_eq!(download.part_bytes(), file(), "{case}");
        }
    }

    #[test]
    fn a_record_for_another_asset_starts_over_without_a_range() {
        let server = TestServer::start(vec![Reply::full(&file(), &[])]);
        let download = Download::new(&server).with_part(&file()[..40], None);
        let mut other = PartRecord::for_asset(&download.asset);
        other.id = 8;
        other.write(&download.record).unwrap();

        download.run().0.unwrap();

        assert_eq!(ranges(&server), [None]);
        assert_eq!(download.part_bytes(), file());
    }

    #[test]
    fn a_second_mismatch_fails_and_deletes_the_part() {
        let server = TestServer::start(vec![
            Reply::partial(&file()[40..], 40, 120, &[]),
            Reply::partial(&file(), 0, 120, &[]),
        ]);
        let download = Download::new(&server).with_part(&file()[..40], None);

        let err = download.run().0.unwrap_err();

        assert!(matches!(err, GithubError::AssetChanged { .. }), "{err}");
        assert!(
            err.to_string().starts_with(
                "download of tool.tar.gz does not match the release asset GitHub lists"
            ),
            "{err}"
        );
        assert_eq!(server.requests().len(), 2);
        assert!(!download.part.exists());
        assert!(!download.record.exists());
    }

    #[test]
    fn a_body_cut_off_part_way_is_resumed_by_the_next_call() {
        let cut_off = Reply::new(
            "206 Partial Content",
            &[
                ("Content-Length", "80".to_string()),
                ("Content-Range", "bytes 20-99/100".to_string()),
            ],
            &file()[20..65],
        );
        let rest = Reply::partial(&file()[65..], 65, SIZE, &[]);
        let server = TestServer::start(vec![cut_off, rest]);
        let download = Download::new(&server).with_part(&file()[..20], None);

        let err = download.run().0.unwrap_err();

        assert!(
            err.to_string()
                .starts_with("download of tool.tar.gz stopped at 65 of 100 bytes ("),
            "{err}"
        );
        assert!(
            err.to_string()
                .ends_with("); run the same command again to resume"),
            "{err}"
        );
        assert_eq!(download.part_bytes(), &file()[..65]);
        assert!(PartRecord::read(&download.record).is_some());

        let (result, offsets) = download.run();

        result.unwrap();
        assert_eq!(offsets, [65]);
        assert_eq!(
            ranges(&server),
            [Some("bytes=20-".to_string()), Some("bytes=65-".to_string())]
        );
        assert_eq!(download.part_bytes(), file());
    }

    #[test]
    fn a_short_body_without_a_length_is_an_error_and_keeps_its_bytes() {
        let server = TestServer::start(vec![Reply::new("200 OK", &[], &file()[..40])]);
        let download = Download::new(&server);

        let err = download.run().0.unwrap_err();

        assert_eq!(
            err.to_string(),
            "download of tool.tar.gz stopped at 40 of 100 bytes (the connection closed early); run the same command again to resume"
        );
        assert_eq!(download.part_bytes(), &file()[..40]);
        assert!(PartRecord::read(&download.record).is_some());
    }

    #[test]
    fn a_body_longer_than_the_asset_is_deleted() {
        let mut longer = file();
        longer.extend_from_slice(&[1; 10]);
        let server = TestServer::start(vec![Reply::full(&longer, &[])]);
        let download = Download::new(&server);

        let err = download.run().0.unwrap_err();

        assert_eq!(
            err.to_string(),
            "download of tool.tar.gz is longer than its listed size of 100 bytes, so it was deleted; run the same command again"
        );
        assert!(!download.part.exists());
        assert!(!download.record.exists());
    }

    #[test]
    fn a_416_for_a_complete_part_means_nothing_is_left() {
        let record = PartRecord {
            url: "u".to_string(),
            id: 7,
            size: SIZE,
            digest: None,
            etag: None,
            last_modified: None,
        };
        let head = AssetHead {
            status: 416,
            content_range: Some("bytes */100".to_string()),
            ..AssetHead::default()
        };
        assert_eq!(classify(&head, SIZE, SIZE, &record), Answer::Complete);
        assert!(matches!(
            classify(&head, 40, SIZE, &record),
            Answer::Discard(_)
        ));
    }

    #[test]
    fn parses_the_content_range_forms_of_the_download_host() {
        assert_eq!(
            parse_content_range("bytes 0-4095/3013794"),
            Some(ContentRange::Bytes {
                start: 0,
                total: 3013794
            })
        );
        assert_eq!(
            parse_content_range("bytes */3013794"),
            Some(ContentRange::Unsatisfied { total: 3013794 })
        );
        for value in [
            "bytes 10-5/100",
            "bytes 0-100/100",
            "items 0-9/10",
            "bytes 0-9/*",
        ] {
            assert_eq!(parse_content_range(value), None, "{value}");
        }
    }

    /// Downloads ckb-debugger v1.1.2's Windows archive (3,013,794 bytes) with
    /// the asset fields the API listed, cuts the part to 1,000,000 bytes, and
    /// resumes it over GitHub's redirect. No API request is made.
    #[test]
    #[ignore = "downloads a release asset from GitHub"]
    fn resumes_a_real_release_asset_over_the_redirect() {
        let dir = tempfile::tempdir().unwrap();
        let part = dir.path().join("ckb-debugger.tar.gz.part");
        let record = dir.path().join("ckb-debugger.tar.gz.part.toml");
        let asset = Asset {
            id: 753671710,
            name: "ckb-debugger_v1.1.2_x86_64-pc-windows-msvc.tar.gz".to_string(),
            size: 3013794,
            browser_download_url: "https://github.com/nervosnetwork/ckb-standalone-debugger/releases/download/v1.1.2/ckb-debugger_v1.1.2_x86_64-pc-windows-msvc.tar.gz".to_string(),
            digest: Some(
                "sha256:1b03e2b6b83be85c1fa2410140eb75aa699a08ab430ecfc6d4fad99443b5b690"
                    .to_string(),
            ),
        };
        let client = GithubClient::from_env();
        let mut offsets = Vec::new();

        download_asset(&client, &asset, &part, &record, &mut |offset| {
            offsets.push(offset)
        })
        .unwrap();
        let saved = PartRecord::read(&record).unwrap();
        assert!(saved.etag.is_some(), "{saved:?}");
        fs::OpenOptions::new()
            .write(true)
            .open(&part)
            .unwrap()
            .set_len(1_000_000)
            .unwrap();
        download_asset(&client, &asset, &part, &record, &mut |offset| {
            offsets.push(offset)
        })
        .unwrap();

        assert_eq!(offsets, [0, 1_000_000]);
        assert_eq!(fs::metadata(&part).unwrap().len(), 3013794);
        assert_eq!(
            crate::integrity::sha256_file(&part).unwrap(),
            "1b03e2b6b83be85c1fa2410140eb75aa699a08ab430ecfc6d4fad99443b5b690"
        );
    }
}
