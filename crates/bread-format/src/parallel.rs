/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! Running independent work side by side, on threads where the platform has
//! them, and one after another where it does not, such as WebAssembly without
//! threads. Either way the results are the same, in the same order: only the
//! time changes.

/// Runs `a` and `b`, side by side where it can, and returns both results.
pub(crate) fn join<A, B>(a: impl FnOnce() -> A + Send, b: impl FnOnce() -> B + Send) -> (A, B)
where
    A: Send,
    B: Send,
{
    if threads() < 2 {
        return (a(), b());
    }
    std::thread::scope(|scope| {
        let other = scope.spawn(b);
        let first = a();
        (first, unwind(other.join()))
    })
}

/// `f` applied to every item, in order, with the items shared out among as
/// many threads as the machine runs at once.
pub(crate) fn map<T, R>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R>
where
    T: Sync,
    R: Send,
{
    let workers = threads().min(items.len());
    if workers < 2 {
        return items.iter().map(f).collect();
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut done: Vec<(usize, R)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut out = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(item) = items.get(i) else {
                            return out;
                        };
                        out.push((i, f(item)));
                    }
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| unwind(handle.join()))
            .collect()
    });
    done.sort_unstable_by_key(|&(i, _)| i);
    done.into_iter().map(|(_, r)| r).collect()
}

/// How many threads the machine runs at once: 1 where it cannot start any.
pub(crate) fn threads() -> usize {
    if cfg!(target_family = "wasm") {
        return 1;
    }
    std::thread::available_parallelism().map_or(1, usize::from)
}

/// A thread's result, or its panic, carried on to the caller.
fn unwind<T>(result: std::thread::Result<T>) -> T {
    result.unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_keeps_the_order_of_the_items() {
        let items: Vec<u64> = (0..1000).collect();
        let squares = map(&items, |&x| x * x);
        assert_eq!(squares, items.iter().map(|x| x * x).collect::<Vec<_>>());
        assert!(map(&[] as &[u8], |&x| x).is_empty());
    }

    #[test]
    fn join_returns_both_results_in_their_places() {
        assert_eq!(join(|| 1, || "two"), (1, "two"));
    }

    #[test]
    #[should_panic(expected = "carried over")]
    fn a_panic_on_another_thread_reaches_the_caller() {
        join(|| (), || -> () { panic!("carried over") });
    }
}
