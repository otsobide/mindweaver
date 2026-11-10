//! Copying a directory, and telling whether two are the same.
//!
//! Shared by gathering, which brings an artifact into the workspace, and
//! installing, which takes it on into a project. Both move whole directories
//! rather than files: a skill's directory belongs to the skill, scripts and
//! references included.
//!
//! [`replace`] stages a replacement beside what it replaces. A [`Stage`]
//! stages it in a scratch folder of its own instead, for when beside is
//! somewhere an agent looks.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::hash::{BuildHasher, Hasher, RandomState};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Copy a directory and everything under it.
///
/// A symlink inside is copied as what it points at (what leaves a clone has
/// to keep working once that clone is replaced), but only when it points
/// somewhere inside `within` (the clone a skill was gathered from, the
/// workspace an installed one comes from) or inside `from` itself. One that
/// points out of both is refused, not followed, because following it is how a
/// key in `~/.ssh` or a `.env` ends up copied into a project that is about to
/// be committed. A link back into a directory already being copied is
/// refused too, rather than copied until the path is too long.
pub fn tree(from: &Path, to: &Path, within: &Path) -> Result<(), io::Error> {
    tree_leaving(from, to, within, &[])
}

/// [`tree`], without whatever is named `leave_out` directly inside `from`.
///
/// Left out rather than copied and then deleted, so what is left out is
/// never read: a link in it pointing out of `within` does not refuse a copy
/// that would never have held it. Only at the top, where a name means what
/// the caller left it out for; further down the same name is somebody's
/// file like any other.
fn tree_leaving(
    from: &Path,
    to: &Path,
    within: &Path,
    leave_out: &[&str],
) -> Result<(), io::Error> {
    let bounds = [fs::canonicalize(within)?, fs::canonicalize(from)?];
    let mut path = Vec::new();
    copy_tree(from, to, &bounds, &mut path, leave_out)
}

fn copy_tree(
    from: &Path,
    to: &Path,
    bounds: &[PathBuf; 2],
    path: &mut Vec<PathBuf>,
    leave_out: &[&str],
) -> Result<(), io::Error> {
    path.push(fs::canonicalize(from)?);
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        if leave_out.iter().any(|name| entry.file_name() == *name) {
            continue;
        }
        let source = entry.path();
        let target = to.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            let (pointed, inside) = followed(&source, bounds)?;
            if !inside {
                return Err(io::Error::other(format!(
                    "{} links to {}, outside {}, so it is not copied",
                    source.display(),
                    pointed.display(),
                    bounds[0].display()
                )));
            }
            if pointed.is_dir() {
                if path.iter().any(|copying| copying.starts_with(&pointed)) {
                    return Err(io::Error::other(format!(
                        "{} links back into a directory it is being copied from, so it is not copied",
                        source.display()
                    )));
                }
                copy_tree(&pointed, &target, bounds, path, &[])?;
            } else {
                fs::copy(&pointed, &target)?;
            }
        } else if kind.is_dir() {
            copy_tree(&source, &target, bounds, path, &[])?;
        } else {
            fs::copy(&source, &target)?;
        }
    }
    path.pop();
    Ok(())
}

/// `path` with every link on the way to it followed, and whether that lies
/// inside one of `bounds`, each a path with its own links followed already
/// (`fs::canonicalize`).
///
/// The one check made before anything a link leads to is read: a copy makes
/// it of each link it meets ([`tree`]), and a read of an artifact's file for
/// the panel makes it of the file ([`crate::contents`]), so neither follows
/// a link the other would refuse. By components, as `Path::starts_with`
/// compares, so `/work/app-old` is not inside `/work/app`.
pub fn followed(path: &Path, bounds: &[PathBuf]) -> Result<(PathBuf, bool), io::Error> {
    let resolved = fs::canonicalize(path)?;
    let inside = bounds.iter().any(|bound| resolved.starts_with(bound));
    Ok((resolved, inside))
}

/// Replace `to` with `from`, saying whether anything actually changed.
///
/// Replaced rather than merged, so a file the source no longer has does not
/// survive as a leftover of an older revision. An identical directory is left
/// alone entirely, down to its modification times, which is what lets a second
/// run report what moved rather than reporting everything.
///
/// All or nothing: the copy is made beside `to` and only put in its place
/// once it is whole, so a copy that fails (a refused link, a full disk)
/// leaves `to` exactly as it was, and leaves nothing where there was nothing.
/// `within` bounds the symlinks followed, as for [`tree`].
pub fn replace(from: &Path, to: &Path, within: &Path) -> Result<Change, io::Error> {
    replace_leaving(from, to, within, &[])
}

