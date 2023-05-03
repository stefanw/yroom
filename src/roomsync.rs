use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use pyo3::{
    prelude::*,
    types::{PyBytes, PyDict},
};

use lib0::{
    decoding::{Cursor, Read},
    encoding::Write,
};
use y_sync::{
    awareness::Awareness,
    sync::{Message, MessageReader, SyncMessage},
};
use yrs::{
    types::ToJson,
    updates::{
        decoder::{Decode, DecoderV1, DecoderV2},
        encoder::{Encode, Encoder, EncoderV1, EncoderV2},
    },
    GetString, ReadTxn, StateVector, Transact, Update,
};

#[derive(Clone, Default, Debug)]
enum ProtocolVersion {
    #[default]
    V1,
    V2,
}

struct EncoderWrapper {
    protocol_version: ProtocolVersion,
    messages: Vec<Message>,
    prefix: Option<String>,
}

impl EncoderWrapper {
    fn new(protocol_version: &ProtocolVersion, prefix: Option<String>) -> Self {
        EncoderWrapper {
            protocol_version: protocol_version.clone(),
            messages: Vec::default(),
            prefix,
        }
    }
    fn push(&mut self, message: Message) {
        self.messages.push(message);
    }
    fn to_vec(&self) -> Vec<u8> {
        match self.protocol_version {
            ProtocolVersion::V1 => {
                if self.messages.is_empty() {
                    return Vec::new();
                }
                let mut encoder = EncoderV1::new();
                if let Some(prefix) = &self.prefix {
                    encoder.write_string(prefix);
                }
                self.messages.iter().for_each(|message| {
                    message.encode(&mut encoder);
                });
                encoder.to_vec()
            }
            ProtocolVersion::V2 => {
                if self.messages.is_empty() {
                    return Vec::new();
                }
                let mut encoder = EncoderV2::new();
                if let Some(prefix) = &self.prefix {
                    encoder.write_string(prefix);
                }
                self.messages.iter().for_each(|message| {
                    message.encode(&mut encoder);
                });
                encoder.to_vec()
            }
        }
    }
}

struct DecoderWrapper<'a> {
    protocol_version: ProtocolVersion,
    decoder_v1: Option<DecoderV1<'a>>,
    decoder_v2: Option<DecoderV2<'a>>,
    pub document_name: Option<String>,
}

impl<'a> DecoderWrapper<'a> {
    fn new(
        protocol_version: &ProtocolVersion,
        cursor: Cursor<'a>,
        name_prefix: bool,
    ) -> Result<Self, lib0::error::Error> {
        let mut document_name = None;
        match protocol_version {
            ProtocolVersion::V1 => {
                let mut decoder = DecoderV1::new(cursor);
                if name_prefix {
                    document_name = Some(decoder.read_string()?.to_string());
                }
                Ok(DecoderWrapper {
                    protocol_version: protocol_version.clone(),
                    decoder_v1: Some(decoder),
                    decoder_v2: None,
                    document_name,
                })
            }
            ProtocolVersion::V2 => match DecoderV2::new(cursor) {
                Ok(mut decoder) => {
                    if name_prefix {
                        document_name = Some(decoder.read_string()?.to_string());
                    }
                    Ok(DecoderWrapper {
                        protocol_version: protocol_version.clone(),
                        decoder_v1: None,
                        decoder_v2: Some(decoder),
                        document_name,
                    })
                }
                Err(err) => Err(err),
            },
        }
    }
}

impl Iterator for DecoderWrapper<'_> {
    type Item = Result<Message, lib0::error::Error>;
    fn next(&mut self) -> Option<Self::Item> {
        match self.protocol_version {
            ProtocolVersion::V1 => MessageReader::new(self.decoder_v1.as_mut().unwrap()).next(),
            ProtocolVersion::V2 => MessageReader::new(self.decoder_v2.as_mut().unwrap()).next(),
        }
    }
}

