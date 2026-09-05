#![cfg(feature = "wedb")]
//! M4 compatibility harness: run identical wedb_embed Redis-style scripts
//! against Fjall (reference) and ObjectLsm, asserting byte-identical results,
//! plus reopen persistence for ObjectLsm.

use std::{fmt::Debug, sync::Arc};

use tempfile::tempdir;
use wedb_embed::{
  Db, Fjall, StreamId, WeDb,
  bitmap::{BitCount, BitOp, BitPos, BitfieldEncoding, BitfieldOperation},
  bloom::{BfReserve, CfReserve},
  geo::{GeoRadius, GeoShape, OriginPoint},
  hash::{HGetEx, HSet, HashLengthMode},
  json::{JsonArrIndex, JsonSet},
  sortedint::SortedintRange,
  stream::{StreamAutoClaim, StreamClaim, StreamPending, StreamTrim},
  string::{GetEx, Set, StringMSet, StringSet},
  timeseries::{AggregationType, DuplicatePolicy, TsCreate, TsMGet, TsMRange, TsRange},
  zset::{Aggregate, RangeLex, RangeScore, ZAdd},
};
use wedb_object_lsm::{Config, MemoryStore, ObjectLsm};

fn object_db(prefix: &str) -> (Db<ObjectLsm>, MemoryStore) {
  let store = MemoryStore::new();
  // Small memtable + small blocks + low compact threshold: forces flushing,
  // multi-block segments and merge compaction during the script.
  let cfg = Config::new(prefix)
    .max_memtable_bytes(1024)
    .block_size(256)
    .max_segments_before_compact(6);
  let engine = ObjectLsm::open(Arc::new(store.clone()), cfg).expect("open objectlsm");
  let db = WeDb::new(engine).ns(0).expect("ns").db(0).expect("db");
  (db, store)
}

fn rec<T: Debug>(log: &mut Vec<String>, tag: &str, r: wedb_embed::Result<T>) {
  log.push(format!("{tag}={r:?}"));
}

fn sort_json_value(v: serde_json::Value) -> serde_json::Value {
  match v {
    serde_json::Value::Object(m) => {
      let mut keys: Vec<String> = m.keys().cloned().collect();
      keys.sort();
      let mut out = serde_json::Map::new();
      for k in keys {
        out.insert(
          k.clone(),
          sort_json_value(m.get(&k).cloned().unwrap_or(serde_json::Value::Null)),
        );
      }
      serde_json::Value::Object(out)
    }
    serde_json::Value::Array(a) => {
      serde_json::Value::Array(a.into_iter().map(sort_json_value).collect())
    }
    other => other,
  }
}

fn canon_json_str(s: &str) -> String {
  match serde_json::from_str::<serde_json::Value>(s) {
    Ok(v) => serde_json::to_string(&sort_json_value(v)).unwrap_or_else(|_| s.to_string()),
    Err(_) => s.to_string(),
  }
}