/// [`replace`], without whatever is named `leave_out` directly inside
/// `from`, as `tree_leaving` leaves it out: what a gather leaves out of the
/// shelf's copy of a skill ([`crate::gather`]). Compared without it too, so
/// a copy that holds all the rest is unchanged.
pub(crate) fn replace_leaving(
    from: &Path,
    to: &Path,
    within: &Path,
    leave_out: &[&str],
) -> Result<Change, io::Error> {
    let old = beside(to, "old");
    let change = put(from, to, within, leave_out, &beside(to, "new"), &old)?;
    if change == Change::Updated {
        fs::remove_dir_all(&old)?;
    }
    Ok(change)
}

/// [`replace`] for one file: `to` made to hold what `from` holds, read
/// through a link at `from`, which its caller has held to where it may lead,
/// and written at `to` as it is spelled, never through a link there: what a
/// gather puts on the shelf of a rule ([`crate::gather`]).
///
/// Written whole through a temporary beside `to` and a rename, so a write
/// that fails leaves `to` as it was; left alone, down to its modification
/// time, when it holds the same bytes already, so a second gather says
/// what moved.
pub(crate) fn replace_file(from: &Path, to: &Path) -> Result<Change, io::Error> {
    let bytes = fs::read(from)?;
    let change = file_change(&bytes, to)?;
    if change == Change::Unchanged {
        return Ok(change);
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    crate::workspace::replace_bytes_unresolved(to, &bytes).map_err(|(path, error)| {
        io::Error::new(
            error.kind(),
            format!("cannot write {}: {error}", path.display()),
        )
    })?;
    Ok(change)
}

/// What [`replace_file`] would do of `from` at `to`, writing nothing: what a
/// gather's dry run says it would do with a rule ([`crate::gather::dry_run`]).
pub(crate) fn compared_file(from: &Path, to: &Path) -> Result<Change, io::Error> {
    file_change(&fs::read(from)?, to)
}

/// What putting `bytes` at `to` is: an addition where nothing is, nothing
/// where a file holds them already, and an update over anything else, a
/// link or a folder included, which the write then refuses or replaces.
fn file_change(bytes: &[u8], to: &Path) -> Result<Change, io::Error> {
    Ok(match fs::symlink_metadata(to) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Change::Added,
        Err(error) => return Err(error),
        Ok(metadata) if metadata.is_file() && fs::read(to)? == bytes => Change::Unchanged,
        Ok(_) => Change::Updated,
    })
}

/// What replacing `to` with `copy`, a whole copy already made, would be,
/// replacing nothing: what a gather's dry run says it would do with a skill
/// it copied somewhere of its own ([`crate::gather::dry_run`]).
pub(crate) fn compared(copy: &Path, to: &Path) -> Result<Change, io::Error> {
    if !to.exists() {
        return Ok(Change::Added);
    }
    Ok(if same(copy, to)? {
        Change::Unchanged
    } else {
        Change::Updated
    })
}

/// Build `from` at `new`, without what is named `leave_out` at its top, and
/// put it at `to`, moving what `to` held to `old` first, which is left for
/// the caller to clear: what a leftover there costs depends on where `old`
/// is.
///
/// Wherever the two are, they have to be on the filesystem `to` is on, so
/// that putting the copy in place is a rename rather than a second copy that
/// could fail halfway.
fn put(
    from: &Path,
    to: &Path,
    within: &Path,
    leave_out: &[&str],
    new: &Path,
    old: &Path,
) -> Result<Change, io::Error> {
    let existed = to.exists();
    if existed && same_leaving(from, to, leave_out)? {
        return Ok(Change::Unchanged);
    }
    if new.exists() {
        fs::remove_dir_all(new)?;
    }
    if let Err(error) = tree_leaving(from, new, within, leave_out) {
        let _ = fs::remove_dir_all(new);
        return Err(error);
    }
    // Made here rather than on the way to `new`, which need not be beside
    // `to`: a project whose skills folder is not there yet, as in a fresh
    // clone, still gets one.
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    if !existed {
        if let Err(error) = moved(new, to) {
            let _ = fs::remove_dir_all(new);
            return Err(error);
        }
        return Ok(Change::Added);
    }
    // Moved aside rather than deleted first: the moment with neither copy in
    // place lasts from one rename to the next rather than a whole delete, and
    // a copy that cannot be put in place can have the old one put back.
    if old.exists() {
        fs::remove_dir_all(old)?;
    }
    if let Err(error) = moved(to, old) {
        let _ = fs::remove_dir_all(new);
        return Err(error);
    }
    if let Err(error) = moved(new, to) {
        let _ = fs::rename(old, to);
        let _ = fs::remove_dir_all(new);
        return Err(error);
    }
    Ok(Change::Updated)
}

