//! Strict ownership is separate from the legacy CLI connection kill callback.
use std::process::{Child, Command};
use std::time::{Duration, Instant};

pub(in crate::backend::services_rpc) const CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);

fn unsafe_exit(reason: &str) -> String {
    format!("Services {reason}; process tree cleanup unverified; preferences remain suspended")
}

pub(in crate::backend::services_rpc) fn verify_empty(
    mut active: impl FnMut() -> Result<bool, String>,
) -> Result<(), String> {
    let deadline = Instant::now() + CLEANUP_TIMEOUT;
    loop {
        if !active()? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(unsafe_exit("owned process tree still active"));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::os::windows::{io::AsRawHandle, process::CommandExt};
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
        QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
    };
    use windows::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};
    use windows::core::PCWSTR;

    struct Handle(HANDLE);
    unsafe impl Send for Handle {}
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
    pub(in crate::backend::services_rpc) struct ProcessTree {
        job: Handle,
    }
    impl ProcessTree {
        pub(in crate::backend::services_rpc) fn prepare(
            command: &mut Command,
        ) -> Result<Self, String> {
            let job = Handle(
                unsafe { CreateJobObjectW(None, PCWSTR::null()) }
                    .map_err(|_| unsafe_exit("job creation failed"))?,
            );
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            unsafe {
                SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as _,
                    std::mem::size_of_val(&limits) as u32,
                )
            }
            .map_err(|_| unsafe_exit("job configuration failed"))?;
            // 启动后再挂 job 会漏掉抢先创建的后代；挂载完成前 root 必须保持挂起。
            command.creation_flags(0x0800_0000 | 0x0000_0004);
            Ok(Self { job })
        }
        pub(in crate::backend::services_rpc) fn attach(
            &mut self,
            child: &Child,
        ) -> Result<(), String> {
            unsafe { AssignProcessToJobObject(self.job.0, HANDLE(child.as_raw_handle())) }
                .map_err(|_| unsafe_exit("job attachment failed"))?;
            let snapshot = Handle(
                unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) }
                    .map_err(|_| unsafe_exit("root thread snapshot failed"))?,
            );
            let mut thread = THREADENTRY32 {
                dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
                ..Default::default()
            };
            unsafe { Thread32First(snapshot.0, &mut thread) }
                .map_err(|_| unsafe_exit("root thread observation failed"))?;
            loop {
                if thread.th32OwnerProcessID == child.id() {
                    let handle = Handle(
                        unsafe { OpenThread(THREAD_SUSPEND_RESUME, false, thread.th32ThreadID) }
                            .map_err(|_| unsafe_exit("root thread access failed"))?,
                    );
                    if unsafe { ResumeThread(handle.0) } != 1 {
                        return Err(unsafe_exit("root thread resume failed"));
                    }
                    return Ok(());
                }
                if unsafe { Thread32Next(snapshot.0, &mut thread) }.is_err() {
                    return Err(unsafe_exit("suspended root thread missing"));
                }
            }
        }
        pub(in crate::backend::services_rpc) fn active(&self) -> Result<bool, String> {
            let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
            unsafe {
                QueryInformationJobObject(
                    Some(self.job.0),
                    JobObjectBasicAccountingInformation,
                    &mut info as *mut _ as _,
                    std::mem::size_of_val(&info) as u32,
                    None,
                )
            }
            .map_err(|_| unsafe_exit("job accounting query failed"))?;
            Ok(info.ActiveProcesses != 0)
        }
        pub(in crate::backend::services_rpc) fn terminate(&self) -> Result<(), String> {
            if self.active()? {
                unsafe { TerminateJobObject(self.job.0, 1) }
                    .map_err(|_| unsafe_exit("job termination failed"))?;
            }
            Ok(())
        }
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::os::unix::process::CommandExt;
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    pub(in crate::backend::services_rpc) struct ProcessTree {
        group: Option<i32>,
    }
    impl ProcessTree {
        pub(in crate::backend::services_rpc) fn prepare(
            command: &mut Command,
        ) -> Result<Self, String> {
            command.process_group(0);
            Ok(Self { group: None })
        }
        pub(in crate::backend::services_rpc) fn attach(
            &mut self,
            child: &Child,
        ) -> Result<(), String> {
            self.group = Some(
                i32::try_from(child.id())
                    .map_err(|_| unsafe_exit("process group identity invalid"))?,
            );
            Ok(())
        }
        fn signal(&self, signal: i32) -> Result<bool, String> {
            let group = self
                .group
                .ok_or_else(|| unsafe_exit("process group missing"))?;
            if unsafe { kill(-group, signal) } == 0 {
                return Ok(true);
            }
            match std::io::Error::last_os_error().raw_os_error() {
                Some(3) => Ok(false), // ESRCH 才是组已不存在；权限或探测错误不能视为退出。
                _ => Err(unsafe_exit("process group operation failed")),
            }
        }
        pub(in crate::backend::services_rpc) fn active(&self) -> Result<bool, String> {
            self.signal(0)
        }
        pub(in crate::backend::services_rpc) fn terminate(&self) -> Result<(), String> {
            self.signal(9).map(|_| ())
        }
    }
}

pub(in crate::backend::services_rpc) use platform::ProcessTree;
impl ProcessTree {
    pub(in crate::backend::services_rpc) fn finish(&self) -> Result<(), String> {
        let termination = self.terminate();
        let observed = verify_empty(|| self.active());
        termination.and(observed)
    }
}

pub(in crate::backend::services_rpc) fn verified_root_and_tree(
    root: std::io::Result<std::process::ExitStatus>,
    tree: Result<(), String>,
) -> Result<(), String> {
    root.map_err(|_| unsafe_exit("root wait failed")).and(tree)
}
