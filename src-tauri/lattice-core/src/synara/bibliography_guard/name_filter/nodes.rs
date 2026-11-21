//! The files and folders the kernel has looked up through the filter and
//! not yet forgotten, and the descriptors that name their inodes.
//!
//! The kernel forgets an inode only under memory pressure or at unmount, so
//! a walk over a big tree keeps hundreds of thousands looked up. Holding a
//! descriptor for each filled the guard's descriptor table within seconds of
//! a `du` (lattice#1221): the filter's session ended, and with it the
//! sandbox. So only the descriptors used last stay open ([`Descriptors`]),
//! a bounded number; any other node is opened again from its folder, one
//! name at a time and without following a link, as the lookup that made it
//! was, and must turn out to be the same inode.

use super::host::{attributes, descriptor_path, is_procfs, open_child, status_of};
use super::is_bibliography;
use fuser::{Errno, FileAttr, INodeNo};
use std::collections::{BTreeMap, HashMap};
use std::ffi::{CString, OsStr};
use std::io;
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// One file or folder the kernel has looked up and not yet forgotten.
pub(super) struct Node {
    /// The number the kernel knows it by; never given to another node.
    id: u64,
    /// The device and inode numbers.
    pub(super) key: (u64, u64),
    kind: libc::mode_t,
    /// Reached through a `*.bib` name. A hard link gives the same inode
    /// another name, so the rule follows the inode.
    bibliography: AtomicBool,
    /// A procfs folder: nothing under it is served. Its files reach other
    /// processes' memory and roots, which the sandbox mounts its own `/proc`
    /// to keep out.
    pub(super) sealed: bool,
    /// The folder it was last looked up in and its name there, which it is
    /// opened again by. The root has none: its descriptor stays open.
    origin: Mutex<Option<(Arc<Node>, CString)>>,
    /// The root's descriptor, which is never closed.
    held: Option<Arc<OwnedFd>>,
    descriptors: Arc<Descriptors>,
}

/// An `O_PATH | O_NOFOLLOW` descriptor that names a node's inode, open for
/// as long as this lives even when [`Descriptors`] closes its own.
pub(super) struct Descriptor(Arc<OwnedFd>);

impl AsRawFd for Descriptor {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

impl Descriptor {
    /// The descriptor's `/proc/self/fd` entry, which reopens the inode with
    /// real access or names it for a call that takes only a path. Valid
    /// while this lives.
    pub(super) fn path(&self) -> CString {
        descriptor_path(self.as_raw_fd())
    }
}

impl Node {
    pub(super) fn is(&self, kind: libc::mode_t) -> bool {
        self.kind == kind
    }

    pub(super) fn is_bibliography(&self) -> bool {
        self.bibliography.load(Ordering::Relaxed)
    }

    /// A descriptor that names this node's inode: the open one, or one opened
    /// again by name in its folder. ESTALE when that name now holds another
    /// inode, or none: the kernel looks the name up again.
    pub(super) fn fd(&self) -> Result<Descriptor, Errno> {
        if let Some(fd) = &self.held {
            return Ok(Descriptor(Arc::clone(fd)));
        }
        if let Some(fd) = self.descriptors.get(self.id) {
            return Ok(Descriptor(fd));
        }
        let (folder, name) = lock(&self.origin).clone().ok_or(Errno::ESTALE)?;
        let folder = folder.fd()?;
        let fd = match self.descriptors.with_room(|| open_child(folder.as_raw_fd(), &name)) {
            Ok(fd) => fd,
            Err(error) if error.raw_os_error() == Some(libc::ENOENT) => return Err(Errno::ESTALE),
            Err(error) => return Err(error.into()),
        };
        let status = status_of(fd.as_raw_fd())?;
        if (status.st_dev, status.st_ino) != self.key || status.st_mode & libc::S_IFMT != self.kind
        {
            return Err(Errno::ESTALE);
        }
        let fd = Arc::new(fd);
        self.descriptors.keep(self.id, Arc::clone(&fd));
        Ok(Descriptor(fd))
    }
}

/// The descriptors used last, by node, at most `capacity` of them.
pub(super) struct Descriptors {
    capacity: usize,
    open: Mutex<Open>,
}

#[derive(Default)]
struct Open {
    /// By node, with when each was last used.
    by_node: HashMap<u64, (Arc<OwnedFd>, u64)>,
    by_use: BTreeMap<u64, u64>,
    clock: u64,
}

impl Open {
    fn touch(&mut self, id: u64) -> Option<Arc<OwnedFd>> {
        self.clock += 1;
        let (fd, used) = self.by_node.get_mut(&id)?;
        self.by_use.remove(used);
        *used = self.clock;
        self.by_use.insert(self.clock, id);
        Some(Arc::clone(fd))
    }