/// `fs::rename`, saying what it was moving where when it fails.
///
/// A rename is what puts a whole copy in place at once, and it cannot cross
/// filesystems: a scratch folder on another one than what it replaces is an
/// error that names both, not a second copy that could fail halfway.
fn moved(from: &Path, to: &Path) -> Result<(), io::Error> {
    fs::rename(from, to).map_err(|error| {
        let why = if error.kind() == io::ErrorKind::CrossesDevices {
            format!("they are on different filesystems ({error})")
        } else {
            error.to_string()
        };
        io::Error::new(
            error.kind(),
            format!("cannot move {} to {}: {why}", from.display(), to.display()),
        )
    })
}

/// One run's own folder in a scratch folder, to build replacements in
/// somewhere other than beside what they replace.
///
/// Beside is where an agent looks. OpenCode's first line globs hidden folders
/// in a skills folder too, so a half-made `.deploy.mindweaver-new` next to
/// `deploy` is a skill it may load, and one an interrupted run leaves there it
/// loads every time after. Built in a scratch folder Mindweaver owns, a copy is out of
/// every agent's sight until a rename puts it in place whole, and what an
/// interrupted run leaves is out of sight too.
///
/// Named after the run, by its process id and a random suffix, rather than
/// after what it replaces, so two runs replacing the same thing at once never
/// build in one folder. Each run holds the scratch folder's lock shared for as
/// long as its stage lives. A run that can take that lock exclusively is alone,
/// so every stage it finds was left by a run that was interrupted, and it
/// clears them before taking its own ([`Stage::swept`]); a run that cannot
/// sweeps nothing, since what it finds may be in use. A lock rather than the
/// process ids in the names, because whether a process is still running is a
/// question each platform answers differently, and a lock released when its
/// process dies answers it on all of them.
///
/// The scratch folder has to be on the filesystem of what is replaced, for
/// the rename to be a move.
#[derive(Debug)]
pub struct Stage {
    dir: PathBuf,
    swept: Vec<PathBuf>,
    // Only ever dropped: closing it releases the lock, after `drop` has
    // removed the folder, so no sweep ever finds a stage being cleared.
    _lock: Option<File>,
}

/// Why [`Stage::place`] put no copy in place, and at which target.
///
/// The target is said apart from the error because the error often does not
/// name it: making the folder a copy goes in fails with what the filesystem
/// says, which names no path, and with several targets the caller cannot
/// tell which one was broken.
///
/// Core's alone, as placing is: each caller turns it into an error of its
/// own, one with a code (`code`), so no front end ever meets this one, and it
/// needs none.
#[derive(Debug)]
pub(crate) struct PlaceError {
    /// The target it failed at, as `to` names it: one that could not be
    /// cleared beside, compared or put in place. None when the copy itself
    /// could not be built, which is no target's doing.
    pub target: Option<PathBuf>,
    pub source: io::Error,
}

impl PlaceError {
    fn at(target: &Path) -> impl FnOnce(io::Error) -> PlaceError + '_ {
        move |source| PlaceError {
            target: Some(target.to_path_buf()),
            source,
        }
    }

    fn building(source: io::Error) -> PlaceError {
        PlaceError {
            target: None,
            source,
        }
    }
}

/// The `.gitignore` a scratch folder is given, so nothing staged in it is
/// committed: `*`, as a Python venv ignores itself, inside the folder it is
/// about rather than in a team's `.gitignore`.
const IGNORE: &str = "\
# Mindweaver builds what it installs here, then moves it into place.
# Nothing in this folder is meant to be committed.
*
";

/// The file in a scratch folder every run with a stage there holds a lock on.
const LOCK: &str = ".lock";

