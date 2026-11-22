use rtrb::Producer;
use std::cell::{Cell, UnsafeCell};
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use vst3::Steinberg::Vst::{
    Event, IAttributeList, IAttributeListTrait, IComponentHandler, IComponentHandlerTrait,
    IEventList, IEventListTrait, IHostApplication, IHostApplicationTrait, IMessage, IMessageTrait,
    IParamValueQueue, IParamValueQueueTrait, IParameterChanges, IParameterChangesTrait, ParamID,
    ParamValue, String128, TChar,
};
use vst3::Steinberg::{
    IBStream, IBStreamTrait, TUID, int32, int64, kInvalidArgument, kNotImplemented, kResultFalse,
    kResultOk, tresult, uint32,
};
use vst3::{Class, ComWrapper, Interface};

const HOST_NAME: &str = "feeling-noob";
pub const MAX_EVENTS: usize = 1024;
const MAX_PARAMETERS: usize = 64;
const MAX_POINTS: usize = 32;

fn same_id(raw: *const TUID, iid: &[u8; 16]) -> bool {
    !raw.is_null() && unsafe { (*raw).map(|c| c as u8) } == *iid
}

fn write_utf16(text: &str, out: &mut [u16]) {
    let mut written = 0;
    for (slot, unit) in out.iter_mut().zip(text.encode_utf16()) {
        *slot = unit;
        written += 1;
    }
    if let Some(end) = out.get_mut(written.min(out.len().saturating_sub(1))) {
        *end = 0;
    }
}

#[derive(Default)]
pub struct HostApplication;

impl Class for HostApplication {
    type Interfaces = (IHostApplication,);
}

impl IHostApplicationTrait for HostApplication {
    unsafe fn getName(&self, name: *mut String128) -> tresult {
        if name.is_null() {
            return kInvalidArgument;
        }
        write_utf16(HOST_NAME, unsafe { &mut *name });
        kResultOk
    }

    unsafe fn createInstance(
        &self,
        cid: *mut TUID,
        iid: *mut TUID,
        obj: *mut *mut c_void,
    ) -> tresult {
        if obj.is_null() {
            return kInvalidArgument;
        }
        let created = if same_id(cid, &IMessage::IID) && same_id(iid, &IMessage::IID) {
            ComWrapper::new(HostMessage::default())
                .to_com_ptr::<IMessage>()
                .map(|p| p.into_raw().cast::<c_void>())
        } else if same_id(cid, &IAttributeList::IID) && same_id(iid, &IAttributeList::IID) {
            ComWrapper::new(HostAttributes::default())
                .to_com_ptr::<IAttributeList>()
                .map(|p| p.into_raw().cast::<c_void>())
        } else {
            None
        };
        unsafe {
            match created {
                Some(pointer) => {
                    *obj = pointer;
                    kResultOk
                }
                None => {
                    *obj = std::ptr::null_mut();
                    kNotImplemented
                }
            }
        }
    }
}

pub struct HostMessage {
    id: Mutex<Option<CString>>,
    attributes: ComWrapper<HostAttributes>,
}

impl Default for HostMessage {
    fn default() -> Self {
        Self {
            id: Mutex::new(None),
            attributes: ComWrapper::new(HostAttributes::default()),
        }
    }
}

impl Class for HostMessage {
    type Interfaces = (IMessage,);
}

impl IMessageTrait for HostMessage {
    unsafe fn getMessageID(&self) -> *const c_char {
        self.id
            .lock()
            .ok()
            .and_then(|id| id.as_ref().map(|s| s.as_ptr()))
            .unwrap_or(std::ptr::null())
    }

    unsafe fn setMessageID(&self, id: *const c_char) {
        let value = (!id.is_null()).then(|| unsafe { CStr::from_ptr(id) }.to_owned());
        if let Ok(mut slot) = self.id.lock() {
            *slot = value;
        }
    }

    unsafe fn getAttributes(&self) -> *mut IAttributeList {
        self.attributes
            .as_com_ref::<IAttributeList>()
            .map_or(std::ptr::null_mut(), |a| a.as_ptr())
    }
}

