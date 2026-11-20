/*
 * SPDX-FileCopyrightText: 2026 SynrgStudio
 * SPDX-License-Identifier: Apache-2.0
 */

//! The COM server Explorer loads: the thumbnail provider, its class factory,
//! and the DLL's exports, registration included.

use std::ffi::c_void;
use std::io::{self, Read};
use std::ptr::null_mut;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use windows::Win32::Foundation::{
    CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_FAIL, E_POINTER, E_UNEXPECTED,
    ERROR_ALREADY_INITIALIZED, ERROR_FILE_NOT_FOUND, HMODULE, S_FALSE, S_OK,
};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateDIBSection, DIB_RGB_COLORS, DeleteObject, HBITMAP,
};
use windows::Win32::System::Com::{IClassFactory, IClassFactory_Impl, IStream};
use windows::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    GetModuleFileNameW, GetModuleHandleExW,
};
use windows::Win32::UI::Shell::PropertiesSystem::{
    IInitializeWithStream, IInitializeWithStream_Impl,
};
use windows::Win32::UI::Shell::{
    IThumbnailProvider, IThumbnailProvider_Impl, SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify,
    WTS_ALPHATYPE, WTSAT_ARGB,
};
use windows_core::{
    BOOL, Error, GUID, HRESULT, IUnknown, Interface, PCWSTR, Ref, Result, implement,
};

use crate::{Picture, decode_png, fit, read_thumbnail};

/// The class id of the thumbnail provider:
/// `{AF9572F8-199D-4710-88C0-5D553D38F093}`.
pub const CLSID: GUID = GUID::from_u128(0xAF9572F8_199D_4710_88C0_5D553D38F093);

/// The shell's key for a type's thumbnail handler, under `ShellEx`.
const THUMBNAIL_HANDLER: &str = "{E357FCCD-A995-4576-B01F-234630154E96}";

/// Objects of this DLL still alive, and locks the shell holds on it: the DLL
/// may be unloaded only when both are zero.
static OBJECTS: AtomicUsize = AtomicUsize::new(0);
static LOCKS: AtomicUsize = AtomicUsize::new(0);

/// Counts one object for as long as it lives.
#[derive(Debug)]
struct Alive;

impl Alive {
    fn new() -> Self {
        OBJECTS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for Alive {
    fn drop(&mut self) {
        OBJECTS.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The thumbnail provider: Explorer hands it the file as a stream, then asks
/// for a picture of at most `cx` pixels a side.
#[implement(IThumbnailProvider, IInitializeWithStream)]
#[derive(Debug)]
pub struct ThumbnailProvider {
    stream: Mutex<Option<IStream>>,
    _alive: Alive,
}

impl ThumbnailProvider {
    pub fn new() -> Self {
        Self {
            stream: Mutex::new(None),
            _alive: Alive::new(),
        }
    }
}

impl Default for ThumbnailProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl IInitializeWithStream_Impl for ThumbnailProvider_Impl {
    fn Initialize(&self, stream: Ref<IStream>, _mode: u32) -> Result<()> {
        let stream = stream.ok()?.clone();
        let mut slot = self.stream.lock().map_err(|_| Error::from(E_UNEXPECTED))?;
        if slot.is_some() {
            return Err(ERROR_ALREADY_INITIALIZED.to_hresult().into());
        }
        *slot = Some(stream);
        Ok(())
    }
}

impl IThumbnailProvider_Impl for ThumbnailProvider_Impl {
    fn GetThumbnail(
        &self,
        cx: u32,
        phbmp: *mut HBITMAP,
        pdwalpha: *mut WTS_ALPHATYPE,
    ) -> Result<()> {
        if phbmp.is_null() || pdwalpha.is_null() {
            return Err(E_POINTER.into());
        }
        let stream = self
            .stream
            .lock()
            .map_err(|_| Error::from(E_UNEXPECTED))?
            .clone()
            .ok_or_else(|| Error::from(E_UNEXPECTED))?;
        // No thumbnail, or one that does not decode: Explorer shows the
        // type's icon instead.
        let picture = read_thumbnail(StreamReader(&stream))
            .and_then(|png| decode_png(&png))
            .ok_or_else(|| Error::from(E_FAIL))?;
        let bitmap = bitmap(&fit(&picture, cx))?;
        // SAFETY: both pointers were checked not null, and Explorer passes
        // them for the provider to write its results into.
        unsafe {
            phbmp.write(bitmap);
            pdwalpha.write(WTSAT_ARGB);
        }
        Ok(())
    }
}

/// Reads an `IStream` through `std::io::Read`.
struct StreamReader<'a>(&'a IStream);

impl Read for StreamReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let len = u32::try_from(buf.len()).unwrap_or(u32::MAX);
        let mut read = 0;
        // SAFETY: `buf` can take `len` bytes, and `read` is a valid place for
        // the count. A short read returns S_FALSE, which is not an error.
        unsafe { self.0.Read(buf.as_mut_ptr().cast(), len, Some(&mut read)) }
            .ok()
            .map_err(io::Error::other)?;
        Ok(read as usize)
    }
}

/// The picture as a 32-bit top-down DIB section, in BGRA with straight alpha,
/// as `WTSAT_ARGB` asks.
fn bitmap(picture: &Picture) -> Result<HBITMAP> {
    let width = i32::try_from(picture.width).map_err(|_| Error::from(E_FAIL))?;
    let height = i32::try_from(picture.height).map_err(|_| Error::from(E_FAIL))?;
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = null_mut();
    // SAFETY: `info` describes a 32-bit DIB of width × height pixels, and
    // `bits` receives the address of its pixels.
    let bitmap = unsafe { CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0) }?;
    if bits.is_null() {
        // SAFETY: `bitmap` was just made here and nothing else refers to it.
        let _ = unsafe { DeleteObject(bitmap.into()) };
        return Err(E_FAIL.into());
    }
    // SAFETY: the section holds width × height pixels of 4 bytes, which is
    // the length of `picture.rgba`, and nothing else refers to it yet.
    let pixels = unsafe { std::slice::from_raw_parts_mut(bits.cast::<u8>(), picture.rgba.len()) };
    for (to, from) in pixels
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(picture.rgba.as_chunks::<4>().0)
    {
        *to = [from[2], from[1], from[0], from[3]];
    }
    Ok(bitmap)
}

