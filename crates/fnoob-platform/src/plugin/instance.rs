use super::PluginError;
use super::com::{ComponentHandler, EventList, MemoryStream, ParamChange, ParameterChanges};
use super::editor::Editor;
use super::module::{ClassInfo, Module};
use rtrb::{Consumer, RingBuffer};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use vst3::Steinberg::Vst::BusDirections_::{kInput, kOutput};
use vst3::Steinberg::Vst::Event_::EventTypes_::{kNoteOffEvent, kNoteOnEvent};
use vst3::Steinberg::Vst::MediaTypes_::{kAudio, kEvent};
use vst3::Steinberg::Vst::ProcessContext_::StatesAndFlags_::{
    kPlaying, kProjectTimeMusicValid, kTempoValid, kTimeSigValid,
};
use vst3::Steinberg::Vst::ProcessModes_::kRealtime;
use vst3::Steinberg::Vst::SymbolicSampleSizes_::kSample32;
use vst3::Steinberg::Vst::{
    AudioBusBuffers, AudioBusBuffers__type0, BusInfo, Event, Event__type0, IAudioProcessor,
    IAudioProcessorTrait, IComponent, IComponentHandler, IComponentTrait, IConnectionPoint,
    IConnectionPointTrait, IEditController, IEditControllerTrait, IEventList, IParameterChanges,
    NoteOffEvent, NoteOnEvent, ProcessContext, ProcessData, ProcessSetup,
};
use vst3::Steinberg::{IBStream, IPluginBaseTrait, TUID, kResultOk};
use vst3::{ComPtr, ComWrapper};

const CHANGE_CAPACITY: usize = 1024;
const STEREO: u64 = 3;

struct Shared {
    connection: Option<(ComPtr<IConnectionPoint>, ComPtr<IConnectionPoint>)>,
    controller: Option<ComPtr<IEditController>>,
    separate_controller: bool,
    component: ComPtr<IComponent>,
    handler: ComWrapper<ComponentHandler>,
    module: Arc<Module>,
}

unsafe impl Send for Shared {}
unsafe impl Sync for Shared {}

impl Drop for Shared {
    fn drop(&mut self) {
        unsafe {
            self.component.setActive(0);
            if let Some((component, controller)) = &self.connection {
                component.disconnect(controller.as_ptr());
                controller.disconnect(component.as_ptr());
            }
            if let Some(controller) = &self.controller {
                controller.setComponentHandler(std::ptr::null_mut());
                if self.separate_controller {
                    controller.terminate();
                }
            }
            self.component.terminate();
        }
        let _ = &self.module;
    }
}

pub struct Plugin {
    shared: Arc<Shared>,
    pub info: ClassInfo,
    pub latency: u32,
}

struct BusBuffers {
    channels: Vec<Vec<f32>>,
    pointers: Vec<*mut f32>,
}

impl BusBuffers {
    fn new(channels: usize, max_block: usize) -> Self {
        let mut channels: Vec<Vec<f32>> = (0..channels).map(|_| vec![0.0; max_block]).collect();
        let pointers = channels.iter_mut().map(|c| c.as_mut_ptr()).collect();
        Self { channels, pointers }
    }

