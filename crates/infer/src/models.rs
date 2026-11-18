use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

pub struct ModelSpec {
    pub file: &'static str,
    pub url: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

pub const DEFAULT_MODEL: ModelSpec = ModelSpec {
    file: "Qwen3-0.6B-Base.Q4_K_M.gguf",
    url: "https://huggingface.co/mradermacher/Qwen3-0.6B-Base-GGUF/resolve/b8b11a6dbbb14c739e25018f3f77abac860c28f4/Qwen3-0.6B-Base.Q4_K_M.gguf",
    bytes: 396_704_960,
    sha256: "1c728e4fe4abb28cde33b59cd66396e62b2ef755f53db44cdb54e9655b863782",
};

pub fn find_local(dir: &Path) -> Option<PathBuf> {
    let preferred = dir.join(DEFAULT_MODEL.file);
    if preferred.is_file() {
        return Some(preferred);
    }
    let mut candidates: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("gguf"))
        })
        .collect();
    candidates.sort();
    candidates.into_iter().next()
}

pub fn download(
    spec: &ModelSpec,
    dir: &Path,
    progress: &dyn Fn(u64, u64),
) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let target = dir.join(spec.file);
    let partial = dir.join(format!("{}.part", spec.file));
    let mut response = ureq::get(spec.url).call().map_err(|e| e.to_string())?;
    let mut reader = response.body_mut().as_reader();
    let mut file = fs::File::create(&partial).map_err(|e| e.to_string())?;
    let mut buffer = vec![0u8; 1 << 16];
    let mut written = 0u64;
    let mut hasher = Sha256::new();
    loop {
        let read = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read]).map_err(|e| e.to_string())?;
        hasher.update(&buffer[..read]);
        written += read as u64;
        progress(written, spec.bytes);
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    if written != spec.bytes {
        let _ = fs::remove_file(&partial);
        return Err(format!(
            "download incomplete: {written} of {} bytes",
            spec.bytes
        ));
    }
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if digest != spec.sha256 {
        let _ = fs::remove_file(&partial);
        return Err(format!("model checksum mismatch: {digest}"));
    }
    fs::rename(&partial, &target).map_err(|e| e.to_string())?;
    Ok(target)
}
