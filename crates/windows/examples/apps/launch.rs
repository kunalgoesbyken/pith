use std::ffi::c_void;
use std::fs;
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::ConvertStringSidToSidW;
use windows::Win32::Security::{
    CreateRestrictedToken, DISABLE_MAX_PRIVILEGE, LUA_TOKEN, PSID, SID_AND_ATTRIBUTES,
    SetTokenInformation, TOKEN_ACCESS_MASK, TOKEN_ADJUST_DEFAULT, TOKEN_ADJUST_SESSIONID,
    TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE, TOKEN_MANDATORY_LABEL, TOKEN_QUERY, TokenIntegrityLevel,
};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject,
};
use windows::Win32::System::SystemServices::SE_GROUP_INTEGRITY;
use windows::Win32::System::Threading::{
    CREATE_PROCESS_LOGON_FLAGS, CREATE_SUSPENDED, CreateProcessWithTokenW, GetCurrentProcess,
    OpenProcessToken, PROCESS_INFORMATION, ResumeThread, STARTUPINFOW, TerminateProcess,
};
use windows::core::{Owned, PCWSTR, PWSTR, w};

use crate::common::wide;
use crate::input::wait_until_gone;
use crate::report::{App, Case, Field, Variant};

const MEDIUM_INTEGRITY_SID: PCWSTR = w!("S-1-16-8192");
const FORCE_ACCESSIBILITY: &str = "--force-renderer-accessibility";
const NO_SANDBOX_FOR_LIMITED_TOKEN: &str = "--no-sandbox";
const FIXTURE_PAGE: &str = "field.html";
const FIXTURE_APP: &str = "electron";
const TERMINATED: u32 = 1;

pub struct Target {
    pub app: App,
    pub exe: PathBuf,
    pub fixtures: PathBuf,
}

struct Profile(PathBuf);

impl Profile {
    fn create(target: &Target, field: Field) -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!("pith-apps-{}-{field}", target.app));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).map_err(|error| format!("profile: {error}"))?;
        Ok(Self(path))
    }
}

impl Drop for Profile {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub struct Launched {
    _job: Owned<HANDLE>,
    _profile: Option<Profile>,
}

impl Launched {
    pub fn close(self, marker: &str) {
        drop(self);
        wait_until_gone(marker);
    }
}

pub fn marker(app: App, field: Field) -> String {
    match app {
        App::Notepad => "Notepad".to_string(),
        _ => fixture_title(field),
    }
}

fn fixture_title(field: Field) -> String {
    format!("pith-fixture-{field}")
}

fn token_rights() -> TOKEN_ACCESS_MASK {
    TOKEN_ADJUST_DEFAULT
        | TOKEN_ADJUST_SESSIONID
        | TOKEN_ASSIGN_PRIMARY
        | TOKEN_DUPLICATE
        | TOKEN_QUERY
}

fn stage<T>(name: &str, result: windows::core::Result<T>) -> Result<T, String> {
    result.map_err(|error| format!("{name}: {error}"))
}

fn limited_token() -> Result<Owned<HANDLE>, String> {
    unsafe {
        let mut current = HANDLE::default();
        stage(
            "open token",
            OpenProcessToken(GetCurrentProcess(), token_rights(), &mut current),
        )?;
        let current = Owned::new(current);
        let mut limited = HANDLE::default();
        stage(
            "restrict token",
            CreateRestrictedToken(
                *current,
                DISABLE_MAX_PRIVILEGE | LUA_TOKEN,
                None,
                None,
                None,
                &mut limited,
            ),
        )?;
        let limited = Owned::new(limited);
        let mut sid = PSID::default();
        stage(
            "integrity sid",
            ConvertStringSidToSidW(MEDIUM_INTEGRITY_SID, &mut sid),
        )?;
        let label = TOKEN_MANDATORY_LABEL {
            Label: SID_AND_ATTRIBUTES {
                Sid: sid,
                Attributes: SE_GROUP_INTEGRITY as u32,
            },
        };
        let labelled = SetTokenInformation(
            *limited,
            TokenIntegrityLevel,
            &label as *const _ as *const c_void,
            std::mem::size_of::<TOKEN_MANDATORY_LABEL>() as u32,
        );
        let _ = LocalFree(Some(HLOCAL(sid.0)));
        stage("integrity label", labelled)?;
        Ok(limited)
    }
}

fn job_for(process: HANDLE) -> Result<Owned<HANDLE>, String> {
    unsafe {
        let job = Owned::new(stage("create job", CreateJobObjectW(None, None))?);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        stage(
            "limit job",
            SetInformationJobObject(
                *job,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ),
        )?;
        stage("assign job", AssignProcessToJobObject(*job, process))?;
        Ok(job)
    }
}

fn spawn_limited(exe: &Path, args: &[String]) -> Result<Owned<HANDLE>, String> {
    let token = limited_token()?;
    let quoted: Vec<String> = std::iter::once(exe.display().to_string())
        .chain(args.iter().cloned())
        .map(|part| format!("\"{part}\""))
        .collect();
    let mut command_line = wide(&quoted.join(" "));
    let startup = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut info = PROCESS_INFORMATION::default();
    unsafe {
        stage(
            "create process",
            CreateProcessWithTokenW(
                *token,
                CREATE_PROCESS_LOGON_FLAGS(0),
                PCWSTR::null(),
                Some(PWSTR(command_line.as_mut_ptr())),
                CREATE_SUSPENDED,
                None,
                PCWSTR::null(),
                &startup,
                &mut info,
            ),
        )?;
        let process = Owned::new(info.hProcess);
        let thread = Owned::new(info.hThread);
        let job = job_for(*process).inspect_err(|_| {
            let _ = TerminateProcess(*process, TERMINATED);
        })?;
        if ResumeThread(*thread) == u32::MAX {
            return Err("resume process: failed".to_string());
        }
        Ok(job)
    }
}

fn arguments(target: &Target, case: Case, profile: Option<&Profile>) -> Vec<String> {
    let page = target.fixtures.join(FIXTURE_PAGE);
    let url = format!(
        "file:///{}?field={}&title={}",
        page.display().to_string().replace('\\', "/"),
        case.field,
        fixture_title(case.field)
    );
    let accessibility = case.variant == Variant::AccessibilityFlag;
    let mut args = Vec::new();
    match (target.app, profile) {
        (App::Chrome | App::Edge, Some(profile)) => {
            args.push(format!("--user-data-dir={}", profile.0.display()));
            args.extend(
                [
                    "--no-first-run",
                    "--no-default-browser-check",
                    NO_SANDBOX_FOR_LIMITED_TOKEN,
                ]
                .map(String::from),
            );
            if accessibility {
                args.push(FORCE_ACCESSIBILITY.to_string());
            }
            args.push(url);
        }
        (App::Electron, _) => {
            args.push(NO_SANDBOX_FOR_LIMITED_TOKEN.to_string());
            if accessibility {
                args.push(FORCE_ACCESSIBILITY.to_string());
            }
            args.push(target.fixtures.join(FIXTURE_APP).display().to_string());
            args.push(url);
        }
        _ => {}
    }
    args
}

pub fn launch(target: &Target, case: Case) -> Result<Launched, String> {
    let profile = match target.app {
        App::Chrome | App::Edge => Some(Profile::create(target, case.field)?),
        App::Electron | App::Notepad => None,
    };
    let args = arguments(target, case, profile.as_ref());
    let job = spawn_limited(&target.exe, &args)?;
    Ok(Launched {
        _job: job,
        _profile: profile,
    })
}
