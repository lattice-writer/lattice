//! The compiler's view of a project: Typst's `World`.
//!
//! Files come from the project folder, except those the editor holds unsaved
//! text for (the overlay), and packages (`@preview/...`) from Typst's own
//! package folders, downloaded from Typst Universe on first use into the
//! cache the `typst` CLI uses too. Fonts are the system's, then the ones
//! Typst bundles (Libertinus Serif, New Computer Modern, DejaVu Sans Mono), so
//! a document asking for none still looks as it does everywhere else.

use std::{
    any::Any,
    collections::HashMap,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::LazyLock,
    time::Duration,
};

use lattice_core::models::Diagnostic;
use typst::{
    Library, LibraryExt as _, World,
    diag::{FileResult, Warned},
    foundations::{Bytes, Datetime},
    syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot},
    text::{Font, FontBook},
    utils::LazyHash,
};
use typst_kit::{
    datetime::Time,
    downloader::Downloader,
    files::{FileLoader, FileStore, FsRoot},
    fonts::FontStore,
    packages::SystemPackages,
};
use typst_layout::PagedDocument;

use crate::jump::Kept;

pub(crate) static FONTS: LazyLock<FontStore> = LazyLock::new(|| {
    let started = std::time::Instant::now();
    let mut fonts = FontStore::new();
    fonts.extend(typst_kit::fonts::system());
    fonts.extend(typst_kit::fonts::embedded());
    log::info!(
        "typst: {} fonts in {} ms",
        fonts.book().families().count(),
        started.elapsed().as_millis()
    );
    fonts
});

pub(crate) static LIBRARY: LazyLock<LazyHash<Library>> =
    LazyLock::new(|| LazyHash::new(Library::default()));

/// Scan the fonts ahead of the first build (a project with a Typst root
/// opened), which would otherwise wait for it.
pub fn prewarm_fonts() {
    LazyLock::force(&FONTS);
    LazyLock::force(&LIBRARY);
}

/// What one compile produced.
pub(crate) struct Compiled {
    pub pdf: Option<Vec<u8>>,
    /// The document the PDF is of, for [`Session::keep`].
    pub document: Option<PagedDocument>,
    pub pages: u32,
    pub diagnostics: Vec<Diagnostic>,
}

/// Project files, with the editor's text in place of what is on disk.
struct Loader {
    project: FsRoot,
    packages: SystemPackages,
    overlay: HashMap<String, String>,
}

impl FileLoader for Loader {
    fn load(&self, id: FileId) -> FileResult<Bytes> {
        match id.root() {
            VirtualRoot::Project => {
                let relative = id.vpath().get_without_slash();
                if let Some(text) = self.overlay.get(relative) {
                    return Ok(Bytes::from_string(text.clone()));
                }
                self.project.load(id.vpath())
            }
            VirtualRoot::Package(spec) => self.packages.obtain(spec)?.load(id.vpath()),
        }
    }
}

/// One project's compiler state, kept between builds.
pub(crate) struct Session {
    root: PathBuf,
    document: String,
    main: FileId,
    files: FileStore<Loader>,
    time: Time,
    /// The last document whose PDF was written, for the jumps.
    kept: Option<Kept>,
}

impl Session {
    pub(crate) fn new(root: &Path, document: &str) -> Self {
        let vpath = VirtualPath::new(document).unwrap_or_else(|_| {
            VirtualPath::new("main.typ").expect("a plain file name is a virtual path")
        });
        let main = RootedPath::new(VirtualRoot::Project, vpath).intern();
        let loader = Loader {
            project: FsRoot::new(root.to_path_buf()),
            packages: SystemPackages::new(Http),
            overlay: HashMap::new(),
        };
        Self {
            root: root.to_path_buf(),
            document: document.to_string(),
            main,
            files: FileStore::new(loader),
            time: Time::system(),
            kept: None,
        }
    }

    pub(crate) fn is_for(&self, root: &Path, document: &str) -> bool {
        self.root == root && self.document == document
    }

    pub(crate) fn compile(&mut self, overlay: &HashMap<String, String>) -> Compiled {
        self.files.loader_mut().overlay = overlay.clone();
        // Every file is read again, but a source whose text is the same keeps
        // its parse, and one that changed is re-parsed incrementally.
        self.files.reset();
        self.time.reset();
        let Warned { output, warnings } = typst::compile::<PagedDocument>(&*self);
        let mut diagnostics: Vec<Diagnostic> = Vec::new();
        let (pdf, document, pages) = match output {
            Ok(document) => {
                let pages = u32::try_from(document.pages().len()).unwrap_or(u32::MAX);
                match typst_pdf::pdf(&document, &typst_pdf::PdfOptions::default()) {
                    Ok(bytes) => (Some(bytes), Some(document), pages),
                    Err(errors) => {
                        diagnostics
                            .extend(errors.iter().map(|e| crate::diagnostics::convert(self, e)));
                        (None, None, pages)
                    }
                }
            }
            Err(errors) => {
                diagnostics.extend(errors.iter().map(|e| crate::diagnostics::convert(self, e)));
                (None, None, 0)
            }
        };
        diagnostics.extend(
            warnings
                .iter()
                .map(|w| crate::diagnostics::convert(self, w)),
        );
        Compiled {
            pdf,
            document,
            pages,
            diagnostics,
        }
    }

    /// Keep `document`, whose PDF was just written, with the sources this
    /// compile read: the next compile re-reads them in place.
    pub(crate) fn keep(&mut self, document: PagedDocument) {
        let (_, read) = self.files.dependencies();
        let read: Vec<FileId> = read.collect();
        let sources = read
            .into_iter()
            .filter_map(|id| Some((id, self.files.source(id).ok()?)))
            .collect();
        self.kept = Some(Kept {
            document,
            sources,
            main: self.main,
        });
    }

    /// The document the jumps read, if a PDF was written this session.
    pub(crate) fn kept(&self) -> Option<&Kept> {
        self.kept.as_ref()
    }
}

impl World for Session {
    fn library(&self) -> &LazyHash<Library> {
        &LIBRARY
    }

    fn book(&self) -> &LazyHash<FontBook> {
        FONTS.book()
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        self.files.source(id)
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.files.file(id)
    }

    fn font(&self, index: usize) -> Option<Font> {
        FONTS.font(index)
    }

    fn today(&self, offset: Option<typst::foundations::Duration>) -> Option<Datetime> {
        self.time.today(offset)
    }
}

/// Typst Universe downloads through lattice-core's HTTP client, so they take
/// the system proxy and trust store as every other request Lattice makes.
struct Http;

impl Downloader for Http {
    fn stream(&self, _: &dyn Any, url: &str) -> io::Result<(Option<usize>, Box<dyn Read>)> {
        let client = lattice_core::http::blocking_as("Lattice", Duration::from_secs(60))
            .build()
            .map_err(io::Error::other)?;
        let response = client.get(url).send().map_err(io::Error::other)?;
        if response.status().as_u16() == 404 {
            return Err(io::Error::new(io::ErrorKind::NotFound, url.to_string()));
        }
        let response = response.error_for_status().map_err(io::Error::other)?;
        let size = response
            .content_length()
            .and_then(|length| usize::try_from(length).ok());
        Ok((size, Box::new(response)))
    }
}

/// A file's project-relative path, or `None` for one in a package.
pub(crate) fn project_path(id: FileId) -> Option<String> {
    matches!(id.root(), VirtualRoot::Project).then(|| id.vpath().get_without_slash().to_string())
}
