# wedb_object_lsm

Object-storage-backed LSM storage engine implementing the
[`wedb_embed_engine`](../engine) `Engine` / `Partition` / `Batch` traits.

Designed as a drop-in backend for `wedb_embed` Redis-style APIs: replace
`WeDb::new(Fjall::open(dir)?)` with an engine whose data lives on
S3-compatible object storage (AWS S3 / Cloudflare R2 / MinIO).

## Architecture (M1 vertical slice)

```text
write:  Batch::commit -> immutable journal group object (atomic PUT)
                        -> apply to per-partition memtable
                        -> spill to immutable sorted segment objects
                        -> publish manifest (segment list + watermark)
read:   memtable hit (µs) -> block-indexed Range GET scans through block cache
crash:  current -> manifest -> replay journal groups newer than watermark

write-adjacent maintenance:
  - merge compaction folds a partition's segments into one (upload -> publish
    manifest -> delete old objects) once max_segments_before_compact is hit
  - applied journal groups (seq <= the last successfully PUBLISHED manifest
  watermark) are deleted — journal GC never runs ahead of a durable manifest, so
  a flush whose manifest publish failed can never lose its only-durable journal
  objects (`failed_manifest_publish_keeps_journals_for_recovery`)
  - opening GCs segments not referenced by the current manifest and superseded
    manifest snapshots
```

- every committed group is a separate immutable journal object, so a PUT is the
  atomic durability point — no torn log tail;
- memtable flush advances the partition watermark; journal groups are replayed
  only when newer than the watermark;
- tombstones shadow older segments across flush / reopen;
- `clear` / `rm_partition` fold prior state into the watermark and mark dropped,
  so replayed journal groups can never resurrect deleted data.

## Milestones

- [x] M0 scaffold: crate + `Store` abstraction + in-memory store + codec
- [x] M1 vertical slice: memtable/journal/segment/manifest, recovery, Engine impl
- [x] M2 ordered streaming iteration, block-indexed segments + block cache
- [x] M3 compaction + orphan GC + journal GC
- [x] M4 compatibility harness vs `wedb_embed` tests + benchmarks
- [x] R2/S3 remote `Store` backend (feature `r2`, `object_store` sync bridge)
- [x] P2 HA: non-blocking lease + standby promotion (`try_open_leased`), fencing,
      and cross-epoch recovery of acked-but-unflushed journals after a leader crash

## Test

```sh
cargo test -p wedb_object_lsm              # engine tests (58)
cargo test -p wedb_object_lsm --features wedb   # + wedb_embed parity harness (2)
cargo bench -p wedb_object_lsm             # point read / scan / insert benches
```

The `wedb` feature adds an optional `wedb_embed` backend bridge plus the M4
compatibility harness: identical Redis-style scripts (string/hash/list/set/
zset/bitmap/stream/geo/json/hll/bloom/cuckoo/tdigest/sortedint/timeseries/
key-expiry + cross-type errors + namespaces) run against both `Fjall`
(reference) and `ObjectLsm`, asserting byte-identical result logs, and a
reopen-persistence test for `ObjectLsm`.

## Cloudflare R2 backend

`R2Store` (feature `r2`) implements the [`Store`] trait over Cloudflare R2 /
any S3-compatible endpoint via `object_store`, bridging async I/O with an
internal tokio runtime. Byte-range reads map to S3 Range GETs.

```sh
# credentials come from the environment (never commit them):
#   R2_BUCKET R2_ACCESS_KEY_ID R2_SECRET_ACCESS_KEY
#   R2_ENDPOINT (or R2_ACCOUNT_ID to derive it)
cargo test -p wedb_object_lsm --features r2 --test r2   # live roundtrip; skips w/o env
```





## Backend comparison benchmark (R2 vs fjall local)

```sh
cargo bench -p wedb_object_lsm --features "r2 wedb" --bench bench_remote
# R2_* env required for the R2 rows; without them only local backends run
# BENCH_N overrides the key count (default 200)
```

