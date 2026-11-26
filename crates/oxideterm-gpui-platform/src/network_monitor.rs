// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::{Arc, Mutex, OnceLock};

type NetworkChangeCallback = Arc<dyn Fn() + Send + Sync>;

/// RAII handle for a network-interface change subscription.
///
/// Dropping the last active subscription tears down the underlying OS watcher.
pub struct NetworkChangeSubscription {
    id: u64,
    unsubscribe: Box<dyn FnOnce(u64) + Send + Sync>,
}

impl Drop for NetworkChangeSubscription {
    fn drop(&mut self) {
        let unsubscribe = std::mem::replace(&mut self.unsubscribe, Box::new(|_| {}));
        unsubscribe(self.id);
    }
}

struct WatcherHubState<B> {
    next_id: u64,
    subscribers: Vec<(u64, NetworkChangeCallback)>,
    backend: Option<B>,
}

impl<B> Default for WatcherHubState<B> {
    fn default() -> Self {
        Self {
            next_id: 1,
            subscribers: Vec::new(),
            backend: None,
        }
    }
}

struct NetworkWatcherHub<B> {
    state: Mutex<WatcherHubState<B>>,
    start_backend: Box<dyn Fn(NetworkChangeCallback) -> Option<B> + Send + Sync>,
}

impl<B: Send + 'static> NetworkWatcherHub<B> {
    fn new(
        start_backend: impl Fn(NetworkChangeCallback) -> Option<B> + Send + Sync + 'static,
    ) -> Self {
        Self {
            state: Mutex::new(WatcherHubState::default()),
            start_backend: Box::new(start_backend),
        }
    }

    fn subscribe(self: &Arc<Self>, callback: NetworkChangeCallback) -> NetworkChangeSubscription {
        let (id, should_start) = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let id = state.next_id;
            state.next_id = state.next_id.wrapping_add(1).max(1);
            let should_start = state.subscribers.is_empty() && state.backend.is_none();
            state.subscribers.push((id, callback));
            (id, should_start)
        };

        if should_start {
            // Start the OS backend outside the subscriber lock so an immediate
            // callback cannot deadlock on `self.state`.
            let hub = Arc::clone(self);
            let dispatch: NetworkChangeCallback = Arc::new(move || hub.notify_all());
            if let Some(backend) = (self.start_backend)(dispatch) {
                let mut retired_backend = None;
                {
                    let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
                    if state.subscribers.is_empty() || state.backend.is_some() {
                        retired_backend = Some(backend);
                    } else {
                        state.backend = Some(backend);
                    }
                }
                drop(retired_backend);
            }
        }

        let hub = Arc::clone(self);
        NetworkChangeSubscription {
            id,
            unsubscribe: Box::new(move |id| hub.unsubscribe(id)),
        }
    }

    fn unsubscribe(&self, id: u64) {
        let retired_backend = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.subscribers.retain(|(entry_id, _)| *entry_id != id);
            if state.subscribers.is_empty() {
                state.backend.take()
            } else {
                None
            }
        };
        // Stop/join OS callbacks outside the lock to prevent lock-order inversion
        // with a concurrently firing OS notification callback.
        drop(retired_backend);
    }

    fn notify_all(&self) {
        let callbacks = {
            let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state
                .subscribers
                .iter()
                .map(|(_, callback)| Arc::clone(callback))
                .collect::<Vec<_>>()
        };
        for callback in callbacks {
            callback();
        }
    }
}

fn global_hub() -> &'static Arc<NetworkWatcherHub<platform::PlatformWatcher>> {
    static HUB: OnceLock<Arc<NetworkWatcherHub<platform::PlatformWatcher>>> = OnceLock::new();
    HUB.get_or_init(|| Arc::new(NetworkWatcherHub::new(platform::PlatformWatcher::start)))
}

/// Subscribes to OS-level network interface / route change notifications.
///
/// The callback runs on an OS background thread and must be lightweight and
/// non-blocking. Multiple subscribers share one process-wide OS watcher.
pub fn subscribe_network_changes(callback: NetworkChangeCallback) -> NetworkChangeSubscription {
    global_hub().subscribe(callback)
}

