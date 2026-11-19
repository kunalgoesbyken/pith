use std::path::PathBuf;

use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree, CoUninitialize, IPersistFile,
};
use windows::Win32::UI::Shell::{
    FOLDERID_Programs, IShellLinkW, KF_FLAG_DEFAULT, SHGetKnownFolderPath, ShellLink,
};
use windows::core::{Error, Interface, PCWSTR, Result, w};

use crate::util::wide;

const LINK_NAME: &str = "Pith.lnk";

fn programs_folder() -> Result<PathBuf> {
    let folder = unsafe { SHGetKnownFolderPath(&FOLDERID_Programs, KF_FLAG_DEFAULT, None) }?;
    let path = PathBuf::from(unsafe { folder.to_string() }.unwrap_or_default());
    unsafe { CoTaskMemFree(Some(folder.0.cast())) };
    Ok(path)
}

fn write_link(target: &PathBuf, link: &PathBuf) -> Result<()> {
    let target = wide(&target.to_string_lossy());
    let link = wide(&link.to_string_lossy());
    let shell: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }?;
    unsafe {
        shell.SetPath(PCWSTR(target.as_ptr()))?;
        shell.SetDescription(w!("Finishes your sentences, on this PC"))?;
        let file: IPersistFile = shell.cast()?;
        file.Save(PCWSTR(link.as_ptr()), true)
    }
}

pub fn ensure_start_menu_shortcut() -> Result<()> {
    let link = programs_folder()?.join(LINK_NAME);
    if link.exists() {
        return Ok(());
    }
    let exe = std::env::current_exe()
        .map_err(|error| Error::new(windows::Win32::Foundation::E_FAIL, error.to_string()))?;
    let initialised = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok();
    let written = write_link(&exe, &link);
    if initialised {
        unsafe { CoUninitialize() };
    }
    written
}
