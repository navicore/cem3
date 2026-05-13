//! DNS resolution for Seq.
//!
//! Resolves hostnames to IP-address strings via libc's `getaddrinfo`,
//! offloaded onto a dedicated OS-thread pool so may-carrier threads
//! never park on the syscall. Inherits all platform-correct resolution
//! behaviour (`/etc/hosts`, systemd-resolved, VPN/corp DNS, mDNS) for
//! free.
//!
//! A small TTL cache collapses fanout to the same host. Cache and
//! worker pool are process-global and lazy-initialised on first call.
//!
//! ## Surface
//!
//! `net.dns.resolve ( String -- List Bool )` returns a list of IP-string
//! representations. On unresolvable name or empty result, pushes
//! `(empty-list, false)`.
//!
//! ## Fanout collapsing
//!
//! Both *sequential* and *concurrent* fanout collapse to a single
//! `getaddrinfo`. The TTL cache (see `CACHE`) catches the sequential
//! case. The in-flight map (see `IN_FLIGHT`) catches the concurrent
//! case: when N strands race to resolve the same uncached host, the
//! first to arrive enqueues exactly one worker job and the others
//! attach their reply channels to the in-flight entry. When the
//! worker returns, it writes the cache and fans the result out to
//! every attached channel. Late arrivers that come in after the
//! fanout pop see the freshly-written cache and short-circuit
//! without ever touching the in-flight map.

use crate::seqstring::global_string;
use crate::stack::{Stack, pop, push};
use crate::value::{Value, VariantData};

use std::collections::HashMap;
use std::net::ToSocketAddrs;
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_WORKERS: usize = 8;
const MAX_WORKERS: usize = 64;
const CACHE_TTL: Duration = Duration::from_secs(60);
const CACHE_MAX: usize = 256;

struct CacheEntry {
    addrs: Vec<String>,
    expires: Instant,
}

struct DnsCache {
    entries: HashMap<String, CacheEntry>,
}

impl DnsCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    fn get(&mut self, host: &str) -> Option<Vec<String>> {
        if let Some(entry) = self.entries.get(host)
            && Instant::now() < entry.expires
        {
            return Some(entry.addrs.clone());
        }
        self.entries.remove(host);
        None
    }

    fn put(&mut self, host: String, addrs: Vec<String>) {
        if self.entries.len() >= CACHE_MAX {
            // Bounded eviction: drop one arbitrary entry. True LRU is
            // overkill for v1 — DNS lookups are O(50ms) on miss, and the
            // 60s TTL means a churned-out entry costs at most one extra
            // worker round-trip.
            if let Some(k) = self.entries.keys().next().cloned() {
                self.entries.remove(&k);
            }
        }
        self.entries.insert(
            host,
            CacheEntry {
                addrs,
                expires: Instant::now() + CACHE_TTL,
            },
        );
    }
}

static CACHE: LazyLock<Mutex<DnsCache>> = LazyLock::new(|| Mutex::new(DnsCache::new()));

type Replies = Vec<may::sync::mpsc::Sender<Vec<String>>>;

/// In-flight resolutions, keyed by hostname. The first strand that
/// wants a hostname inserts an entry containing its reply channel,
/// then enqueues a worker job; subsequent strands wanting the same
/// hostname find the entry and *append* their reply channels instead
/// of enqueuing duplicate work. When the worker returns, it removes
/// the entry and fans the result out to every channel. Closes the
/// "N concurrent first-resolves of the same uncached host enqueue N
/// worker jobs" gap from PR1.
static IN_FLIGHT: LazyLock<Mutex<HashMap<String, Replies>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

struct Job {
    hostname: String,
}

