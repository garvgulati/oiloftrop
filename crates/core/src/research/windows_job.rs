//! Keep only the calculator in a Windows job owned by the host. The OS closes
//! its non-inherited handle on forced host termination and kills the child.
#![allow(
    unsafe_code,
    reason = "Narrow Win32 job binding; handles are owned and pointers borrow live values"
)]

use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

pub(super) struct ChildJob {
    _handle: OwnedHandle,
}

impl ChildJob {
    pub(super) fn attach(child: &tokio::process::Child) -> std::io::Result<Self> {
        let process = child
            .raw_handle()
            .ok_or_else(std::io::Error::last_os_error)?;
        // SAFETY: no name/security pointers; the returned non-null handle is new,
        // non-inheritable and transferred exactly once to OwnedHandle.
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let handle = unsafe { OwnedHandle::from_raw_handle(handle) };
        // SAFETY: this Win32 POD structure permits zero for every other limit.
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: owned live job handle, correctly sized borrowed structure; the
        // API copies it synchronously and does not retain the pointer.
        let configured = unsafe {
            SetInformationJobObject(
                handle.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&limits).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        };
        if configured == 0 {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: the child is still borrowed and has not been waited/reaped.
        // No input is written before assignment; an early host exit leaves the
        // trusted calculator with empty stdin, which it rejects.
        if unsafe { AssignProcessToJobObject(handle.as_raw_handle(), process) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self { _handle: handle })
    }
}