enum Attribute {
    Int(i64),
    Float(f64),
    Text(Vec<u16>),
    Binary(Vec<u8>),
}

#[derive(Default)]
pub struct HostAttributes {
    values: Mutex<HashMap<Vec<u8>, Attribute>>,
}

impl Class for HostAttributes {
    type Interfaces = (IAttributeList,);
}

fn key(id: *const c_char) -> Option<Vec<u8>> {
    (!id.is_null()).then(|| unsafe { CStr::from_ptr(id) }.to_bytes().to_vec())
}

impl HostAttributes {
    fn set(&self, id: *const c_char, value: Attribute) -> tresult {
        match (key(id), self.values.lock()) {
            (Some(key), Ok(mut values)) => {
                values.insert(key, value);
                kResultOk
            }
            _ => kInvalidArgument,
        }
    }

    fn with<R>(&self, id: *const c_char, read: impl FnOnce(&Attribute) -> Option<R>) -> Option<R> {
        let key = key(id)?;
        let values = self.values.lock().ok()?;
        values.get(&key).and_then(read)
    }
}

impl IAttributeListTrait for HostAttributes {
    unsafe fn setInt(&self, id: *const c_char, value: int64) -> tresult {
        self.set(id, Attribute::Int(value))
    }

    unsafe fn getInt(&self, id: *const c_char, value: *mut int64) -> tresult {
        match self.with(id, |a| match a {
            Attribute::Int(v) => Some(*v),
            _ => None,
        }) {
            Some(v) if !value.is_null() => {
                unsafe { *value = v };
                kResultOk
            }
            _ => kResultFalse,
        }
    }

    unsafe fn setFloat(&self, id: *const c_char, value: f64) -> tresult {
        self.set(id, Attribute::Float(value))
    }

    unsafe fn getFloat(&self, id: *const c_char, value: *mut f64) -> tresult {
        match self.with(id, |a| match a {
            Attribute::Float(v) => Some(*v),
            _ => None,
        }) {
            Some(v) if !value.is_null() => {
                unsafe { *value = v };
                kResultOk
            }
            _ => kResultFalse,
        }
    }

    unsafe fn setString(&self, id: *const c_char, string: *const TChar) -> tresult {
        if string.is_null() {
            return kInvalidArgument;
        }
        let mut text = Vec::new();
        let mut index = 0;
        loop {
            let unit = unsafe { *string.add(index) };
            if unit == 0 {
                break;
            }
            text.push(unit);
            index += 1;
        }
        self.set(id, Attribute::Text(text))
    }

    unsafe fn getString(&self, id: *const c_char, string: *mut TChar, size: uint32) -> tresult {
        let units = (size / 2) as usize;
        if string.is_null() || units == 0 {
            return kInvalidArgument;
        }
        let out = unsafe { std::slice::from_raw_parts_mut(string, units) };
        match self.with(id, |a| match a {
            Attribute::Text(t) => Some(t.clone()),
            _ => None,
        }) {
            Some(text) => {
                let count = text.len().min(units - 1);
                out[..count].copy_from_slice(&text[..count]);
                out[count] = 0;
                kResultOk
            }
            None => kResultFalse,
        }
    }