Observed (Cloudflare R2 public internet from CN, re-measured 2026-09-03; local
rows N=200, R2 strict N=40, R2 grouped N=200):

| backend | insert (commit) | point read (warm) | scan (warm) |
| --- | --- | --- | --- |
| fjall (local disk) | ~0.9 µs | ~0.3 µs | ~0.2 µs |
| objectlsm (memory) | ~1.6 µs | ~0.7 µs | ~0.2 µs |
| objectlsm (file, grouped 20 ms) | ~0.8 µs | ~1.7 µs | ~0.1 µs |
| objectlsm (R2, strict) | ~0.26 s /op (per-commit PUT) | ~4 ms (Range GET) | µs (cache warm) |
| objectlsm (R2, grouped 25 ms) | ~0.9 µs ack + flush | ~8.9 ms (Range GET) | µs (cache warm) |

Group-commit (`Config::journal_window_ms(Some(ms))`) turns queued commits into
one journal object per window. For 40 durable commits the measured insert
wall time dropped from ~10.2 s (strict, one PUT per commit) to µs-level acks
in grouped mode; the flush/compact step was ~10.6 s strict vs ~1.5 s grouped
(N=200). Ack in grouped mode means "buffered"; durability
is reached at the next flush (`persist()` forces one, so call it before
shutdown or whenever you need a sync point), matching an AOF-every-N-ms
trade-off. Warm cached reads/scans stay µs-level on R2.

Run: `OBJLSM_WINDOW_MS=25 BENCH_N=200 cargo bench -p wedb_object_lsm --features "r2 wedb" --bench bench_remote`

### R2 scale check (grouped 25 ms, 2026-09-04)

| keys | insert wall | insert/op | compact wall | warm read/op | object state after compact | reopen |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| 1,000 | 0.85 ms ack | 0.84 µs | 1.8 s | 3.39 ms | 1 segment, 0 journal, disk 95.5 KB | all keys recovered in 0.69 s |
| 10,000 (auto-compaction during writes) | 38.1 s | 3.81 ms | 45.5 s | 3.33 ms | 1 segment, 0 journal, disk 951 KB | all keys recovered in 1.21 s |
| 10,000 (compaction deferred) | 21.6 s | 2.16 ms | 50.8 s | 3.24 ms | 1 segment, 0 journal, disk 951 KB | all keys recovered in 1.19 s |

The deferred-compaction run disables automatic compaction in the write loop (`max_segments_before_compact(1_000_000)`) and folds everything in the explicit compact phase; it is a **soak-scale check**, not a claim of maximum throughput: under
this configuration the journal flusher periodically blocks memtable flushes on
remote R2 PUTs, making wall time non-linear. The important reliability results
are stable: journal GC reaches zero objects, compaction folds the dataset to a
single segment, cold reads stay low single-digit ms, and every key is recovered
after reopen. Reopen time grows sub-linearly with key count here (manifest /
segment metadata, not a full key replay).

Recovery replay fetches and decodes journal objects in parallel (bounded
waves of 512 objects, up to 8 workers, apply stays strictly seq-ordered): an
offline 60k-key strict-mode FileStore reopen measured ~5.6 s serial vs ~3.3 s
parallel (~1.7x on this machine); on R2 the per-object GET latency dominates,
so the parallel fetch gives a larger win on failover/reopen. Probe:
cargo test --release -p wedb_object_lsm --test perf_reopen -- --ignored.

## Multi-instance shared bucket: lease + sharding

`ObjectLsm::open_leased(store, cfg, LeaseOptions)` makes an instance the
exclusive writer of `cfg.prefix`:

- acquisition is atomic create-if-absent on `<prefix>/lease`
  (`Store::create`; R2 uses `PutMode::Create`, MemoryStore is atomic);
- the owner renews via a background heartbeat (TTL/3); losing renewal marks
  the lease lost, so a crashed writer is taken over once its lease expires;
- the lease is released when the last engine handle drops.

Parallel writers share a bucket by sharding into disjoint prefixes:

