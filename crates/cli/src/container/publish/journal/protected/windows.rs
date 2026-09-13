//! Owner-only DACLs are installed atomically at directory creation, without an
//! inheritable parent ACL. Files inherit only that owner ACE. Reopening checks
//! the actual handle's owner, DACL, file type and link count before reading.
use anyhow::{ensure, Result};
use std::{
    ffi::c_void,
    fs::File,
    os::windows::{
        ffi::OsStrExt,
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    ptr::null_mut,
};
use windows_sys::Win32::{
    Foundation::{LocalFree, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE},
    Security::{
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
            SDDL_REVISION_1, SE_FILE_OBJECT,
        },
        EqualSid, GetAce, GetSecurityDescriptorControl, GetTokenInformation, TokenUser, ACCESS_ALLOWED_ACE, ACE_HEADER,
        DACL_SECURITY_INFORMATION, INHERIT_ONLY_ACE, OWNER_SECURITY_INFORMATION, SECURITY_ATTRIBUTES,
        SE_DACL_PROTECTED, TOKEN_QUERY, TOKEN_USER,
    },
    Storage::FileSystem::{
        CreateDirectoryW, CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, CREATE_NEW,
        FILE_ALL_ACCESS, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, READ_CONTROL,
    },
    System::{
        SystemServices::ACCESS_ALLOWED_ACE_TYPE,
        Threading::{GetCurrentProcess, OpenProcessToken},
    },
};

struct LocalAllocation(*mut c_void);
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: this pointer is allocated by a successful Windows security
        // descriptor/SID conversion API and is freed exactly once.
        unsafe {
            LocalFree(self.0);
        }
    }
}

fn check(ok: i32) -> Result<()> {
    ensure!(ok != 0, "protected publication storage Windows operation failed");
    Ok(())
}

/// Aligned backing storage owns the TOKEN_USER and its SID together.
fn user() -> Result<Vec<usize>> {
    unsafe {
        let mut raw = null_mut();
        check(OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw))?;
        let token = OwnedHandle::from_raw_handle(raw);
        let mut length = 0;
        GetTokenInformation(token.as_raw_handle(), TokenUser, null_mut(), 0, &mut length);
        ensure!(
            (size_of::<TOKEN_USER>()..=1024).contains(&(length as usize)),
            "invalid Windows user identity size"
        );
        let mut data = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
        check(GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            data.as_mut_ptr().cast(),
            length,
            &mut length,
        ))?;
        Ok(data)
    }
}

fn owner_descriptor() -> Result<LocalAllocation> {
    unsafe {
        let user = user()?;
        let sid = (*(user.as_ptr().cast::<TOKEN_USER>())).User.Sid;
        let mut sid_string = null_mut();
        check(ConvertSidToStringSidW(sid, &mut sid_string))?;
        let _sid_string = LocalAllocation(sid_string.cast());
        // Windows SID strings are bounded by the SID representation. Read only
        // through the terminating NUL in the returned allocation.
        let mut length = 0;
        while *sid_string.add(length) != 0 {
            length += 1;
        }
        let sid = String::from_utf16(std::slice::from_raw_parts(sid_string, length))?;
        let sddl: Vec<_> = format!("O:{sid}D:P(A;OICI;FA;;;{sid})")
            .encode_utf16()
            .chain([0])
            .collect();
        let mut descriptor = null_mut();
        check(ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        ))?;
        Ok(LocalAllocation(descriptor))
    }
}

pub(in super::super) fn create_directory(path: &Path) -> Result<()> {
    unsafe {
        let descriptor = owner_descriptor()?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
        let path_wide: Vec<_> = path.as_os_str().encode_wide().chain([0]).collect();
        ensure!(
            !path_wide[..path_wide.len() - 1].contains(&0),
            "invalid publication path"
        );
        check(CreateDirectoryW(path_wide.as_ptr(), &attributes))?;
    }
    directory(path)
}

pub(in super::super) fn directory(path: &Path) -> Result<()> {
    let file = File::options()
        .access_mode(READ_CONTROL)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    verify(&file, true)
}

pub(in super::super) fn file(path: &Path, create: bool, write: bool) -> Result<File> {
    let file = if create {
        // Explicit owner is necessary even with a private parent: elevated
        // Windows tokens can default newly created files to Administrators.
        unsafe {
            let descriptor = owner_descriptor()?;
            let attributes = SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor.0,
                bInheritHandle: 0,
            };
            let path_wide: Vec<_> = path.as_os_str().encode_wide().chain([0]).collect();
            ensure!(
                !path_wide[..path_wide.len() - 1].contains(&0),
                "invalid publication path"
            );
            let raw = CreateFileW(
                path_wide.as_ptr(),
                GENERIC_READ | if write { GENERIC_WRITE } else { 0 },
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                &attributes,
                CREATE_NEW,
                FILE_FLAG_OPEN_REPARSE_POINT,
                null_mut(),
            );
            ensure!(raw != INVALID_HANDLE_VALUE, "cannot create protected publication file");
            File::from_raw_handle(raw)
        }
    } else {
        File::options()
            .read(true)
            .write(write)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?
    };
    verify_file(&file)?;
    Ok(file)
}

pub(in super::super) fn temporary(directory: &Path) -> Result<tempfile::NamedTempFile> {
    let path = directory.join(format!(".publication-{}.tmp", uuid::Uuid::now_v7()));
    let file = file(&path, true, true)?;
    Ok(tempfile::NamedTempFile::from_parts(
        file,
        tempfile::TempPath::from_path(path),
    ))
}