/// Lazy job queue. First send spawns the worker pool. Workers live for
/// the lifetime of the process; the static sender drops only at shutdown.
///
/// `None` means every requested worker failed to spawn (resource
/// starvation, ulimit, etc.). In that case the FFI fast-fails to
/// `(empty-list, false)` instead of panicking forever.
///
/// Note: `SEQ_DNS_WORKERS=0` silently falls back to `DEFAULT_WORKERS`
/// — disabling the pool makes no architectural sense (the syscall has
/// to run *somewhere*), so we treat 0 as "unset" rather than rejecting.
static JOB_QUEUE: LazyLock<Option<std_mpsc::Sender<Job>>> = LazyLock::new(|| {
    let (tx, rx) = std_mpsc::channel::<Job>();
    let rx = Arc::new(Mutex::new(rx));

    let workers = std::env::var("SEQ_DNS_WORKERS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_WORKERS)
        .min(MAX_WORKERS);

    let mut spawned = 0usize;
    for i in 0..workers {
        let rx = rx.clone();
        if thread::Builder::new()
            .name(format!("seq-dns-{i}"))
            .spawn(move || worker_loop(rx))
            .is_ok()
        {
            spawned += 1;
        }
        // Spawn failures degrade the pool gracefully — we keep
        // whatever workers the OS allowed. A poisoned LazyLock would
        // turn a soft starvation event into permanent FFI panics.
    }
    if spawned == 0 { None } else { Some(tx) }
});

fn worker_loop(rx: Arc<Mutex<std_mpsc::Receiver<Job>>>) {
    loop {
        // Only one worker holds the lock while inside recv(); others
        // queue on the Mutex. As soon as a job arrives the receiver
        // releases the lock, another worker enters recv(), and the
        // current worker spends the bulk of its time in getaddrinfo.
        // Parallelism is genuine; the lock contention is sub-microsecond.
        let job = match rx.lock().unwrap().recv() {
            Ok(j) => j,
            Err(_) => return, // sender dropped — process shutting down
        };
        let addrs = resolve_blocking(&job.hostname);
        if !addrs.is_empty() {
            CACHE
                .lock()
                .unwrap()
                .put(job.hostname.clone(), addrs.clone());
        }
        // Cache is written before the IN_FLIGHT fanout so a late
        // arriver that races the fanout (between this worker popping
        // IN_FLIGHT and the senders firing) will see the cache hit and
        // never need to attach.
        let senders = IN_FLIGHT
            .lock()
            .unwrap()
            .remove(&job.hostname)
            .unwrap_or_default();
        for s in senders {
            let _ = s.send(addrs.clone()); // requester may have died — drop
        }
    }
}

fn resolve_blocking(hostname: &str) -> Vec<String> {
    // Port 0 is a dummy — getaddrinfo wants host:port but we only need
    // the address list. Deduplicate because A and AAAA records can
    // produce the same address representation in some configurations.
    match (hostname, 0u16).to_socket_addrs() {
        Ok(iter) => {
            let mut seen = Vec::new();
            for sa in iter {
                let ip = sa.ip().to_string();
                if !seen.contains(&ip) {
                    seen.push(ip);
                }
            }
            seen
        }
        Err(_) => Vec::new(),
    }
}

/// Resolve a hostname to a `Vec<IpAddr>`, with an IP-literal fast path.
///
/// If `hostname` parses as an IP literal, returns `vec![that_ip]`
/// without touching the worker pool or the cache. Otherwise falls
/// back to `resolve(hostname)` and parses each returned string.
///
/// This is the preferred entry point for any caller that's going to
/// build `SocketAddr`s from the result (TCP connect, UDP send-to,
/// HTTP SSRF validation). Returns an empty Vec on resolution failure.
pub fn resolve_to_ips(hostname: &str) -> Vec<std::net::IpAddr> {
    if let Ok(ip) = hostname.parse::<std::net::IpAddr>() {
        return vec![ip];
    }
    resolve(hostname)
        .iter()
        .filter_map(|s| s.parse::<std::net::IpAddr>().ok())
        .collect()
}

/// Resolve a hostname to a list of IP-address strings.
///
/// Cooperative: yields the strand via the may-channel recv() while a
/// worker thread runs `getaddrinfo`. Returns an empty Vec on any
/// failure (empty hostname, worker pool failed to start, send/recv
/// error, or unresolvable name). Other runtime modules call this
/// directly so they share the cache and worker pool with the FFI
/// surface instead of opening a parallel path to `getaddrinfo`.
///
/// For callers that produce `IpAddr` values, prefer
/// [`resolve_to_ips`] — it short-circuits IP-literal input to skip
/// the worker round-trip.
pub fn resolve(hostname: &str) -> Vec<String> {
    if hostname.is_empty() {
        return Vec::new();
    }
    if let Some(addrs) = CACHE.lock().unwrap().get(hostname) {
        return addrs;
    }

    let (reply_tx, reply_rx) = may::sync::mpsc::channel::<Vec<String>>();

    // Attach our reply channel to the in-flight map. If we're the
    // first arriver for this hostname we become the *leader* and
    // enqueue a worker job; otherwise we attach to the existing entry
    // and wait for the leader's worker to fan the result out.
    let became_leader = {
        let mut in_flight = IN_FLIGHT.lock().unwrap();
        match in_flight.get_mut(hostname) {
            Some(senders) => {
                senders.push(reply_tx);
                false
            }
            None => {
                in_flight.insert(hostname.to_string(), vec![reply_tx]);
                true
            }
        }
    };

    if became_leader {
        let enqueue_err = match JOB_QUEUE.as_ref() {
            Some(s) => s
                .send(Job {
                    hostname: hostname.to_string(),
                })
                .is_err(),
            None => true, // worker pool failed to start
        };
        if enqueue_err {
            // No worker will pop this hostname; nothing will fan
            // results out. Drain ourselves and any followers that
            // raced us, signalling empty-result failure.
            let senders = IN_FLIGHT
                .lock()
                .unwrap()
                .remove(hostname)
                .unwrap_or_default();
            for s in senders {
                let _ = s.send(Vec::new());
            }
            return Vec::new();
        }
    }

    reply_rx.recv().unwrap_or_default()
}

/// Resolve a hostname to a list of IP-address strings.
///
/// Stack effect: `( String -- Variant Bool )`
///
/// The Variant is a `List` (tag `"List"`) of IP-string `Value::String`s.
/// On unresolvable hostname, empty result, or type mismatch, pushes
/// `(empty-list, false)`. Yields the strand cooperatively while the
/// lookup runs on the worker pool.
///
/// # Safety
/// Stack must have a String (hostname) on top.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_dns_resolve(stack: Stack) -> Stack {
    unsafe {
        let (stack, host_val) = pop(stack);
        let host = match host_val {
            Value::String(s) => s,
            _ => return push_failure(stack),
        };
        let hostname = host.as_str_or_empty().to_string();
        let addrs = resolve(&hostname);
        push_result(stack, addrs)
    }
}

unsafe fn push_result(stack: Stack, addrs: Vec<String>) -> Stack {
    unsafe {
        if addrs.is_empty() {
            return push_failure(stack);
        }
        let fields = addrs
            .into_iter()
            .map(|s| Value::String(global_string(s)))
            .collect();
        let list = Value::Variant(Arc::new(VariantData::new(
            global_string("List".to_string()),
            fields,
        )));
        let stack = push(stack, list);
        push(stack, Value::Bool(true))
    }
}

unsafe fn push_failure(stack: Stack) -> Stack {
    unsafe {
        let empty = Value::Variant(Arc::new(VariantData::new(
            global_string("List".to_string()),
            vec![],
        )));
        let stack = push(stack, empty);
        push(stack, Value::Bool(false))
    }
}
