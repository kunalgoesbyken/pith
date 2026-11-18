use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use pith_core::ModelPreset;
use sha2::{Digest, Sha256};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
const BODY_TIMEOUT: Duration = Duration::from_secs(60 * 60);

pub struct ModelSpec {
    pub file: &'static str,
    pub url: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

pub const STANDARD_MODEL: ModelSpec = ModelSpec {
    file: "Qwen3-0.6B-Base.Q4_K_M.gguf",
    url: "https://huggingface.co/mradermacher/Qwen3-0.6B-Base-GGUF/resolve/b8b11a6dbbb14c739e25018f3f77abac860c28f4/Qwen3-0.6B-Base.Q4_K_M.gguf",
    bytes: 396_704_960,
    sha256: "1c728e4fe4abb28cde33b59cd66396e62b2ef755f53db44cdb54e9655b863782",
};

pub const LITE_MODEL: ModelSpec = ModelSpec {
    file: "SmolLM2-360M.Q4_K_M.gguf",
    url: "https://huggingface.co/QuantFactory/SmolLM2-360M-GGUF/resolve/38cd16bc62ab1b1f0d320be8b518ac2fd80faa88/SmolLM2-360M.Q4_K_M.gguf",
    bytes: 270_589_952,
    sha256: "be3035f2b21abe1038dbb1b9212791353715020bbc71265bde00da129ec320e6",
};

pub fn preset(model: ModelPreset) -> &'static ModelSpec {
    match model {
        ModelPreset::Standard => &STANDARD_MODEL,
        ModelPreset::Lite => &LITE_MODEL,
    }
}

pub fn find_local(dir: &Path, spec: &ModelSpec) -> Option<PathBuf> {
    let path = dir.join(spec.file);
    fs::metadata(&path)
        .is_ok_and(|m| m.len() == spec.bytes)
        .then_some(path)
}

pub fn download(
    spec: &ModelSpec,
    dir: &Path,
    progress: &dyn Fn(u64, u64),
) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let target = dir.join(spec.file);
    let partial = dir.join(format!("{}.part", spec.file));
    let result = fetch(spec, &partial, progress).and_then(|()| {
        fs::rename(&partial, &target).map_err(|e| e.to_string())
    });
    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result.map(|()| target)
}

fn fetch(spec: &ModelSpec, partial: &Path, progress: &dyn Fn(u64, u64)) -> Result<(), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_recv_response(Some(RESPONSE_TIMEOUT))
        .timeout_recv_body(Some(BODY_TIMEOUT))
        .build()
        .into();
    let mut response = agent.get(spec.url).call().map_err(|e| e.to_string())?;
    let mut reader = response.body_mut().with_config().limit(spec.bytes + 1).reader();
    let mut file = fs::File::create(partial).map_err(|e| e.to_string())?;
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
    file.sync_all().map_err(|e| e.to_string())?;
    if written != spec.bytes {
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
        return Err(format!("model checksum mismatch: {digest}"));
    }
    Ok(())
}