impl Stage {
    /// Take a folder of one's own in `scratch`, making `scratch` first if it
    /// is not there, and clearing what interrupted runs left in it when no
    /// other run is using it.
    pub fn open(scratch: &Path) -> Result<Stage, io::Error> {
        fs::create_dir_all(scratch)?;
        ignore(scratch)?;
        let (lock, swept) = hold(scratch)?;
        let dir = claim(scratch)?;
        Ok(Stage {
            dir,
            swept,
            _lock: lock,
        })
    }

    /// The folder this run builds in.
    pub fn path(&self) -> &Path {
        &self.dir
    }

    /// What interrupted runs had left that this stage cleared: the stages in
    /// the scratch folder when it was opened, then what earlier versions had
    /// staged beside each target it replaced.
    pub fn swept(&self) -> &[PathBuf] {
        &self.swept
    }

    /// [`replace`], building the copy in this stage instead of beside `to`.
    ///
    /// What an interrupted run of an earlier version left beside `to`, where
    /// they all staged, is cleared first ([`Stage::swept`]).
    ///
    /// Mutable, because a stage builds one copy at a time in one place.
    pub fn replace(&mut self, from: &Path, to: &Path, within: &Path) -> Result<Change, io::Error> {
        let changes = self
            .place(from, std::slice::from_ref(&to.to_path_buf()), within, &[])
            .map_err(|error| error.source)?;
        Ok(changes[0])
    }

    /// [`replace`] for several targets at once, and all of them or none: a
    /// copy of `from` at each of `to`, every one the same bytes.
    ///
    /// Built once, in this stage, and copied from there for each target that
    /// does not already hold it. Copied from `from` each time, two copies
    /// could differ when `from` changes in between, a workspace's own skill
    /// being edited say, and an agent reading both folders (OpenCode reads
    /// `.agents/skills` and `.claude/skills`) would be handed two versions of
    /// one skill. Each target is compared with that copy rather than with
    /// `from`, so one that already holds it is left alone, down to its times,
    /// and the others are replaced.
    ///
    /// Only once every copy is whole is any put in place, a rename each, and
    /// a target that cannot be put in place takes every one already moved
    /// back out again: each target is left as it was, the copy it had put
    /// back where it replaced one. The folder a target goes in, made on the
    /// way, is left.
    ///
    /// What is named `leave_out` directly inside `from` goes into none of
    /// the copies and is never read, as `tree_leaving` leaves it out: which
    /// names, and why, is the caller's to say.
    ///
    /// The changes come back in the order of `to`, which has to name each
    /// folder on disk once: two spellings of one folder would be put in
    /// place twice, and the second rename would find the first's copy there.
    /// What an interrupted run of an earlier version left beside each target
    /// is cleared first, as for [`Stage::replace`].
    ///
    /// A failure says which target it was at ([`PlaceError::target`]).
    pub(crate) fn place(
        &mut self,
        from: &Path,
        to: &[PathBuf],
        within: &Path,
        leave_out: &[&str],
    ) -> Result<Vec<Change>, PlaceError> {
        self.place_over(from, to, within, leave_out, &[])
    }

