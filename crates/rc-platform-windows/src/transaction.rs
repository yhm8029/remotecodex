use anyhow::{bail, Result};
use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, WAIT_ABANDONED_0, WAIT_OBJECT_0, WAIT_TIMEOUT},
    System::Threading::{
        CreateMutexExW, ReleaseMutex, WaitForSingleObject, MUTEX_MODIFY_STATE,
        SYNCHRONIZATION_SYNCHRONIZE,
    },
};

pub struct TransactionGuard {
    handle: windows_sys::Win32::Foundation::HANDLE,
    abandoned: bool,
}

impl TransactionGuard {
    pub fn acquire() -> Result<Self> {
        let sid = super::current_user_sid_string()?;
        let name = format!("Global\\RemoteCodex-Tailscale-{sid}");
        let wide = name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let security = super::LocalSecurity::new()?;
        let handle = unsafe {
            CreateMutexExW(
                &security.attributes(),
                wide.as_ptr(),
                0,
                MUTEX_MODIFY_STATE | SYNCHRONIZATION_SYNCHRONIZE,
            )
        };
        if handle.is_null() {
            bail!("Could not create the RemoteCodex settings transaction lock")
        }
        let state = unsafe { WaitForSingleObject(handle, 2500) };
        let abandoned = match classify_wait(state) {
            Ok(value) => value,
            Err(error) => {
                unsafe { CloseHandle(handle) };
                return Err(error);
            }
        };
        Ok(Self { handle, abandoned })
    }

    pub fn was_abandoned(&self) -> bool {
        self.abandoned
    }
}

fn classify_wait(state: u32) -> Result<bool> {
    if state == WAIT_OBJECT_0 {
        Ok(false)
    } else if state == WAIT_ABANDONED_0 {
        Ok(true)
    } else if state == WAIT_TIMEOUT {
        bail!("RemoteCodex settings are busy")
    } else {
        let error = unsafe { GetLastError() };
        bail!("Could not acquire the RemoteCodex settings lock: {error}")
    }
}

impl Drop for TransactionGuard {
    fn drop(&mut self) {
        unsafe {
            ReleaseMutex(self.handle);
            CloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ptr::null_mut, sync::mpsc, thread};
    use windows_sys::Win32::System::Threading::CreateMutexExW;

    #[test]
    fn isolated_abandoned_mutex_is_classified_and_released() {
        let name = format!("Local\\RemoteCodex-test-{}", uuid::Uuid::new_v4());
        let wide = name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let handle = unsafe {
            CreateMutexExW(
                null_mut(),
                wide.as_ptr(),
                0,
                MUTEX_MODIFY_STATE | SYNCHRONIZATION_SYNCHRONIZE,
            )
        };
        assert!(!handle.is_null());
        let (ready_tx, ready_rx) = mpsc::channel();
        let thread_name = wide.clone();
        thread::spawn(move || {
            let owned = unsafe {
                CreateMutexExW(
                    null_mut(),
                    thread_name.as_ptr(),
                    0,
                    MUTEX_MODIFY_STATE | SYNCHRONIZATION_SYNCHRONIZE,
                )
            };
            assert!(!owned.is_null());
            assert_eq!(unsafe { WaitForSingleObject(owned, 1000) }, WAIT_OBJECT_0);
            ready_tx.send(()).unwrap();
            unsafe { CloseHandle(owned) };
        });
        ready_rx.recv().unwrap();
        let state = unsafe { WaitForSingleObject(handle, 1000) };
        assert!(classify_wait(state).unwrap());
        unsafe {
            ReleaseMutex(handle);
            CloseHandle(handle);
        }
    }
}