impl From<u8> for ProtocolVersion {
    fn from(version: u8) -> Self {
        match version {
            1 => ProtocolVersion::V1,
            2 => ProtocolVersion::V2,
            // TODO: make this more graceful
            _ => panic!("Invalid encoder version"),
        }
    }
}

#[derive(Clone, Debug)]
struct YRoomSettings {
    pub protocol_version: ProtocolVersion,
    pub name_prefix: bool,
    pub server_start_sync: bool,
}

impl Default for YRoomSettings {
    fn default() -> Self {
        YRoomSettings {
            protocol_version: ProtocolVersion::V1,
            name_prefix: false,
            server_start_sync: true,
        }
    }
}

impl FromPyObject<'_> for YRoomSettings {
    fn extract(ob: &PyAny) -> PyResult<Self> {
        let settings = ob.downcast::<PyDict>()?;

        let protocol_version: ProtocolVersion = match settings.get_item("PROTOCOL_VERSION") {
            Some(protocol_version) => protocol_version.extract::<u8>()?.into(),
            None => ProtocolVersion::V1,
        };
        let name_prefix = match settings.get_item("PROTOCOL_NAME_PREFIX") {
            Some(name_prefix) => name_prefix.extract::<bool>()?,
            None => false,
        };
        let server_start_sync = match settings.get_item("SERVER_START_SYNC") {
            Some(server_start_sync) => server_start_sync.extract::<bool>()?,
            None => true,
        };

        Ok(YRoomSettings {
            protocol_version,
            name_prefix,
            server_start_sync,
        })
    }
}

#[pyclass]
pub struct YRoomMessage {
    #[pyo3(get)]
    pub payload: PyObject,
    #[pyo3(get)]
    pub broadcast_payload: PyObject,
}

#[pymethods]
impl YRoomMessage {
    pub fn __str__(&self) -> String {
        format!(
            "YRoomMessage(payload: {}, broadcast_payload: {})",
            self.payload, self.broadcast_payload
        )
    }

    pub fn __repr__(&self) -> String {
        self.__str__()
    }
}

#[pyclass]
pub struct YRoomManager {
    rooms: HashMap<String, YRoom>,
    default_settings: YRoomSettings,
    room_settings: Vec<(String, YRoomSettings)>,
}

impl YRoomManager {
    fn new_with_settings(settings: &PyDict) -> Self {
        let default_settings = match settings.get_item(DEFAULT_KEY) {
            Some(default) => default.extract::<YRoomSettings>().unwrap(),
            None => YRoomSettings::default(),
        };
        let mut room_settings = Vec::new();
        for (key, value) in settings.iter() {
            let key = key.extract::<String>().unwrap();
            if key == DEFAULT_KEY {
                continue;
            }
            room_settings.push((key, value.extract::<YRoomSettings>().unwrap()));
        }

        YRoomManager {
            rooms: HashMap::new(),
            default_settings,
            room_settings,
        }
    }
    fn new_with_default() -> Self {
        YRoomManager {
            rooms: HashMap::new(),
            default_settings: YRoomSettings::default(),
            room_settings: Vec::default(),
        }
    }
    fn get_room_with_data(&mut self, room: &str, data: Vec<u8>) -> &mut YRoom {
        let settings = self.find_settings(room);
        self.rooms.entry(room.to_string()).or_insert_with(|| {
            log::info!(
                "Creating new YRoom '{}' with data and settings {:?}",
                room,
                settings
            );
            YRoom::new(settings, Some(data))
        })
    }

    fn get_room(&mut self, room: &str) -> &mut YRoom {
        let settings = self.find_settings(room);
        self.rooms.entry(room.to_string()).or_insert_with(|| {
            log::info!("Creating new YRoom '{}' with settings {:?}", room, settings);
            YRoom::new(settings, None)
        })
    }

    fn find_settings(&self, room: &str) -> YRoomSettings {
        for (prefix, config) in &self.room_settings {
            if room.starts_with(prefix) {
                return config.clone();
            }
        }
        self.default_settings.clone()
    }
}

const DEFAULT_KEY: &str = "default";

