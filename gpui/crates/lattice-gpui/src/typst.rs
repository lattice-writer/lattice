//! Typst support, downloaded on first use.
//!
//! The Typst compiler and its fonts would add about 40 MB to every copy of
//! Lattice, most of whose writers never open a .typ, so they ship as their
//! own release asset: the `lattice-typst` helper (`install`), which the app
//! starts and keeps running to compile (`helper`). A .typ opens with its
//! highlighting either way. The first build or live compile of one asks to
//! download the helper, with its size ("Typst support needs a one-time
//! download (~N MB). Download now?"); Download fetches it with its progress,
//! checks it with the updater's key, installs it and runs the compile that
//! asked; Not now leaves the project as it was, and typing stops asking for
//! the rest of the session (a build asks again). A writer who said yes once
//! is not asked again when an update of the app needs the helper of its
//! version. Settings › Editor & builds removes it or downloads it again.
//!
//! `LATTICE_TYPST_HELPER` names a helper to use as installed (a local
//! build, for development and the e2e, which then needs no network); a
//! release build reads it only in a scripted run.

pub mod helper;
pub mod install;

use std::path::PathBuf;

use gpui::{App, AppContext as _, Global, Task};
use lattice_i18n::t;
use lattice_ui::components::{ToastLevel, ToastProgress};

use crate::{
    confirm::{self, Confirm},
    notices::{self, Notice},
};

const NOTICE_KEY: &str = "typst-support";

/// Where Typst support is.
#[derive(Clone, Debug, PartialEq)]
pub enum Phase {
    Idle,
    /// The question is up, or its size is being looked up.
    Asking,
    Downloading(Option<f32>),
    Installing,
}

pub struct TypstSupport {
    folder: PathBuf,
    /// A helper to use as installed (`LATTICE_TYPST_HELPER`).
    dev: Option<PathBuf>,
    source: Option<install::Source>,
    phase: Phase,
    /// Not now, this session: typing compiles nothing until a build asks.
    declined: bool,
    /// A download failed this session: typing stops fetching (and saying
    /// so) on every pause, and a build asks again instead of fetching.
    fetch_failed: bool,
    /// This version's helper was downloaded again this session because the
    /// installed one was outdated: a download that is outdated too is not
    /// fetched again on every build.
    replaced_outdated: bool,
    /// What runs once the helper is installed: the compiles that asked.
    waiting: Vec<Box<dyn FnOnce(&mut App)>>,
    _task: Option<Task<()>>,
}

impl Global for TypstSupport {}

fn support_mut(cx: &mut App) -> Option<&mut TypstSupport> {
    if cx.has_global::<TypstSupport>() {
        Some(cx.global_mut::<TypstSupport>())
    } else {
        None
    }
}

/// Set up Typst support: the helper in the app's data folder (a folder of
/// the run's own in a scripted run, which keeps nothing).
pub fn init(scripted: bool, cx: &mut App) {
    let folder = crate::shell::workspaces::state_dir(scripted)
        .unwrap_or_else(|| {
            std::env::temp_dir().join(format!("lattice-scenario-{}", std::process::id()))
        })
        .join("typst");
    let dev = std::env::var_os("LATTICE_TYPST_HELPER")
        .filter(|path| !path.is_empty() && (cfg!(debug_assertions) || scripted))
        .map(PathBuf::from);
    cx.set_global(TypstSupport {
        folder,
        dev,
        source: install::source(),
        phase: Phase::Idle,
        declined: false,
        fetch_failed: false,
        replaced_outdated: false,
        waiting: Vec::new(),
        _task: None,
    });
}

/// The helper a compile runs, if Typst support is installed.
pub fn binary(cx: &App) -> Option<PathBuf> {
    let support = cx.try_global::<TypstSupport>()?;
    support
        .dev
        .clone()
        .or_else(|| install::installed(&support.folder))
}

