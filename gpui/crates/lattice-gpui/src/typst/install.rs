//! Where the Typst helper comes from and where it goes: the release asset
//! beside the app's own feed, checked with the updater's minisign key, and
//! unpacked into the app's data folder under the app's version, so an
//! update of the app asks for the helper of its own version.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use crate::updater::install as fetch;

/// The helper's file name inside its archive and its folder.
pub const BINARY: &str = if cfg!(windows) {
    "lattice-typst.exe"
} else {
    "lattice-typst"
};

/// Where a download comes from: the archive's addresses, tried in order (the
/// feed's home, then the address it had before it moved), and the key its
/// `.sig` must verify against.
#[derive(Clone, Debug, PartialEq)]
pub struct Source {
    pub urls: Vec<String>,
    pub public_key: String,
}

/// The release asset for this Mac, as release.yml names it: only Apple
/// silicon, the one platform the native app ships for.
pub fn asset() -> Option<String> {
    cfg!(all(target_os = "macos", target_arch = "aarch64"))
        .then(|| format!("Lattice-typst_{}_aarch64.tar.gz", lattice_core::VERSION))
}

/// The archive's addresses beside each feed in `endpoints`: a feed that is
/// always the latest release's (`…/releases/latest/download/…`) points at
/// this version's tag instead, so the helper matches the app; any other feed
/// (Lattice Beta's fixed tag) has the archive in its own folder.
pub fn urls(endpoints: &[&str], asset: &str) -> Vec<String> {
    endpoints
        .iter()
        .filter_map(|endpoint| {
            let folder = match endpoint.split_once("/releases/latest/download/") {
                Some((repository, _)) => {
                    format!("{repository}/releases/download/v{}", lattice_core::VERSION)
                }
                None => endpoint.rsplit_once('/')?.0.to_string(),
            };
            Some(format!("{folder}/{asset}"))
        })
        .collect()
}

/// This app's source: the flavor's feeds and the updater's key, or the one
/// archive `LATTICE_TYPST_HELPER_URL` names (a local file, for rehearsing a
/// release), still checked with that key.
pub fn source() -> Option<Source> {
    let asset = asset()?;
    let public_key = crate::updater::channel()?.public_key;
    let urls = match std::env::var("LATTICE_TYPST_HELPER_URL") {
        Ok(url) if !url.is_empty() => vec![url],
        _ => urls(crate::flavor::current().endpoints, &asset),
    };
    Some(Source { urls, public_key })
}

/// The archive's size, from the first address that answers, for the
/// question that asks to download it.
pub fn size(source: &Source) -> Option<u64> {
    source.urls.iter().find_map(|url| {
        if let Some(path) = url.strip_prefix("file://") {
            return std::fs::metadata(path).ok().map(|meta| meta.len());
        }
        let client = lattice_core::http::blocking_as(
            concat!("Lattice/", env!("CARGO_PKG_VERSION"), " (GPUI)"),
            Duration::from_secs(15),
        )
        .build()
        .ok()?;
        let response = client.head(url).send().ok()?.error_for_status().ok()?;
        response.content_length().filter(|length| *length > 0)
    })
}

/// Download the archive from the first address that serves it with its
/// signature, and check one against the other.
pub fn download(source: &Source, mut progress: impl FnMut(Option<f32>)) -> Result<Vec<u8>, String> {
    let mut last = Err("Typst support has no download address".to_string());
    for url in &source.urls {
        last = fetch::fetch_text(&format!("{url}.sig")).and_then(|signature| {
            let bytes = fetch::download(url, &mut progress)?;
            fetch::verify(&bytes, &signature, &source.public_key)?;
            Ok(bytes)
        });
        if last.is_ok() {
            break;
        }
    }
    last
}

/// The helper installed for this version in `folder`, if any.
pub fn installed(folder: &Path) -> Option<PathBuf> {
    let binary = folder.join(lattice_core::VERSION).join(BINARY);
    binary.is_file().then_some(binary)
}

/// Whether another version's helper is in `folder`: the writer said yes
/// once, so the app's own version downloads without asking again.
pub fn installed_before(folder: &Path) -> bool {
    std::fs::read_dir(folder).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            entry.file_name() != lattice_core::VERSION && entry.path().join(BINARY).is_file()
        })
    })
}

/// The installed helper's size on disk, for Settings.
pub fn installed_size(folder: &Path) -> Option<u64> {
    installed(folder).and_then(|binary| std::fs::metadata(binary).ok().map(|meta| meta.len()))
}

