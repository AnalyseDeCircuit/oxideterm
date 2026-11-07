use mach2::{mach_init::mach_host_self, mach_port::mach_port_deallocate, traps::mach_task_self};
use std::fmt::Write;

pub fn macos_cpu_counters() -> Result<String, String> {
    let mut count = 0;
    let mut words = 0;
    let mut info = std::ptr::null_mut();
    // Mach owns the returned allocation; both it and the host send right are
    // released in this call. No sampling state survives outside the profiler.
    let status = unsafe {
        let host = mach_host_self();
        let result = libc::host_processor_info(
            host,
            libc::PROCESSOR_CPU_LOAD_INFO,
            &mut count,
            &mut info,
            &mut words,
        );
        mach_port_deallocate(mach_task_self(), host);
        result
    };
    if status != libc::KERN_SUCCESS {
        return Err("macOS CPU counters are unavailable.".into());
    }
    let mut output = String::from("===STAT===\n");
    let valid = !info.is_null() && words as usize == count as usize * libc::CPU_STATE_MAX as usize;
    if valid {
        let ticks = unsafe { std::slice::from_raw_parts(info, words as usize) };
        let mut total = [0u64; 4];
        let mut cores = String::new();
        for (index, ticks) in ticks.chunks_exact(4).enumerate() {
            let ticks = [ticks[0], ticks[1], ticks[2], ticks[3]].map(|tick| tick as u32 as u64);
            for i in 0..4 {
                total[i] += ticks[i];
            }
            writeln!(
                cores,
                "cpu{index} {} {} {} {} 0 0 0 0",
                ticks[0], ticks[3], ticks[1], ticks[2]
            )
            .unwrap();
        }
        writeln!(
            output,
            "cpu {} {} {} {} 0 0 0 0",
            total[0], total[3], total[1], total[2]
        )
        .unwrap();
        output.push_str(&cores);
    }
    if !info.is_null() {
        unsafe {
            libc::vm_deallocate(
                mach_task_self(),
                info as libc::vm_address_t,
                words as libc::vm_size_t
                    * std::mem::size_of::<libc::integer_t>() as libc::vm_size_t,
            );
        }
    }
    if valid {
        Ok(output)
    } else {
        Err("macOS returned invalid CPU counter dimensions.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_cpu_sample_reports_each_logical_processor() {
        let output = macos_cpu_counters().unwrap();
        let sysctl = std::process::Command::new("/usr/sbin/sysctl")
            .args(["-n", "hw.logicalcpu"])
            .output()
            .unwrap();
        let count: usize = String::from_utf8(sysctl.stdout)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let metrics = crate::parse_resource_metrics(&output, None, 1);
        assert_eq!(
            metrics
                .cpu_per_core
                .iter()
                .map(|core| core.index)
                .collect::<Vec<_>>(),
            (0..count as u32).collect::<Vec<_>>()
        );
        assert!(
            metrics
                .cpu_per_core
                .iter()
                .all(|core| core.percent.is_none())
        );
    }
}
