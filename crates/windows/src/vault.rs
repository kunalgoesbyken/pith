use pith_core::Vault;
use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};
use windows::core::PCWSTR;

const ENTROPY: &[u8] = b"pith-style-1";

pub struct DpapiVault;

fn blob(bytes: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: bytes.len() as u32,
        pbData: bytes.as_ptr().cast_mut(),
    }
}

fn take(output: CRYPT_INTEGER_BLOB) -> Vec<u8> {
    let bytes =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe { LocalFree(Some(HLOCAL(output.pbData.cast()))) };
    bytes
}

impl Vault for DpapiVault {
    fn seal(&self, plain: &[u8]) -> Result<Vec<u8>, String> {
        let mut output = CRYPT_INTEGER_BLOB::default();
        unsafe {
            CryptProtectData(
                &blob(plain),
                PCWSTR::null(),
                Some(&blob(ENTROPY)),
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
        .map_err(|error| error.to_string())?;
        Ok(take(output))
    }

    fn open(&self, sealed: &[u8]) -> Result<Vec<u8>, String> {
        let mut output = CRYPT_INTEGER_BLOB::default();
        unsafe {
            CryptUnprotectData(
                &blob(sealed),
                None,
                Some(&blob(ENTROPY)),
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
        .map_err(|error| error.to_string())?;
        Ok(take(output))
    }
}