    fn close(&mut self, id: u64) {
        if let Some((_, used)) = self.by_node.remove(&id) {
            self.by_use.remove(&used);
        }
    }

    fn close_oldest(&mut self) {
        if let Some((_, id)) = self.by_use.pop_first() {
            self.by_node.remove(&id);
        }
    }
}

impl Descriptors {
    fn new(capacity: usize) -> Self {
        Self { capacity: capacity.max(1), open: Mutex::default() }
    }

    fn get(&self, id: u64) -> Option<Arc<OwnedFd>> {
        lock(&self.open).touch(id)
    }

    fn keep(&self, id: u64, fd: Arc<OwnedFd>) {
        let mut open = lock(&self.open);
        open.close(id);
        while open.by_node.len() >= self.capacity {
            open.close_oldest();
        }
        open.clock += 1;
        let used = open.clock;
        open.by_node.insert(id, (fd, used));
        open.by_use.insert(used, id);
    }

    fn close(&self, id: u64) {
        lock(&self.open).close(id);
    }

    /// Run `open`, and once more after closing half of these when the
    /// process is out of descriptors: what the sandbox opens comes before
    /// what can be opened again.
    pub(super) fn with_room<T>(&self, open: impl Fn() -> io::Result<T>) -> io::Result<T> {
        match open() {
            Err(error) if matches!(error.raw_os_error(), Some(libc::EMFILE | libc::ENFILE)) => {
                let mut opened = lock(&self.open);
                let keep = opened.by_node.len() / 2;
                while opened.by_node.len() > keep {
                    opened.close_oldest();
                }
                drop(opened);
                open()
            }
            result => result,
        }
    }

    #[cfg(test)]
    pub(super) fn open_count(&self) -> usize {
        lock(&self.open).by_node.len()
    }
}

/// A lock that a panic in one request does not take, so it does not take
/// the mount and the sandbox's whole file system with it.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Default)]
struct Table {
    /// By the number the kernel knows it by, with how many lookups it holds.
    by_id: HashMap<u64, (Arc<Node>, u64)>,
    by_key: HashMap<(u64, u64), u64>,
    last_id: u64,
}

pub(super) struct Nodes {
    table: Mutex<Table>,
    descriptors: Arc<Descriptors>,
}

impl Nodes {
    /// A table whose root, the kernel's node 1, is the folder `root` names,
    /// keeping at most `open` other descriptors open.
    pub(super) fn new(root: OwnedFd, open: usize) -> io::Result<Self> {
        let status = status_of(root.as_raw_fd())?;
        let descriptors = Arc::new(Descriptors::new(open));
        let root = Node {
            id: INodeNo::ROOT.0,
            key: (status.st_dev, status.st_ino),
            kind: status.st_mode & libc::S_IFMT,
            bibliography: AtomicBool::new(false),
            sealed: false,
            origin: Mutex::new(None),
            held: Some(Arc::new(root)),
            descriptors: Arc::clone(&descriptors),
        };
        let mut table = Table::default();
        table.by_key.insert(root.key, root.id);
        table.by_id.insert(root.id, (Arc::new(root), 1));
        table.last_id = INodeNo::ROOT.0;
        Ok(Self { table: Mutex::new(table), descriptors })
    }

    pub(super) fn descriptors(&self) -> &Descriptors {
        &self.descriptors
    }

    pub(super) fn get(&self, id: INodeNo) -> Result<Arc<Node>, Errno> {
        lock(&self.table).by_id.get(&id.0).map(|(node, _)| Arc::clone(node)).ok_or(Errno::ENOENT)
    }

