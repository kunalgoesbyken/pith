use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{E_FAIL, MAX_PATH};
use windows::Win32::Storage::FileSystem::WIN32_FIND_DATAW;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree, CoUninitialize, IPersistFile, STGM_READ,
};
use windows::Win32::UI::Shell::{
    FOLDERID_Programs, IShellLinkW, KF_FLAG_DEFAULT, SHGetKnownFolderPath, ShellLink,
};
use windows::core::{Error, Interface, PCWSTR, Result, w};

const LINK_NAME: &str = "Pith.lnk";

fn path_text(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

fn programs_folder() -> Result<PathBuf> {
    let folder = unsafe { SHGetKnownFolderPath(&FOLDERID_Programs, KF_FLAG_DEFAULT, None) }?;
    let path = PathBuf::from(OsString::from_wide(unsafe { folder.as_wide() }));
    unsafe { CoTaskMemFree(Some(folder.0.cast())) };
    Ok(path)
}

fn target_of(shell: &IShellLinkW, file: &IPersistFile, link: &[u16]) -> Option<PathBuf> {
    unsafe { file.Load(PCWSTR(link.as_ptr()), STGM_READ) }.ok()?;
    let mut buffer = [0u16; MAX_PATH as usize];
    unsafe { shell.GetPath(&mut buffer, std::ptr::null_mut::<WIN32_FIND_DATAW>(), 0) }.ok()?;
    let length = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    Some(PathBuf::from(OsString::from_wide(&buffer[..length])))
}

fn write_link(exe: &Path, link: &Path) -> Result<()> {
    let shell: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }?;
    let file: IPersistFile = shell.cast()?;
    let link = path_text(link);
    if target_of(&shell, &file, &link).is_some_and(|target| target.is_file()) {
        return Ok(());
    }
    let exe = path_text(exe);
    unsafe {
        shell.SetPath(PCWSTR(exe.as_ptr()))?;
        shell.SetDescription(w!("Finishes your sentences, on this PC"))?;
        file.Save(PCWSTR(link.as_ptr()), true)
    }
}

pub fn ensure_start_menu_shortcut() -> Result<()> {
    let link = programs_folder()?.join(LINK_NAME);
    let exe = std::env::current_exe().map_err(|error| Error::new(E_FAIL, error.to_string()))?;
    let initialised = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    let written = write_link(&exe, &link);
    if initialised {
        unsafe { CoUninitialize() };
    }
    written
}
