//! Secure Zeroizing Memory abstractions (`SecretBytes`, `SecretString`) for AdeshLang Crypto.

use std::fmt;
use zeroize::Zeroizing;

pub struct SecretBytes {
    inner: Zeroizing<Vec<u8>>,
    #[allow(dead_code)]
    locked: bool,
}

impl SecretBytes {
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            inner: Zeroizing::new(data),
            locked: false,
        }
    }

    pub fn generate(len: usize) -> Self {
        let mut buf = vec![0u8; len];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut buf);
        Self::new(buf)
    }

    pub fn expose(&self) -> &[u8] {
        &self.inner
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn lock_memory(&mut self) -> Result<(), String> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::System::Memory::VirtualLock;
            if !self.inner.is_empty() {
                let res = unsafe { VirtualLock(self.inner.as_ptr() as *const _, self.inner.len()) };
                if res != 0 {
                    self.locked = true;
                    return Ok(());
                } else {
                    return Err("Windows VirtualLock failed".to_string());
                }
            }
        }
        #[cfg(unix)]
        {
            if !self.inner.is_empty() {
                let res = unsafe { libc::mlock(self.inner.as_ptr() as *const _, self.inner.len()) };
                if res == 0 {
                    self.locked = true;
                    return Ok(());
                } else {
                    return Err("Unix mlock failed".to_string());
                }
            }
        }
        Ok(())
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretBytes(<redacted len={}>)", self.inner.len())
    }
}

impl fmt::Display for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretBytes(<redacted>)")
    }
}

pub struct SecretString {
    inner: Zeroizing<String>,
}

impl SecretString {
    pub fn new(s: String) -> Self {
        Self {
            inner: Zeroizing::new(s),
        }
    }

    pub fn expose(&self) -> &str {
        &self.inner
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretString(<redacted>)")
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SecretString(<redacted>)")
    }
}
