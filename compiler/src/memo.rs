//! Results kept across calls: a compile or a language-service analysis of the same text with the
//! same settings, whose resolver still returns what it returned then, is the same result.

use std::cell::RefCell;
use std::thread::LocalKey;

use crate::typeck::Resolver;

/// How many results each cache keeps.
const KEEP: usize = 6;

/// What changes a result besides the source and the files read: the NS bindings, overflow checks
/// and strict borrows, and whether it's a test build.
#[derive(PartialEq, Clone, Copy)]
struct Settings {
    bindings: u64,
    overflow: bool,
    strict: bool,
    test: bool,
}

fn settings() -> Settings {
    Settings {
        bindings: crate::bindings::generation(),
        overflow: crate::codegen::overflow_checks(),
        strict: crate::nll::strict(),
        test: crate::adt::test_build(),
    }
}

pub(crate) struct Entry<T> {
    src: String,
    filename: String,
    settings: Settings,
    /// Every file read through the resolver, in order, with what it returned.
    reads: Vec<(String, Option<String>)>,
    value: T,
}

pub(crate) type Cache<T> = RefCell<Vec<Entry<T>>>;

/// `f(src, filename, resolver)`'s result from `cache` when an earlier call had the same text,
/// file name and settings and the resolver gives the same answers for the files it read; else
/// `f`'s result, kept. Within one call a file is read from the resolver only once.
pub(crate) fn cached<T: Clone + 'static>(
    cache: &'static LocalKey<Cache<T>>,
    src: &str,
    filename: &str,
    resolver: Resolver,
    f: impl FnOnce(Resolver) -> T,
) -> T {
    let now = settings();
    let hit = cache.with(|c| {
        let c = c.borrow();
        c.iter()
            .position(|e| e.src == src && e.filename == filename && e.settings == now)
            .map(|i| {
                (
                    i,
                    c[i].reads
                        .iter()
                        .map(|(p, _)| p.clone())
                        .collect::<Vec<_>>(),
                )
            })
    });
    if let Some((i, paths)) = hit {
        // (The resolver is called outside the borrow: it's the host's code.)
        let answers: Vec<Option<String>> = paths.iter().map(|p| resolver(p)).collect();
        let found = cache.with(|c| {
            let mut c = c.borrow_mut();
            if i < c.len()
                && c[i].src == src
                && c[i].reads.iter().map(|(_, t)| t).eq(answers.iter())
            {
                // (Most recent last.)
                let e = c.remove(i);
                let v = e.value.clone();
                c.push(e);
                Some(v)
            } else {
                None
            }
        });
        if let Some(v) = found {
            return v;
        }
    }
    let reads: RefCell<Vec<(String, Option<String>)>> = RefCell::new(Vec::new());
    let recording = |p: &str| -> Option<String> {
        if let Some((_, t)) = reads.borrow().iter().find(|(q, _)| q == p) {
            return t.clone();
        }
        let t = resolver(p);
        reads.borrow_mut().push((p.to_string(), t.clone()));
        t
    };
    let value = f(&recording);
    // (Settings changed during the call, by a nested compile: not kept.)
    if settings() == now {
        cache.with(|c| {
            let mut c = c.borrow_mut();
            c.retain(|e| !(e.src == src && e.filename == filename && e.settings == now));
            if c.len() >= KEEP {
                c.remove(0);
            }
            c.push(Entry {
                src: src.to_string(),
                filename: filename.to_string(),
                settings: now,
                reads: reads.into_inner(),
                value: value.clone(),
            });
        });
    }
    value
}