```rust
let cfg0 = Config::for_shard("myapp/db", 0); // .../shard-0
let cfg1 = Config::for_shard("myapp/db", 1); // .../shard-1
ObjectLsm::open_leased(store.clone(), cfg0, lease("w0"))?; // independent writer
ObjectLsm::open_leased(store.clone(), cfg1, lease("w1"))?;
```

Notes: stale-lease takeover and renewal use an atomic compare-and-swap
(`Store::put_if_matches`): the lease is replaced only if its current payload is
unchanged. `MemoryStore` compares bytes directly; `R2Store` maps it to S3
`If-Match` using the single-part object ETag (MD5). Each acquisition carries a
monotonic **fencing epoch** embedded in every journal group and in the
manifest; manifest publishing performs a conditional update of the `current`
pointer (CAS), so only one epoch's state is ever visible, and recovery ignores
journal groups from a different epoch. Once a manifest exists, a takeover
replays only the journals of the epoch recorded in `current` (older epochs are
fenced off); before the first manifest publish the successor replays every
epoch's journals, so a crash right after acking never loses data (see HA
section below). For a *graceful* handoff, call `compact`/`persist` before
releasing the lease so the state is fully folded. Lost-lease detection remains
heartbeat-based. Each shard is a separate engine instance —
cross-shard queries/cluster routing stay an application concern (as in Redis
Cluster).

### HA: automatic failover of a crashed writer

Standby supervisors poll the lease without blocking instead of calling the
blocking `open_leased`:

```rust
// one non-blocking attempt: Ok(None) while the active writer holds the lease
if let Some(engine) = ObjectLsm::try_open_leased(store.clone(), cfg.clone(), opts)? {
  // we won the lease and recovered the published state
}
```

- `Lease::try_acquire_once` performs a single acquisition attempt (create-if-
  absent, or CAS takeover of an expired lease); `acquire`/`open_leased` are now
  thin retry loops over it.
- `ObjectLsm::try_open_leased` combines "win the lease if free" with engine
  recovery and bumps the fencing epoch; the same-prefix lease + manifest-CAS +
  epoch rules guarantee exactly one writer (`concurrent_standbys_promote_
  exactly_one_writer`).

