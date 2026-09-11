use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutostartStatus {
    Disabled,
    Enabled,
    Conflict,
}

pub fn command(agent: &Path, config: &Path) -> Result<String, String> {
    if !agent.is_absolute() || !config.is_absolute() {
        return Err("Agent and config paths must be absolute".into());
    }
    let agent = agent
        .to_str()
        .ok_or_else(|| "Agent path is not Unicode".to_owned())?;
    let config = config
        .to_str()
        .ok_or_else(|| "Config path is not Unicode".to_owned())?;
    if agent
        .chars()
        .chain(config.chars())
        .any(|c| matches!(c, '"' | '\r' | '\n' | '\0'))
    {
        return Err("Autostart paths contain an unsupported character".into());
    }
    Ok(format!(r#""{agent}" --config "{config}" run"#))
}

pub fn classify(stored: Option<&str>, expected: &str) -> AutostartStatus {
    match stored {
        None => AutostartStatus::Disabled,
        Some(value) if value == expected => AutostartStatus::Enabled,
        Some(_) => AutostartStatus::Conflict,
    }
}

pub fn may_remove(stored: Option<&str>, expected: &str) -> bool {
    stored == Some(expected)
}

#[cfg(windows)]
mod registry {
    use std::{mem::size_of, ptr::null_mut};
    use windows_sys::Win32::{Foundation::*, System::Registry::*};
    const KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE_NAME: &str = "RemoteCodex Agent";
    struct Key(HKEY);
    impl Drop for Key {
        fn drop(&mut self) {
            unsafe {
                RegCloseKey(self.0);
            }
        }
    }
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }
    fn open(access: REG_SAM_FLAGS) -> Result<Key, String> {
        let mut key = null_mut();
        let result = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                wide(KEY_PATH).as_ptr(),
                0,
                access,
                &mut key,
            )
        };
        if result != ERROR_SUCCESS {
            return Err(format!("Could not open per-user autostart key: {result}"));
        }
        Ok(Key(key))
    }
    pub fn read() -> Result<Option<String>, String> {
        let key = open(KEY_QUERY_VALUE)?;
        let name = wide(VALUE_NAME);
        let (mut kind, mut bytes) = (0, 0);
        let result = unsafe {
            RegQueryValueExW(
                key.0,
                name.as_ptr(),
                null_mut(),
                &mut kind,
                null_mut(),
                &mut bytes,
            )
        };
        if result == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if result != ERROR_SUCCESS {
            return Err(format!("Could not query autostart value: {result}"));
        }
        if kind != REG_SZ || bytes < 2 || bytes > 32 * 1024 || bytes % 2 != 0 {
            return Err("Autostart value has an unsupported type or size".into());
        }
        let mut value = vec![0u16; bytes as usize / size_of::<u16>()];
        let result = unsafe {
            RegQueryValueExW(
                key.0,
                name.as_ptr(),
                null_mut(),
                &mut kind,
                value.as_mut_ptr().cast::<u8>(),
                &mut bytes,
            )
        };
        if result != ERROR_SUCCESS || kind != REG_SZ {
            return Err(format!("Could not read autostart value: {result}"));
        }
        if value.last() != Some(&0) {
            return Err("Autostart value is not terminated".into());
        }
        value.pop();
        String::from_utf16(&value)
            .map(Some)
            .map_err(|_| "Autostart value is not valid UTF-16".into())
    }
    pub fn write(value: &str) -> Result<(), String> {
        let key = open(KEY_SET_VALUE)?;
        let name = wide(VALUE_NAME);
        let value = wide(value);
        let bytes = (value.len() * size_of::<u16>()) as u32;
        let result = unsafe {
            RegSetValueExW(
                key.0,
                name.as_ptr(),
                0,
                REG_SZ,
                value.as_ptr().cast::<u8>(),
                bytes,
            )
        };
        if result != ERROR_SUCCESS {
            return Err(format!("Could not set autostart value: {result}"));
        }
        Ok(())
    }
    pub fn delete() -> Result<(), String> {
        let key = open(KEY_SET_VALUE)?;
        let result = unsafe { RegDeleteValueW(key.0, wide(VALUE_NAME).as_ptr()) };
        if result != ERROR_SUCCESS && result != ERROR_FILE_NOT_FOUND {
            return Err(format!("Could not remove autostart value: {result}"));
        }
        Ok(())
    }
}

#[cfg(windows)]
pub fn status(agent: &Path, config: &Path) -> Result<AutostartStatus, String> {
    let expected = command(agent, config)?;
    Ok(classify(registry::read()?.as_deref(), &expected))
}
#[cfg(windows)]
pub fn set_enabled(agent: &Path, config: &Path, enabled: bool) -> Result<AutostartStatus, String> {
    let expected = command(agent, config)?;
    let stored = registry::read()?;
    if enabled {
        match classify(stored.as_deref(), &expected) {
            AutostartStatus::Disabled => registry::write(&expected)?,
            AutostartStatus::Enabled => {}
            AutostartStatus::Conflict => {
                return Err("A different autostart value is present; it was preserved".into())
            }
        }
    } else if may_remove(stored.as_deref(), &expected) {
        registry::delete()?;
    } else if stored.is_some() {
        return Err(
            "The autostart value does not exactly match RemoteCodex; it was preserved".into(),
        );
    }
    status(agent, config)
}
#[cfg(not(windows))]
pub fn status(_: &Path, _: &Path) -> Result<AutostartStatus, String> {
    Err("Autostart is available only on Windows".into())
}
#[cfg(not(windows))]
pub fn set_enabled(_: &Path, _: &Path, _: bool) -> Result<AutostartStatus, String> {
    Err("Autostart is available only on Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builds_exact_quoted_command() {
        let agent = Path::new(r"C:\Program Files\RemoteCodex\rc-agent.exe");
        let config = Path::new(r"C:\Users\Test User\RemoteCodex\agent.toml");
        assert_eq!(
            command(agent, config).unwrap(),
            r#""C:\Program Files\RemoteCodex\rc-agent.exe" --config "C:\Users\Test User\RemoteCodex\agent.toml" run"#
        );
    }
    #[test]
    fn rejects_relative_and_unsafe_paths() {
        let absolute = Path::new(r"C:\RemoteCodex\agent.toml");
        assert!(command(Path::new("rc-agent.exe"), absolute).is_err());
        assert!(command(Path::new("C:\\bad\"name.exe"), absolute).is_err());
    }
    #[test]
    fn only_exact_owned_value_is_enabled_and_removable() {
        let expected = r#""C:\RemoteCodex\rc-agent.exe" --config "C:\cfg.toml" run"#;
        assert_eq!(classify(None, expected), AutostartStatus::Disabled);
        assert_eq!(classify(Some(expected), expected), AutostartStatus::Enabled);
        assert!(may_remove(Some(expected), expected));
        for different in [format!(" {expected}"), expected.to_ascii_uppercase()] {
            assert_eq!(
                classify(Some(&different), expected),
                AutostartStatus::Conflict
            );
            assert!(!may_remove(Some(&different), expected));
        }
        assert!(!may_remove(None, expected));
    }
}