    /// [`Stage::place`], replacing each of `to` also named in `over` even
    /// where it holds the same names and bytes, which it is then said to
    /// have been updated: for a target its caller knows is not the copy it
    /// writes though [`same`] says it is, a copy holding a link to the same
    /// bytes, which [`same`] follows.
    pub(crate) fn place_over(
        &mut self,
        from: &Path,
        to: &[PathBuf],
        within: &Path,
        leave_out: &[&str],
        over: &[PathBuf],
    ) -> Result<Vec<Change>, PlaceError> {
        for target in to {
            self.clear_beside(target).map_err(PlaceError::at(target))?;
        }
        let built = self.fresh("new").map_err(PlaceError::building)?;
        if let Err(error) = tree_leaving(from, &built, within, leave_out) {
            let _ = fs::remove_dir_all(&built);
            return Err(PlaceError::building(error));
        }

        let mut changes = Vec::with_capacity(to.len());
        let mut moving = Vec::new();
        for (index, target) in to.iter().enumerate() {
            let change = match target.exists() {
                true if !over.contains(target)
                    && same(&built, target).map_err(PlaceError::at(target))? =>
                {
                    Change::Unchanged
                }
                true => Change::Updated,
                false => Change::Added,
            };
            if change != Change::Unchanged {
                moving.push(index);
            }
            changes.push(change);
        }
        if moving.is_empty() {
            self.clear(&[built]);
            return Ok(changes);
        }

        // Every copy but the last is copied from what was built, before
        // anything moves; the last is what was built.
        let mut copies = Vec::with_capacity(moving.len());
        for (count, &index) in moving.iter().enumerate() {
            if count + 1 == moving.len() {
                copies.push(built.clone());
                continue;
            }
            let copy = self
                .fresh(&format!("new-{index}"))
                .map_err(PlaceError::building)?;
            if let Err(error) = tree(&built, &copy, &built) {
                self.clear(&copies);
                let _ = fs::remove_dir_all(&copy);
                let _ = fs::remove_dir_all(&built);
                return Err(PlaceError::building(error));
            }
            copies.push(copy);
        }

        let mut placed: Vec<(&Path, Option<PathBuf>)> = Vec::with_capacity(moving.len());
        for (&index, copy) in moving.iter().zip(&copies) {
            let target = to[index].as_path();
            match self.put_in_place(copy, target, index) {
                Ok(old) => placed.push((target, old)),
                Err(error) => {
                    self.take_back(&placed);
                    self.clear(&copies);
                    return Err(PlaceError::at(target)(error));
                }
            }
        }
        // Every new copy is in place whatever happens here, so an old one
        // that will not go is litter in the stage, not a failed install:
        // dropping the stage tries again, and the next run sweeps what that
        // leaves.
        for (_, old) in placed {
            if let Some(old) = old {
                let _ = fs::remove_dir_all(old);
            }
        }
        Ok(changes)
    }

    /// Move `copy` to `target`, moving what `target` holds aside into the
    /// stage first and saying where, so it can be put back or cleared.
    fn put_in_place(
        &self,
        copy: &Path,
        target: &Path,
        index: usize,
    ) -> Result<Option<PathBuf>, io::Error> {
        // Made here rather than on the way to the copy, which is in the
        // stage: a project whose skills folder is not there yet, as in a
        // fresh clone, still gets one.
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        if !target.exists() {
            moved(copy, target)?;
            return Ok(None);
        }
        // Moved aside rather than deleted first, as `put` does it: the moment
        // with neither copy in place lasts from one rename to the next, and a
        // copy that cannot be put in place can have the old one put back.
        let old = self.fresh(&format!("old-{index}"))?;
        moved(target, &old)?;
        if let Err(error) = moved(copy, target) {
            let _ = fs::rename(&old, target);
            return Err(error);
        }
        Ok(Some(old))
    }

    /// Undo what [`Stage::place`] put in place, last first: each new copy
    /// moved back into the stage, where dropping it clears it, and the copy
    /// it replaced moved back where it was.
    ///
    /// As far as it can: a rename that fails here leaves that target with
    /// the new copy, or with none, which is no worse than stopping, and the
    /// error that brought it here is the one said.
    fn take_back(&self, placed: &[(&Path, Option<PathBuf>)]) {
        for (index, (target, old)) in placed.iter().enumerate().rev() {
            if let Ok(undone) = self.fresh(&format!("undone-{index}")) {
                let _ = fs::rename(target, undone);
            }
            if let Some(old) = old {
                let _ = fs::rename(old, target);
            }
        }
    }

    /// Remove what this stage built, as far as it can.
    fn clear(&self, built: &[PathBuf]) {
        for path in built {
            let _ = fs::remove_dir_all(path);
        }
    }

    /// A path in this stage named `name`, cleared of anything an earlier
    /// replacement through it left there.
    fn fresh(&self, name: &str) -> Result<PathBuf, io::Error> {
        let path = self.dir.join(name);
        if fs::symlink_metadata(&path).is_ok() {
            fs::remove_dir_all(&path)?;
        }
        Ok(path)
    }