#[cfg(target_os = "macos")]
mod platform {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    };
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    use core_foundation::{
        array::CFArray,
        runloop::{CFRunLoop, kCFRunLoopCommonModes, kCFRunLoopDefaultMode},
        string::CFString,
    };
    use system_configuration::dynamic_store::{
        SCDynamicStore, SCDynamicStoreBuilder, SCDynamicStoreCallBackContext,
    };

    use super::NetworkChangeCallback;

    pub(super) struct PlatformWatcher {
        pub(super) stop_requested: Arc<AtomicBool>,
        run_loop: CFRunLoop,
        thread: Option<JoinHandle<()>>,
    }

    fn on_dynamic_store_change(
        _store: SCDynamicStore,
        changed_keys: CFArray<CFString>,
        callback: &mut NetworkChangeCallback,
    ) {
        if !changed_keys.is_empty() {
            callback();
        }
    }

    impl PlatformWatcher {
        pub(super) fn start(on_change: NetworkChangeCallback) -> Option<Self> {
            let stop_requested = Arc::new(AtomicBool::new(false));
            let thread_stop = Arc::clone(&stop_requested);
            let (ready_tx, ready_rx) = mpsc::sync_channel::<Option<CFRunLoop>>(1);

            let thread = thread::Builder::new()
                .name("oxideterm-network-monitor".to_string())
                .spawn(move || {
                    let context = SCDynamicStoreCallBackContext {
                        callout: on_dynamic_store_change,
                        info: on_change,
                    };
                    let Some(store) = SCDynamicStoreBuilder::new("com.oxideterm.network-monitor")
                        .callback_context(context)
                        .build()
                    else {
                        let _ = ready_tx.send(None);
                        return;
                    };

                    let keys = CFArray::from_CFTypes(&[
                        CFString::from_static_string("State:/Network/Global/IPv4"),
                        CFString::from_static_string("State:/Network/Global/IPv6"),
                    ]);
                    let patterns = CFArray::from_CFTypes(&[
                        CFString::from_static_string("State:/Network/Interface/.*/Link"),
                        CFString::from_static_string("State:/Network/Interface/.*/IPv4"),
                        CFString::from_static_string("State:/Network/Interface/.*/IPv6"),
                    ]);
                    if !store.set_notification_keys(&keys, &patterns) {
                        let _ = ready_tx.send(None);
                        return;
                    }
                    let Some(source) = store.create_run_loop_source() else {
                        let _ = ready_tx.send(None);
                        return;
                    };

                    let run_loop = CFRunLoop::get_current();
                    unsafe {
                        run_loop.add_source(&source, kCFRunLoopCommonModes);
                    }
                    if ready_tx.send(Some(run_loop.clone())).is_err() {
                        unsafe {
                            run_loop.remove_source(&source, kCFRunLoopCommonModes);
                        }
                        return;
                    }

                    // Check `stop_requested` on every cycle so a `stop()` issued
                    // before the run loop first enters cannot be missed.
                    while !thread_stop.load(Ordering::Acquire) {
                        unsafe {
                            CFRunLoop::run_in_mode(
                                kCFRunLoopDefaultMode,
                                Duration::from_secs(60),
                                false,
                            );
                        }
                    }

                    unsafe {
                        run_loop.remove_source(&source, kCFRunLoopCommonModes);
                    }
                })
                .ok()?;

            match ready_rx.recv().ok().flatten() {
                Some(run_loop) => Some(Self {
                    stop_requested,
                    run_loop,
                    thread: Some(thread),
                }),
                None => {
                    let _ = thread.join();
                    None
                }
            }
        }
    }

    impl Drop for PlatformWatcher {
        fn drop(&mut self) {
            self.stop_requested.store(true, Ordering::Release);
            self.run_loop.stop();
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use std::ffi::c_void;

    use windows::Win32::{
        Foundation::{HANDLE, NO_ERROR},
        NetworkManagement::IpHelper::{
            CancelMibChangeNotify2, MIB_IPINTERFACE_ROW, MIB_NOTIFICATION_TYPE,
            NotifyIpInterfaceChange,
        },
        Networking::WinSock::AF_UNSPEC,
    };

    use super::NetworkChangeCallback;

    pub(super) struct PlatformWatcher {
        handle: HANDLE,
        context_ptr: *mut NetworkChangeCallback,
    }

    // `NotifyIpInterfaceChange` handles and boxed `Arc<dyn Fn() + Send + Sync>`
    // are safe to tear down from any thread via `CancelMibChangeNotify2`.
    unsafe impl Send for PlatformWatcher {}

    unsafe extern "system" fn ip_interface_change_callback(
        caller_context: *const c_void,
        _row: *const MIB_IPINTERFACE_ROW,
        _notification_type: MIB_NOTIFICATION_TYPE,
    ) {
        if caller_context.is_null() {
            return;
        }
        let callback = unsafe { &*(caller_context as *const NetworkChangeCallback) };
        callback();
    }

    impl PlatformWatcher {
        pub(super) fn start(on_change: NetworkChangeCallback) -> Option<Self> {
            let context_ptr = Box::into_raw(Box::new(on_change));
            let mut handle = HANDLE::default();
            let status = unsafe {
                NotifyIpInterfaceChange(
                    AF_UNSPEC,
                    Some(ip_interface_change_callback),
                    Some(context_ptr as *const c_void),
                    false,
                    &mut handle,
                )
            };
            if status != NO_ERROR {
                unsafe {
                    drop(Box::from_raw(context_ptr));
                }
                return None;
            }
            Some(Self {
                handle,
                context_ptr,
            })
        }
    }

    impl Drop for PlatformWatcher {
        fn drop(&mut self) {
            // `CancelMibChangeNotify2` waits for any in-flight callback to
            // finish before returning, so freeing `context_ptr` afterward is safe.
            unsafe {
                let _ = CancelMibChangeNotify2(self.handle);
                drop(Box::from_raw(self.context_ptr));
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::mem;
    use std::os::fd::RawFd;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::thread::{self, JoinHandle};

    use super::NetworkChangeCallback;

    pub(super) struct PlatformWatcher {
        stop_requested: Arc<AtomicBool>,
        wake_write_fd: RawFd,
        thread: Option<JoinHandle<()>>,
    }

    impl PlatformWatcher {
        pub(super) fn start(on_change: NetworkChangeCallback) -> Option<Self> {
            let nl_fd = unsafe {
                libc::socket(
                    libc::AF_NETLINK,
                    libc::SOCK_RAW | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                    libc::NETLINK_ROUTE,
                )
            };
            if nl_fd < 0 {
                return None;
            }

            let mut addr: libc::sockaddr_nl = unsafe { mem::zeroed() };
            addr.nl_family = libc::AF_NETLINK as u16;
            addr.nl_groups =
                (libc::RTMGRP_LINK | libc::RTMGRP_IPV4_IFADDR | libc::RTMGRP_IPV6_IFADDR) as u32;
            let bind_rc = unsafe {
                libc::bind(
                    nl_fd,
                    &addr as *const libc::sockaddr_nl as *const libc::sockaddr,
                    mem::size_of::<libc::sockaddr_nl>() as libc::socklen_t,
                )
            };
            if bind_rc < 0 {
                unsafe {
                    libc::close(nl_fd);
                }
                return None;
            }

            let mut pipe_fds = [0 as RawFd; 2];
            if unsafe { libc::pipe2(pipe_fds.as_mut_ptr(), libc::O_NONBLOCK | libc::O_CLOEXEC) } < 0
            {
                unsafe {
                    libc::close(nl_fd);
                }
                return None;
            }
            let wake_read_fd = pipe_fds[0];
            let wake_write_fd = pipe_fds[1];

            let stop_requested = Arc::new(AtomicBool::new(false));
            let thread_stop = Arc::clone(&stop_requested);
            let thread = match thread::Builder::new()
                .name("oxideterm-network-monitor".to_string())
                .spawn(move || {
                    let mut buf = [0u8; 4096];
                    while !thread_stop.load(Ordering::Acquire) {
                        let mut poll_fds = [
                            libc::pollfd {
                                fd: nl_fd,
                                events: libc::POLLIN,
                                revents: 0,
                            },
                            libc::pollfd {
                                fd: wake_read_fd,
                                events: libc::POLLIN,
                                revents: 0,
                            },
                        ];
                        let rc = unsafe { libc::poll(poll_fds.as_mut_ptr(), 2, -1) };
                        if rc < 0 {
                            let err = std::io::Error::last_os_error();
                            if err.kind() == std::io::ErrorKind::Interrupted {
                                continue;
                            }
                            break;
                        }
                        if poll_fds[1].revents != 0 || thread_stop.load(Ordering::Acquire) {
                            break;
                        }
                        if (poll_fds[0].revents & (libc::POLLIN | libc::POLLERR)) != 0 {
                            let mut saw_change = false;
                            loop {
                                let n = unsafe {
                                    libc::recv(
                                        nl_fd,
                                        buf.as_mut_ptr() as *mut libc::c_void,
                                        buf.len(),
                                        libc::MSG_DONTWAIT,
                                    )
                                };
                                if n > 0 {
                                    saw_change = true;
                                    continue;
                                }
                                if n < 0 {
                                    let err = std::io::Error::last_os_error();
                                    match err.raw_os_error() {
                                        Some(libc::EAGAIN | libc::EWOULDBLOCK) => break,
                                        Some(libc::EINTR) => continue,
                                        Some(libc::ENOBUFS) => {
                                            // Kernel dropped netlink messages during a burst;
                                            // still notify subscribers so probes run once.
                                            saw_change = true;
                                            break;
                                        }
                                        _ => break,
                                    }
                                }
                                break;
                            }
                            if saw_change {
                                on_change();
                            }
                        }
                    }

                    unsafe {
                        libc::close(nl_fd);
                        libc::close(wake_read_fd);
                    }
                }) {
                Ok(thread) => thread,
                Err(_) => {
                    unsafe {
                        libc::close(nl_fd);
                        libc::close(wake_read_fd);
                        libc::close(wake_write_fd);
                    }
                    return None;
                }
            };

            Some(Self {
                stop_requested,
                wake_write_fd,
                thread: Some(thread),
            })
        }
    }

    impl Drop for PlatformWatcher {
        fn drop(&mut self) {
            self.stop_requested.store(true, Ordering::Release);
            let byte = [1u8; 1];
            unsafe {
                let _ = libc::write(self.wake_write_fd, byte.as_ptr() as *const libc::c_void, 1);
            }
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
            unsafe {
                libc::close(self.wake_write_fd);
            }
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod platform {
    use super::NetworkChangeCallback;

    pub(super) struct PlatformWatcher;

    impl PlatformWatcher {
        pub(super) fn start(_on_change: NetworkChangeCallback) -> Option<Self> {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct FakeBackend {
        stops: Arc<AtomicUsize>,
    }

    impl Drop for FakeBackend {
        fn drop(&mut self) {
            self.stops.fetch_add(1, Ordering::AcqRel);
        }
    }

    #[test]
    fn watcher_hub_shares_single_backend_and_stops_when_last_subscriber_drops() {
        let starts = Arc::new(AtomicUsize::new(0));
        let stops = Arc::new(AtomicUsize::new(0));
        let trigger_slot: Arc<Mutex<Option<NetworkChangeCallback>>> = Arc::new(Mutex::new(None));

        let starts_for_backend = Arc::clone(&starts);
        let stops_for_backend = Arc::clone(&stops);
        let trigger_for_backend = Arc::clone(&trigger_slot);
        let hub = Arc::new(NetworkWatcherHub::new(move |dispatch| {
            starts_for_backend.fetch_add(1, Ordering::AcqRel);
            *trigger_for_backend.lock().unwrap() = Some(dispatch);
            Some(FakeBackend {
                stops: Arc::clone(&stops_for_backend),
            })
        }));

        let hits_a = Arc::new(AtomicUsize::new(0));
        let hits_b = Arc::new(AtomicUsize::new(0));
        let sub_a = hub.subscribe({
            let hits = Arc::clone(&hits_a);
            Arc::new(move || {
                hits.fetch_add(1, Ordering::AcqRel);
            })
        });
        let sub_b = hub.subscribe({
            let hits = Arc::clone(&hits_b);
            Arc::new(move || {
                hits.fetch_add(1, Ordering::AcqRel);
            })
        });

        assert_eq!(starts.load(Ordering::Acquire), 1);
        assert_eq!(stops.load(Ordering::Acquire), 0);

        trigger_slot.lock().unwrap().as_ref().unwrap()();
        assert_eq!(hits_a.load(Ordering::Acquire), 1);
        assert_eq!(hits_b.load(Ordering::Acquire), 1);

        drop(sub_a);
        assert_eq!(stops.load(Ordering::Acquire), 0);
        trigger_slot.lock().unwrap().as_ref().unwrap()();
        assert_eq!(hits_a.load(Ordering::Acquire), 1);
        assert_eq!(hits_b.load(Ordering::Acquire), 2);

        drop(sub_b);
        assert_eq!(stops.load(Ordering::Acquire), 1);

        let sub_c = hub.subscribe(Arc::new(|| {}));
        assert_eq!(starts.load(Ordering::Acquire), 2);
        drop(sub_c);
        assert_eq!(stops.load(Ordering::Acquire), 2);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_network_monitor_starts_and_shuts_down_cleanly() {
        let watcher = platform::PlatformWatcher::start(Arc::new(|| {}))
            .expect("SCDynamicStore watcher should initialize on macOS");
        let stop_requested = Arc::clone(&watcher.stop_requested);
        assert!(!stop_requested.load(Ordering::Acquire));
        drop(watcher);
        assert!(stop_requested.load(Ordering::Acquire));
    }
}