    unsafe fn setBinary(&self, id: *const c_char, data: *const c_void, size: uint32) -> tresult {
        let bytes = if data.is_null() || size == 0 {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size as usize) }.to_vec()
        };
        self.set(id, Attribute::Binary(bytes))
    }

    unsafe fn getBinary(
        &self,
        id: *const c_char,
        data: *mut *const c_void,
        size: *mut uint32,
    ) -> tresult {
        if data.is_null() || size.is_null() {
            return kInvalidArgument;
        }
        match self.with(id, |a| match a {
            Attribute::Binary(b) => Some((b.as_ptr(), b.len())),
            _ => None,
        }) {
            Some((pointer, len)) => {
                unsafe {
                    *data = pointer.cast();
                    *size = len as uint32;
                }
                kResultOk
            }
            None => kResultFalse,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamChange {
    pub id: u32,
    pub value: f64,
}

pub struct ComponentHandler {
    changes: Mutex<Producer<ParamChange>>,
    pub edited: AtomicBool,
    pub restart: AtomicI32,
}

impl ComponentHandler {
    pub fn new(changes: Producer<ParamChange>) -> Self {
        Self {
            changes: Mutex::new(changes),
            edited: AtomicBool::new(false),
            restart: AtomicI32::new(0),
        }
    }
}

impl Class for ComponentHandler {
    type Interfaces = (IComponentHandler,);
}

impl IComponentHandlerTrait for ComponentHandler {
    unsafe fn beginEdit(&self, _id: ParamID) -> tresult {
        kResultOk
    }

    unsafe fn performEdit(&self, id: ParamID, value: ParamValue) -> tresult {
        if let Ok(mut changes) = self.changes.lock() {
            let _ = changes.push(ParamChange { id, value });
        }
        self.edited.store(true, Ordering::Release);
        kResultOk
    }

    unsafe fn endEdit(&self, _id: ParamID) -> tresult {
        kResultOk
    }

    unsafe fn restartComponent(&self, flags: int32) -> tresult {
        self.restart.fetch_or(flags, Ordering::AcqRel);
        kResultOk
    }
}

#[derive(Default)]
struct StreamData {
    bytes: Vec<u8>,
    position: usize,
}

#[derive(Default)]
pub struct MemoryStream {
    data: Mutex<StreamData>,
}

impl MemoryStream {
    pub fn with_bytes(bytes: &[u8]) -> Self {
        Self {
            data: Mutex::new(StreamData {
                bytes: bytes.to_vec(),
                position: 0,
            }),
        }
    }

    pub fn bytes(&self) -> Vec<u8> {
        self.data
            .lock()
            .map(|d| d.bytes.clone())
            .unwrap_or_default()
    }

    pub fn rewind(&self) {
        if let Ok(mut data) = self.data.lock() {
            data.position = 0;
        }
    }
}

impl Class for MemoryStream {
    type Interfaces = (IBStream,);
}

impl IBStreamTrait for MemoryStream {
    unsafe fn read(&self, buffer: *mut c_void, count: int32, read: *mut int32) -> tresult {
        let Ok(mut data) = self.data.lock() else {
            return kResultFalse;
        };
        let wanted = count.max(0) as usize;
        let available = data.bytes.len().saturating_sub(data.position);
        let n = wanted.min(available);
        if n > 0 && !buffer.is_null() {
            let from = &data.bytes[data.position..data.position + n];
            unsafe { std::ptr::copy_nonoverlapping(from.as_ptr(), buffer.cast::<u8>(), n) };
        }
        data.position += n;
        if !read.is_null() {
            unsafe { *read = n as int32 };
        }
        if n == 0 && wanted > 0 {
            kResultFalse
        } else {
            kResultOk
        }
    }

    unsafe fn write(&self, buffer: *mut c_void, count: int32, written: *mut int32) -> tresult {
        let Ok(mut data) = self.data.lock() else {
            return kResultFalse;
        };
        let n = count.max(0) as usize;
        if n > 0 && !buffer.is_null() {
            let source = unsafe { std::slice::from_raw_parts(buffer.cast::<u8>(), n) };
            let end = data.position + n;
            if data.bytes.len() < end {
                data.bytes.resize(end, 0);
            }
            let position = data.position;
            data.bytes[position..end].copy_from_slice(source);
            data.position = end;
        }
        if !written.is_null() {
            unsafe { *written = n as int32 };
        }
        kResultOk
    }

    unsafe fn seek(&self, offset: int64, mode: int32, result: *mut int64) -> tresult {
        let Ok(mut data) = self.data.lock() else {
            return kResultFalse;
        };
        let base = match mode {
            0 => 0,
            1 => data.position as i64,
            2 => data.bytes.len() as i64,
            _ => return kInvalidArgument,
        };
        let target = base + offset;
        if target < 0 {
            return kInvalidArgument;
        }
        data.position = target as usize;
        if !result.is_null() {
            unsafe { *result = target };
        }
        kResultOk
    }

    unsafe fn tell(&self, position: *mut int64) -> tresult {
        let Ok(data) = self.data.lock() else {
            return kResultFalse;
        };
        if position.is_null() {
            return kInvalidArgument;
        }
        unsafe { *position = data.position as i64 };
        kResultOk
    }
}

pub struct EventList {
    events: UnsafeCell<Vec<Event>>,
}

impl EventList {
    pub fn new() -> Self {
        Self {
            events: UnsafeCell::new(Vec::with_capacity(MAX_EVENTS)),
        }
    }

    pub fn push(&self, event: Event) {
        let events = unsafe { &mut *self.events.get() };
        if events.len() < events.capacity() {
            events.push(event);
        }
    }

    pub fn clear(&self) {
        unsafe { &mut *self.events.get() }.clear();
    }
}

impl Class for EventList {
    type Interfaces = (IEventList,);
}

impl IEventListTrait for EventList {
    unsafe fn getEventCount(&self) -> int32 {
        unsafe { &*self.events.get() }.len() as int32
    }

    unsafe fn getEvent(&self, index: int32, event: *mut Event) -> tresult {
        let events = unsafe { &*self.events.get() };
        match usize::try_from(index).ok().and_then(|i| events.get(i)) {
            Some(found) if !event.is_null() => {
                unsafe { *event = *found };
                kResultOk
            }
            _ => kInvalidArgument,
        }
    }

    unsafe fn addEvent(&self, _event: *mut Event) -> tresult {
        kResultFalse
    }
}

pub struct ParamQueue {
    id: Cell<u32>,
    points: UnsafeCell<Vec<(i32, f64)>>,
}

impl ParamQueue {
    fn new() -> Self {
        Self {
            id: Cell::new(0),
            points: UnsafeCell::new(Vec::with_capacity(MAX_POINTS)),
        }
    }

    #[allow(clippy::mut_from_ref)]
    fn points(&self) -> &mut Vec<(i32, f64)> {
        unsafe { &mut *self.points.get() }
    }
}

impl Class for ParamQueue {
    type Interfaces = (IParamValueQueue,);
}

impl IParamValueQueueTrait for ParamQueue {
    unsafe fn getParameterId(&self) -> ParamID {
        self.id.get()
    }

    unsafe fn getPointCount(&self) -> int32 {
        self.points().len() as int32
    }

    unsafe fn getPoint(&self, index: int32, offset: *mut int32, value: *mut ParamValue) -> tresult {
        match usize::try_from(index)
            .ok()
            .and_then(|i| self.points().get(i).copied())
        {
            Some((o, v)) if !offset.is_null() && !value.is_null() => {
                unsafe {
                    *offset = o;
                    *value = v;
                }
                kResultOk
            }
            _ => kInvalidArgument,
        }
    }

    unsafe fn addPoint(&self, offset: int32, value: ParamValue, index: *mut int32) -> tresult {
        let points = self.points();
        if points.len() >= points.capacity() {
            return kResultFalse;
        }
        points.push((offset, value));
        if !index.is_null() {
            unsafe { *index = points.len() as int32 - 1 };
        }
        kResultOk
    }
}

pub struct ParameterChanges {
    queues: Vec<ComWrapper<ParamQueue>>,
    used: Cell<usize>,
}

impl ParameterChanges {
    pub fn new() -> Self {
        Self {
            queues: (0..MAX_PARAMETERS)
                .map(|_| ComWrapper::new(ParamQueue::new()))
                .collect(),
            used: Cell::new(0),
        }
    }

    pub fn clear(&self) {
        for queue in &self.queues[..self.used.get()] {
            queue.points().clear();
        }
        self.used.set(0);
    }

    fn queue_for(&self, id: u32) -> Option<usize> {
        let used = self.used.get();
        if let Some(found) = self.queues[..used].iter().position(|q| q.id.get() == id) {
            return Some(found);
        }
        let queue = self.queues.get(used)?;
        queue.id.set(id);
        queue.points().clear();
        self.used.set(used + 1);
        Some(used)
    }

    pub fn add(&self, id: u32, offset: i32, value: f64) {
        if let Some(index) = self.queue_for(id) {
            let points = self.queues[index].points();
            if points.len() < points.capacity() {
                points.push((offset, value));
            }
        }
    }

    fn queue_pointer(&self, index: usize) -> *mut IParamValueQueue {
        self.queues
            .get(index)
            .and_then(|q| q.as_com_ref::<IParamValueQueue>())
            .map_or(std::ptr::null_mut(), |q| q.as_ptr())
    }
}

impl Class for ParameterChanges {
    type Interfaces = (IParameterChanges,);
}

impl IParameterChangesTrait for ParameterChanges {
    unsafe fn getParameterCount(&self) -> int32 {
        self.used.get() as int32
    }

    unsafe fn getParameterData(&self, index: int32) -> *mut IParamValueQueue {
        match usize::try_from(index) {
            Ok(i) if i < self.used.get() => self.queue_pointer(i),
            _ => std::ptr::null_mut(),
        }
    }

    unsafe fn addParameterData(
        &self,
        id: *const ParamID,
        index: *mut int32,
    ) -> *mut IParamValueQueue {
        if id.is_null() {
            return std::ptr::null_mut();
        }
        match self.queue_for(unsafe { *id }) {
            Some(i) => {
                if !index.is_null() {
                    unsafe { *index = i as int32 };
                }
                self.queue_pointer(i)
            }
            None => std::ptr::null_mut(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vst3::Steinberg::IBStream_::IStreamSeekMode_::kIBSeekSet;

    #[test]
    fn memory_stream_reads_back_what_was_written() {
        let stream = MemoryStream::default();
        let mut data = [1u8, 2, 3, 4];
        let mut done = 0;
        unsafe {
            assert_eq!(
                stream.write(data.as_mut_ptr().cast(), 4, &mut done),
                kResultOk
            );
            assert_eq!(done, 4);
            let mut pos = 0;
            stream.seek(1, kIBSeekSet, &mut pos);
            let mut out = [0u8; 8];
            stream.read(out.as_mut_ptr().cast(), 8, &mut done);
            assert_eq!((done, &out[..3]), (3, &[2, 3, 4][..]));
        }
        assert_eq!(stream.bytes(), [1, 2, 3, 4]);
    }

    #[test]
    fn parameter_changes_group_points_by_id() {
        let changes = ParameterChanges::new();
        changes.add(7, 0, 0.5);
        changes.add(9, 0, 0.1);
        changes.add(7, 10, 0.6);
        unsafe {
            assert_eq!(changes.getParameterCount(), 2);
            let queue = changes.getParameterData(0);
            assert!(!queue.is_null());
        }
        assert_eq!(changes.queues[0].points().len(), 2);
        changes.clear();
        unsafe { assert_eq!(changes.getParameterCount(), 0) };
    }

    #[test]
    fn host_names_itself_and_makes_messages() {
        let host = HostApplication;
        let mut name: String128 = [0; 128];
        unsafe { host.getName(&mut name) };
        let end = name.iter().position(|&c| c == 0).unwrap();
        assert_eq!(String::from_utf16_lossy(&name[..end]), HOST_NAME);
        let mut cid: TUID = IMessage::IID.map(|b| b as c_char);
        let mut iid = cid;
        let mut object = std::ptr::null_mut();
        unsafe {
            assert_eq!(
                host.createInstance(&mut cid, &mut iid, &mut object),
                kResultOk
            );
            let message = vst3::ComPtr::from_raw(object.cast::<IMessage>()).unwrap();
            message.setMessageID(c"hello".as_ptr());
            assert_eq!(CStr::from_ptr(message.getMessageID()), c"hello");
            let attributes = message.getAttributes();
            let attributes = vst3::ComRef::from_raw(attributes).unwrap();
            attributes.setInt(c"n".as_ptr(), 42);
            let mut n = 0;
            attributes.getInt(c"n".as_ptr(), &mut n);
            assert_eq!(n, 42);
        }
    }
}