    /// Remove the `.<name>.mindweaver-new` and `-old` that an interrupted run
    /// of an earlier version left beside `to`, and the `.<name>.mindflayer-new`
    /// and `-old` of one from before Mindweaver's names (D-26): a skill an
    /// agent may load, in a folder no sweep looks in.
    ///
    /// Without the lock a sweep waits for, because no run of this version
    /// builds there, and runs of those versions cleared each other's anyway.
    /// One that will not go fails the replacement, as it failed theirs, rather
    /// than staying where an agent reads; one another run removed first is
    /// that run's to say.
    fn clear_beside(&mut self, to: &Path) -> Result<(), io::Error> {
        let spelled = [BESIDE, FLAYER_BESIDE];
        for left in spelled
            .into_iter()
            .flat_map(|by| ["new", "old"].map(|what| beside_by(to, by, what)))
        {
            // A folder, not a link: no version ever staged one.
            if !fs::symlink_metadata(&left).is_ok_and(|meta| meta.is_dir()) {
                continue;
            }
            match fs::remove_dir_all(&left) {
                Ok(()) => self.swept.push(left),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(io::Error::new(
                        error.kind(),
                        format!(
                            "cannot clear {}, left by an install that did not finish: {error}",
                            left.display()
                        ),
                    ))
                }
            }
        }
        Ok(())
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Give `scratch` the `.gitignore` that keeps it out of git, unless it has
/// one. `create_new`, so one somebody changed is left as they left it.
fn ignore(scratch: &Path) -> Result<(), io::Error> {
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(scratch.join(".gitignore"))
    {
        Ok(mut file) => file.write_all(IGNORE.as_bytes()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
    }
}

/// Hold `scratch`'s lock shared, sweeping first if it can be had exclusively.
///
/// Released between the two rather than downgraded, which `std` cannot do: a
/// run that takes the lock in between finds nothing of this one's to sweep,
/// because this one has no folder yet.
fn hold(scratch: &Path) -> Result<(Option<File>, Vec<PathBuf>), io::Error> {
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(scratch.join(LOCK))?;
    match lock.try_lock() {
        Ok(()) => {
            let swept = sweep(scratch);
            lock.unlock()?;
            lock.lock_shared()?;
            Ok((Some(lock), swept))
        }
        Err(TryLockError::WouldBlock) => {
            lock.lock_shared()?;
            Ok((Some(lock), Vec::new()))
        }
        // A filesystem without locks. No run there can tell a live stage from
        // a leftover, so none sweeps, and the names still keep runs apart.
        // Any error, not only `Unsupported`: NFS without its lock daemon says
        // "no locks available", which `std` leaves uncategorized, and failing
        // on it would fail every install there.
        Err(TryLockError::Error(_)) => Ok((None, Vec::new())),
    }
}

/// Remove every stage in `scratch`, saying which. Only for a run holding the
/// lock alone, for which none of them is in use.
///
/// Only what is named the way a stage is: anything else in here, somebody
/// else put here. And what cannot be removed is left for the next run rather
/// than failing this one, because it is litter in a folder nothing reads.
fn sweep(scratch: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(scratch) else {
        return Vec::new();
    };
    let mut swept = Vec::new();
    for entry in entries.flatten() {
        let is_stage = entry.file_type().is_ok_and(|kind| kind.is_dir())
            && entry.file_name().to_str().is_some_and(is_stage_name);
        if is_stage && fs::remove_dir_all(entry.path()).is_ok() {
            swept.push(entry.path());
        }
    }
    swept.sort();
    swept
}

/// Every stage in `scratch` an interrupted run left, or a run going on now
/// is building in, which nothing short of the lock tells apart: what
/// [`sweep`] would remove, read and never removed, for doctor to say
/// (`crate::doctor`), which takes no lock. In name order; none when
/// `scratch` is not there or cannot be listed, as a sweep finds none.
pub(crate) fn stages_in(scratch: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(scratch) else {
        return Vec::new();
    };
    let mut stages: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_dir())
                && entry.file_name().to_str().is_some_and(is_stage_name)
        })
        .map(|entry| entry.path())
        .collect();
    stages.sort();
    stages
}

/// Every older version an interrupted install of an earlier version left
/// beside a copy in `folder`, `.<name>.mindweaver-new` or `-old`, or a
/// Mindflayer build's: what [`Stage`] clears beside a copy it replaces,
/// read and never removed, each with the name of the copy it was beside,
/// in path order. None when `folder` is not there or cannot be listed.
pub(crate) fn left_beside(folder: &Path) -> Vec<(PathBuf, String)> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut left: Vec<(PathBuf, String)> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            let file_name = entry.file_name().into_string().ok()?;
            let named = file_name.strip_prefix('.')?;
            let name = [BESIDE, FLAYER_BESIDE].into_iter().find_map(|by| {
                ["new", "old"]
                    .into_iter()
                    .find_map(|what| named.strip_suffix(&format!("{by}{what}")))
            })?;
            (!name.is_empty()).then(|| (entry.path(), name.to_owned()))
        })
        .collect();
    left.sort();
    left
}

