use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub struct ModelSpec {
    pub file: &'static str,
    pub url: &'static str,
    pub bytes: u64,
}

pub const DEFAULT_MODEL: ModelSpec = ModelSpec {
    file: "Qwen3-0.6B-Base.Q4_K_M.gguf",
    url: "https://huggingface.co/mradermacher/Qwen3-0.6B-Base-GGUF/resolve/main/Qwen3-0.6B-Base.Q4_K_M.gguf",
    bytes: 396_704_960,
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
    loop {
        let read = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read]).map_err(|e| e.to_string())?;
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
    fs::rename(&partial, &target).map_err(|e| e.to_string())?;
    Ok(target)
}