    /// Record one more kernel lookup of the inode `fd` names, which was
    /// reached as `name` in `parent`, and answer its number and attributes.
    pub(super) fn remember(
        &self, parent: &Arc<Node>, name: &OsStr, fd: OwnedFd,
    ) -> Result<(INodeNo, FileAttr, Arc<Node>), Errno> {
        let status = status_of(fd.as_raw_fd())?;
        let key = (status.st_dev, status.st_ino);
        let kind = status.st_mode & libc::S_IFMT;
        // Another file system starts here. Only a mount can put procfs in the
        // tree, so this is the one place to look for it.
        let sealed = kind == libc::S_IFDIR && key.0 != parent.key.0 && is_procfs(fd.as_raw_fd())?;
        let origin =
            Some((Arc::clone(parent), CString::new(name.as_bytes()).map_err(|_| Errno::EINVAL)?));
        let mut table = lock(&self.table);
        let known = table.by_key.get(&key).copied().and_then(|id| {
            let (node, lookups) = table.by_id.get_mut(&id)?;
            // With no descriptor open on it, the inode a node named can
            // have been removed and its number given to a new one, which
            // can be another kind of file.
            (node.kind == kind && node.sealed == sealed).then(|| {
                *lookups += 1;
                Arc::clone(node)
            })
        });
        let node = match known {
            Some(node) => {
                *lock(&node.origin) = origin;
                // Its open descriptor, if it has one, is this same inode.
                if self.descriptors.get(node.id).is_none() {
                    self.descriptors.keep(node.id, Arc::new(fd));
                }
                node
            }
            None => {
                table.last_id += 1;
                let id = table.last_id;
                let node = Arc::new(Node {
                    id,
                    key,
                    kind,
                    bibliography: AtomicBool::new(false),
                    sealed,
                    origin: Mutex::new(origin),
                    held: None,
                    descriptors: Arc::clone(&self.descriptors),
                });
                table.by_key.insert(key, id);
                table.by_id.insert(id, (Arc::clone(&node), 1));
                self.descriptors.keep(id, Arc::new(fd));
                node
            }
        };
        drop(table);
        // With one name the inode is a bibliography exactly when that name
        // is; with several, when any of them has been.
        if kind == libc::S_IFDIR || status.st_nlink <= 1 {
            node.bibliography.store(is_bibliography(name), Ordering::Relaxed);
        } else if is_bibliography(name) {
            node.bibliography.store(true, Ordering::Relaxed);
        }
        Ok((INodeNo(node.id), attributes(&status), node))
    }

    /// Note that the inode now at `name` in `folder` was moved there, so a
    /// node of it is opened again by that name.
    pub(super) fn moved(&self, folder: &Arc<Node>, name: &OsStr) {
        let Ok(c_name) = CString::new(name.as_bytes()) else { return };
        let Ok(folder_fd) = folder.fd() else { return };
        let Ok(fd) = self.descriptors.with_room(|| open_child(folder_fd.as_raw_fd(), &c_name))
        else {
            return;
        };
        let Ok(status) = status_of(fd.as_raw_fd()) else { return };
        let table = lock(&self.table);
        let Some(id) = table.by_key.get(&(status.st_dev, status.st_ino)) else { return };
        if let Some((node, _)) = table.by_id.get(id) {
            *lock(&node.origin) = Some((Arc::clone(folder), c_name));
        }
    }