/// How many hex digits follow the process id in a stage's name.
const SUFFIX_LEN: usize = 16;

/// Whether `name` is one [`claim`] could have given a stage.
fn is_stage_name(name: &str) -> bool {
    name.split_once('-').is_some_and(|(pid, suffix)| {
        !pid.is_empty()
            && pid.bytes().all(|byte| byte.is_ascii_digit())
            && suffix.len() == SUFFIX_LEN
            && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}

/// A name for something this run makes that no other run picks: this
/// process's id and a random suffix.
///
/// The id says whose it is to anybody looking, and the suffix keeps apart
/// two runs of one process, or a process that reuses the id of one that
/// left something behind. Neither makes the name safe to take on its own:
/// whatever is made under it is made with `create_new` or `create_dir`, so
/// a name that is somehow there already is refused rather than shared.
pub(crate) fn unique_name() -> String {
    format!(
        "{}-{:0width$x}",
        std::process::id(),
        random(),
        width = SUFFIX_LEN
    )
}

/// Make a folder in `scratch` no other run has, named by [`unique_name`] and
/// made with `create_dir`, which refuses a name that is already there rather
/// than sharing it.
fn claim(scratch: &Path) -> Result<PathBuf, io::Error> {
    let mut attempts = 0;
    loop {
        let dir = scratch.join(unique_name());
        match fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists && attempts < 8 => {
                attempts += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

/// A number nobody can guess, without a dependency for it: `std` draws its
/// hashers' keys from the operating system. The count keeps two calls in one
/// process apart even if those keys were not random.
pub(crate) fn random() -> u64 {
    static CALLS: AtomicU64 = AtomicU64::new(0);
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(CALLS.fetch_add(1, Ordering::Relaxed));
    hasher.finish()
}

/// What a hidden sibling of a copy is named by, between the copy's name and
/// what it is (`new` or `old`): spelled here alone, since the next install
/// clears what an interrupted one left by this very name.
const BESIDE: &str = ".mindweaver-";
/// [`BESIDE`] as a build from before Mindweaver's names spelled it (D-26),
/// which the next install clears too, never writes.
const FLAYER_BESIDE: &str = ".mindflayer-";

/// A hidden sibling of `to`: where [`replace`] stages, and where installs
/// staged before [`Stage`].
fn beside(to: &Path, what: &str) -> PathBuf {
    beside_by(to, BESIDE, what)
}

/// The sibling of `to` named by `by`, [`BESIDE`] or [`FLAYER_BESIDE`].
fn beside_by(to: &Path, by: &str, what: &str) -> PathBuf {
    let name = to
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    to.with_file_name(format!(".{name}{by}{what}"))
}

/// What replacing a directory turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Added,
    Updated,
    Unchanged,
}

/// Whether two directories hold the same names and the same bytes.
pub fn same(a: &Path, b: &Path) -> Result<bool, io::Error> {
    same_leaving(a, b, &[])
}

/// Whether `b` holds what a copy of `a` leaving out `leave_out` would
/// (`Stage::place`): the same names and bytes, but for what is named
/// `leave_out` directly inside `a`. Only `a`'s are passed over, so a `b`
/// that holds one of those names is not that copy.
pub fn same_leaving(a: &Path, b: &Path, leave_out: &[&str]) -> Result<bool, io::Error> {
    let mut left = listing(a)?;
    left.retain(|name| !leave_out.iter().any(|left_out| name == *left_out));
    let right = listing(b)?;
    if left != right {
        return Ok(false);
    }
    for name in left {
        let (one, two) = (a.join(&name), b.join(&name));
        if one.is_dir() {
            if !same(&one, &two)? {
                return Ok(false);
            }
        } else if fs::read(&one)? != fs::read(&two)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The sorted names directly inside a directory.
fn listing(path: &Path) -> Result<Vec<OsString>, io::Error> {
    let mut names: Vec<OsString> = fs::read_dir(path)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<_, _>>()?;
    names.sort();
    Ok(names)
}

#[cfg(test)]
#[path = "copy_test.rs"]
mod tests;