/// Unpack `archive` (a gzipped tar holding the helper) into `folder` under
/// this version, beside it first so the helper appears whole, and remove
/// every other version's.
pub fn unpack(folder: &Path, archive: &[u8]) -> Result<PathBuf, String> {
    std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    let staging = tempfile::Builder::new()
        .prefix(".staging-")
        .tempdir_in(folder)
        .map_err(|error| error.to_string())?;
    tar::Archive::new(flate2::read::GzDecoder::new(archive))
        .unpack(staging.path())
        .map_err(|error| format!("could not unpack Typst support: {error}"))?;
    let binary = staging.path().join(BINARY);
    if !binary.is_file() {
        return Err(format!("the download holds no {BINARY}"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))
            .map_err(|error| error.to_string())?;
    }
    let target = folder.join(lattice_core::VERSION);
    if target.exists() {
        std::fs::remove_dir_all(&target).map_err(|error| error.to_string())?;
    }
    std::fs::rename(staging.keep(), &target).map_err(|error| error.to_string())?;
    remove_others(folder);
    Ok(target.join(BINARY))
}

/// Remove every version but this one's.
fn remove_others(folder: &Path) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name() != lattice_core::VERSION && entry.path().is_dir() {
            std::fs::remove_dir_all(entry.path()).ok();
        }
    }
}

/// Remove every installed helper.
pub fn remove(folder: &Path) -> Result<(), String> {
    match std::fs::remove_dir_all(folder) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.to_string()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archive(name: &str, body: &[u8]) -> Vec<u8> {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
            Vec::new(),
            flate2::Compression::fast(),
        ));
        let mut header = tar::Header::new_gnu();
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, name, body).unwrap();
        builder.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn the_archive_sits_beside_the_feed_at_this_versions_tag() {
        let urls = urls(
            &[
                "https://github.com/lattice-writer/lattice/releases/latest/download/latest-native.json",
                "https://github.com/leo1oel/lattice/releases/download/lattice-beta/latest.json",
            ],
            "Lattice-typst_1.2.3_aarch64.tar.gz",
        );
        assert_eq!(
            urls,
            [
                format!(
                    "https://github.com/lattice-writer/lattice/releases/download/v{}/Lattice-typst_1.2.3_aarch64.tar.gz",
                    lattice_core::VERSION
                ),
                "https://github.com/leo1oel/lattice/releases/download/lattice-beta/Lattice-typst_1.2.3_aarch64.tar.gz".to_string(),
            ]
        );
    }

    #[test]
    fn unpacking_installs_this_version_and_removes_the_others() {
        let folder = tempfile::tempdir().unwrap();
        let old = folder.path().join("0.0.1");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join(BINARY), b"old").unwrap();
        assert!(installed(folder.path()).is_none());
        assert!(installed_before(folder.path()));

        let binary = unpack(folder.path(), &archive(BINARY, b"helper")).unwrap();
        assert_eq!(installed(folder.path()), Some(binary.clone()));
        assert_eq!(std::fs::read(&binary).unwrap(), b"helper");
        assert!(!old.exists() && !installed_before(folder.path()));
        assert_eq!(installed_size(folder.path()), Some(6));

        remove(folder.path()).unwrap();
        assert!(installed(folder.path()).is_none());
        remove(folder.path()).unwrap();
    }

    #[test]
    fn an_archive_without_the_helper_installs_nothing() {
        let folder = tempfile::tempdir().unwrap();
        assert!(unpack(folder.path(), &archive("README", b"no")).is_err());
        assert!(installed(folder.path()).is_none());
    }

    #[test]
    fn a_download_whose_signature_does_not_verify_is_refused() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("helper.tar.gz");
        std::fs::write(&file, archive(BINARY, b"helper")).unwrap();
        std::fs::write(
            folder.path().join("helper.tar.gz.sig"),
            include_str!("../updater/fixtures/signed.txt.sig"),
        )
        .unwrap();
        let source = Source {
            urls: vec![format!("file://{}", file.display())],
            public_key: include_str!("../updater/fixtures/test.pub").to_string(),
        };
        assert_eq!(
            size(&source),
            std::fs::metadata(&file).ok().map(|meta| meta.len())
        );
        let error = download(&source, |_| {}).unwrap_err();
        assert!(error.contains("signature"), "{error}");
    }
}