pub(in super::super) fn verify_file(file: &File) -> Result<()> {
    verify(file, false)
}

fn verify(file: &File, directory: bool) -> Result<()> {
    unsafe {
        let mut info: BY_HANDLE_FILE_INFORMATION = std::mem::zeroed();
        check(GetFileInformationByHandle(file.as_raw_handle(), &mut info))?;
        ensure!(
            info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT == 0
                && (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) == directory
                && (directory || info.nNumberOfLinks == 1),
            "publication storage must not contain reparse points or additional file links"
        );
        let mut owner = null_mut();
        let mut dacl = null_mut();
        let mut descriptor = null_mut();
        ensure!(
            GetSecurityInfo(
                file.as_raw_handle(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                &mut owner,
                null_mut(),
                &mut dacl,
                null_mut(),
                &mut descriptor
            ) == 0,
            "cannot verify protected publication storage permissions"
        );
        let _descriptor = LocalAllocation(descriptor);
        let user = user()?;
        let sid = (*(user.as_ptr().cast::<TOKEN_USER>())).User.Sid;
        ensure!(
            !owner.is_null() && EqualSid(owner, sid) != 0 && !dacl.is_null() && (*dacl).AceCount == 1,
            "publication storage must allow only its current owner"
        );
        let mut ace = null_mut();
        check(GetAce(dacl, 0, &mut ace))?;
        let header = &*ace.cast::<ACE_HEADER>();
        ensure!(
            header.AceType as u32 == ACCESS_ALLOWED_ACE_TYPE
                && header.AceSize as usize >= size_of::<ACCESS_ALLOWED_ACE>(),
            "invalid publication access entry"
        );
        let ace = &*ace.cast::<ACCESS_ALLOWED_ACE>();
        ensure!(
            ace.Header.AceType as u32 == ACCESS_ALLOWED_ACE_TYPE
                && ace.Header.AceFlags as u32 & INHERIT_ONLY_ACE == 0
                && ace.Mask == FILE_ALL_ACCESS
                && EqualSid((&raw const ace.SidStart).cast_mut().cast(), sid) != 0,
            "publication storage must allow only its current owner"
        );
        if directory {
            let mut control = 0;
            let mut revision = 0;
            check(GetSecurityDescriptorControl(descriptor, &mut control, &mut revision))?;
            ensure!(
                control & SE_DACL_PROTECTED != 0,
                "publication directory must exclude inherited permissions"
            );
        }
    }
    Ok(())
}

/// Deny DELETE sharing for every directory component for the entire journal
/// lifetime. Even a writable parent then cannot rename/replace an opened child
/// while secret files are accessed by path. Reparse ancestors are rejected.
pub(in super::super) fn pin_parents(path: &Path) -> Result<Vec<File>> {
    use windows_sys::Win32::Storage::FileSystem::FILE_READ_ATTRIBUTES;
    let absolute = std::path::absolute(path)?;
    let mut files = Vec::new();
    for path in absolute.ancestors().collect::<Vec<_>>().into_iter().rev() {
        let file = File::options()
            .access_mode(FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        unsafe {
            let mut info: BY_HANDLE_FILE_INFORMATION = std::mem::zeroed();
            check(GetFileInformationByHandle(file.as_raw_handle(), &mut info))?;
            ensure!(
                info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT == 0
                    && info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0,
                "publication directory ancestors must not be reparse points"
            );
        }
        files.push(file);
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_only_directory_and_files_survive_reopen_and_reject_extra_links() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("publication");
        create_directory(&path).unwrap();
        let body = path.join("body");
        drop(file(&body, true, true).unwrap());
        drop(file(&body, false, false).unwrap());
        let progress = temporary(&path).unwrap();
        verify_file(progress.as_file()).unwrap();
        std::fs::hard_link(&body, path.join("extra-link")).unwrap();
        assert!(file(&body, false, false).is_err());
    }

    #[test]
    fn reparse_points_are_rejected_before_reading() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("publication");
        create_directory(&path).unwrap();
        let body = path.join("body");
        drop(file(&body, true, true).unwrap());
        let link = path.join("link");
        std::os::windows::fs::symlink_file(&body, &link).unwrap();
        assert!(file(&link, false, false).is_err());
        let link = root.path().join("linked-directory");
        std::os::windows::fs::symlink_dir(&path, &link).unwrap();
        assert!(directory(&link).is_err());
    }

    #[test]
    fn opening_a_file_with_a_permissive_dacl_fails_closed() {
        use windows_sys::Win32::Security::{Authorization::SetNamedSecurityInfoW, PROTECTED_DACL_SECURITY_INFORMATION};
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("publication");
        create_directory(&path).unwrap();
        let body = path.join("body");
        drop(file(&body, true, true).unwrap());
        let path_wide: Vec<_> = body.as_os_str().encode_wide().chain([0]).collect();
        // A NULL DACL grants everyone access. This is an owned empty fixture;
        // no secret bytes are written before or after changing its ACL.
        unsafe {
            assert_eq!(
                SetNamedSecurityInfoW(
                    path_wide.as_ptr(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    null_mut(),
                    null_mut(),
                    null_mut(),
                    null_mut()
                ),
                0
            );
        }
        assert!(file(&body, false, false).is_err());
    }
}