    /// Drop `count` kernel lookups of a node, and the node with its last.
    pub(super) fn forget(&self, id: INodeNo, count: u64) {
        if id == INodeNo::ROOT {
            return;
        }
        let mut table = lock(&self.table);
        let Some((node, lookups)) = table.by_id.get_mut(&id.0) else { return };
        *lookups = lookups.saturating_sub(count);
        if *lookups == 0 {
            let key = node.key;
            table.by_id.remove(&id.0);
            // A newer node can have the key since (`remember`).
            if table.by_key.get(&key) == Some(&id.0) {
                table.by_key.remove(&key);
            }
            drop(table);
            self.descriptors.close(id.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::host::open_folder;
    use super::*;
    use crate::test_support::TempDir;
    use std::fs;

    const OPEN: usize = 16;

    fn lookup(nodes: &Nodes, parent: &Arc<Node>, name: &str) -> Result<Arc<Node>, Errno> {
        let c_name = CString::new(name).unwrap();
        let fd = open_child(parent.fd()?.as_raw_fd(), &c_name)?;
        Ok(nodes.remember(parent, OsStr::new(name), fd)?.2)
    }

    /// However many inodes the kernel holds looked up, only the last few
    /// keep a descriptor, and each of the others is opened again on use.
    #[test]
    fn a_walk_keeps_a_bounded_number_of_descriptors_open() {
        let scratch = TempDir::new("bib-guard-nodes");
        for folder in 0..10 {
            for file in 0..30 {
                let folder = scratch.join(format!("f{folder}"));
                fs::create_dir_all(&folder).unwrap();
                fs::write(folder.join(format!("{file}.txt")), "x").unwrap();
            }
        }
        let nodes = Nodes::new(open_folder(&scratch).unwrap(), OPEN).unwrap();
        let root = nodes.get(INodeNo::ROOT).unwrap();
        let mut looked_up = Vec::new();
        for folder in 0..10 {
            let folder = lookup(&nodes, &root, &format!("f{folder}")).unwrap();
            for file in 0..30 {
                looked_up.push(lookup(&nodes, &folder, &format!("{file}.txt")).unwrap());
                assert!(nodes.descriptors.open_count() <= OPEN);
            }
        }
        for node in &looked_up {
            let status = status_of(node.fd().unwrap().as_raw_fd()).unwrap();
            assert_eq!((status.st_dev, status.st_ino), node.key);
            assert!(nodes.descriptors.open_count() <= OPEN);
        }
    }

    /// A node whose descriptor was closed is opened again only as the inode
    /// it was: not as what was put at its name since.
    #[test]
    fn a_node_opened_again_must_be_the_same_inode() {
        let scratch = TempDir::new("bib-guard-nodes-stale");
        fs::write(scratch.join("a.txt"), "a").unwrap();
        let nodes = Nodes::new(open_folder(&scratch).unwrap(), 1).unwrap();
        let root = nodes.get(INodeNo::ROOT).unwrap();
        let node = lookup(&nodes, &root, "a.txt").unwrap();
        fs::write(scratch.join("b.txt"), "b").unwrap();
        // Its descriptor goes to the newer lookup.
        lookup(&nodes, &root, "b.txt").unwrap();

        fs::rename(scratch.join("a.txt"), scratch.join("moved.txt")).unwrap();
        assert_eq!(node.fd().err(), Some(Errno::ESTALE));
        fs::rename(scratch.join("b.txt"), scratch.join("a.txt")).unwrap();
        assert_eq!(node.fd().err(), Some(Errno::ESTALE));
        fs::rename(scratch.join("a.txt"), scratch.join("b.txt")).unwrap();
        fs::rename(scratch.join("moved.txt"), scratch.join("a.txt")).unwrap();
        assert!(node.fd().is_ok());

        // Moved through the filter, it is found under its new name.
        fs::rename(scratch.join("a.txt"), scratch.join("c.txt")).unwrap();
        lookup(&nodes, &root, "b.txt").unwrap();
        nodes.moved(&root, OsStr::new("c.txt"));
        assert!(node.fd().is_ok());
    }

    /// Out of descriptors, the ones that can be opened again make room.
    #[test]
    fn running_out_of_descriptors_closes_the_oldest_first() {
        let scratch = TempDir::new("bib-guard-nodes-room");
        let nodes = Nodes::new(open_folder(&scratch).unwrap(), OPEN).unwrap();
        let root = nodes.get(INodeNo::ROOT).unwrap();
        for file in 0..OPEN {
            fs::write(scratch.join(format!("{file}")), "x").unwrap();
            lookup(&nodes, &root, &format!("{file}")).unwrap();
        }
        assert_eq!(nodes.descriptors.open_count(), OPEN);
        let attempts = std::cell::Cell::new(0);
        let opened = nodes.descriptors.with_room(|| {
            attempts.set(attempts.get() + 1);
            if attempts.get() == 1 {
                Err(io::Error::from_raw_os_error(libc::EMFILE))
            } else {
                Ok(())
            }
        });
        assert!(opened.is_ok());
        assert_eq!(nodes.descriptors.open_count(), OPEN / 2);
    }
}