#[pymethods]
impl YRoomManager {
    #[new]
    fn new(settings: Option<&PyDict>) -> Self {
        match settings {
            Some(settings) => Self::new_with_settings(settings),
            None => Self::new_with_default(),
        }
    }

    pub fn connect(&mut self, room: String, conn_id: u64) -> YRoomMessage {
        self.get_room(&room).connect(conn_id)
    }
    pub fn connect_with_data(&mut self, room: String, conn_id: u64, data: Vec<u8>) -> YRoomMessage {
        self.get_room_with_data(&room, data).connect(conn_id)
    }

    pub fn handle_message(&mut self, room: String, conn_id: u64, payload: Vec<u8>) -> YRoomMessage {
        self.get_room(&room).handle_message(conn_id, payload)
    }

    pub fn disconnect(&mut self, room: String, conn_id: u64) -> YRoomMessage {
        let broadcast_payload = self.get_room(&room).disconnect(conn_id);
        Python::with_gil(|py| YRoomMessage {
            payload: PyBytes::new(py, &[]).into(),
            broadcast_payload: PyBytes::new(py, &broadcast_payload).into(),
        })
    }

    pub fn has_room(&self, room: String) -> bool {
        self.rooms.contains_key(&room)
    }

    pub fn is_room_alive(&self, room: String) -> bool {
        let room = self.rooms.get(&room);
        match room {
            Some(room) => room.is_alive(),
            None => false,
        }
    }

    pub fn serialize_room(&self, room: String) -> PyObject {
        let yroom = self.rooms.get(&room);
        Python::with_gil(|py| match yroom {
            None => py.None(),
            Some(yroom) => PyBytes::new(py, &yroom.serialize()).into(),
        })
    }

    pub fn remove_room(&mut self, room: String) {
        self.rooms.remove(&room);
    }

    pub fn list_rooms(&self) -> Vec<String> {
        self.rooms.keys().cloned().collect()
    }

    pub fn export_map(&self, room: String, name: String) -> PyObject {
        let yroom = self.rooms.get(&room);
        match yroom {
            Some(room) => {
                let doc = room.awareness.doc();
                let obj = doc.get_or_insert_map(&name);
                let serialized = obj.to_json(&doc.transact());
                let mut result = Default::default();
                serialized.to_json(&mut result);
                Python::with_gil(|py| result.to_object(py))
            }
            None => Python::with_gil(|py| py.None()),
        }
    }

    pub fn export_array(&self, room: String, name: String) -> PyObject {
        let yroom = self.rooms.get(&room);
        match yroom {
            Some(room) => {
                let doc = room.awareness.doc();
                let obj = doc.get_or_insert_array(&name);
                let serialized = obj.to_json(&doc.transact());
                let mut result = Default::default();
                serialized.to_json(&mut result);
                Python::with_gil(|py| result.to_object(py))
            }
            None => Python::with_gil(|py| py.None()),
        }
    }

    pub fn export_text(&self, room: String, name: String) -> PyObject {
        let yroom = self.rooms.get(&room);
        match yroom {
            Some(room) => {
                let doc = room.awareness.doc();
                let obj = doc.get_or_insert_text(&name);

                let serialized = obj.get_string(&doc.transact());
                Python::with_gil(|py| serialized.to_object(py))
            }
            None => Python::with_gil(|py| py.None()),
        }
    }

    pub fn export_xml_element(&self, room: String, name: String) -> PyObject {
        let yroom = self.rooms.get(&room);
        match yroom {
            Some(room) => {
                let doc = room.awareness.doc();
                let obj = doc.get_or_insert_xml_element(&name);

                let serialized = obj.get_string(&doc.transact());
                Python::with_gil(|py| serialized.to_object(py))
            }
            None => Python::with_gil(|py| py.None()),
        }
    }

    pub fn export_xml_text(&self, room: String, name: String) -> PyObject {
        let yroom = self.rooms.get(&room);
        match yroom {
            Some(room) => {
                let doc = room.awareness.doc();
                let obj = doc.get_or_insert_xml_text(&name);

                let serialized = obj.get_string(&doc.transact());
                Python::with_gil(|py| serialized.to_object(py))
            }
            None => Python::with_gil(|py| py.None()),
        }
    }

