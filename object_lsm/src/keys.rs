//! Object key layout helpers.
//!
//! Layout under the instance `prefix`:
//! ```text
//! <prefix>/manifest/current          pointer object -> latest manifest seq
//! <prefix>/manifest/<seq:020>        immutable manifest snapshots
//! <prefix>/journal/<seq:020>         immutable committed record groups (unfenced)
//! <prefix>/journal/<epoch>/<seq:020>  same, fenced/leased writers (per epoch)
//! <prefix>/seg/<partition>/<id:020>  immutable sorted segment objects
//! ```
//! Zero-padded numeric suffixes keep `list` lexicographic = numeric order.

/// Pointer object holding the latest manifest sequence number (decimal).
pub fn current_key(prefix: &str) -> String {
  format!("{prefix}/manifest/current")
}

pub fn manifest_key(prefix: &str, seq: u64) -> String {
  format!("{prefix}/manifest/{seq:020}")
}

pub fn journal_key(prefix: &str, seq: u64) -> String {
  format!("{prefix}/journal/{seq:020}")
}

/// Epoch-namespaced journal key. Unfenced engines (epoch 0) keep the legacy
/// flat layout; fenced engines isolate their journal objects per epoch so a
/// stale writer can never overwrite a successor's object.
pub fn journal_key_epoch(prefix: &str, seq: u64, epoch: u128) -> String {
  if epoch == 0 {
    journal_key(prefix, seq)
  } else {
    format!("{prefix}/journal/{epoch}/{seq:020}")
  }
}

pub fn segment_key(prefix: &str, part: &str, id: u64) -> String {
  format!("{prefix}/seg/{part}/{id:020}")
}

/// Root key prefix under which all segment objects live.
pub fn segment_root(prefix: &str) -> String {
  format!("{prefix}/seg/")
}

pub fn journal_prefix(prefix: &str) -> String {
  format!("{prefix}/journal/")
}

/// Listing prefix for the current fencing epoch.
pub fn journal_prefix_epoch(prefix: &str, epoch: u128) -> String {
  if epoch == 0 {
    journal_prefix(prefix)
  } else {
    format!("{prefix}/journal/{epoch}/")
  }
}

pub fn manifest_prefix(prefix: &str) -> String {
  format!("{prefix}/manifest/")
}

/// Parse the tail of a journal object key below the fixed `<prefix>/journal/`
/// root: either `<seq:020>` (legacy unfenced layout) or `<epoch>/<seq:020>`
/// (epoch-namespaced layout used by leased writers).
pub fn parse_journal_tail(key: &str, root: &str) -> Option<(u128, u64)> {
  let tail = key.strip_prefix(root)?;
  match tail.split_once('/') {
    Some((epoch, seq)) => Some((epoch.parse().ok()?, seq.parse().ok()?)),
    None => Some((0, tail.parse().ok()?)),
  }
}
/// Extract the trailing numeric sequence from a key like `<...>/<seq:020>`.
pub fn parse_tail_seq(key: &str) -> Option<u64> {
  let (_, tail) = key.rsplit_once('/')?;
  tail.parse::<u64>().ok()
}
