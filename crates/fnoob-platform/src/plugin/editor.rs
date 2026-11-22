use super::PluginError;
use std::sync::Once;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use vst3::Steinberg::Vst::{IEditController, IEditControllerTrait};
use vst3::Steinberg::{
    IPlugFrame, IPlugFrameTrait, IPlugView, IPlugViewTrait, ViewRect, kPlatformTypeHWND, kResultOk,
    tresult,
};
use vst3::{Class, ComPtr, ComRef, ComWrapper};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow,
    GWLP_USERDATA, GetWindowLongPtrW, IDC_ARROW, LoadCursorW, RegisterClassW, SW_HIDE,
    SW_SHOWNORMAL, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_CLOSE, WNDCLASSW, WS_CAPTION, WS_MINIMIZEBOX,
    WS_OVERLAPPED, WS_SYSMENU,
};

const STYLE: u32 = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
static REGISTER: Once = Once::new();

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn class_name() -> Vec<u16> {
    wide("fnoob-plugin-editor")
}

unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if message == WM_CLOSE {
        let flag = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const AtomicBool;
        if !flag.is_null() {
            unsafe { (*flag).store(true, Ordering::Release) };
        }
        unsafe { ShowWindow(hwnd, SW_HIDE) };
        return 0;
    }
    unsafe { DefWindowProcW(hwnd, message, w, l) }
}

fn register_class() {
    REGISTER.call_once(|| {
        let name = class_name();
        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: unsafe { GetModuleHandleW(std::ptr::null()) },
            hIcon: std::ptr::null_mut(),
            hCursor: unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) },
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: name.as_ptr(),
        };
        unsafe { RegisterClassW(&class) };
    });
}

fn outer_size(width: i32, height: i32) -> (i32, i32) {
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: width.max(1),
        bottom: height.max(1),
    };
    unsafe { AdjustWindowRectEx(&mut rect, STYLE, 0, 0) };
    (rect.right - rect.left, rect.bottom - rect.top)
}

struct PlugFrame {
    hwnd: AtomicIsize,
}

impl Class for PlugFrame {
    type Interfaces = (IPlugFrame,);
}

impl IPlugFrameTrait for PlugFrame {
    unsafe fn resizeView(&self, view: *mut IPlugView, size: *mut ViewRect) -> tresult {
        if size.is_null() {
            return kResultOk;
        }
        let rect = unsafe { *size };
        let (width, height) = outer_size(rect.right - rect.left, rect.bottom - rect.top);
        let hwnd = self.hwnd.load(Ordering::Acquire) as HWND;
        unsafe {
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                width,
                height,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
            if let Some(view) = ComRef::from_raw(view) {
                view.onSize(size);
            }
        }
        kResultOk
    }
}

pub struct Editor {
    view: ComPtr<IPlugView>,
    _frame: ComWrapper<PlugFrame>,
    hwnd: HWND,
    closed: Box<AtomicBool>,
}

impl Editor {
    pub(super) fn open(
        controller: &ComPtr<IEditController>,
        title: &str,
    ) -> Result<Editor, PluginError> {
        let view = unsafe { ComPtr::from_raw(controller.createView(c"editor".as_ptr())) }
            .ok_or(PluginError::Editor("the plugin has no editor"))?;
        if unsafe { view.isPlatformTypeSupported(kPlatformTypeHWND) } != kResultOk {
            return Err(PluginError::Editor("Windows editors are not supported"));
        }
        let mut rect = ViewRect {
            left: 0,
            top: 0,
            right: 400,
            bottom: 300,
        };
        unsafe { view.getSize(&mut rect) };
        let (width, height) = outer_size(rect.right - rect.left, rect.bottom - rect.top);
        register_class();
        let class = class_name();
        let caption = wide(title);
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                caption.as_ptr(),
                STYLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                width,
                height,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            )
        };
        if hwnd.is_null() {
            return Err(PluginError::Editor("the window could not be created"));
        }
        let closed = Box::new(AtomicBool::new(false));
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, &*closed as *const AtomicBool as isize);
        }
        let frame = ComWrapper::new(PlugFrame {
            hwnd: AtomicIsize::new(hwnd as isize),
        });
        unsafe {
            if let Some(frame) = frame.as_com_ref::<IPlugFrame>() {
                view.setFrame(frame.as_ptr());
            }
            if view.attached(hwnd, kPlatformTypeHWND) != kResultOk {
                view.setFrame(std::ptr::null_mut());
                DestroyWindow(hwnd);
                return Err(PluginError::Editor("the editor could not open"));
            }
            ShowWindow(hwnd, SW_SHOWNORMAL);
        }
        Ok(Editor {
            view,
            _frame: frame,
            hwnd,
            closed,
        })
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }

    pub fn focus(&self) {
        unsafe {
            ShowWindow(self.hwnd, SW_SHOWNORMAL);
            SetForegroundWindow(self.hwnd);
        }
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        unsafe {
            self.view.removed();
            self.view.setFrame(std::ptr::null_mut());
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            DestroyWindow(self.hwnd);
        }
    }
}
