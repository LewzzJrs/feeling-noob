use windows_sys::Win32::System::StationsAndDesktops::{
    CreateDesktopW, DESKTOP_CREATEMENU, DESKTOP_CREATEWINDOW, DESKTOP_ENUMERATE,
    DESKTOP_HOOKCONTROL, DESKTOP_READOBJECTS, DESKTOP_WRITEOBJECTS, SetThreadDesktop,
};

pub fn move_thread_to_hidden_desktop() -> bool {
    let name: Vec<u16> = "fnoob-scan-hidden"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let access = DESKTOP_CREATEWINDOW
        | DESKTOP_CREATEMENU
        | DESKTOP_READOBJECTS
        | DESKTOP_WRITEOBJECTS
        | DESKTOP_HOOKCONTROL
        | DESKTOP_ENUMERATE;
    unsafe {
        let desktop = CreateDesktopW(
            name.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            access,
            std::ptr::null(),
        );
        !desktop.is_null() && SetThreadDesktop(desktop) != 0
    }
}