    fn header(&mut self) -> AudioBusBuffers {
        AudioBusBuffers {
            numChannels: self.channels.len() as i32,
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: self.pointers.as_mut_ptr(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transport {
    pub playing: bool,
    pub position: u64,
    pub tempo: f64,
    pub numerator: i32,
    pub denominator: i32,
    pub quarter_notes: f64,
}

pub struct PluginProcessor {
    processor: ComPtr<IAudioProcessor>,
    events: ComWrapper<EventList>,
    params: ComWrapper<ParameterChanges>,
    changes: Consumer<ParamChange>,
    inputs: Vec<BusBuffers>,
    outputs: Vec<BusBuffers>,
    input_headers: Vec<AudioBusBuffers>,
    output_headers: Vec<AudioBusBuffers>,
    context: ProcessContext,
    sample_rate: f64,
    max_block: usize,
    shared: Arc<Shared>,
}

unsafe impl Send for PluginProcessor {}

impl Drop for PluginProcessor {
    fn drop(&mut self) {
        unsafe { self.processor.setProcessing(0) };
        let _ = &self.shared;
    }
}

fn bus_channels(component: &ComPtr<IComponent>, media: i32, direction: i32) -> Vec<usize> {
    let count = unsafe { component.getBusCount(media, direction) }.max(0);
    (0..count)
        .map(|index| {
            let mut info: BusInfo = unsafe { std::mem::zeroed() };
            if unsafe { component.getBusInfo(media, direction, index, &mut info) } == kResultOk {
                info.channelCount.max(0) as usize
            } else {
                0
            }
        })
        .collect()
}

fn stream_pointer(stream: &ComWrapper<MemoryStream>) -> *mut IBStream {
    stream
        .as_com_ref::<IBStream>()
        .map_or(std::ptr::null_mut(), |s| s.as_ptr())
}

fn sync_controller(shared: &Shared) {
    let Some(controller) = &shared.controller else {
        return;
    };
    let stream = ComWrapper::new(MemoryStream::default());
    unsafe {
        if shared.component.getState(stream_pointer(&stream)) == kResultOk {
            stream.rewind();
            controller.setComponentState(stream_pointer(&stream));
        }
    }
}

fn apply_state(
    shared: &Shared,
    component_state: &[u8],
    controller_state: Option<&[u8]>,
) -> Result<(), PluginError> {
    let stream = ComWrapper::new(MemoryStream::with_bytes(component_state));
    if unsafe { shared.component.setState(stream_pointer(&stream)) } != kResultOk {
        return Err(PluginError::State("restored"));
    }
    if let Some(controller) = &shared.controller {
        stream.rewind();
        unsafe { controller.setComponentState(stream_pointer(&stream)) };
        if let Some(bytes) = controller_state {
            let stream = ComWrapper::new(MemoryStream::with_bytes(bytes));
            unsafe { controller.setState(stream_pointer(&stream)) };
        }
    }
    Ok(())
}

pub fn instantiate(
    module: &Arc<Module>,
    info: &ClassInfo,
    sample_rate: f64,
    max_block: usize,
    state: Option<(&[u8], Option<&[u8]>)>,
) -> Result<(Plugin, Box<PluginProcessor>), PluginError> {
    let host = module.host_context();
    let component: ComPtr<IComponent> = module
        .create(&info.cid)
        .ok_or(PluginError::Create("component"))?;
    if unsafe { component.initialize(host) } != kResultOk {
        return Err(PluginError::Setup("to start"));
    }
    let (controller, separate_controller) = match component.cast::<IEditController>() {
        Some(controller) => (Some(controller), false),
        None => {
            let mut tuid: TUID = [0; 16];
            let created = if unsafe { component.getControllerClassId(&mut tuid) } == kResultOk {
                module
                    .create::<IEditController>(&tuid.map(|c| c as u8))
                    .filter(|c| unsafe { c.initialize(host) } == kResultOk)
            } else {
                None
            };
            let separate = created.is_some();
            (created, separate)
        }
    };
    let connection = if separate_controller {
        match (
            component.cast::<IConnectionPoint>(),
            controller
                .as_ref()
                .and_then(|c| c.cast::<IConnectionPoint>()),
        ) {
            (Some(a), Some(b)) => {
                unsafe {
                    a.connect(b.as_ptr());
                    b.connect(a.as_ptr());
                }
                Some((a, b))
            }
            _ => None,
        }
    } else {
        None
    };
    let (changes_tx, changes_rx) = RingBuffer::new(CHANGE_CAPACITY);
    let handler = ComWrapper::new(ComponentHandler::new(changes_tx));
    if let (Some(controller), Some(handler)) =
        (&controller, handler.as_com_ref::<IComponentHandler>())
    {
        unsafe { controller.setComponentHandler(handler.as_ptr()) };
    }
    let shared = Arc::new(Shared {
        connection,
        controller,
        separate_controller,
        component,
        handler,
        module: Arc::clone(module),
    });
    sync_controller(&shared);
    let processor = shared
        .component
        .cast::<IAudioProcessor>()
        .ok_or(PluginError::Create("audio processor"))?;
    let component = &shared.component;
    let input_count = unsafe { component.getBusCount(kAudio, kInput) }.max(0);
    let output_count = unsafe { component.getBusCount(kAudio, kOutput) }.max(0);
    if output_count == 0 {
        return Err(PluginError::Setup("to provide an audio output"));
    }
    unsafe {
        component.activateBus(kAudio, kOutput, 0, 1);
        if input_count > 0 {
            component.activateBus(kAudio, kInput, 0, 1);
        }
        if component.getBusCount(kEvent, kInput) > 0 {
            component.activateBus(kEvent, kInput, 0, 1);
        }
        let mut ins = vec![STEREO; input_count as usize];
        let mut outs = vec![STEREO; output_count as usize];
        processor.setBusArrangements(
            ins.as_mut_ptr(),
            input_count,
            outs.as_mut_ptr(),
            output_count,
        );
        if processor.canProcessSampleSize(kSample32) != kResultOk {
            return Err(PluginError::Setup("32-bit float audio"));
        }
        let mut setup = ProcessSetup {
            processMode: kRealtime,
            symbolicSampleSize: kSample32,
            maxSamplesPerBlock: max_block as i32,
            sampleRate: sample_rate,
        };
        if processor.setupProcessing(&mut setup) != kResultOk {
            return Err(PluginError::Setup("the sample rate or block size"));
        }
    }
    if let Some((component_state, controller_state)) = state {
        apply_state(&shared, component_state, controller_state)?;
    }
    if unsafe { shared.component.setActive(1) } != kResultOk {
        return Err(PluginError::Setup("to activate"));
    }
    unsafe { processor.setProcessing(1) };
    let latency = unsafe { processor.getLatencySamples() };
    let mut inputs: Vec<BusBuffers> = bus_channels(&shared.component, kAudio, kInput)
        .into_iter()
        .map(|channels| BusBuffers::new(channels, max_block))
        .collect();
    let mut outputs: Vec<BusBuffers> = bus_channels(&shared.component, kAudio, kOutput)
        .into_iter()
        .map(|channels| BusBuffers::new(channels, max_block))
        .collect();
    let input_headers = inputs.iter_mut().map(BusBuffers::header).collect();
    let output_headers = outputs.iter_mut().map(BusBuffers::header).collect();
    let plugin = Plugin {
        shared: Arc::clone(&shared),
        info: info.clone(),
        latency,
    };
    let processor = Box::new(PluginProcessor {
        processor,
        events: ComWrapper::new(EventList::new()),
        params: ComWrapper::new(ParameterChanges::new()),
        changes: changes_rx,
        inputs,
        outputs,
        input_headers,
        output_headers,
        context: unsafe { std::mem::zeroed() },
        sample_rate,
        max_block,
        shared,
    });
    Ok((plugin, processor))
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParameterInfo {
    pub id: u32,
    pub name: String,
    pub default: f64,
}

impl Plugin {
    pub fn parameters(&self) -> Vec<ParameterInfo> {
        use vst3::Steinberg::Vst::ParameterInfo_::ParameterFlags_::{kCanAutomate, kIsHidden};
        let Some(controller) = &self.shared.controller else {
            return Vec::new();
        };
        let count = unsafe { controller.getParameterCount() }.max(0);
        (0..count)
            .filter_map(|index| {
                let mut info: vst3::Steinberg::Vst::ParameterInfo = unsafe { std::mem::zeroed() };
                if unsafe { controller.getParameterInfo(index, &mut info) } != kResultOk {
                    return None;
                }
                if info.flags & kCanAutomate == 0 || info.flags & kIsHidden != 0 {
                    return None;
                }
                let end = info
                    .title
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(info.title.len());
                Some(ParameterInfo {
                    id: info.id,
                    name: String::from_utf16_lossy(&info.title[..end]),
                    default: info.defaultNormalizedValue,
                })
            })
            .collect()
    }

    pub fn normalized(&self, id: u32) -> Option<f64> {
        let controller = self.shared.controller.as_ref()?;
        Some(unsafe { controller.getParamNormalized(id) })
    }

    pub fn show_value(&self, id: u32, value: f64) {
        if let Some(controller) = &self.shared.controller {
            unsafe { controller.setParamNormalized(id, value.clamp(0.0, 1.0)) };
        }
    }

    pub fn state(&self) -> Result<(Vec<u8>, Option<Vec<u8>>), PluginError> {
        let stream = ComWrapper::new(MemoryStream::default());
        if unsafe { self.shared.component.getState(stream_pointer(&stream)) } != kResultOk {
            return Err(PluginError::State("saved"));
        }
        let controller = self.shared.controller.as_ref().and_then(|controller| {
            let stream = ComWrapper::new(MemoryStream::default());
            (unsafe { controller.getState(stream_pointer(&stream)) } == kResultOk)
                .then(|| stream.bytes())
        });
        Ok((stream.bytes(), controller))
    }

    pub fn take_edited(&self) -> bool {
        self.shared.handler.edited.swap(false, Ordering::AcqRel)
    }

    pub fn open_editor(&self, title: &str) -> Result<Editor, PluginError> {
        let controller = self
            .shared
            .controller
            .as_ref()
            .ok_or(PluginError::Editor("the plugin has no editor"))?;
        Editor::open(controller, title)
    }
}

impl PluginProcessor {
    pub fn max_block(&self) -> usize {
        self.max_block
    }

    pub fn note(&self, offset: usize, pitch: u8, velocity: u8) {
        let mut event: Event = unsafe { std::mem::zeroed() };
        event.sampleOffset = offset as i32;
        if velocity > 0 {
            event.r#type = kNoteOnEvent as u16;
            event.__field0 = Event__type0 {
                noteOn: NoteOnEvent {
                    channel: 0,
                    pitch: i16::from(pitch),
                    tuning: 0.0,
                    velocity: f32::from(velocity.min(127)) / 127.0,
                    length: 0,
                    noteId: -1,
                },
            };
        } else {
            event.r#type = kNoteOffEvent as u16;
            event.__field0 = Event__type0 {
                noteOff: NoteOffEvent {
                    channel: 0,
                    pitch: i16::from(pitch),
                    velocity: 0.0,
                    noteId: -1,
                    tuning: 0.0,
                },
            };
        }
        self.events.push(event);
    }

    pub fn set_param(&mut self, id: u32, value: f64) {
        self.params.add(id, 0, value.clamp(0.0, 1.0));
    }

    pub fn process(&mut self, left: &mut [f32], right: &mut [f32], transport: &Transport) {
        let frames = left.len().min(right.len()).min(self.max_block);
        while let Ok(change) = self.changes.pop() {
            self.params.add(change.id, 0, change.value);
        }
        if let Some(main) = self.inputs.first_mut() {
            match main.channels.as_mut_slice() {
                [mono] => {
                    for (i, sample) in mono[..frames].iter_mut().enumerate() {
                        *sample = (left[i] + right[i]) * 0.5;
                    }
                }
                [l, r, ..] => {
                    l[..frames].copy_from_slice(&left[..frames]);
                    r[..frames].copy_from_slice(&right[..frames]);
                }
                [] => {}
            }
        }
        let mut state = kTempoValid | kTimeSigValid | kProjectTimeMusicValid;
        if transport.playing {
            state |= kPlaying;
        }
        self.context.state = state as u32;
        self.context.sampleRate = self.sample_rate;
        self.context.projectTimeSamples = transport.position as i64;
        self.context.projectTimeMusic = transport.quarter_notes;
        self.context.tempo = transport.tempo;
        self.context.timeSigNumerator = transport.numerator;
        self.context.timeSigDenominator = transport.denominator;
        let mut data = ProcessData {
            processMode: kRealtime,
            symbolicSampleSize: kSample32,
            numSamples: frames as i32,
            numInputs: self.input_headers.len() as i32,
            numOutputs: self.output_headers.len() as i32,
            inputs: self.input_headers.as_mut_ptr(),
            outputs: self.output_headers.as_mut_ptr(),
            inputParameterChanges: self
                .params
                .as_com_ref::<IParameterChanges>()
                .map_or(std::ptr::null_mut(), |p| p.as_ptr()),
            outputParameterChanges: std::ptr::null_mut(),
            inputEvents: self
                .events
                .as_com_ref::<IEventList>()
                .map_or(std::ptr::null_mut(), |e| e.as_ptr()),
            outputEvents: std::ptr::null_mut(),
            processContext: &mut self.context,
        };
        let result = unsafe { self.processor.process(&mut data) };
        self.events.clear();
        self.params.clear();
        if result != kResultOk {
            return;
        }
        if let Some(main) = self.outputs.first() {
            match main.channels.as_slice() {
                [mono] => {
                    left[..frames].copy_from_slice(&mono[..frames]);
                    right[..frames].copy_from_slice(&mono[..frames]);
                }
                [l, r, ..] => {
                    left[..frames].copy_from_slice(&l[..frames]);
                    right[..frames].copy_from_slice(&r[..frames]);
                }
                [] => {}
            }
        }
    }
}