    pub fn export_xml_fragment(&self, room: String, name: String) -> PyObject {
        let yroom = self.rooms.get(&room);
        match yroom {
            Some(room) => {
                let doc = room.awareness.doc();
                let obj = doc.get_or_insert_xml_fragment(&name);

                let serialized = obj.get_string(&doc.transact());
                Python::with_gil(|py| serialized.to_object(py))
            }
            None => Python::with_gil(|py| py.None()),
        }
    }
}

pub struct YRoom {
    awareness: Awareness,
    connections: Arc<Mutex<HashMap<u64, HashSet<u64>>>>,
    settings: YRoomSettings,
}

impl YRoom {
    fn new(settings: YRoomSettings, update_vec: Option<Vec<u8>>) -> Self {
        let mut awareness = Awareness::default();
        if let Some(update_vec) = update_vec {
            let update = Update::decode_v1(&update_vec);
            match update {
                Ok(update) => {
                    let mut txn = awareness.doc_mut().transact_mut();
                    txn.apply_update(update);
                }
                Err(e) => log::error!("Error decoding update: {}", e),
            }
        }
        YRoom {
            awareness,
            connections: Arc::new(Mutex::new(HashMap::new())),
            settings,
        }
    }

    fn connect(&mut self, conn_id: u64) -> YRoomMessage {
        let connections = self.connections.lock();
        connections
            .unwrap()
            .entry(conn_id)
            .or_insert_with(HashSet::new);

        let mut encoder = EncoderWrapper::new(&self.settings.protocol_version, None);

        if self.settings.server_start_sync {
            let sv = self.awareness.doc().transact().state_vector();
            encoder.push(Message::Sync(SyncMessage::SyncStep1(sv)));
            if !self.awareness.clients().is_empty() {
                if let Ok(awareness_update) = self.awareness.update() {
                    encoder.push(Message::Awareness(awareness_update));
                }
            }
        }
        let payload = encoder.to_vec();
        Python::with_gil(|py| YRoomMessage {
            payload: PyBytes::new(py, &payload).into(),
            broadcast_payload: PyBytes::new(py, &[]).into(),
        })
    }

