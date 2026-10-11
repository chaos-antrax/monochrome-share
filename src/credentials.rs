//! Current-user, local-PC storage in Windows Credential Manager. No plaintext files.
use serde::{Deserialize, Serialize};
use windows_sys::Win32::{Foundation::*, Security::Credentials::*};

pub const TARGET: &str = "MonochromeCapture/CloudflareTURN/v1";

#[derive(Default, Serialize, Deserialize)]
pub struct Credentials {
    pub key: String,
    pub token: String,
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

pub fn save(target: &str, credentials: &Credentials) -> anyhow::Result<()> {
    anyhow::ensure!(
        !credentials.key.is_empty() && !credentials.token.is_empty(),
        "Both TURN credentials are required"
    );
    let mut name = wide(target);
    let mut username = wide("Cloudflare TURN");
    let mut blob = serde_json::to_vec(credentials)?;
    anyhow::ensure!(
        blob.len() <= CRED_MAX_CREDENTIAL_BLOB_SIZE as usize,
        "Credentials exceed the Windows storage limit"
    );
    let entry = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: name.as_mut_ptr(),
        UserName: username.as_mut_ptr(),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        ..Default::default()
    };
    if unsafe { CredWriteW(&entry, 0) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

pub fn load(target: &str) -> anyhow::Result<Option<Credentials>> {
    let mut entry = std::ptr::null_mut();
    if unsafe { CredReadW(wide(target).as_ptr(), CRED_TYPE_GENERIC, 0, &mut entry) } == 0 {
        let code = unsafe { GetLastError() };
        if code == ERROR_NOT_FOUND {
            return Ok(None);
        }
        return Err(std::io::Error::from_raw_os_error(code as i32).into());
    }
    // CredRead allocates the entire entry; release it even when parsing fails.
    let result = unsafe {
        if (*entry).CredentialBlobSize == 0 || (*entry).CredentialBlob.is_null() {
            Err(anyhow::anyhow!("Saved credentials are empty"))
        } else {
            serde_json::from_slice(std::slice::from_raw_parts(
                (*entry).CredentialBlob,
                (*entry).CredentialBlobSize as usize,
            ))
            // Do not expose serde errors, which can include input contents.
            .map_err(|_| anyhow::anyhow!("Saved credentials could not be decoded"))
        }
    };
    unsafe { CredFree(entry as _) };
    result.map(Some)
}

pub fn forget(target: &str) -> anyhow::Result<()> {
    if unsafe { CredDeleteW(wide(target).as_ptr(), CRED_TYPE_GENERIC, 0) } == 0 {
        let code = unsafe { GetLastError() };
        if code != ERROR_NOT_FOUND {
            return Err(std::io::Error::from_raw_os_error(code as i32).into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn current_user_store_roundtrip_update_and_delete() {
        // Unique synthetic entry: never reads or overwrites the user's real token.
        let target = format!("MonochromeCapture/Test/{}", uuid::Uuid::new_v4());
        struct Cleanup(String);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = forget(&self.0);
            }
        }
        let _cleanup = Cleanup(target.clone());
        assert!(load(&target).unwrap().is_none());
        for token in ["synthetic-first", "synthetic-updated"] {
            save(
                &target,
                &Credentials {
                    key: "test-key".into(),
                    token: token.into(),
                },
            )
            .unwrap();
            let restored = load(&target).unwrap().unwrap();
            assert_eq!(restored.key, "test-key");
            assert_eq!(restored.token, token);
        }
        forget(&target).unwrap();
        forget(&target).unwrap();
        assert!(load(&target).unwrap().is_none());
    }
}
