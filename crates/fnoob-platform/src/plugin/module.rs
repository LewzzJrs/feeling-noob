use super::PluginError;
use super::com::HostApplication;
use std::ffi::{c_char, c_void};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use vst3::Steinberg::{
    FIDString, FUnknown, IPluginFactory, IPluginFactory2, IPluginFactory2Trait, IPluginFactory3,
    IPluginFactory3Trait, IPluginFactoryTrait, PClassInfo, PClassInfo2, kResultOk,
};
use vst3::{ComPtr, ComWrapper, Interface};
use windows_sys::Win32::Foundation::{FreeLibrary, HMODULE};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

const AUDIO_MODULE_CLASS: &str = "Audio Module Class";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassInfo {
    pub cid: [u8; 16],
    pub name: String,
    pub category: String,
    pub sub_categories: String,
    pub vendor: String,
}

impl ClassInfo {
    pub fn is_audio_module(&self) -> bool {
        self.category == AUDIO_MODULE_CLASS
    }

    pub fn is_instrument(&self) -> bool {
        self.sub_categories
            .split('|')
            .any(|s| s.eq_ignore_ascii_case("Instrument"))
    }
}

pub struct Module {
    factory: Option<ComPtr<IPluginFactory>>,
    host: ComWrapper<HostApplication>,
    handle: HMODULE,
    pub path: PathBuf,
}

unsafe impl Send for Module {}
unsafe impl Sync for Module {}

fn binary_path(path: &Path) -> PathBuf {
    if path.is_dir() {
        let name = path.file_name().unwrap_or_default();
        path.join("Contents").join("x86_64-win").join(name)
    } else {
        path.to_path_buf()
    }
}

pub(super) fn text<const N: usize>(raw: &[c_char; N]) -> String {
    let bytes: Vec<u8> = raw
        .iter()
        .take_while(|&&c| c != 0)
        .map(|&c| c as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn cid_bytes(raw: &[c_char; 16]) -> [u8; 16] {
    raw.map(|c| c as u8)
}

impl Module {
    pub fn load(path: &Path) -> Result<Arc<Module>, PluginError> {
        let binary = binary_path(path);
        let wide: Vec<u16> = binary
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let handle = unsafe { LoadLibraryW(wide.as_ptr()) };
        if handle.is_null() {
            return Err(PluginError::Load(
                std::io::Error::last_os_error().to_string(),
            ));
        }
        let mut module = Module {
            factory: None,
            host: ComWrapper::new(HostApplication),
            handle,
            path: path.to_path_buf(),
        };
        unsafe {
            if let Some(init) = GetProcAddress(handle, c"InitDll".as_ptr().cast()) {
                let init: unsafe extern "system" fn() -> bool = std::mem::transmute(init);
                init();
            }
            let get = GetProcAddress(handle, c"GetPluginFactory".as_ptr().cast())
                .ok_or(PluginError::NoFactory)?;
            let get: unsafe extern "system" fn() -> *mut IPluginFactory = std::mem::transmute(get);
            let factory = ComPtr::from_raw(get()).ok_or(PluginError::NoFactory)?;
            if let Some(third) = factory.cast::<IPluginFactory3>()
                && let Some(host) = module
                    .host
                    .as_com_ref::<vst3::Steinberg::Vst::IHostApplication>()
            {
                third.setHostContext(host.as_ptr().cast::<FUnknown>());
            }
            module.factory = Some(factory);
        }
        Ok(Arc::new(module))
    }

    pub(super) fn host_context(&self) -> *mut FUnknown {
        self.host
            .as_com_ref::<vst3::Steinberg::Vst::IHostApplication>()
            .map_or(std::ptr::null_mut(), |h| h.as_ptr().cast())
    }

    pub fn classes(&self) -> Vec<ClassInfo> {
        let Some(factory) = &self.factory else {
            return Vec::new();
        };
        let second = factory.cast::<IPluginFactory2>();
        let count = unsafe { factory.countClasses() }.max(0);
        let mut classes = Vec::new();
        for index in 0..count {
            unsafe {
                if let Some(second) = &second {
                    let mut info: PClassInfo2 = std::mem::zeroed();
                    if second.getClassInfo2(index, &mut info) == kResultOk {
                        classes.push(ClassInfo {
                            cid: cid_bytes(&info.cid),
                            name: text(&info.name),
                            category: text(&info.category),
                            sub_categories: text(&info.subCategories),
                            vendor: text(&info.vendor),
                        });
                        continue;
                    }
                }
                let mut info: PClassInfo = std::mem::zeroed();
                if factory.getClassInfo(index, &mut info) == kResultOk {
                    classes.push(ClassInfo {
                        cid: cid_bytes(&info.cid),
                        name: text(&info.name),
                        category: text(&info.category),
                        sub_categories: String::new(),
                        vendor: String::new(),
                    });
                }
            }
        }
        classes
    }

    pub(super) fn create<I: Interface>(&self, cid: &[u8; 16]) -> Option<ComPtr<I>> {
        let factory = self.factory.as_ref()?;
        let mut object: *mut c_void = std::ptr::null_mut();
        unsafe {
            let result = factory.createInstance(
                cid.as_ptr() as FIDString,
                I::IID.as_ptr() as FIDString,
                &mut object,
            );
            if result != kResultOk {
                return None;
            }
            ComPtr::from_raw(object.cast::<I>())
        }
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        self.factory = None;
        unsafe {
            if let Some(exit) = GetProcAddress(self.handle, c"ExitDll".as_ptr().cast()) {
                let exit: unsafe extern "system" fn() -> bool = std::mem::transmute(exit);
                exit();
            }
            FreeLibrary(self.handle);
        }
    }
}

pub fn scan(path: &Path) -> Result<Vec<ClassInfo>, PluginError> {
    let module = Module::load(path)?;
    Ok(module
        .classes()
        .into_iter()
        .filter(ClassInfo::is_audio_module)
        .collect())
}