/// Whether typing in a .typ should compile: Typst support is installed, or
/// its question may still be asked.
pub fn live_compiles(cx: &App) -> bool {
    binary(cx).is_some()
        || cx.try_global::<TypstSupport>().is_some_and(|support| {
            !support.declined && !support.fetch_failed && support.phase == Phase::Idle
        })
}

/// What Settings shows.
pub fn phase(cx: &App) -> Phase {
    cx.try_global::<TypstSupport>()
        .map_or(Phase::Idle, |support| support.phase.clone())
}

/// The installed helper's size, for Settings.
pub fn installed_size(cx: &App) -> Option<u64> {
    let support = cx.try_global::<TypstSupport>()?;
    install::installed_size(&support.folder)
}

/// Whether this Mac can download Typst support at all.
pub fn available(cx: &App) -> bool {
    cx.try_global::<TypstSupport>()
        .is_some_and(|support| support.source.is_some() || support.dev.is_some())
}

/// Point the download elsewhere, for a scenario: a local archive signed with
/// the unit tests' key.
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
pub fn use_source(source: install::Source, cx: &mut App) {
    if let Some(support) = support_mut(cx) {
        support.source = Some(source);
        support.dev = None;
        support.declined = false;
        support.fetch_failed = false;
    }
}

/// Megabytes as the question and Settings say them.
fn megabytes(bytes: u64) -> u64 {
    bytes.div_ceil(1_000_000)
}

/// A compile needs Typst support and it is not installed, or the installed
/// helper is outdated (it speaks an older wire protocol: a build of this
/// version from before the jumps): ask to download it (or download it, for
/// a writer who said yes before), then run `then`.
/// A live compile (`typing`) asks once a session; after Not now it does
/// nothing.
pub fn require(typing: bool, then: impl FnOnce(&mut App) + 'static, cx: &mut App) {
    let Some(support) = support_mut(cx) else {
        return;
    };
    if support.phase != Phase::Idle {
        if !typing {
            support.waiting.push(Box::new(then));
        }
        return;
    }
    if typing && (support.declined || support.fetch_failed) {
        return;
    }
    // A helper named by `LATTICE_TYPST_HELPER` is not replaced by a download;
    // the build already said it is out of date.
    if support.dev.is_some() {
        return;
    }
    let outdated = install::installed(&support.folder).is_some();
    if outdated && support.replaced_outdated {
        // Downloaded again this session and still outdated: say so, rather
        // than fetch it on every build or let a jump do nothing.
        notices::error(
            t!("Typst support"),
            t!("The downloaded Typst support doesn’t match this version of Lattice."),
            cx,
        );
        return;
    }
    let Some(source) = support.source.clone() else {
        notices::error(
            t!("Typst support isn’t available on this computer"),
            t!("Lattice publishes Typst support for Macs with Apple silicon."),
            cx,
        );
        return;
    };
    support.waiting.push(Box::new(then));
    if (outdated || install::installed_before(&support.folder)) && !support.fetch_failed {
        support.replaced_outdated |= outdated;
        download(cx);
        return;
    }
    support.phase = Phase::Asking;
    let size = cx.background_spawn(async move { install::size(&source) });
    let task = cx.spawn(async move |cx| {
        let size = size.await;
        cx.update(|cx| ask(size, cx));
    });
    if let Some(support) = support_mut(cx) {
        support._task = Some(task);
    }
}

fn ask(size: Option<u64>, cx: &mut App) {
    let title = match size {
        Some(bytes) => t!(
            "Typst support needs a one-time download (~{size} MB). Download now?",
            size = megabytes(bytes)
        )
        .to_string(),
        None => t!("Typst support needs a one-time download. Download now?").to_string(),
    };
    confirm::ask(
        Confirm::new(title)
            .body(t!("Lattice compiles Typst with a component it downloads once. Until then, Typst files open with highlighting only. Settings › Editor & builds can remove it."))
            .confirm_label(t!("Download"))
            .cancel_label(t!("Not now"))
            .on_confirm(|_, cx| download(cx))
            .on_cancel(decline),
        cx,
    );
}