fn script<E>(db: &Db<E>) -> wedb_embed::Result<Vec<String>>
where
  E: wedb_embed::Engine,
  wedb_embed::Error: From<E::Error>,
{
  let mut log = Vec::new();

  // ================= string ==============================================
  rec(&mut log, "set1", db.set("s1", "hello", []));
  rec(&mut log, "get1", db.get("s1"));
  rec(&mut log, "set_nx_blocked", db.set("s1", "nx", [Set::Nx]));
  rec(&mut log, "set_xx", db.set("s1", "xx", [Set::Xx]));
  rec(&mut log, "get_xx", db.get("s1"));
  rec(&mut log, "set_get_prev", db.set("s1", "gv", [Set::Get]));
  rec(&mut log, "get_after_set_get", db.get("s1"));
  rec(&mut log, "set_one", db.set_one("sone", "vone"));
  rec(&mut log, "get_one", db.get("sone"));
  rec(
    &mut log,
    "set_with",
    db.set_with("sw", "vw", &StringSet::default()),
  );
  rec(&mut log, "setex", db.setex("s_ex", "v_ex", 60_000));
  rec(
    &mut log,
    "getex_persist",
    db.getex("s_ex", [GetEx::Persist]),
  );
  rec(&mut log, "getset", db.getset("s1", "replaced"));
  rec(&mut log, "get_after_getset", db.get("s1"));
  rec(&mut log, "getdel", db.getdel("s1"));
  rec(&mut log, "get_after_getdel", db.get("s1"));
  rec(&mut log, "append", db.append("s1", "!!"));
  rec(&mut log, "strlen", db.strlen("s1"));
  rec(&mut log, "getrange", db.getrange("s1", (0, 4)));
  rec(&mut log, "setrange", db.setrange("s1", 1, "XYZ"));
  rec(&mut log, "get_setrange", db.get("s1"));
  rec(&mut log, "incr", db.incr("cnt"));
  rec(&mut log, "incrby", db.incrby("cnt", 10));
  rec(&mut log, "decr", db.decr("cnt"));
  rec(&mut log, "decrby", db.decrby("cnt", 3));
  rec(&mut log, "incrby_ex", db.incrby_ex("cnt", 8, 0, true));
  rec(&mut log, "incrbyfloat", db.incrbyfloat("fcnt", 2.25));
  rec(
    &mut log,
    "incrbyfloat_ex",
    db.incrbyfloat_ex("fcnt", 0.25, 0, true),
  );
  rec(&mut log, "mset", db.mset(&[("m1", "a"), ("m2", "b")]));
  rec(&mut log, "mget", db.mget(&["m1", "m2", "nope"]));
  rec(&mut log, "msetnx", db.msetnx(&[("m3", "c"), ("m4", "d")]));
  rec(
    &mut log,
    "msetnx_dup",
    db.msetnx(&[("m1", "a"), ("m5", "e")]),
  );
  rec(&mut log, "setnx", db.setnx("nx1", "v"));
  rec(&mut log, "setnx_dup", db.setnx("nx1", "v2"));
  rec(&mut log, "setxx", db.setxx("xx1", "v", 0));
  rec(&mut log, "setxx_miss", db.setxx("xx_none", "v", 0));
  rec(
    &mut log,
    "mset_with",
    db.mset_with(&[("mw", "v")], StringMSet::default()),
  );
  rec(&mut log, "cas_ok", db.cas("cas", "0", "1", 0));
  rec(&mut log, "cas_bad", db.cas("cas", "0", "2", 0));
  rec(&mut log, "cad", db.cad("cas", "1"));
  rec(&mut log, "digest", db.digest("m1"));
  db.set("lcs_a", "OHMYMISTAKE", [])?;
  db.set("lcs_b", "HEYMYMOMENT", [])?;
  rec(&mut log, "lcs", db.lcs("lcs_a", "lcs_b", []));
  rec(&mut log, "get_with_expire", db.get_with_expire("m1"));
  rec(&mut log, "delex", db.delex("m1", []));
  rec(&mut log, "del_m", db.del(&["m1", "m2", "nope"]));

  // ================= cross-type WRONGTYPE ================================
  rec(&mut log, "set_ctype", db.set("conflict", "str", []));
  rec(
    &mut log,
    "hget_ctype_err",
    Ok(db.hget("conflict", "f").is_err()),
  );
  rec(&mut log, "llen_ctype_err", Ok(db.llen("conflict").is_err()));
  rec(
    &mut log,
    "zscore_ctype_err",
    Ok(db.zscore("conflict", b"m").is_err()),
  );
  rec(
    &mut log,
    "sadd_ctype_err",
    Ok(db.sadd("conflict", &["m"]).is_err()),
  );
  rec(
    &mut log,
    "xadd_ctype_err",
    Ok(
      db.xadd("conflict", Some(StreamId::new(1, 0)), &[("k", "v")])
        .is_err(),
    ),
  );
  rec(
    &mut log,
    "bitcount_ctype_err",
    Ok(db.bitcount("conflict", []).is_err()),
  );

  // ================= hash ================================================
  rec(
    &mut log,
    "hset1",
    db.hset("h1", &[("f1", "a"), ("f2", "b"), ("f3", "c")]),
  );
  rec(&mut log, "hset2", db.hset("h1", &[("f1", "a2")]));
  rec(&mut log, "hset_one", db.hset_one("h1", "f4", "d"));
  rec(&mut log, "hget1", db.hget("h1", "f1"));
  rec(&mut log, "hexists1", db.hexists("h1", "f1"));
  rec(&mut log, "hexists2", db.hexists("h1", "zz"));
  rec(&mut log, "hlen1", db.hlen("h1"));
  rec(
    &mut log,
    "hlen_mode",
    db.hlen_with_mode("h1", HashLengthMode::Accurate),
  );
  rec(&mut log, "hkeys1", db.hkeys("h1"));
  rec(&mut log, "hvals1", db.hvals("h1"));
  rec(&mut log, "hgetall1", db.hgetall("h1"));
  rec(&mut log, "hincrby1", db.hincrby("h1", "n", 5));
  rec(&mut log, "hincrbyfloat", db.hincrbyfloat("h1", "n", 1.5));
  rec(&mut log, "hmget1", db.hmget("h1", &["f2", "zz"]));
  rec(&mut log, "hstrlen", db.hstrlen("h1", "f1"));
  rec(&mut log, "hsetnx", db.hsetnx("h1", "fnx", "v"));
  rec(&mut log, "hsetnx_dup", db.hsetnx("h1", "fnx", "v2"));
  rec(&mut log, "hrandfield_empty", db.hrandfield("h1", 0, true));
  let mut hit = Vec::new();
  db.hiter("h1", |f, v| {
    hit.push((f.to_vec(), v.to_vec()));
    true
  })?;
  rec(&mut log, "hiter", Ok(hit));
  rec(&mut log, "hscan", db.hscan("h1", 0, 10, None));
  rec(&mut log, "hexpire", db.hexpire("h1", &["f1"], 3600, []));
  rec(
    &mut log,
    "hpexpire",
    db.hpexpire("h1", &["f2"], 3_600_000, []),
  );
  rec(
    &mut log,
    "hexpireat",
    db.hexpireat("h1", &["f3"], 2_000_000_000, []),
  );
  rec(
    &mut log,
    "hpexpireat",
    db.hpexpireat("h1", &["f4"], 2_000_000_000_000, []),
  );
  rec(
    &mut log,
    "httl_pos",
    Ok(
      db.httl("h1", &["f1"])
        .map(|v| v.into_iter().map(|x| x > 0).collect::<Vec<_>>()),
    ),
  );
  rec(
    &mut log,
    "hpttl_pos",
    Ok(
      db.hpttl("h1", &["f2"])
        .map(|v| v.into_iter().map(|x| x > 0).collect::<Vec<_>>()),
    ),
  );
  rec(&mut log, "hexpiretime", db.hexpiretime("h1", &["f3"]));
  rec(&mut log, "hpexpiretime", db.hpexpiretime("h1", &["f4"]));
  rec(&mut log, "hpersist", db.hpersist("h1", &["f1"]));
  rec(&mut log, "hgetdel", db.hgetdel("h1", &["fnx"]));
  rec(
    &mut log,
    "hsetex",
    db.hsetex("h1", &[("f_ex", "v_ex")], [HSet::Ex(3600)]),
  );
  rec(
    &mut log,
    "hgetex",
    db.hgetex("h1", "f_ex", [HGetEx::Persist]),
  );
  rec(
    &mut log,
    "hrangebylex",
    db.hrangebylex("h1", RangeLex::unbounded()),
  );
  rec(&mut log, "hdel1", db.hdel("h1", &["f3", "zz"]));
  rec(&mut log, "hlen2", db.hlen("h1"));

  // ================= list ================================================
  rec(&mut log, "rpush1", db.rpush("l1", &["a", "b", "c"]));
  rec(&mut log, "lpush1", db.lpush("l1", &["z"]));
  rec(&mut log, "lpushx", db.lpushx("l1", &["x_head"]));
  rec(&mut log, "rpushx", db.rpushx("l1", &["x_tail"]));
  rec(&mut log, "llen1", db.llen("l1"));
  rec(&mut log, "lrange1", db.lrange("l1", (0, -1)));
  rec(&mut log, "lrange2", db.lrange("l1", (0, 1)));
  rec(&mut log, "lindex1", db.lindex("l1", 1));
  rec(&mut log, "lset", db.lset("l1", 0, "first_elem"));
  rec(&mut log, "linsert", db.linsert("l1", true, "a", "before_a"));
  rec(&mut log, "lpos", db.lpos("l1", "b", []));
  rec(&mut log, "lpop1", db.lpop("l1", 1));
  rec(&mut log, "rpop1", db.rpop("l1", 2));
  rec(&mut log, "llen2", db.llen("l1"));
  rec(&mut log, "lrem", db.lrem("l1", 1, "first_elem"));
  rec(&mut log, "ltrim", db.ltrim("l1", (0, 2)));
  rec(&mut log, "lmove", db.lmove("l1", "l2", false, true));
  rec(&mut log, "rpoplpush", db.rpoplpush("l2", "l1"));
  rec(&mut log, "lrange_l1", db.lrange("l1", (0, -1)));
  rec(&mut log, "lrange_l2", db.lrange("l2", (0, -1)));

  // ================= set =================================================
  rec(&mut log, "sadd1", db.sadd("st1", &["x", "y", "z"]));
  rec(&mut log, "sadd2", db.sadd("st1", &["y", "w"]));
  rec(&mut log, "scard1", db.scard("st1"));
  rec(&mut log, "sismember1", db.sismember("st1", "x"));
  rec(&mut log, "sismember2", db.sismember("st1", "nope"));
  rec(&mut log, "smismember", db.smismember("st1", &["x", "nope"]));
  rec(&mut log, "smembers1", db.smembers("st1"));
  rec(&mut log, "srandmember_all", db.srandmember("st1", 100));
  let mut sit = Vec::new();
  db.siter("st1", |m| {
    sit.push(m.to_vec());
    true
  })?;
  rec(&mut log, "siter", Ok(sit));
  rec(&mut log, "sscan", db.sscan("st1", 0, None, Some(10)));
  rec(&mut log, "smove", db.smove("st1", "st2", "w"));
  rec(&mut log, "smembers_st2", db.smembers("st2"));
  rec(&mut log, "srem1", db.srem("st1", &["y", "nope"]));
  rec(&mut log, "smembers2", db.smembers("st1"));
  rec(
    &mut log,
    "overwrite_set",
    db.overwrite_set("st1", &["a", "b", "c"]),
  );
  rec(&mut log, "sadd_st3", db.sadd("st3", &["b", "c", "d"]));
  rec(
    &mut log,
    "sinter",
    Ok(db.sinter(&["st1", "st3"]).map(|mut v| {
      v.sort();
      v
    })),
  );
  rec(
    &mut log,
    "sinterstore",
    db.sinterstore("st_i", &["st1", "st3"]),
  );
  rec(&mut log, "sintercard", db.sintercard(&["st1", "st3"], 10));
  rec(
    &mut log,
    "sunion",
    Ok(db.sunion(&["st1", "st3"]).map(|mut v| {
      v.sort();
      v
    })),
  );
  rec(
    &mut log,
    "sunionstore",
    db.sunionstore("st_u", &["st1", "st3"]),
  );
  rec(&mut log, "sunioncard", db.sunioncard(&["st1", "st3"], 10));
  rec(
    &mut log,
    "sdiff",
    Ok(db.sdiff(&["st1", "st3"]).map(|mut v| {
      v.sort();
      v
    })),
  );
  rec(
    &mut log,
    "sdiffstore",
    db.sdiffstore("st_d", &["st1", "st3"]),
  );
  rec(&mut log, "sdiffcard", db.sdiffcard(&["st1", "st3"], 10));

  // ================= zset ================================================
  rec(
    &mut log,
    "zadd1",
    db.zadd(
      "z1",
      &[
        (1.0, b"a".as_slice()),
        (3.0, b"c".as_slice()),
        (2.0, b"b".as_slice()),
      ],
      [ZAdd::Nx],
    ),
  );
  rec(
    &mut log,
    "zadd2",
    db.zadd("z1", &[(2.5, b"b".as_slice())], []),
  );
  rec(&mut log, "zcard1", db.zcard("z1"));
  rec(&mut log, "zscore1", db.zscore("z1", b"b"));
  rec(&mut log, "zscore2", db.zscore("z1", b"zz"));
  rec(
    &mut log,
    "zmscore",
    db.zmscore("z1", &[b"a".as_slice(), b"b".as_slice()]),
  );
  let zm = db.zmget("z1", &[b"a".as_slice(), b"b".as_slice()]);
  rec(
    &mut log,
    "zmget",
    Ok(zm.map(|m| {
      let mut v: Vec<_> = m.into_iter().collect();
      v.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
      v
    })),
  );
  rec(&mut log, "zincrby", db.zincrby("z1", 0.5, b"b"));
  rec(&mut log, "zrank1", db.zrank("z1", b"b"));
  rec(
    &mut log,
    "zrank_with_score",
    db.zrank_with_score("z1", b"b"),
  );
  rec(&mut log, "zrevrank", db.zrevrank("z1", b"b"));
  rec(
    &mut log,
    "zrevrank_with_score",
    db.zrevrank_with_score("z1", b"b"),
  );
  rec(&mut log, "zrange1", db.zrange("z1", b"0", b"-1", []));
  rec(&mut log, "zrangebyrank", db.zrangebyrank("z1", (0, -1)));
  rec(&mut log, "zrevrange", db.zrevrange("z1", (0, -1)));
  let score_spec = RangeScore::new(1.0, 3.0);
  rec(&mut log, "zcount", db.zcount("z1", score_spec));
  rec(
    &mut log,
    "zrangebyscore",
    db.zrangebyscore("z1", score_spec),
  );
  rec(
    &mut log,
    "zrevrangebyscore",
    db.zrevrangebyscore("z1", score_spec),
  );
  let lex_spec = RangeLex::unbounded();
  rec(&mut log, "zlexcount", db.zlexcount("z1", &lex_spec));
  rec(&mut log, "zrangebylex", db.zrangebylex("z1", &lex_spec));
  rec(
    &mut log,
    "zrangebylex_with_scores",
    db.zrangebylex_with_scores("z1", &lex_spec),
  );
  rec(
    &mut log,
    "zrevrangebylex",
    db.zrevrangebylex("z1", &lex_spec),
  );
  rec(
    &mut log,
    "zrevrangebylex_with_scores",
    db.zrevrangebylex_with_scores("z1", &lex_spec),
  );
  rec(&mut log, "zrandmember_all", db.zrandmember("z1", 100));
  rec(&mut log, "zget_all", db.zget_all("z1"));
  let mut zit = Vec::new();
  db.ziter("z1", |m, s| {
    zit.push((m.to_vec(), s));
    true
  })?;
  rec(&mut log, "ziter", Ok(zit));
  let mut zit_rev = Vec::new();
  db.ziter_rev("z1", |m, s| {
    zit_rev.push((m.to_vec(), s));
    true
  })?;
  rec(&mut log, "ziter_rev", Ok(zit_rev));
  rec(&mut log, "zscan", db.zscan("z1", 0, None, Some(10)));
  rec(&mut log, "zpopmin", db.zpopmin("z1", 1));
  rec(&mut log, "zpopmax", db.zpopmax("z1", 1));
  rec(&mut log, "bzpopmin", db.bzpopmin(&[b"z1".as_slice()]));
  rec(&mut log, "bzpopmax", db.bzpopmax(&[b"z1".as_slice()]));
  rec(&mut log, "zrem", db.zrem("z1", &[b"a".as_slice()]));
  rec(&mut log, "zcard_after", db.zcard("z1"));
  rec(
    &mut log,
    "overwrite_zset",
    db.overwrite_zset("z1", &[(b"m1".as_slice(), 1.0), (b"m2".as_slice(), 2.0)]),
  );
  rec(
    &mut log,
    "zadd_z2",
    db.zadd(
      "z2",
      &[(2.0, b"m2".as_slice()), (3.0, b"m3".as_slice())],
      [],
    ),
  );
  rec(
    &mut log,
    "zunion",
    db.zunion(
      &[(b"z1".as_slice(), 1.0), (b"z2".as_slice(), 1.0)],
      Aggregate::Sum,
    ),
  );
  rec(
    &mut log,
    "zunionstore",
    db.zunionstore(
      "z_u",
      &[(b"z1".as_slice(), 1.0), (b"z2".as_slice(), 1.0)],
      Aggregate::Sum,
    ),
  );
  rec(
    &mut log,
    "zinter",
    db.zinter(
      &[(b"z1".as_slice(), 1.0), (b"z2".as_slice(), 1.0)],
      Aggregate::Sum,
    ),
  );
  rec(
    &mut log,
    "zinterstore",
    db.zinterstore(
      "z_i",
      &[(b"z1".as_slice(), 1.0), (b"z2".as_slice(), 1.0)],
      Aggregate::Sum,
    ),
  );
  rec(
    &mut log,
    "zintercard",
    db.zintercard(&[b"z1".as_slice(), b"z2".as_slice()], 10),
  );
  rec(
    &mut log,
    "zdiff",
    db.zdiff(&[b"z1".as_slice(), b"z2".as_slice()]),
  );
  rec(
    &mut log,
    "zdiffstore",
    db.zdiffstore("z_d", &[b"z1".as_slice(), b"z2".as_slice()]),
  );
  rec(
    &mut log,
    "zremrangebyrank",
    db.zremrangebyrank("z1", (0, 0)),
  );
  rec(
    &mut log,
    "zremrangebyscore",
    db.zremrangebyscore("z1", RangeScore::new(1.5, 2.5)),
  );
  rec(
    &mut log,
    "zremrangebylex",
    db.zremrangebylex("z1", RangeLex::default()),
  );

  // ================= bitmap ==============================================
  rec(&mut log, "setbit1", db.setbit("bm1", 10, 1));
  rec(&mut log, "setbit2", db.setbit("bm1", 20, 1));
  rec(&mut log, "getbit1", db.getbit("bm1", 10));
  rec(&mut log, "getbit2", db.getbit("bm1", 15));
  rec(&mut log, "bitcount", db.bitcount("bm1", []));
  rec(
    &mut log,
    "bitcount_range",
    db.bitcount("bm1", [BitCount::Range(0, 2)]),
  );
  rec(&mut log, "bitpos", db.bitpos("bm1", 1, []));
  rec(
    &mut log,
    "bitpos_range",
    db.bitpos("bm1", 1, [BitPos::Range(0, 2)]),
  );
  rec(&mut log, "setbit_bm2", db.setbit("bm2", 10, 1));
  rec(
    &mut log,
    "bitop_or",
    db.bitop(BitOp::Or, "bm_or", &[b"bm1".as_slice(), b"bm2".as_slice()]),
  );
  rec(&mut log, "bitcount_or", db.bitcount("bm_or", []));
  rec(
    &mut log,
    "bitfield_get",
    db.bitfield(
      "bm1",
      [BitfieldOperation::get(BitfieldEncoding::Unsigned(8), 0)],
    ),
  );
  rec(
    &mut log,
    "bitfield_ro",
    db.bitfield_read_only(
      "bm1",
      [BitfieldOperation::get(BitfieldEncoding::Unsigned(8), 0)],
    ),
  );
  rec(&mut log, "get_bitmap_bytes", db.get_bitmap_bytes("bm1"));

  // ================= stream ==============================================
  let s1_id = db.xadd("s2", Some(StreamId::new(1, 0)), &[("k1", "v1")])?;
  let s2_id = db.xadd("s2", Some(StreamId::new(2, 0)), &[("k2", "v2")])?;
  let s3_id = db.xadd("s2", Some(StreamId::new(3, 0)), &[("k3", "v3")])?;
  rec(&mut log, "xadd_ids", Ok((s1_id, s2_id, s3_id)));
  rec(&mut log, "xlast_id", db.xlast_id("s2"));
  rec(&mut log, "xlen", db.xlen("s2"));
  rec(
    &mut log,
    "xrange",
    db.xrange("s2", (StreamId::min(), StreamId::max())),
  );
  rec(
    &mut log,
    "xrevrange",
    db.xrevrange("s2", (StreamId::max(), StreamId::min())),
  );
  rec(&mut log, "xread", db.xread("s2", StreamId::min(), Some(2)));
  rec(
    &mut log,
    "xread_streams",
    db.xread_streams(&[("s2", StreamId::min())], Some(2)),
  );
  rec(
    &mut log,
    "xgroup_create",
    db.xgroup_create("s2", "g", "0", false, None),
  );
  rec(
    &mut log,
    "xgroup_create_consumer",
    db.xgroup_create_consumer("s2", "g", "c1"),
  );
  rec(&mut log, "xinfo_groups", db.xinfo_groups("s2"));
  rec(
    &mut log,
    "xreadgroup",
    db.xreadgroup("s2", "g", "c1", ">", Some(5), false),
  );
  rec(
    &mut log,
    "xreadgroup_streams",
    db.xreadgroup_streams("g", "c1", &[("s2", ">")], Some(5), false),
  );
  rec(&mut log, "xpending_summary", db.xpending_summary("s2", "g"));
  let pr = db.xpending_range("s2", "g", StreamPending::default());
  rec(
    &mut log,
    "xpending_range_ids",
    Ok(pr.map(|v| v.into_iter().map(|n| n.id).collect::<Vec<_>>())),
  );
  rec(
    &mut log,
    "xclaim",
    db.xclaim("s2", "g", "c2", 0, &[s1_id], StreamClaim::default()),
  );
  rec(
    &mut log,
    "xautoclaim",
    db.xautoclaim("s2", "g", "c2", StreamAutoClaim::default()),
  );
  rec(&mut log, "xack", db.xack("s2", "g", &[s1_id, s2_id]));
  rec(
    &mut log,
    "xpending_summary_after",
    db.xpending_summary("s2", "g"),
  );
  rec(&mut log, "xinfo_stream", db.xinfo_stream("s2", true, None));
  rec(&mut log, "xtrim", db.xtrim("s2", StreamTrim::maxlen(1)));
  rec(&mut log, "xlen_after_trim", db.xlen("s2"));
  rec(&mut log, "xdel", db.xdel("s2", &[s1_id]));
  rec(
    &mut log,
    "xgroup_set_id",
    db.xgroup_set_id("s2", "g", "0", None),
  );
  rec(
    &mut log,
    "xgroup_del_consumer",
    db.xgroup_del_consumer("s2", "g", "c1"),
  );
  rec(&mut log, "xgroup_destroy", db.xgroup_destroy("s2", "g"));
  rec(
    &mut log,
    "xsetid",
    db.xsetid("s2", StreamId::new(100, 0), None, None),
  );
  rec(&mut log, "xlast_id_after_setid", db.xlast_id("s2"));

  // ================= key / expiry ========================================
  rec(&mut log, "key_set", db.set("k_str", "v_str", []));
  rec(&mut log, "key_hset", db.hset("k_hash", &[("f", "v")]));
  rec(&mut log, "key_lpush", db.lpush("k_list", &["v"]));
  rec(
    &mut log,
    "key_exists",
    db.exists(&["k_str", "k_list", "nope"]),
  );
  rec(&mut log, "key_exists_one", db.exists_one("k_str"));
  rec(&mut log, "key_keys", db.keys("k_*"));
  rec(&mut log, "key_count", db.key_count());
  rec(&mut log, "key_type", db.type_of("k_str"));
  rec(&mut log, "key_expire", db.expire("k_str", 3600));
  rec(&mut log, "key_ttl_pos", Ok(db.ttl("k_str").map(|v| v > 0)));
  rec(
    &mut log,
    "key_pttl_pos",
    Ok(db.pttl("k_str").map(|v| v > 0)),
  );
  rec(&mut log, "key_persist", db.persist("k_str"));
  rec(
    &mut log,
    "key_expireat",
    db.expireat("k_str", 2_000_000_000_000),
  );
  rec(&mut log, "key_expiretime", db.expiretime("k_str"));
  rec(&mut log, "key_pexpiretime", db.pexpiretime("k_str"));
  rec(&mut log, "key_get_expire_at", db.get_key_expire_at("k_str"));
  rec(&mut log, "key_pexpire", db.pexpire("k_str", 60_000));
  rec(
    &mut log,
    "key_pexpireat",
    db.pexpireat("k_str", 2_000_000_000_000_000),
  );
  rec(&mut log, "key_del", db.del(&["k_str", "k_hash", "k_list"]));
  rec(&mut log, "key_del_one", db.del_one("k_str"));
  rec(&mut log, "key_exists_after", db.exists(&["k_str"]));

  // ================= timeseries ==========================================
  rec(&mut log, "ts_create_one", db.ts_create_one("ts_a"));
  rec(
    &mut log,
    "ts_create",
    db.ts_create(
      "ts_cpu",
      [
        TsCreate::DuplicatePolicy(DuplicatePolicy::Last),
        TsCreate::Labels(vec![("sensor".to_string(), "temp".to_string())]),
      ],
    ),
  );
  rec(&mut log, "ts_add1", db.ts_add("ts_a", 1000, 25.0, None, []));
  rec(
    &mut log,
    "ts_add2",
    db.ts_add("ts_a", 2000, 26.5, Some(DuplicatePolicy::Last), []),
  );
  rec(
    &mut log,
    "ts_incrby",
    db.ts_incrby("ts_a", 1.0, Some(3000), []),
  );
  rec(
    &mut log,
    "ts_decrby",
    db.ts_decrby("ts_a", 0.5, Some(4000), []),
  );
  rec(&mut log, "ts_get", db.ts_get("ts_a"));
  rec(
    &mut log,
    "ts_range_one",
    db.ts_range_one("ts_a", (0, 10000)),
  );
  rec(
    &mut log,
    "ts_revrange_one",
    db.ts_revrange_one("ts_a", (0, 10000)),
  );
  rec(
    &mut log,
    "ts_range",
    db.ts_range("ts_a", (0, 10000), [TsRange::Count(2)]),
  );
  rec(
    &mut log,
    "ts_revrange",
    db.ts_revrange("ts_a", (0, 10000), [TsRange::Count(2)]),
  );
  rec(
    &mut log,
    "ts_range_agg",
    db.ts_range(
      "ts_a",
      (0, 10000),
      [TsRange::Aggregation(AggregationType::Avg, 2000)],
    ),
  );
  rec(&mut log, "ts_madd_one", db.ts_madd_one("ts_b", 2000, 20.0));
  rec(
    &mut log,
    "ts_madd",
    db.ts_madd(&[("ts_a", 5000, 27.0), ("ts_b", 3000, 21.0)]),
  );
  rec(&mut log, "ts_info", db.ts_info("ts_a"));
  rec(
    &mut log,
    "ts_alter",
    db.ts_alter(
      "ts_a",
      Some(86_400_000),
      None,
      Some(DuplicatePolicy::Block),
      None,
    ),
  );
  rec(
    &mut log,
    "ts_queryindex",
    db.ts_queryindex(&["sensor=temp".to_string()]),
  );
  rec(
    &mut log,
    "ts_mget",
    db.ts_mget([TsMGet::Filters(vec!["sensor=temp".to_string()])]),
  );
  rec(
    &mut log,
    "ts_mrange",
    db.ts_mrange(
      (0, 10000),
      [TsMRange::Filters(vec!["sensor=temp".to_string()])],
    ),
  );
  rec(
    &mut log,
    "ts_mrevrange",
    db.ts_mrevrange(
      (0, 10000),
      [TsMRange::Filters(vec!["sensor=temp".to_string()])],
    ),
  );
  rec(
    &mut log,
    "ts_createrule",
    db.ts_createrule("ts_cpu", "ts_cpu_avg", AggregationType::Avg, 5000, None),
  );
  rec(
    &mut log,
    "ts_deleterule",
    db.ts_deleterule("ts_cpu", "ts_cpu_avg"),
  );
  rec(&mut log, "ts_del", db.ts_del("ts_a", (1000, 2000)));

  // ================= json ================================================
  rec(
    &mut log,
    "json_set_one",
    db.json_set_one(
      "doc:1",
      "$",
      r#"{"user":{"name":"Alice","age":25,"active":true,"tags":["rust","db"]}}"#,
    ),
  );
  rec(
    &mut log,
    "json_set",
    db.json_set("doc:1", "$.user.age", "26", [JsonSet::Xx]),
  );
  rec(
    &mut log,
    "json_get_one",
    Ok(
      db.json_get_one("doc:1", "$.user.name")
        .map(|o| o.map(|s| canon_json_str(&s))),
    ),
  );
  rec(
    &mut log,
    "json_get",
    Ok(
      db.json_get("doc:1", &["$.user"], [])
        .map(|o| o.map(|s| canon_json_str(&s))),
    ),
  );
  rec(
    &mut log,
    "json_get_formatted",
    Ok(
      db.json_get_formatted("doc:1", &["$.user"], Some("  "), Some("\n"), Some(" "))
        .map(|o| o.map(|s| canon_json_str(&s))),
    ),
  );
  rec(
    &mut log,
    "json_mset",
    db.json_mset(&[("doc:2", "$", r#"{"val":100}"#)]),
  );
  rec(
    &mut log,
    "json_mset_one",
    db.json_mset_one("doc:3", "$", r#"{"x":1}"#),
  );
  rec(
    &mut log,
    "json_mget",
    Ok(db.json_mget(&["doc:1", "doc:2"], "$.user.name").map(|v| {
      v.into_iter()
        .map(|o| o.map(|s| canon_json_str(&s)))
        .collect::<Vec<_>>()
    })),
  );
  rec(
    &mut log,
    "json_type",
    db.json_type("doc:1", Some("$.user.name")),
  );
  rec(
    &mut log,
    "json_numincrby",
    Ok(
      db.json_numincrby("doc:1", "$.user.age", "1.0")
        .map(|o| o.map(|s| canon_json_str(&s))),
    ),
  );
  rec(
    &mut log,
    "json_nummultby",
    Ok(
      db.json_nummultby("doc:1", "$.user.age", "2.0")
        .map(|o| o.map(|s| canon_json_str(&s))),
    ),
  );
  rec(
    &mut log,
    "json_strappend",
    db.json_strappend("doc:1", Some("$.user.name"), r#"" Smith""#),
  );
  rec(
    &mut log,
    "json_strlen",
    db.json_strlen("doc:1", Some("$.user.name")),
  );
  rec(
    &mut log,
    "json_arrappend",
    db.json_arrappend("doc:1", "$.user.tags", &[r#""lsm""#]),
  );
  rec(
    &mut log,
    "json_arrinsert",
    db.json_arrinsert("doc:1", "$.user.tags", 0, &[r#""core""#]),
  );
  rec(
    &mut log,
    "json_arrindex",
    db.json_arrindex(
      "doc:1",
      "$.user.tags",
      r#""rust""#,
      [JsonArrIndex::Start(0)],
    ),
  );
  rec(
    &mut log,
    "json_arrlen",
    db.json_arrlen("doc:1", Some("$.user.tags")),
  );
  rec(
    &mut log,
    "json_arrpop",
    Ok(db.json_arrpop("doc:1", Some("$.user.tags"), None).map(|v| {
      v.into_iter()
        .map(|o| o.map(|s| canon_json_str(&s)))
        .collect::<Vec<_>>()
    })),
  );
  rec(
    &mut log,
    "json_arrtrim",
    db.json_arrtrim("doc:1", "$.user.tags", 0, 1),
  );
  rec(
    &mut log,
    "json_toggle",
    db.json_toggle("doc:1", Some("$.user.active")),
  );
  rec(
    &mut log,
    "json_merge",
    db.json_merge("doc:1", "$.user", r#"{"city":"Shanghai"}"#),
  );
  rec(
    &mut log,
    "json_objkeys",
    Ok(db.json_objkeys("doc:1", Some("$.user")).map(|v| {
      v.into_iter()
        .map(|o| {
          o.map(|mut k| {
            k.sort();
            k
          })
        })
        .collect::<Vec<_>>()
    })),
  );
  rec(
    &mut log,
    "json_objlen",
    db.json_objlen("doc:1", Some("$.user")),
  );
  rec(
    &mut log,
    "json_debug_memory",
    db.json_debug_memory("doc:1", None),
  );
  rec(&mut log, "json_info", db.json_info("doc:1"));
  rec(
    &mut log,
    "json_clear",
    db.json_clear("doc:1", Some("$.user.tags")),
  );
  rec(
    &mut log,
    "json_del_part",
    db.json_del("doc:1", Some("$.user.city")),
  );
  rec(&mut log, "json_del_all", db.json_del("doc:1", None));

  // ================= geo =================================================
  rec(
    &mut log,
    "geo_add1",
    db.geoadd_one("cities", 13.361389, 38.115556, "Palermo"),
  );
  rec(
    &mut log,
    "geo_add2",
    db.geoadd(
      "cities",
      &[
        (116.4074, 39.9042, "Beijing"),
        (121.4737, 31.2304, "Shanghai"),
      ],
      [],
    ),
  );
  rec(&mut log, "geo_pos", db.geopos_one("cities", "Palermo"));
  rec(
    &mut log,
    "geo_pos_multi",
    db.geopos("cities", &["Palermo", "Beijing"]),
  );
  rec(&mut log, "geo_hash", db.geohash_one("cities", "Palermo"));
  rec(
    &mut log,
    "geo_hash_multi",
    db.geohash("cities", &["Palermo", "Beijing"]),
  );
  rec(
    &mut log,
    "geo_dist",
    db.geodist("cities", "Palermo", "Beijing", Some("km")),
  );
  rec(
    &mut log,
    "geo_radius",
    db.georadius("cities", 116.4074, 39.9042, 1000.0, &GeoRadius::default()),
  );
  rec(
    &mut log,
    "geo_radius_member",
    db.georadiusbymember("cities", "Beijing", 500.0, &GeoRadius::default()),
  );
  let origin = OriginPoint::coord(116.4074, 39.9042);
  let mut shape = GeoShape::new_circular(116.4074, 39.9042, 2_000_000.0);
  rec(
    &mut log,
    "geo_search",
    db.geosearch("cities", &origin, &mut shape, []),
  );
  rec(
    &mut log,
    "geo_search_store",
    db.geosearchstore("stored_cities", "cities", &origin, &mut shape, []),
  );

  // ================= hll =================================================
  rec(&mut log, "hll_add1", db.pfadd_one("hll1", "a"));
  rec(&mut log, "hll_add_multi", db.pfadd("hll2", &["c", "d"]));
  rec(&mut log, "hll_add_multi2", db.pfadd("hll1", &["b", "c"]));
  rec(&mut log, "hll_count1", db.pfcount(&[b"hll1"]));
  rec(&mut log, "hll_count2", db.pfcount(&[b"hll1", b"hll2"]));
  rec(&mut log, "hll_count_one", db.pfcount_one("hll1"));
  rec(
    &mut log,
    "hll_merge",
    db.pfmerge(
      b"hll_m".as_slice(),
      &[b"hll1".as_slice(), b"hll2".as_slice()],
    ),
  );
  rec(&mut log, "hll_count_merged", db.pfcount(&[b"hll_m"]));
  rec(&mut log, "hll_selftest", Ok(db.pfselftest()));

  // ================= bloom & cuckoo ======================================
  rec(
    &mut log,
    "bf_reserve",
    db.bf_reserve("bf", 0.01, 1000, [BfReserve::Expansion(2)]),
  );
  rec(
    &mut log,
    "bf_reserve_one",
    db.bf_reserve_one("bf_single", 0.01, 1000),
  );
  rec(&mut log, "bf_add1", db.bf_add("bf", "alpha"));
  rec(&mut log, "bf_madd", db.bf_madd("bf", &["beta", "gamma"]));
  rec(&mut log, "bf_insert", db.bf_insert("bf", &["delta"], []));
  rec(
    &mut log,
    "bf_insert_one",
    db.bf_insert_one("bf", "epsilon", []),
  );
  rec(&mut log, "bf_exists1", db.bf_exists("bf", "alpha"));
  rec(&mut log, "bf_exists2", db.bf_exists("bf", "unknown"));
  rec(
    &mut log,
    "bf_mexists",
    db.bf_mexists("bf", &[b"alpha".as_slice(), b"unknown".as_slice()]),
  );
  rec(&mut log, "bf_info", db.bf_info("bf"));
  rec(&mut log, "bf_card", db.bf_card("bf"));
  rec(
    &mut log,
    "cf_reserve",
    db.cf_reserve(
      "cf",
      1000,
      [
        CfReserve::BucketSize(2),
        CfReserve::MaxIterations(500),
        CfReserve::Expansion(1),
      ],
    ),
  );
  rec(
    &mut log,
    "cf_reserve_one",
    db.cf_reserve_one("cf_single", 1000),
  );
  rec(&mut log, "cf_add", db.cf_add("cf", "elem1"));
  rec(&mut log, "cf_addnx", db.cf_addnx("cf", "elem2"));
  rec(&mut log, "cf_insert", db.cf_insert("cf", &["elem3"], []));
  rec(
    &mut log,
    "cf_insert_one",
    db.cf_insert_one("cf", "elem4", []),
  );
  rec(
    &mut log,
    "cf_insertnx_one",
    db.cf_insertnx_one("cf", "elem5", []),
  );
  rec(
    &mut log,
    "cf_insertnx",
    db.cf_insertnx("cf", &["elem6"], []),
  );
  rec(&mut log, "cf_exists", db.cf_exists("cf", "elem1"));
  rec(
    &mut log,
    "cf_mexists",
    db.cf_mexists("cf", &[b"elem1".as_slice(), b"unknown".as_slice()]),
  );
  rec(&mut log, "cf_count", db.cf_count("cf", "elem1"));
  rec(&mut log, "cf_del", db.cf_del("cf", "elem1"));
  rec(&mut log, "cf_info", db.cf_info("cf"));

  // ================= tdigest =============================================
  rec(&mut log, "td_create", db.tdigest_create("td", 100.0));
  rec(
    &mut log,
    "td_add",
    db.tdigest_add("td", &[10.0, 20.0, 30.0, 40.0, 50.0, 90.0, 99.0]),
  );
  rec(&mut log, "td_add_one", db.tdigest_add_one("td", 25.5));
  rec(&mut log, "td_min", db.tdigest_min("td"));
  rec(&mut log, "td_max", db.tdigest_max("td"));
  rec(
    &mut log,
    "td_quantile",
    db.tdigest_quantile("td", &[0.5, 0.95, 0.99]),
  );
  rec(
    &mut log,
    "td_quantile_one",
    db.tdigest_quantile_one("td", 0.5),
  );
  rec(&mut log, "td_cdf", db.tdigest_cdf("td", &[25.0, 50.0]));
  rec(&mut log, "td_cdf_one", db.tdigest_cdf_one("td", 25.0));
  rec(&mut log, "td_rank", db.tdigest_rank("td", &[30.0]));
  rec(&mut log, "td_rank_one", db.tdigest_rank_one("td", 30.0));
  rec(&mut log, "td_revrank", db.tdigest_revrank("td", &[30.0]));
  rec(
    &mut log,
    "td_revrank_one",
    db.tdigest_revrank_one("td", 30.0),
  );
  rec(&mut log, "td_byrank", db.tdigest_byrank("td", &[2]));
  rec(&mut log, "td_byrank_one", db.tdigest_byrank_one("td", 0));
  rec(&mut log, "td_byrevrank", db.tdigest_byrevrank("td", &[2]));
  rec(
    &mut log,
    "td_byrevrank_one",
    db.tdigest_byrevrank_one("td", 0),
  );
  rec(
    &mut log,
    "td_trimmed_mean",
    db.tdigest_trimmed_mean("td", 0.1, 0.9),
  );
  rec(&mut log, "td_info", db.tdigest_info("td"));
  rec(&mut log, "td_create_b", db.tdigest_create("td_b", 100.0));
  rec(
    &mut log,
    "td_add_b",
    db.tdigest_add("td_b", &[60.0, 70.0, 80.0]),
  );
  rec(&mut log, "td_create_m", db.tdigest_create("td_m", 100.0));
  rec(
    &mut log,
    "td_merge",
    db.tdigest_merge(
      b"td_m".as_slice(),
      &[b"td".as_slice(), b"td_b".as_slice()],
      [],
    ),
  );
  rec(&mut log, "td_reset", db.tdigest_reset("td_b"));

  // ================= sortedint ===========================================
  rec(
    &mut log,
    "si_add",
    db.si_add("si", &[100, 200, 300, 400, 500]),
  );
  rec(&mut log, "si_add_one", db.si_add_one("si", 600));
  rec(&mut log, "si_exists1", db.si_exists("si", 200));
  rec(&mut log, "si_exists2", db.si_exists("si", 250));
  rec(&mut log, "si_mexist", db.si_mexist("si", &[100, 250, 300]));
  rec(&mut log, "si_card", db.si_card("si"));
  rec(&mut log, "si_members", db.si_members("si"));
  rec(&mut log, "si_rank", db.si_rank("si", 200));
  rec(&mut log, "si_revrank", db.si_revrank("si", 200));
  rec(&mut log, "si_range", db.si_range("si", 0, 0, 3, false));
  rec(&mut log, "si_rev_range", db.si_rev_range("si", 0, 0, 3));
  let si_spec = SortedintRange {
    min: 200,
    max: 400,
    ..Default::default()
  };
  rec(&mut log, "si_count", db.si_count("si", &si_spec));
  rec(
    &mut log,
    "si_range_by_value",
    db.si_range_by_value("si", &si_spec),
  );
  rec(
    &mut log,
    "si_rev_range_by_value",
    db.si_rev_range_by_value("si", &si_spec),
  );
  rec(
    &mut log,
    "si_rem_range_by_value",
    db.si_rem_range_by_value(
      "si",
      &SortedintRange {
        min: 100,
        max: 150,
        ..Default::default()
      },
    ),
  );
  rec(
    &mut log,
    "si_rem_range_by_rank",
    db.si_rem_range_by_rank("si", (0, 0)),
  );
  rec(&mut log, "si_rem", db.si_rem("si", &[500, 600]));
  rec(&mut log, "si_card_after", db.si_card("si"));
  let mut siit = Vec::new();
  db.si_iter("si", |id| {
    siit.push(id);
    true
  })?;
  rec(&mut log, "si_iter", Ok(siit));

  // ================= namespaces / multi-db ===============================
  let other = db.wedb().ns(7)?.db(0)?;
  rec(&mut log, "set_ns7", other.set("ns7key", "v", []));
  rec(&mut log, "get_ns7", other.get("ns7key"));
  rec(&mut log, "get_default_ns7", db.get("ns7key"));
  rec(&mut log, "rm_ns7", db.wedb().ns(7)?.rm());
  rec(&mut log, "get_ns7_after_rm", other.get("ns7key"));

  Ok(log)
}

#[test]
fn parity_with_fjall() -> wedb_embed::Result<()> {
  let dir = tempdir()?;
  let fj = WeDb::new(Fjall::open(dir.path())?).ns(0)?.db(0)?;
  let fjall_log = script(&fj)?;

  let (obj, _store) = object_db("compat/parity");
  let object_log = script(&obj)?;

  for (i, (a, b)) in fjall_log.iter().zip(&object_log).enumerate() {
    if a != b {
      panic!("first divergence at index {i}: fjall={a:?} object={b:?}");
    }
  }
  assert_eq!(fjall_log.len(), object_log.len(), "log length differs");

  // dbsize is engine-approximate (Fjall counts physical history, ObjectLsm
  // counts live keys); only sanity-check that both see a populated catalog.
  assert!(fj.wedb().dbsize()? >= 1);
  assert!(obj.wedb().dbsize()? >= 1);
  Ok(())
}

fn write_batch(db: &Db<ObjectLsm>) -> wedb_embed::Result<()> {
  db.set("p_str", "persisted", [])?;
  db.rpush("p_list", &["a", "b", "c"])?;
  db.hset("p_hash", &[("f1", "v1"), ("f2", "v2")])?;
  db.zadd(
    "p_zset",
    &[(1.0, b"m1".as_slice()), (2.0, b"m2".as_slice())],
    [],
  )?;
  db.xadd("p_stream", Some(StreamId::new(1, 0)), &[("k", "v")])?;
  Ok(())
}

#[test]
fn objectlsm_reopen_persistence() -> wedb_embed::Result<()> {
  let (db, store) = object_db("compat/reopen");
  write_batch(&db)?;
  drop(db);

  let engine = ObjectLsm::open(
    Arc::new(store),
    Config::new("compat/reopen")
      .max_memtable_bytes(1024)
      .block_size(256)
      .max_segments_before_compact(6),
  )
  .expect("reopen");
  let db2 = WeDb::new(engine).ns(0)?.db(0)?;

  assert_eq!(db2.get("p_str")?, Some(b"persisted".to_vec()));
  assert_eq!(
    db2.lrange("p_list", (0, -1))?,
    vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]
  );
  assert_eq!(
    db2.hgetall("p_hash")?,
    vec![
      (b"f1".to_vec(), b"v1".to_vec()),
      (b"f2".to_vec(), b"v2".to_vec())
    ]
  );
  assert_eq!(
    db2.zrange("p_zset", b"0", b"-1", [])?,
    vec![(b"m1".to_vec(), 1.0), (b"m2".to_vec(), 2.0)]
  );
  assert_eq!(db2.xlen("p_stream")?, 1);
  Ok(())
}
