// OxideTerm modification: dispatches background work through the native Win32 thread pool.

use std::{
    ffi::c_void,
    mem::size_of,
    ptr::NonNull,
    sync::atomic::{AtomicBool, Ordering},
    thread::{ThreadId, current},
    time::{Duration, Instant},
};

use anyhow::Context;
use util::ResultExt;
use windows::Win32::{
    Foundation::{FILETIME, LPARAM, WPARAM},
    Media::{timeBeginPeriod, timeEndPeriod},
    System::Threading::{
        CloseThreadpoolTimer, CreateThreadpoolTimer, GetCurrentThread, PTP_CALLBACK_INSTANCE,
        PTP_TIMER, SetThreadPriority, SetThreadpoolTimer, THREAD_PRIORITY_TIME_CRITICAL,
        TP_CALLBACK_ENVIRON_V3, TP_CALLBACK_PRIORITY, TP_CALLBACK_PRIORITY_HIGH,
        TP_CALLBACK_PRIORITY_LOW, TP_CALLBACK_PRIORITY_NORMAL, TrySubmitThreadpoolCallback,
    },
    UI::WindowsAndMessaging::PostMessageW,
};

use crate::{HWND, SafeHwnd, WM_GPUI_TASK_DISPATCHED_ON_MAIN_THREAD};
use gpui::{
    GLOBAL_THREAD_TIMINGS, PlatformDispatcher, Priority, PriorityQueueSender, RunnableVariant,
    TaskTiming, ThreadTaskTimings, TimerResolutionGuard,
};

pub(crate) struct WindowsDispatcher {
    pub(crate) wake_posted: AtomicBool,
    main_sender: PriorityQueueSender<RunnableVariant>,
    main_thread_id: ThreadId,
    pub(crate) platform_window_handle: SafeHwnd,
    validation_number: usize,
}

impl WindowsDispatcher {
    pub(crate) fn new(
        main_sender: PriorityQueueSender<RunnableVariant>,
        platform_window_handle: HWND,
        validation_number: usize,
    ) -> Self {
        let main_thread_id = current().id();
        let platform_window_handle = platform_window_handle.into();

        WindowsDispatcher {
            main_sender,
            main_thread_id,
            platform_window_handle,
            validation_number,
            wake_posted: AtomicBool::new(false),
        }
    }

    fn dispatch_on_threadpool(&self, priority: TP_CALLBACK_PRIORITY, runnable: RunnableVariant) {
        let callback_environment = TP_CALLBACK_ENVIRON_V3 {
            Version: 3,
            Size: size_of::<TP_CALLBACK_ENVIRON_V3>() as u32,
            CallbackPriority: priority,
            ..Default::default()
        };
        let runnable_pointer = runnable.into_raw();
        let result = unsafe {
            TrySubmitThreadpoolCallback(
                Some(run_work_callback),
                Some(runnable_pointer.as_ptr().cast::<c_void>()),
                Some(&callback_environment),
            )
        };
        if let Err(error) = result {
            // SAFETY: Submission failed, so the callback cannot consume this unique raw pointer.
            drop(unsafe { RunnableVariant::from_raw(runnable_pointer) });
            log::error!("failed to submit Win32 thread-pool work: {error}");
        }
    }

    fn dispatch_on_threadpool_after(&self, runnable: RunnableVariant, duration: Duration) {
        let runnable_pointer = runnable.into_raw();
        let timer = match unsafe {
            CreateThreadpoolTimer(
                Some(run_timer_callback),
                Some(runnable_pointer.as_ptr().cast::<c_void>()),
                None,
            )
        } {
            Ok(timer) => timer,
            Err(error) => {
                // SAFETY: Timer creation failed, so no callback owns this unique raw pointer.
                drop(unsafe { RunnableVariant::from_raw(runnable_pointer) });
                log::error!("failed to create Win32 thread-pool timer: {error}");
                return;
            }
        };
        let due_time = relative_threadpool_due_time(duration);
        unsafe {
            SetThreadpoolTimer(timer, Some(&due_time), 0, None);
        }
    }

    #[inline(always)]
    pub(crate) fn execute_runnable(runnable: RunnableVariant) {
        let start = Instant::now();

        let location = runnable.metadata().location;
        let mut timing = TaskTiming {
            location,
            start,
            end: None,
        };
        gpui::profiler::add_task_timing(timing);

        runnable.run();

        let end = Instant::now();
        timing.end = Some(end);

        gpui::profiler::add_task_timing(timing);
    }
}

impl PlatformDispatcher for WindowsDispatcher {
    fn get_all_timings(&self) -> Vec<ThreadTaskTimings> {
        let global_thread_timings = GLOBAL_THREAD_TIMINGS.lock();
        ThreadTaskTimings::convert(&global_thread_timings)
    }