/// Not now: the compiles that asked do not run, and typing asks no more.
fn decline(cx: &mut App) {
    if let Some(support) = support_mut(cx) {
        support.phase = Phase::Idle;
        support.declined = true;
        support.waiting.clear();
    }
    cx.refresh_windows();
}

/// Download and install the helper, then run what waits on it.
pub fn download(cx: &mut App) {
    let Some(support) = support_mut(cx) else {
        return;
    };
    let Some(source) = support.source.clone() else {
        return;
    };
    if matches!(support.phase, Phase::Downloading(_) | Phase::Installing) {
        return;
    }
    support.phase = Phase::Downloading(None);
    let folder = support.folder.clone();
    notices::show(
        Notice::new(ToastLevel::Info, t!("Downloading Typst support…"))
            .key(NOTICE_KEY)
            .running(),
        cx,
    );
    cx.refresh_windows();
    let (progress_tx, mut progress_rx) = futures::channel::mpsc::unbounded::<Option<f32>>();
    let work = cx.background_spawn(async move {
        let bytes = install::download(&source, |share| {
            progress_tx.unbounded_send(share).ok();
        })?;
        helper::stop();
        install::unpack(&folder, &bytes)
    });
    let task = cx.spawn(async move |cx| {
        use futures::StreamExt as _;
        while let Some(share) = progress_rx.next().await {
            cx.update(|cx| {
                if let Some(support) = support_mut(cx) {
                    support.phase = Phase::Downloading(share);
                }
                notices::update_progress(
                    NOTICE_KEY,
                    share.map_or(ToastProgress::Indeterminate, ToastProgress::Fraction),
                    cx,
                );
            });
        }
        cx.update(|cx| {
            if let Some(support) = support_mut(cx) {
                support.phase = Phase::Installing;
            }
        });
        let installed = work.await;
        cx.update(|cx| {
            let waiting = support_mut(cx)
                .map(|support| {
                    support.phase = Phase::Idle;
                    support.fetch_failed = installed.is_err();
                    std::mem::take(&mut support.waiting)
                })
                .unwrap_or_default();
            match installed {
                Ok(_) => {
                    notices::show(
                        Notice::new(ToastLevel::Success, t!("Typst support is installed"))
                            .key(NOTICE_KEY),
                        cx,
                    );
                    for then in waiting {
                        then(cx);
                    }
                }
                Err(error) => notices::show(
                    Notice::new(ToastLevel::Error, t!("Couldn’t download Typst support"))
                        .detail(error)
                        .key(NOTICE_KEY),
                    cx,
                ),
            }
            cx.refresh_windows();
        });
    });
    if let Some(support) = support_mut(cx) {
        support._task = Some(task);
    }
}

/// Remove the installed helper, after asking.
pub fn ask_remove(cx: &mut App) {
    confirm::ask(
        Confirm::new(t!("Remove Typst support?"))
            .body(t!("Typst files keep their highlighting. The next Typst build asks to download it again."))
            .confirm_label(t!("Remove"))
            .destructive()
            .on_confirm(|_, cx| remove(cx)),
        cx,
    );
}

/// Stop the helper and delete it, off the UI thread: stopping waits for
/// the compile in progress to notice.
pub fn remove(cx: &mut App) {
    let Some(support) = cx.try_global::<TypstSupport>() else {
        return;
    };
    let folder = support.folder.clone();
    let removed = cx.background_spawn(async move {
        helper::stop();
        install::remove(&folder)
    });
    cx.spawn(async move |cx| {
        let removed = removed.await;
        cx.update(|cx| {
            if let Err(error) = removed {
                notices::error(t!("Couldn’t remove Typst support"), error, cx);
            }
            cx.refresh_windows();
        });
    })
    .detach();
}