/// Makes thumbnail providers for COM.
#[implement(IClassFactory)]
#[derive(Debug)]
struct Factory {
    _alive: Alive,
}

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(
        &self,
        outer: Ref<IUnknown>,
        iid: *const GUID,
        object: *mut *mut c_void,
    ) -> Result<()> {
        if object.is_null() || iid.is_null() {
            return Err(E_POINTER.into());
        }
        // SAFETY: checked not null; COM passes it for the object to go in.
        unsafe { object.write(null_mut()) };
        if !outer.is_null() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let provider: IUnknown = ThumbnailProvider::new().into();
        // SAFETY: `iid` and `object` are COM's, checked not null.
        unsafe { provider.query(iid, object) }.ok()
    }

    fn LockServer(&self, lock: BOOL) -> Result<()> {
        if lock.as_bool() {
            LOCKS.fetch_add(1, Ordering::SeqCst);
        } else {
            LOCKS.fetch_sub(1, Ordering::SeqCst);
        }
        Ok(())
    }
}

/// COM asks the DLL for the factory of a class.
#[unsafe(no_mangle)]
extern "system" fn DllGetClassObject(
    clsid: *const GUID,
    iid: *const GUID,
    object: *mut *mut c_void,
) -> HRESULT {
    if clsid.is_null() || iid.is_null() || object.is_null() {
        return E_POINTER;
    }
    // SAFETY: checked not null; COM passes it for the factory to go in.
    unsafe { object.write(null_mut()) };
    // SAFETY: checked not null; COM passes a class id.
    if unsafe { *clsid } != CLSID {
        return CLASS_E_CLASSNOTAVAILABLE;
    }
    let factory: IClassFactory = Factory {
        _alive: Alive::new(),
    }
    .into();
    // SAFETY: `iid` and `object` are COM's, checked not null.
    unsafe { factory.query(iid, object) }
}

/// COM asks whether the DLL may be unloaded.
#[unsafe(no_mangle)]
extern "system" fn DllCanUnloadNow() -> HRESULT {
    if OBJECTS.load(Ordering::SeqCst) == 0 && LOCKS.load(Ordering::SeqCst) == 0 {
        S_OK
    } else {
        S_FALSE
    }
}

/// `regsvr32 bread_thumbnails.dll`: registers the provider for the current
/// user. An installer writes the same keys itself, see the README.
#[unsafe(no_mangle)]
extern "system" fn DllRegisterServer() -> HRESULT {
    to_hresult(register())
}

/// `regsvr32 /u bread_thumbnails.dll`: removes what `DllRegisterServer`
/// wrote.
#[unsafe(no_mangle)]
extern "system" fn DllUnregisterServer() -> HRESULT {
    to_hresult(unregister())
}

fn to_hresult(result: Result<()>) -> HRESULT {
    result.map_or_else(|err| err.code(), |()| S_OK)
}

fn clsid_text() -> String {
    format!("{{{CLSID:?}}}")
}

fn register() -> Result<()> {
    let path = module_path()?;
    let classes = windows_registry::CURRENT_USER.create(r"Software\Classes")?;
    let class = classes.create(format!(r"CLSID\{}", clsid_text()))?;
    class.set_string("", "Bread thumbnail provider")?;
    let server = class.create("InprocServer32")?;
    server.set_string("", &path)?;
    server.set_string("ThreadingModel", "Apartment")?;
    classes
        .create(format!(r".bread\ShellEx\{THUMBNAIL_HANDLER}"))?
        .set_string("", clsid_text())?;
    changed_associations();
    Ok(())
}

fn unregister() -> Result<()> {
    let classes = windows_registry::CURRENT_USER.create(r"Software\Classes")?;
    let handler = format!(r".bread\ShellEx\{THUMBNAIL_HANDLER}");
    let ours = classes
        .open(&handler)
        .and_then(|key| key.get_string(""))
        .is_ok_and(|value| value.eq_ignore_ascii_case(&clsid_text()));
    if ours {
        classes.remove_tree(&handler)?;
    }
    match classes.remove_tree(format!(r"CLSID\{}", clsid_text())) {
        Err(err) if err.code() == ERROR_FILE_NOT_FOUND.to_hresult() => {}
        other => other?,
    }
    changed_associations();
    Ok(())
}

/// Tells the shell that file associations changed, so it looks again.
fn changed_associations() {
    // SAFETY: SHCNE_ASSOCCHANGED takes no items.
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
}

/// The path of this DLL.
fn module_path() -> Result<String> {
    let mut module = HMODULE::default();
    let here = module_path as *const () as *const u16;
    // SAFETY: with FROM_ADDRESS the name is an address inside the module to
    // find, this function's own; UNCHANGED_REFCOUNT takes no reference.
    unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            PCWSTR(here),
            &mut module,
        )
    }?;
    let mut buffer = vec![0u16; 32_768];
    // SAFETY: `module` is this DLL, loaded while this code runs.
    let len = unsafe { GetModuleFileNameW(Some(module), &mut buffer) } as usize;
    if len == 0 || len >= buffer.len() {
        return Err(Error::from_thread());
    }
    String::from_utf16(&buffer[..len]).map_err(|_| Error::from(E_FAIL))
}