    fn get_current_thread_timings(&self) -> gpui::ThreadTaskTimings {
        gpui::profiler::get_current_thread_task_timings()
    }

    fn is_main_thread(&self) -> bool {
        current().id() == self.main_thread_id
    }

    fn dispatch(&self, runnable: RunnableVariant, priority: Priority) {
        let priority = match priority {
            Priority::RealtimeAudio => {
                panic!("RealtimeAudio priority should use spawn_realtime, not dispatch")
            }
            Priority::High => TP_CALLBACK_PRIORITY_HIGH,
            Priority::Medium => TP_CALLBACK_PRIORITY_NORMAL,
            Priority::Low => TP_CALLBACK_PRIORITY_LOW,
        };
        self.dispatch_on_threadpool(priority, runnable);
    }

    fn dispatch_on_main_thread(&self, runnable: RunnableVariant, priority: Priority) {
        match self.main_sender.send(priority, runnable) {
            Ok(_) => {
                if !self.wake_posted.swap(true, Ordering::AcqRel) {
                    unsafe {
                        PostMessageW(
                            Some(self.platform_window_handle.as_raw()),
                            WM_GPUI_TASK_DISPATCHED_ON_MAIN_THREAD,
                            WPARAM(self.validation_number),
                            LPARAM(0),
                        )
                        .log_err();
                    }
                }
            }
            Err(runnable) => {
                // NOTE: Runnable may wrap a Future that is !Send.
                //
                // This is usually safe because we only poll it on the main thread.
                // However if the send fails, we know that:
                // 1. main_receiver has been dropped (which implies the app is shutting down)
                // 2. we are on a background thread.
                // It is not safe to drop something !Send on the wrong thread, and
                // the app will exit soon anyway, so we must forget the runnable.
                std::mem::forget(runnable);
            }
        }
    }

    fn dispatch_after(&self, duration: Duration, runnable: RunnableVariant) {
        self.dispatch_on_threadpool_after(runnable, duration);
    }

    fn spawn_realtime(&self, f: Box<dyn FnOnce() + Send>) {
        std::thread::spawn(move || {
            // SAFETY: always safe to call
            let thread_handle = unsafe { GetCurrentThread() };

            // SAFETY: thread_handle is a valid handle to the current thread
            unsafe { SetThreadPriority(thread_handle, THREAD_PRIORITY_TIME_CRITICAL) }
                .context("thread priority")
                .log_err();

            f();
        });
    }

    fn increase_timer_resolution(&self) -> TimerResolutionGuard {
        unsafe {
            timeBeginPeriod(1);
        }
        util::defer(Box::new(|| unsafe {
            timeEndPeriod(1);
        }))
    }
}

/// Converts a Rust duration to the negative 100-nanosecond interval used by Win32 timers.
fn relative_threadpool_due_time(duration: Duration) -> FILETIME {
    let tick_count = duration
        .as_nanos()
        .saturating_add(99)
        .div_euclid(100)
        .min(i64::MAX as u128) as i64;
    let encoded_interval = tick_count.saturating_neg() as u64;
    FILETIME {
        dwLowDateTime: encoded_interval as u32,
        dwHighDateTime: (encoded_interval >> 32) as u32,
    }
}

unsafe extern "system" fn run_work_callback(
    _instance: PTP_CALLBACK_INSTANCE,
    context: *mut c_void,
) {
    let runnable_pointer =
        NonNull::new(context.cast::<()>()).expect("Win32 work callback received null context");
    // SAFETY: Each successful submission transfers exactly one raw runnable to one callback.
    let runnable = unsafe { RunnableVariant::from_raw(runnable_pointer) };
    WindowsDispatcher::execute_runnable(runnable);
}

unsafe extern "system" fn run_timer_callback(
    _instance: PTP_CALLBACK_INSTANCE,
    context: *mut c_void,
    timer: PTP_TIMER,
) {
    let runnable_pointer =
        NonNull::new(context.cast::<()>()).expect("Win32 timer callback received null context");
    // SAFETY: Each timer transfers exactly one raw runnable to its one-shot callback.
    let runnable = unsafe { RunnableVariant::from_raw(runnable_pointer) };
    WindowsDispatcher::execute_runnable(runnable);
    unsafe {
        CloseThreadpoolTimer(timer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_due_time_uses_negative_hundred_nanosecond_ticks() {
        let due_time = relative_threadpool_due_time(Duration::from_micros(25));
        let encoded =
            u64::from(due_time.dwLowDateTime) | (u64::from(due_time.dwHighDateTime) << 32);

        assert_eq!(encoded as i64, -250);
    }
}