    pub fn handle_message(&mut self, conn_id: u64, payload: Vec<u8>) -> YRoomMessage {
        log::debug!("message: {:?}", payload);
        let cursor = Cursor::new(&payload);
        let decoder = match DecoderWrapper::new(
            &self.settings.protocol_version,
            cursor,
            self.settings.name_prefix,
        ) {
            Ok(decoder) => decoder,
            Err(e) => {
                log::error!("Error decoding message: {}", e);
                // TODO: return error message
                return Python::with_gil(|py| YRoomMessage {
                    payload: PyBytes::new(py, &[]).into(),
                    broadcast_payload: PyBytes::new(py, &[]).into(),
                });
            }
        };

        let mut sync_encoder = EncoderWrapper::new(
            &self.settings.protocol_version,
            decoder.document_name.clone(),
        );
        let mut update_encoder = EncoderWrapper::new(
            &self.settings.protocol_version,
            decoder.document_name.clone(),
        );

        decoder.for_each(|message_result| match message_result {
            Ok(message) => match message {
                Message::Sync(SyncMessage::SyncStep1(sv)) => {
                    let txn = self.awareness.doc_mut().transact_mut();
                    let data = match self.settings.protocol_version {
                        ProtocolVersion::V1 => txn.encode_diff_v1(&sv),
                        ProtocolVersion::V2 => {
                            let mut enc = EncoderV2::new();
                            txn.encode_diff(&sv, &mut enc);
                            enc.to_vec()
                        }
                    };
                    log::debug!("message: {:?}", data);
                    let message = Message::Sync(SyncMessage::SyncStep2(data));
                    sync_encoder.push(message);
                }
                Message::Sync(SyncMessage::SyncStep2(data)) => {
                    let update = match self.settings.protocol_version {
                        ProtocolVersion::V1 => Update::decode_v1(&data),
                        ProtocolVersion::V2 => Update::decode_v2(&data),
                    };
                    match update {
                        Ok(update) => {
                            let mut txn = self.awareness.doc_mut().transact_mut();
                            txn.apply_update(update);
                        }
                        Err(e) => log::error!("Error decoding sync step 2: {}", e),
                    }
                }
                Message::Sync(SyncMessage::Update(data)) => {
                    let update = Update::decode_v1(&data);
                    match update {
                        Ok(update) => {
                            let mut txn = self.awareness.doc_mut().transact_mut();
                            txn.apply_update(update);
                            let message = Message::Sync(SyncMessage::Update(data));
                            update_encoder.push(message)
                        }
                        Err(e) => log::error!("Error decoding update: {}", e),
                    }
                }
                Message::Auth(_) => {
                    // TODO: check this. Always reply with permission granted
                    log::warn!("Auth message received. Replying with permission granted");
                    sync_encoder.push(Message::Auth(None))
                }
                Message::AwarenessQuery => {
                    if let Ok(awareness_update) = self.awareness.update() {
                        sync_encoder.push(Message::Awareness(awareness_update))
                    }
                }
                Message::Awareness(awareness_update) => {
                    // Add/remove client ids to conn ids
                    self.connections
                        .lock()
                        .unwrap()
                        .entry(conn_id)
                        .or_insert_with(HashSet::new);
                    let connections = self.connections.clone();
                    {
                        let _sub = self.awareness.on_update(move |_, ev| {
                            let mut connections = connections.lock().unwrap();
                            let client_ids = connections.get_mut(&conn_id).unwrap();
                            ev.added().iter().for_each(|client_id| {
                                client_ids.insert(*client_id);
                            });
                            ev.removed().iter().for_each(|client_id| {
                                client_ids.remove(client_id);
                            });
                        });
                        if let Err(e) = self.awareness.apply_update(awareness_update) {
                            log::error!("Error applying awareness update: {}", e);
                        }
                    }
                    if let Ok(awareness_update) = self.awareness.update() {
                        update_encoder.push(Message::Awareness(awareness_update))
                    }
                }
                Message::Custom(custom_type, _) => {
                    // FIXME: handle custom
                    log::warn!("Unhandled custom message received. Type: {}", custom_type);
                }
            },
            Err(err) => {
                log::warn!("Bad message from connection {}: {:?}", conn_id, err);
            }
        });

        Python::with_gil(|py| YRoomMessage {
            payload: PyBytes::new(py, &sync_encoder.to_vec()).into(),
            broadcast_payload: PyBytes::new(py, &update_encoder.to_vec()).into(),
        })
    }

    pub fn disconnect(&mut self, conn_id: u64) -> Vec<u8> {
        {
            let mut connections = self.connections.lock().unwrap();
            let client_ids = connections.get(&conn_id);
            if let Some(client_ids) = client_ids {
                client_ids.iter().for_each(|client_id| {
                    self.awareness.remove_state(*client_id);
                });
            }
            connections.remove(&conn_id);
        }
        // FIXME: Can't give possibly necessary name prefix on disconnect
        let mut encoder = EncoderWrapper::new(&self.settings.protocol_version, None);
        if let Ok(awareness_update) = self.awareness.update() {
            encoder.push(Message::Awareness(awareness_update));
        }
        encoder.to_vec()
    }

    pub fn serialize(&self) -> Vec<u8> {
        let txn = self.awareness.doc().transact();
        match self.settings.protocol_version {
            ProtocolVersion::V1 => txn.encode_state_as_update_v1(&StateVector::default()),
            ProtocolVersion::V2 => txn.encode_state_as_update_v2(&StateVector::default()),
        }
    }

    pub fn is_alive(&self) -> bool {
        !self.connections.lock().unwrap().is_empty()
    }
}