- Every takeover also folds whatever recovery replayed into segments and
publishes a manifest under the new epoch, so current is never left anchored
to an epoch that cannot see this writer's own journals (	akeover_anchor_
preserves_successor_acks_across_handoff).
- **Pre-manifest recovery**: when a leader crashes after acknowledged writes
  but before its *first* manifest publish, the successor replays every journal
  object under `<prefix>/journal/` across epochs (sorted by seq), so acked
  writes survive even though no manifest ever existed. Journals of superseded
  epochs are fenced off once a manifest exists, and once a takeover folds them
  into its anchor manifest they are garbage-collected automatically (at
  takeover and at every open); only journals above the folded watermark (from a
  fenced writer's uncertainty window) linger, and those can be reclaimed by an
  object-store lifecycle rule (e.g. expire `*/journal/*` older than N days).
- Live R2 test `r2_same_prefix_auto_failover_after_leader_crash`: leader writes
  strict-mode keys (never flushed), crashes; a standby promotes after lease
  expiry and recovers every acked key from real Cloudflare R2.

Writer-handoff rule (unchanged): to make *all* acknowledged writes durable
across a *graceful* handoff without relying on the new recovery path, call
`persist()`/`compact()` before releasing the lease. Group-commit acks are
buffered until the next flush and can be lost on a crash — the AOF-every-N-ms
trade-off documented above.


## fjall-alignment notes

- `Partition::approximate_len` is O(#segments + memtable) — never a full
  partition scan — so `WeDb::dbsize`-style calls stay cheap; duplicates across
  segments intentionally overcount (approximate, like fjall's count).
- recovery skips whole journal objects whose end-seq is at/below every
  partition watermark, instead of decoding the entire history.
- the segment-index cache is FIFO-bounded (4096) so metadata memory cannot
  grow unboundedly with churn.
- dropping the engine (windowed group-commit) flushes the pending journal
  before stopping the background flusher, so a clean shutdown does not lose
  acknowledged writes.




## Local filesystem backend + process-level crash injection

`FileStore` (no feature gate) implements [`Store`] over a local directory:
objects are files under `root/key`, with byte-range reads, create-if-absent,
compare-and-swap and prefix listing. It is a test/reference backend (object
writes are not guaranteed to be atomic at the filesystem level).

`tests/crash.rs` spawns the test binary itself, performs a scenario, then calls
`std::process::abort()` at a precise durability step and reopens the same
directory in a fresh process to assert crash consistency:
journal-after-commit, flush-after-compact, segment-upload-before-manifest,
clear-publish-before-delete, compact-publish-before-delete.


### Read replicas: followers over a shared prefix

`ObjectLsm::open_follower(store, cfg, refresh)` opens a read-only engine over the
SAME prefix a leader writes, without acquiring the lease and without ever
writing to the store:

- reads track the leader's *published* state: segments folded by a manifest
  publish plus the durable journal tail (strict-mode / flushed group-commit
  objects) above the manifest watermark;
- a background poller (interval = `refresh`; `None` disables it — call
  `engine.refresh()` manually, e.g. in tests) re-reads `current` and swaps a
  fresh read-only snapshot into the engine's view;
- every store-mutating call (insert/rm/clear/compact/persist/rm_partition)
  fails with a read-only error, and opening/refreshing/reading a follower adds
  or removes no objects (`follower_never_writes_to_the_store`);
- followers cross fencing-epoch boundaries: after a leader failover they pick
  up the successor's manifest and keep serving the union of published state;
- caveats: visibility is bounded by what the leader made durable (a
  group-commit ack still buffered in the leader is invisible) and refreshes are
  eventually consistent; a read on any stale snapshot (not only a long-lived
  iterator) can hit a segment the leader deleted after compaction, in which
  case a Range GET error surfaces to the caller; a manual `refresh()` racing a
  background poll can momentarily install an older snapshot.

Live R2 test `r2_follower_reads_shared_bucket_prefix` verifies the read-replica
topology on real Cloudflare R2.

### Two processes on one bucket (sharded writers + cross readers)

"Two processes read and write the same bucket" is supported with an explicit
topology: each process owns a disjoint shard prefix as its exclusive leased
writer and reads the other shards through followers.

```rust
// process A: owns shard-0, reads shard-1 via a follower
let writer_a = ObjectLsm::open_leased(store.clone(), Config::for_shard("myapp/db", 0), lease("A"))?;
let reader_of_b = ObjectLsm::open_follower(store.clone(), Config::for_shard("myapp/db", 1), poll)?;
// process B: owns shard-1, reads shard-0 via a follower
let writer_b = ObjectLsm::open_leased(store.clone(), Config::for_shard("myapp/db", 1), lease("B"))?;
let reader_of_a = ObjectLsm::open_follower(store.clone(), Config::for_shard("myapp/db", 0), poll)?;
```

Consistency model: per-shard writes are strongly consistent under the
single-writer lease + epoch fencing (a second writer on the same prefix is
rejected; a crashed writer is taken over without data loss). Cross-shard reads
are eventually consistent — a follower converges to the other writer's
*published* state on each refresh. Nothing is ever overwritten or lost across
the two processes. Verified offline
(`two_writers_share_bucket_with_cross_read_consistency`) and live on real
Cloudflare R2 (`r2_two_writers_one_bucket_cross_reads`).

For "two processes writing the SAME prefix simultaneously": a second *leased*
writer is rejected by design — the lease + fencing guarantee exactly one writer
per prefix, which is what makes the single-prefix data strongly consistent (use
shards or a failover/supervisor if you need a second writer on one prefix). Plain
ObjectLsm::open` is a single-instance exclusive access path that takes no lease
and participates in no fencing: never mix it with a leased writer on the same
prefix.
