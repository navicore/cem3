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
//! ## Known limitation: no single-flight
//!
//! The cache collapses *sequential* fanout — repeated resolves of the
//! same host after one has filled the cache hit the fast path. It does
//! *not* deduplicate *concurrent* first-resolves: N strands racing to
//! resolve the same uncached host enqueue N worker jobs, each running
//! its own `getaddrinfo` and writing the cache on return. Wasted work
//! under bursty load (e.g., connection-pool warm-up), but not a
//! correctness issue. Single-flight via an in-flight map keyed by
//! hostname is a planned follow-up.

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

struct Job {
    hostname: String,
    reply: may::sync::mpsc::Sender<Vec<String>>,
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
        let _ = job.reply.send(addrs); // requester may have died — drop
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
        if hostname.is_empty() {
            return push_failure(stack);
        }

        // Fast path: cache hit.
        if let Some(addrs) = CACHE.lock().unwrap().get(&hostname) {
            return push_result(stack, addrs);
        }

        // Slow path: queue on the worker pool. The may-channel recv()
        // yields the strand until a worker thread sends the reply.
        let sender = match JOB_QUEUE.as_ref() {
            Some(s) => s,
            None => return push_failure(stack), // pool failed to start
        };
        let (reply_tx, reply_rx) = may::sync::mpsc::channel::<Vec<String>>();
        let job = Job {
            hostname: hostname.clone(),
            reply: reply_tx,
        };
        if sender.send(job).is_err() {
            return push_failure(stack);
        }
        let addrs = match reply_rx.recv() {
            Ok(a) => a,
            Err(_) => return push_failure(stack),
        };
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
